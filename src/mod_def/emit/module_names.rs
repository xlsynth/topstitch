// SPDX-License-Identifier: Apache-2.0

use super::{EmitOptions, NetCollection, VastFile, VastFileType};
use crate::mod_def::ParameterSpec;
use crate::mod_def::core::InstanceConnections;
use crate::{IO, ModDef, ModDefCore, Primitive, Usage};
use indexmap::{IndexMap, IndexSet};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::sync::Arc;

type DefinitionId = *const RwLock<ModDefCore>;

/// A single interface supplies the declaration and every instance's named port bindings.
pub(super) struct ModuleInterface {
    pub name: String,
    pub ports: IndexMap<String, IO>,
    pub parameters: IndexMap<String, ParameterSpec>,
    pub enum_ports: IndexMap<String, String>,
    pub primitive: Option<Primitive>,
    mapped: bool,
    pub usage: Usage,
    imported: bool,
}

/// Instance membership, connection handles, and local names captured together for one body.
pub(super) struct ModuleBody {
    pub definition: ModDef,
    pub instances: IndexMap<String, (ModDef, InstanceConnections)>,
    pub port_connections: InstanceConnections,
    pub inline_ops: IndexSet<String>,
    pub nets: NetCollection,
}

impl ModuleBody {
    /// Takes the local structural view without holding a graph lock during connection tracing.
    fn capture(definition: &ModDef, stub: bool) -> Self {
        let mut body = Self {
            definition: definition.clone(),
            instances: IndexMap::new(),
            port_connections: IndexMap::new(),
            inline_ops: IndexSet::new(),
            nets: NetCollection::new(),
        };
        if stub {
            return body;
        }
        let core = definition.core.read();
        body.port_connections = core.mod_def_connections.clone();
        for (name, target) in &core.instances {
            if core.inlines_primitive(name) {
                body.inline_ops.insert(name.clone());
            }
            let pins = core
                .mod_inst_connections
                .get(name)
                .cloned()
                .unwrap_or_default();
            body.instances.insert(
                name.clone(),
                (
                    ModDef {
                        core: target.clone(),
                    },
                    pins,
                ),
            );
        }
        if !body.inline_ops.is_empty() || !core.primitives.anonymous.is_empty() {
            body.nets.reserve_names(&core);
            for name in &body.inline_ops {
                if core.primitives.anonymous.contains(name) {
                    body.nets
                        .reserve_inline_outputs(name, &core.instances[name].read());
                }
            }
        }
        body
    }
}

/// The captured module bodies and their SystemVerilog names for one emission.
#[derive(Default)]
pub(super) struct EmissionHierarchy {
    interfaces: HashMap<DefinitionId, ModuleInterface>,
    bodies: HashMap<DefinitionId, ModuleBody>,
    order: Vec<DefinitionId>,
}

impl EmissionHierarchy {
    /// Gives ordinary modules first claim on their names, including external cells at stop boundaries.
    pub(super) fn new(root: &ModDef) -> Self {
        let mut hierarchy = Self::default();
        hierarchy.collect(root, true, &mut HashMap::new());
        let mut reserved = HashSet::new();
        let mut imported_stubs = Vec::new();
        let mut helpers = Vec::new();
        for id in &hierarchy.order {
            let interface = hierarchy.interfaces.get_mut(id).unwrap();
            if interface.mapped {
                helpers.push(*id);
                continue;
            }
            if interface.usage == Usage::EmitStubAndStop {
                // Local stubs declare plain wires at the elaborated widths.
                interface.enum_ports.clear();
                if interface.imported {
                    interface.parameters.clear();
                    imported_stubs.push(*id);
                    continue;
                }
            }
            assert!(
                reserved.insert(interface.name.clone()),
                "Two distinct modules with the same name: {}",
                interface.name
            );
        }
        // References beyond a stop boundary still need the library's original module name.
        let mut external_names = HashSet::new();
        for (id, interface) in &hierarchy.interfaces {
            if interface.primitive.is_none() && !hierarchy.bodies.contains_key(id) {
                assert!(
                    interface.parameters.is_empty() || !reserved.contains(&interface.name),
                    "External parameter overrides refer to {} but a local definition of that name declares no parameters",
                    interface.name
                );
                external_names.insert(interface.name.clone());
            }
        }
        reserved.extend(external_names);
        let mut suffixes = HashMap::new();
        for id in imported_stubs.into_iter().chain(helpers) {
            let interface = hierarchy.interfaces.get_mut(&id).unwrap();
            if !reserved.insert(interface.name.clone()) {
                let suffix = suffixes.entry(interface.name.clone()).or_insert(1usize);
                loop {
                    let name = format!("{}_{suffix}", interface.name);
                    *suffix += 1;
                    if reserved.insert(name.clone()) {
                        interface.name = name;
                        break;
                    }
                }
            }
        }
        hierarchy
    }

    /// Uses the first observed interface even if the same definition occurs along another path.
    fn capture_interface(&mut self, definition: &ModDef) -> &ModuleInterface {
        self.interfaces
            .entry(Arc::as_ptr(&definition.core))
            .or_insert_with(|| {
                let core = definition.core.read();
                ModuleInterface {
                    name: core.name.clone(),
                    ports: core.ports.clone(),
                    parameters: core.parameters.clone(),
                    enum_ports: core.enum_ports.clone(),
                    primitive: core.primitive.clone(),
                    mapped: core.primitives.mapped_body,
                    usage: core.usage.clone(),
                    imported: core.verilog_import.is_some(),
                }
            })
    }

    /// Visits captured children first. A later open path can descend beyond an earlier stop.
    fn collect(
        &mut self,
        definition: &ModDef,
        descend: bool,
        visited: &mut HashMap<DefinitionId, bool>,
    ) {
        let interface = self.capture_interface(definition);
        if interface.usage == Usage::EmitNothingAndStop
            || (interface.usage == Usage::EmitDefinitionAndStop && interface.imported)
        {
            return;
        }
        let descend = descend && interface.usage == Usage::EmitDefinitionAndDescend;
        let stub = interface.usage == Usage::EmitStubAndStop;
        let id = Arc::as_ptr(&definition.core);
        let first_visit = match visited.entry(id) {
            Entry::Occupied(mut entry) => {
                if *entry.get() || !descend {
                    return;
                }
                entry.insert(true);
                false
            }
            Entry::Vacant(entry) => {
                entry.insert(descend);
                true
            }
        };
        let body = self
            .bodies
            .entry(id)
            .or_insert_with(|| ModuleBody::capture(definition, stub));
        let children: Vec<_> = body
            .instances
            .values()
            .map(|(child, _)| child.clone())
            .collect();
        for child in children {
            let interface = self.capture_interface(&child);
            let mapped = interface.mapped;
            if interface.primitive.is_some() && !mapped {
                continue;
            }
            if descend || mapped {
                self.collect(&child, descend, visited);
            }
        }
        if first_visit {
            self.order.push(id);
        }
    }

    /// Looks up the exact declaration used by this reference, including external cells.
    pub(super) fn interface(&self, definition: &ModDef) -> &ModuleInterface {
        &self.interfaces[&Arc::as_ptr(&definition.core)]
    }

    /// Renders the captured bodies with the module names allocated for this hierarchy.
    pub(super) fn emit(mut self, options: &EmitOptions) -> String {
        let mut file = VastFile::new(VastFileType::SystemVerilog);
        for id in std::mem::take(&mut self.order) {
            let body = self.bodies.remove(&id).unwrap();
            body.emit(&self, &mut file, options);
        }
        file.emit()
    }
}
