// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::rc::Rc;

use super::net_names::check_identifier;
use crate::{IO, ModDef, Usage};

impl ModDef {
    /// TODO(sherbst) 2025-10-27: Remove this.
    pub fn validate(&self) {}

    /// Checks emitted identifiers and the port declarations of references to the same external binding.
    pub(super) fn check_emission_interfaces(
        &self,
        check_native_name: fn(&str, &str, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut pending = vec![(self.clone(), None)];
        let mut visited = HashSet::new();
        let mut external_ports = HashMap::new();
        while let Some((module, reference)) = pending.pop() {
            if !visited.insert(std::sync::Arc::as_ptr(&module.core)) {
                continue;
            }
            let core = module.core.read();
            // External interfaces appear as SV text in the FFI template or stub.
            let check_name =
                if core.usage == Usage::EmitDefinitionAndDescend && core.verilog_import.is_none() {
                    check_native_name
                } else {
                    check_identifier
                };
            check_name(&core.name, "module name", &core.name)?;
            for port in core.ports.keys() {
                check_name(port, "port name", &core.name)?;
            }
            for param in core.parameters.keys() {
                check_identifier(param, "parameter name", &core.name)?;
            }
            for ty in core.enum_ports.values() {
                for part in ty.split("::") {
                    check_identifier(part, "port type", &core.name)?;
                }
            }
            if core.usage == Usage::EmitDefinitionAndDescend {
                for wire in &core.specified_net_names {
                    check_name(wire, "net name", &core.name)?;
                }
                for (name, child) in &core.instances {
                    check_name(name, "instance name", &core.name)?;
                    pending.push((
                        ModDef {
                            core: child.clone(),
                        },
                        Some(format!("{}.{name}", core.name)),
                    ));
                }
            } else if let Some(reference) = reference {
                let ports: &mut HashMap<String, (IO, Rc<str>)> = external_ports
                    .entry((core.name.clone(), core.sv_parameter_bindings()?))
                    .or_default();
                let reference = Rc::<str>::from(reference);
                // A view may omit ports. Every shared pin must describe the same SV interface.
                for (name, io) in &core.ports {
                    match ports.entry(name.clone()) {
                        Entry::Occupied(entry) => {
                            let (previous, previous_reference) = entry.get();
                            if previous != io {
                                return Err(format!(
                                    "conflicting declarations of external port {}.{name} with the same parameter overrides: {previous_reference} declares {previous:?}, {reference} declares {io:?}",
                                    core.name
                                ));
                            }
                        }
                        Entry::Vacant(entry) => {
                            entry.insert((io.clone(), reference.clone()));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
