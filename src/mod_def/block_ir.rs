// SPDX-License-Identifier: Apache-2.0

mod electrical;
mod external;

use external::{DefinitionId, ExternalBinding, behavioral_model, bind_externals};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use indexmap::IndexMap;
use xlsynth_pir::ir::{
    Block, BlockPort, FileTable, InstantiationKind, MemberType, NodePayload, Package,
    PackageMember, Type,
};
use xlsynth_pir::{BValue, BlockBuilder, IrBits, IrValue};

use super::ModuleOrigin;
use super::net_names::NetNames;
use crate::connection::resolve::ResolvedNet;
use crate::{ConnectedItem, IO, ModDef, Port, PortSliceConnection, Usage};

/// The one imported package and the block roots required by the generated hierarchy.
struct ImportedLinkage {
    package: Arc<Package>,
    roots: HashSet<String>,
}

impl ModDef {
    /// Exports a structural hierarchy as a verified XLS Block IR package.
    ///
    /// This method returns IR. To render it as SystemVerilog, pass the package and
    /// `BlockCodegenOptions` to `xlsynth_codegen::emit_system_verilog`.
    /// [`super::EmitOptions`] controls formatting on the direct SV path.
    ///
    /// The selected module is the package top. Port and instance names and packed bit
    /// positions are preserved. Ports are data bits, including one-bit signals named
    /// `clk`. The structural exporter does not infer registers or clock domains.
    /// Unclocked children imported from Block IR retain their native implementation. Clocked
    /// imported children, imported SV children, and stopped generated children become opaque PIR
    /// external blocks; link their separate SV definitions with the emitted wrapper. Analyses
    /// requiring leaf behavior reject these opaque boundaries. A clocked behavioral extern model
    /// is also opaque because its clock must remain an explicitly wired TopStitch connection.
    /// Native and external inouts use resolved electrical nets.
    /// Electrical slices and concatenations retain passive connectivity. Named public aliases
    /// and output pins requiring electrical readback cannot be exported faithfully yet.
    /// Evaluation and proof consumers must independently support electrical nets.
    /// Use [`ModDef::emit_sv_via_block_ir`] to include the requested SV stub definitions.
    /// Returns an error for an external root, incomplete wiring, unsupported inout connections, or
    /// names the XLS builder cannot represent. An explicitly named net has the width of
    /// the requested port slice. Bit zero corresponds to the slice's least significant bit.
    /// The default `xlsynth-codegen` SV backend preserves those explicit names. Other
    /// downstream emitters may rename the IR nodes when producing SystemVerilog.
    ///
    /// ```
    /// use topstitch::{IO, ModDef};
    ///
    /// let top = ModDef::new("UpperNibble");
    /// let byte = top.add_port("byte_in", IO::Input(8));
    /// byte.slice(3, 0).unused();
    /// top.add_port("nibble", IO::Output(4)).connect(&byte.slice(7, 4));
    /// let package = top.to_block_ir()?;
    /// assert_eq!(package.get_top_block().unwrap().name, "UpperNibble");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn to_block_ir(&self) -> Result<Package> {
        let mut leaves = IndexMap::new();
        self.build_block_ir_package(&mut leaves)
    }

    /// Emits SystemVerilog for this hierarchy with XLS codegen and includes one VAST
    /// declaration for each explicitly requested stub. Other externs are supplied by the caller.
    /// A stub at the root emits only its declaration.
    pub fn emit_sv_via_block_ir(&self) -> Result<String> {
        match self.get_usage() {
            Usage::EmitStubAndStop => {
                self.check_emission_interfaces(super::net_names::check_identifier)
                    .map_err(anyhow::Error::msg)?;
                return self.emit_sv_stub().map_err(anyhow::Error::msg);
            }
            Usage::EmitNothingAndStop if self.imported_block_ir_package().is_some() => {}
            Usage::EmitNothingAndStop | Usage::EmitDefinitionAndStop => return Ok(String::new()),
            Usage::EmitDefinitionAndDescend => {}
        }
        let mut leaves = IndexMap::new();
        let package = self.build_block_ir_package(&mut leaves)?;
        let mut stubs = IndexMap::new();
        for leaf in leaves
            .values()
            .filter(|leaf| leaf.get_usage() == Usage::EmitStubAndStop)
        {
            let name = leaf.get_name();
            ensure!(
                stubs.insert(name.clone(), leaf).is_none(),
                "two distinct stub definitions named {name}"
            );
        }
        for leaf in leaves.values() {
            let core = leaf.core.read();
            if let Some(stub) = stubs.get(&core.name)
                && !Arc::ptr_eq(&leaf.core, &stub.core)
            {
                let stub = stub.core.read();
                let stub_parameters = stub.sv_parameter_bindings().map_err(anyhow::Error::msg)?;
                let parameters = core.sv_parameter_bindings().map_err(anyhow::Error::msg)?;
                ensure!(
                    core.ports.len() == stub.ports.len()
                        && core.ports.iter().all(|(name, decl)| {
                            stub.ports
                                .get(name)
                                .is_some_and(|other| decl.io == other.io)
                        })
                        && parameters.iter().all(|(name, _)| {
                            stub_parameters
                                .iter()
                                .any(|(stub_name, _)| stub_name == name)
                        }),
                    "external instance of {} has an interface or parameter override incompatible with its emitted stub",
                    core.name
                );
            }
        }
        let mut sv = String::new();
        for stub in stubs.values() {
            sv.push_str(&stub.emit_sv_stub().map_err(anyhow::Error::msg)?);
        }
        let package = if package.members.iter().any(|member| {
            matches!(member, PackageMember::Block(block) if block.nodes.iter().any(|node| matches!(node.payload, NodePayload::Invoke { .. })))
        }) {
            xlsynth_codegen::inline_invokes_for_codegen(package, None)?
        } else {
            package
        };
        sv.push_str(
            &xlsynth_codegen::emit_system_verilog(&package, &Default::default())?.system_verilog,
        );
        Ok(sv)
    }

    /// Builds the package while retaining the external definitions encountered during traversal.
    fn build_block_ir_package(
        &self,
        leaves: &mut IndexMap<DefinitionId, ModDef>,
    ) -> Result<Package> {
        self.check_emission_interfaces(super::net_names::check_block_ir_identifier)
            .map_err(anyhow::Error::msg)?;
        let name = self.get_name();
        if let Some(source) = self.imported_block_ir_package() {
            let mut package = (*source).clone();
            package.name = name.clone();
            package.set_top_block(&name).map_err(anyhow::Error::msg)?;
            Self::check_rtl_module_collisions(&package, &name)?;
            return Ok(package);
        }
        ensure!(
            !self.is_external_leaf(),
            "{name} is external; export a parent that instantiates it"
        );
        let mut blocks = IndexMap::new();
        let mut imported = None;
        self.collect_definitions(&mut blocks, leaves, &mut imported, &mut HashSet::new())?;
        let mut package = match imported {
            Some(linkage) => Self::extract_imported_dependencies(linkage)?,
            None => Package {
                name: name.clone(),
                file_table: FileTable::new(),
                members: Vec::new(),
                top: None,
            },
        };
        package.name = name.clone();
        package.top = None;
        // BlockBuilder reserves names across both functions and blocks.
        for member in &package.members {
            let imported_name = &member.graph().name;
            ensure!(
                !blocks.contains_key(imported_name),
                "imported Block IR member {imported_name} conflicts with a generated module"
            );
        }
        let bindings = bind_externals(&blocks, leaves)?;
        for (name, module) in blocks {
            module
                .build_block(&mut package, &bindings)
                .with_context(|| format!("exporting module {name}"))?;
        }
        package.set_top_block(&name).map_err(anyhow::Error::msg)?;
        Self::check_rtl_module_collisions(&package, &name)?;
        Ok(package)
    }

    /// Returns the verified source package retained by an imported Block IR module.
    fn imported_block_ir_package(&self) -> Option<Arc<Package>> {
        match &self.core.read().origin {
            ModuleOrigin::BlockIr(package) => Some(Arc::clone(package)),
            _ => None,
        }
    }

    /// A stopped child has a separately supplied implementation at the export boundary.
    fn is_external_leaf(&self) -> bool {
        self.frozen() || self.get_usage() != Usage::EmitDefinitionAndDescend
    }

    /// Collects native blocks in dependency order and reserves their names before naming externs.
    fn collect_definitions(
        &self,
        blocks: &mut IndexMap<String, ModDef>,
        leaves: &mut IndexMap<DefinitionId, ModDef>,
        imported: &mut Option<ImportedLinkage>,
        visiting: &mut HashSet<String>,
    ) -> Result<()> {
        let name = self.get_name();
        if let Some(package) = self.imported_block_ir_package() {
            let block = package
                .get_block(&name)
                .with_context(|| format!("imported Block IR package has no block {name}"))?;
            if block.clock_port_name().is_some() {
                // Native PIR child clocks are implicit, while TopStitch permits arbitrary
                // clock wiring. Preserve the SV interface as an opaque external boundary.
                leaves
                    .entry(Arc::as_ptr(&self.core))
                    .or_insert_with(|| self.clone());
                return Ok(());
            }
            Self::link_imported_package(imported, package, &name)?;
            return Ok(());
        }
        if let Some(previous) = blocks.get(&name) {
            ensure!(
                Arc::ptr_eq(&previous.core, &self.core),
                "two distinct modules named {name}"
            );
            return Ok(());
        }
        ensure!(
            visiting.insert(name.clone()),
            "recursive module hierarchy at {name}"
        );
        for inst in self.get_instances() {
            let child = inst.get_mod_def();
            if child.imported_block_ir_package().is_some() || !child.is_external_leaf() {
                child
                    .collect_definitions(blocks, leaves, imported, visiting)
                    .with_context(|| format!("exporting {name}.{}", inst.name()))?;
            } else {
                let model = behavioral_model(&child.core.read()).cloned();
                if let Some(model) = model {
                    Self::link_imported_package(imported, model.package, &model.block)?;
                }
                leaves.entry(Arc::as_ptr(&child.core)).or_insert(child);
            }
        }
        visiting.remove(&name);
        blocks.insert(name, self.clone());
        Ok(())
    }

    /// Registers a root within the shared imported package.
    fn link_imported_package(
        imported: &mut Option<ImportedLinkage>,
        package: Arc<Package>,
        root: &str,
    ) -> Result<()> {
        if let Some(previous) = imported {
            ensure!(
                Arc::ptr_eq(&previous.package, &package),
                "cannot link distinct imported Block IR packages; import all blocks from one BlockIrLibrary"
            );
            previous.roots.insert(root.to_owned());
        } else {
            *imported = Some(ImportedLinkage {
                package,
                roots: HashSet::from([root.to_owned()]),
            });
        }
        Ok(())
    }

    /// Copies only source blocks and functions reachable from imported roots.
    fn extract_imported_dependencies(linkage: ImportedLinkage) -> Result<Package> {
        let members = linkage
            .package
            .members
            .iter()
            .map(|member| {
                let kind = match member {
                    PackageMember::Function(_) => MemberType::Function,
                    PackageMember::Block(_) => MemberType::Block,
                };
                ((member.graph().name.clone(), kind), member)
            })
            .collect::<HashMap<_, _>>();
        let mut pending = linkage
            .roots
            .into_iter()
            .map(|name| (name, MemberType::Block))
            .collect::<Vec<_>>();
        let mut reachable = HashSet::new();
        while let Some(key) = pending.pop() {
            if !reachable.insert(key.clone()) {
                continue;
            }
            let member = members
                .get(&key)
                .with_context(|| format!("missing imported Block IR dependency {}", key.0))?;
            if let PackageMember::Block(block) = member {
                for instance in &block.instantiations {
                    if let Some(child) = instance.referenced_block() {
                        pending.push((child.to_owned(), MemberType::Block));
                    } else if instance.kind == InstantiationKind::Extern {
                        pending.push((instance.block.clone(), MemberType::Function));
                    }
                }
            }
            pending.extend(
                xlsynth_pir::ir_utils::direct_function_references(member.graph())
                    .into_iter()
                    .map(|name| (name.to_owned(), MemberType::Function)),
            );
        }
        Ok(Package {
            name: linkage.package.name.clone(),
            file_table: linkage.package.file_table.clone(),
            members: linkage
                .package
                .members
                .iter()
                .filter(|member| {
                    let kind = match member {
                        PackageMember::Function(_) => MemberType::Function,
                        PackageMember::Block(_) => MemberType::Block,
                    };
                    reachable.contains(&(member.graph().name.clone(), kind))
                })
                .cloned()
                .collect(),
            top: None,
        })
    }

    /// Rejects external RTL references that would bind to an emitted native module.
    fn check_rtl_module_collisions(package: &Package, top: &str) -> Result<()> {
        let mut emitted = HashSet::new();
        let mut pending = vec![top.to_owned()];
        let mut external = HashSet::new();
        while let Some(name) = pending.pop() {
            if !emitted.insert(name.clone()) {
                continue;
            }
            let block = package
                .get_block(&name)
                .with_context(|| format!("missing emitted Block IR block {name}"))?;
            for instance in &block.instantiations {
                match &instance.kind {
                    InstantiationKind::Block => pending.push(instance.block.clone()),
                    kind if kind.is_external_block() => {
                        external.insert(instance.rtl_module_name().unwrap().to_owned());
                    }
                    _ => {}
                }
            }
        }
        for name in external {
            ensure!(
                !emitted.contains(&name),
                "external module {name} conflicts with an emitted Block IR block"
            );
        }
        Ok(())
    }

    /// Creates all source values before wiring sinks so instance order cannot affect connectivity.
    fn build_block(
        &self,
        package: &mut Package,
        bindings: &HashMap<DefinitionId, Rc<ExternalBinding>>,
    ) -> Result<()> {
        let mut builder = BlockBuilder::new(&self.get_name());
        let ports = self.get_ports(None);
        let mut sources = IndexMap::new();
        let mut sinks = Vec::new();
        let mut requests = Vec::new();
        let mut inouts = Vec::new();
        let has_names = !self.core.read().specified_net_names.is_empty();
        for port in &ports {
            match port.io() {
                IO::Input(width) => {
                    let value = builder.input_port(port.name(), Type::Bits(width))?;
                    sources.insert(port.clone(), value);
                }
                IO::Output(_) => sinks.push((port.clone(), None)),
                IO::InOut(_) => inouts.push((port.clone(), None)),
            }
        }
        for inst in self.get_instances() {
            let child = inst.get_mod_def();
            let external = bindings.get(&Arc::as_ptr(&child.core));
            let handle = if let Some(binding) = external {
                binding.instantiate(&mut builder, package, inst.name())?
            } else {
                builder.instantiate(inst.name(), package.get_block(&child.get_name()).unwrap())?
            };
            for port in inst.get_ports(None) {
                if has_names && let Some(connections) = port.get_port_connections() {
                    for connection in &*connections.read() {
                        if let ConnectedItem::Wire(wire) = &connection.other {
                            requests.push((connection.this.clone(), wire.name.clone()));
                        }
                    }
                }
                let ir_port = port.name();
                if matches!(port.io(), IO::InOut(_)) {
                    inouts.push((port, Some(handle)));
                } else if port.is_driver() {
                    let value = builder.instantiation_output(handle, ir_port)?;
                    sources.insert(port, value);
                } else {
                    let ir_name = ir_port.to_owned();
                    sinks.push((port, Some((handle, ir_name))));
                }
            }
        }

        let mut names = NetNames::new(&self.core.read()).map_err(anyhow::Error::msg)?;
        let named_inouts =
            electrical::emit(&mut builder, &mut names, &inouts, &requests, &mut sources)?;

        // Driver ports must also have complete wiring or explicit unused markers.
        for port in sources.keys().filter(|p| p.is_driver()) {
            port_values(port, |net| net.driving_value())?;
        }
        // Define aliases from physical drivers before resolving sinks through the aliases.
        let mut wires = HashMap::new();
        for (slice, name) in requests {
            if let Some(value) = named_inouts.get(&name) {
                wires.insert(name, *value);
                continue;
            }
            let source = sources.get(&slice.port);
            let value = match source {
                Some(source) => *source,
                None => build_value(
                    &mut builder,
                    port_values(&slice.port, |net| net.electrical_value())?,
                    &sources,
                    &wires,
                )?,
            };
            let named_value = if slice.width() != slice.port.io().width() {
                builder.bit_slice(value, slice.lsb, slice.width())?
            } else if source.is_none() {
                // Naming a whole receiver must preserve any name on its source.
                builder.identity(value)?
            } else {
                value
            };
            builder
                .set_name(named_value, &name)
                .with_context(|| format!("naming {} as {name}", slice.debug_string()))?;
            wires.insert(name, named_value);
        }
        for (port, instance) in sinks {
            let pieces = port_values(&port, |net| net.named_driving_value())?;
            let value = build_value(&mut builder, pieces, &sources, &wires)?;
            if let Some((handle, ir_name)) = instance {
                builder.instantiation_input(handle, &ir_name, value)?;
            } else {
                builder.output_port(port.name(), value)?;
            }
        }

        let mut block = builder.build_in_package(package)?;
        let core = self.core.read();
        reserve_wire_names(&mut block, names, &core.specified_net_names);
        // Source values must exist before output_port(), but positional callers see
        // the original port declaration order, including interleaved inputs/outputs.
        let mut order = std::mem::take(&mut block.ports);
        order.sort_by_key(|p| {
            let name = match p {
                BlockPort::Input(node) | BlockPort::Output(node) | BlockPort::Inout(node) => {
                    block.port_name(*node)
                }
                BlockPort::Clock(name) => name,
            };
            core.ports.get_index_of(name).unwrap()
        });
        block.ports = order;
        package.members.push(PackageMember::Block(block));
        Ok(())
    }
}

/// Resolves the port within its containing module, without crossing child boundaries.
fn port_values(
    port: &Port,
    select: impl Fn(ResolvedNet<'_>) -> Option<ConnectedItem>,
) -> Result<Vec<PortSliceConnection>> {
    let name = port.debug_string();
    let connections = port
        .get_port_connections()
        .with_context(|| format!("{name} is unconnected"))?;
    connections
        .read()
        .resolved_sources(&port.io(), &name, select)
        .map_err(anyhow::Error::msg)
}

/// Builds a sink value from whole or sliced driver ports, named wires and constants.
fn build_value(
    builder: &mut BlockBuilder,
    pieces: Vec<PortSliceConnection>,
    sources: &IndexMap<Port, BValue>,
    wires: &HashMap<String, BValue>,
) -> Result<BValue> {
    let mut parts = Vec::new();
    for piece in pieces {
        let (value, width, lsb) = match piece.other {
            ConnectedItem::PortSlice(slice) => {
                (sources[&slice.port], slice.port.io().width(), slice.lsb)
            }
            ConnectedItem::Wire(wire) => (wires[&wire.name], wire.width, wire.lsb),
            ConnectedItem::Tieoff(tie) => {
                let (_, mut bytes) = tie.value.to_bytes_le();
                bytes.resize(tie.width.div_ceil(8), 0);
                parts.push(
                    builder.literal(IrValue::Bits(IrBits::from_le_bytes(tie.width, &bytes)?))?,
                );
                continue;
            }
            ConnectedItem::Unused(_) => {
                bail!("{} requires a driving value", piece.this.debug_string())
            }
        };
        let part_width = piece.this.width();
        parts.push(if part_width == width {
            value
        } else {
            builder.bit_slice(value, lsb, part_width)?
        });
    }
    Ok(if parts.len() == 1 {
        parts[0]
    } else {
        builder.concat(&parts)?
    })
}

/// Keep incidental block2sv names from claiming explicitly requested names first.
/// The emitter can otherwise append a suffix, changing a physical constraint's target.
fn reserve_wire_names(block: &mut Block, mut names: NetNames, requested: &HashSet<String>) {
    // Leave a native output unnamed when codegen can connect it directly to the requested
    // electrical net; an incidental output name would prevent that physical pin binding.
    let electrical_outputs = block
        .graph
        .nodes
        .iter()
        .filter_map(|node| match &node.payload {
            NodePayload::ExtInoutDrive { net, drive, .. } if requested.contains(net) => {
                Some(drive.index)
            }
            _ => None,
        })
        .collect::<HashSet<_>>();
    for (index, node) in block.graph.nodes.iter_mut().enumerate() {
        if node.name.is_some()
            || (electrical_outputs.contains(&index)
                && matches!(node.payload, NodePayload::InstantiationOutput { .. }))
        {
            continue;
        }
        let base = match &node.payload {
            NodePayload::InstantiationOutput {
                instantiation,
                port_name,
            } => format!("{instantiation}_{}", port_name.replace('.', "_")),
            NodePayload::Literal(_)
            | NodePayload::BitSlice { .. }
            | NodePayload::Nary(xlsynth_pir::ir::NaryOp::Concat, _) => {
                format!("{}_{}", node.payload.get_operator(), node.text_id)
            }
            _ => continue,
        };
        node.name = names.reserve_block_ir_generated(&base);
    }
}
