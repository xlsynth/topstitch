// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, bail, ensure};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use xlsynth_pir::ir::{
    Block, BlockPort, Instantiation, InstantiationKind, NodePayload, NodeRef, OpaqueExternPort,
    OpaqueExternPortKind, Package, PackageMember, Type,
};
use xlsynth_pir::ir_parser::Parser;

use super::core::{BlockIrModel, ExternalModule};
use super::{ModDef, ModuleOrigin};
use crate::port::PortType;
use crate::{IO, Usage};

/// Options for importing modules from an XLS Block IR package.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockIrParserConfig<'a> {
    /// Package-form XLS IR.
    pub ir: &'a str,
    /// Include name-only nested native blocks and reusable external module instances.
    pub include_hierarchy: bool,
}

/// An extern instantiation declared in an XLS block hierarchy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockIrExternCall<'a> {
    /// Dot-separated instance path starting with the queried block.
    pub path: String,
    /// Name of the referenced IR function.
    pub function: &'a str,
    /// Decoded template bytes, if the function supplies an FFI code template.
    pub sv_template: Option<&'a [u8]>,
}

/// One verified XLS package shared by module and hierarchy queries.
pub struct BlockIrLibrary {
    package: Arc<Package>,
    blocks: IndexMap<String, usize>,
    include_hierarchy: bool,
}

impl BlockIrLibrary {
    /// Parses and verifies a package once with the supplied import options.
    pub fn from_config(config: &BlockIrParserConfig<'_>) -> Result<Self> {
        let package = Parser::new(config.ir)
            .parse_and_verify_package()
            .context("could not import XLS Block IR package")?;
        let blocks = package
            .members
            .iter()
            .enumerate()
            .filter_map(|(index, member)| match member {
                PackageMember::Block(block) => Some((block.name.clone(), index)),
                _ => None,
            })
            .collect();
        Ok(Self {
            package: Arc::new(package),
            blocks,
            include_hierarchy: config.include_hierarchy,
        })
    }

    /// Parses a package with hierarchy extraction disabled.
    pub fn from_ir(ir: &str) -> Result<Self> {
        Self::from_config(&BlockIrParserConfig {
            ir,
            ..Default::default()
        })
    }

    /// Lists all blocks in package declaration order.
    pub fn module_names(&self) -> Vec<String> {
        self.blocks.keys().cloned().collect()
    }

    /// Imports the named block's interface and optional instance hierarchy.
    ///
    /// Nested native blocks contain names only, like SV hierarchy import. External module
    /// instances with supported interfaces retain their reusable ports and parameter bindings;
    /// unsupported interfaces remain frozen, name-only references for hierarchy inspection.
    /// Use [`Self::extern_module`] to import an external instance directly, or
    /// [`Self::extern_calls`] to inspect references to foreign functions.
    pub fn module(&self, name: &str) -> Result<ModDef> {
        self.import_module(name, &mut HashMap::new())
    }

    /// Imports the external module specialization declared by an instance of a package block.
    ///
    /// External interfaces and parameter overrides belong to an instance, so two instances of
    /// the same RTL module may return distinct definitions. This works regardless of
    /// `include_hierarchy`. Native blocks should be imported through [`Self::module`]; legacy
    /// foreign-function calls have no module interface and remain available through
    /// [`Self::extern_calls`].
    pub fn extern_module(&self, parent_block: &str, instance_name: &str) -> Result<ModDef> {
        let block = self.block(parent_block)?;
        let instance = block
            .instantiations
            .iter()
            .find(|instance| instance.name == instance_name)
            .with_context(|| {
                format!("no instance '{instance_name}' in XLS block '{parent_block}'")
            })?;
        self.import_extern_instance(instance)
            .with_context(|| format!("importing external instance {parent_block}.{instance_name}"))
    }

    /// Imports every block in declaration order, with a separate interface for each root.
    /// Within this result, occurrences of the same nested block share a definition,
    /// including changes to its definition-level metadata.
    pub fn modules(&self) -> Result<Vec<ModDef>> {
        let mut definitions = HashMap::new();
        self.blocks
            .keys()
            .map(|name| self.import_module(name, &mut definitions))
            .collect()
    }

    /// Reports foreign-function extern calls under the named block in depth-first declaration
    /// order. External module instances and their behavioral models are not function calls.
    /// This query works with either setting of `include_hierarchy`.
    pub fn extern_calls(&self, name: &str) -> Result<Vec<BlockIrExternCall<'_>>> {
        let block = self.block(name)?;
        let mut templates = HashMap::new();
        for member in &self.package.members {
            if let PackageMember::Function(function) = member
                && let Some(template) = function
                    .ffi_proto()
                    .and_then(|ffi| ffi.code_template.as_deref())
            {
                templates.insert(function.name.as_str(), template);
            }
        }
        let mut calls = Vec::new();
        let mut path = vec![block.name.as_str()];
        self.collect_extern_calls(
            block,
            &templates,
            &mut path,
            &mut HashSet::new(),
            &mut calls,
        )?;
        Ok(calls)
    }

    /// Imports a root interface using the supplied cache for name-only descendants.
    fn import_module(
        &self,
        name: &str,
        definitions: &mut HashMap<String, ModDef>,
    ) -> Result<ModDef> {
        let block = self.block(name)?;
        let module = ModDef::import_block_interface(block)?;
        if self.include_hierarchy {
            self.populate_hierarchy(&module, block, definitions)?;
        }
        freeze_import(&module, &self.package);
        Ok(module)
    }

    /// Looks up a block by its package member index.
    fn block(&self, name: &str) -> Result<&Block> {
        let index = self.blocks.get(name).with_context(|| {
            format!(
                "no block named '{name}' in XLS package '{}'",
                self.package.name
            )
        })?;
        let PackageMember::Block(block) = &self.package.members[*index] else {
            unreachable!()
        };
        Ok(block)
    }

    /// Copies instance names and references while sharing repeated block definitions.
    fn populate_hierarchy(
        &self,
        parent: &ModDef,
        block: &Block,
        definitions: &mut HashMap<String, ModDef>,
    ) -> Result<()> {
        for inst in &block.instantiations {
            match &inst.kind {
                InstantiationKind::Extern => continue,
                InstantiationKind::BehavioralExternBlock { .. }
                | InstantiationKind::OpaqueExternBlock { .. } => {
                    match self.import_extern_instance(inst) {
                        Ok(definition) => {
                            parent.instantiate(&definition, Some(&inst.name), None);
                        }
                        Err(error) => {
                            let module_name = match &inst.kind {
                                InstantiationKind::BehavioralExternBlock {
                                    module_name, ..
                                } => module_name,
                                InstantiationKind::OpaqueExternBlock { .. } => &inst.block,
                                _ => unreachable!(),
                            };
                            let definition = ModDef::new(module_name);
                            parent.instantiate(&definition, Some(&inst.name), None);
                            let mut core = definition.core.write();
                            core.origin = ModuleOrigin::HierarchyOnly(format!("{error:#}"));
                            core.usage = Usage::EmitNothingAndStop;
                        }
                    }
                    continue;
                }
                InstantiationKind::Block => {}
            }
            let child = if let Some(definition) = definitions.get(&inst.block) {
                definition
            } else {
                let definition = ModDef::new(&inst.block);
                self.populate_hierarchy(&definition, self.block(&inst.block)?, definitions)?;
                freeze_import(&definition, &self.package);
                definitions.entry(inst.block.clone()).or_insert(definition)
            };
            parent.instantiate(child, Some(&inst.name), None);
        }
        Ok(())
    }

    /// Projects one concrete external instance into a frozen, wireable module definition.
    fn import_extern_instance(&self, instance: &Instantiation) -> Result<ModDef> {
        let (module, parameters, input_casts, model) = match &instance.kind {
            InstantiationKind::OpaqueExternBlock {
                ports,
                parameters,
                input_casts,
            } => (
                ModDef::import_opaque_interface(&instance.block, ports)?,
                parameters,
                input_casts,
                None,
            ),
            InstantiationKind::BehavioralExternBlock {
                module_name,
                parameters,
                input_casts,
            } => (
                ModDef::import_block_interface_as(self.block(&instance.block)?, module_name)?,
                parameters,
                input_casts,
                Some(BlockIrModel {
                    package: Arc::clone(&self.package),
                    block: instance.block.clone(),
                }),
            ),
            InstantiationKind::Block => bail!(
                "instance '{}' references native block '{}'; use module({:?})",
                instance.name,
                instance.block,
                instance.block
            ),
            InstantiationKind::Extern => bail!(
                "instance '{}' is a foreign-function extern without a module interface; use extern_calls()",
                instance.name
            ),
        };
        for name in parameters.keys() {
            check_sv_name(name, "parameter name", &module.get_name())
                .with_context(|| format!("cannot bind parameter {}.{name}", module.get_name()))?;
        }
        let mut core = module.core.write();
        for (name, sv_type) in input_casts {
            let decl = core.ports.get_mut(name).with_context(|| {
                format!(
                    "external instance {} casts unknown port {name}",
                    instance.name
                )
            })?;
            ensure!(
                matches!(decl.io, IO::Input(_)),
                "external instance {} casts non-input port {name}",
                instance.name
            );
            decl.ty = PortType::EnumInputCast(sv_type.clone());
        }
        core.origin = ModuleOrigin::External(ExternalModule {
            sv_parameters: parameters.clone(),
            model,
        });
        core.usage = Usage::EmitNothingAndStop;
        drop(core);
        Ok(module)
    }

    /// Walks each call's instance path, skipping known call-free subtrees on later visits.
    fn collect_extern_calls<'a>(
        &'a self,
        block: &'a Block,
        templates: &HashMap<&str, &'a [u8]>,
        path: &mut Vec<&'a str>,
        extern_free: &mut HashSet<&'a str>,
        calls: &mut Vec<BlockIrExternCall<'a>>,
    ) -> Result<()> {
        if extern_free.contains(block.name.as_str()) {
            return Ok(());
        }
        let start = calls.len();
        for inst in &block.instantiations {
            path.push(&inst.name);
            match &inst.kind {
                InstantiationKind::Block => {
                    self.collect_extern_calls(
                        self.block(&inst.block)?,
                        templates,
                        path,
                        extern_free,
                        calls,
                    )?;
                }
                InstantiationKind::Extern => {
                    calls.push(BlockIrExternCall {
                        path: path.join("."),
                        function: &inst.block,
                        sv_template: templates.get(inst.block.as_str()).copied(),
                    });
                }
                InstantiationKind::BehavioralExternBlock { .. }
                | InstantiationKind::OpaqueExternBlock { .. } => {}
            }
            path.pop();
        }
        if calls.len() == start {
            extern_free.insert(&block.name);
        }
        Ok(())
    }
}

impl ModDef {
    /// Imports a named XLS block as an external module from package-form Block IR.
    ///
    /// The block and port names must be simple identifiers matching the separately supplied SystemVerilog
    /// implementation. The parent emits ordinary instances of that module.
    /// A clock becomes a one-bit input. Data ports must be nonempty bit vectors.
    /// Input and output SV type overrides are unsupported; an inout override is
    /// retained in the source package and may be wired through a bit-vector net.
    /// Ports retain their declaration order.
    ///
    /// Requires the `block-ir` Cargo feature.
    pub fn from_block_ir(name: &str, ir: &str) -> Result<Self> {
        BlockIrLibrary::from_ir(ir)?.module(name)
    }

    /// Creates an unfrozen module with the block's directly wireable interface.
    fn import_block_interface(block: &Block) -> Result<Self> {
        Self::import_block_interface_as(block, &block.name)
    }

    /// Projects a block interface onto the module name used by its RTL implementation.
    fn import_block_interface_as(block: &Block, name: &str) -> Result<Self> {
        check_sv_name(name, "module name", name)
            .with_context(|| format!("cannot instantiate XLS block '{name}'"))?;
        let module = Self::new(name);
        for port in &block.ports {
            let (port_name, io) = match port {
                BlockPort::Clock(clock) => (clock.as_str(), IO::Input(1)),
                BlockPort::Input(n) => (block.port_name(*n), IO::Input(port_width(block, *n)?)),
                BlockPort::Output(n) => (block.port_name(*n), IO::Output(port_width(block, *n)?)),
                BlockPort::Inout(n) => (block.port_name(*n), IO::InOut(port_width(block, *n)?)),
            };
            check_sv_name(port_name, "port name", name)
                .with_context(|| format!("cannot connect XLS port '{name}.{port_name}'"))?;
            module.add_port(port_name, io);
        }

        Ok(module)
    }

    /// Creates an external module from its inline, declaration-ordered opaque interface.
    fn import_opaque_interface(name: &str, ports: &[OpaqueExternPort]) -> Result<Self> {
        check_sv_name(name, "module name", name)
            .with_context(|| format!("cannot instantiate external module '{name}'"))?;
        let module = Self::new(name);
        for port in ports {
            check_sv_name(&port.name, "port name", name)
                .with_context(|| format!("cannot connect external port '{name}.{}'", port.name))?;
            let io = match &port.kind {
                OpaqueExternPortKind::Clock => IO::Input(1),
                OpaqueExternPortKind::Input(ty) => IO::Input(bits_width(ty, name, &port.name)?),
                OpaqueExternPortKind::Output(ty) => IO::Output(bits_width(ty, name, &port.name)?),
                OpaqueExternPortKind::Inout(ty) => IO::InOut(bits_width(ty, name, &port.name)?),
            };
            module.add_port(&port.name, io);
        }
        Ok(module)
    }
}

/// Marks a definition as an externally supplied, immutable module.
fn freeze_import(module: &ModDef, package: &Arc<Package>) {
    let mut core = module.core.write();
    core.origin = ModuleOrigin::BlockIr(Arc::clone(package));
    core.usage = Usage::EmitNothingAndStop;
}

/// Returns the vector width that can be wired directly in the SV wrapper.
fn port_width(block: &Block, node: NodeRef) -> Result<usize> {
    let name = block.port_name(node);
    let ty = block.port_type(node);
    let width = bits_width(ty, &block.name, name)?;
    if let NodePayload::InputPort {
        sv_type: Some(ty), ..
    }
    | NodePayload::OutputPort {
        sv_type: Some(ty), ..
    } = &block.get_node(node).payload
    {
        bail!("{}.{name}: unsupported SV type override '{ty}'", block.name);
    }
    Ok(width)
}

/// Returns the nonzero bit-vector width of a directly wireable external port.
fn bits_width(ty: &Type, module: &str, port: &str) -> Result<usize> {
    let Type::Bits(width) = ty else {
        bail!("{module}.{port}: unsupported XLS port type {ty}; expected bits[N]");
    };
    ensure!(*width > 0, "{module}.{port}: zero-width XLS port");
    Ok(*width)
}

/// TopStitch references external identifiers verbatim when emitting instances.
fn check_sv_name(name: &str, role: &str, module: &str) -> Result<()> {
    super::net_names::check_identifier(name, role, module)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("'{name}' is not a simple SystemVerilog identifier"))
}
