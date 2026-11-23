// SPDX-License-Identifier: Apache-2.0

mod external;

use external::{DefinitionId, ExternalBinding, bind_externals};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use indexmap::IndexMap;
use xlsynth_pir::ir::{Block, BlockPort, FileTable, NodePayload, Package, PackageMember, Type};
use xlsynth_pir::{BValue, BlockBuilder, IrBits, IrValue};

use super::net_names::NetNames;
use crate::connection::resolve::ResolvedNet;
use crate::{ConnectedItem, IO, ModDef, Port, PortSliceConnection, Usage};

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
    /// Imported children and children whose [`Usage`] stops traversal become PIR extern
    /// instances. Link their SV definitions with the output of `block2sv`. The extern
    /// functions have placeholder bodies and do not model the behavior of those leaves.
    /// Proofs across these boundaries require a model or the linked RTL. Inout ports
    /// are not supported; use the direct SystemVerilog emitter for hierarchies with inouts.
    /// Use [`ModDef::emit_sv_via_block_ir`] to include the requested SV stub definitions.
    /// Returns an error for an external root, incomplete wiring, unsupported inout connections, or
    /// names the XLS builder cannot represent. An explicitly named net retains the width of
    /// its instance port and the original indices of the requested bits. For a partial
    /// naming request the IR aliases the whole port value. The bits outside the requested
    /// range therefore carry the corresponding port bits, while direct SV leaves them undriven.
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
                self.check_block_ir_ports()?;
                self.check_emission_interfaces(super::net_names::check_identifier)
                    .map_err(anyhow::Error::msg)?;
                return self.emit_sv_stub().map_err(anyhow::Error::msg);
            }
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
                ensure!(
                    core.ports.len() == stub.ports.len()
                        && core.ports.iter().all(|(name, decl)| {
                            stub.ports
                                .get(name)
                                .is_some_and(|other| decl.io == other.io)
                        })
                        && core
                            .parameters
                            .keys()
                            .all(|p| stub.parameters.contains_key(p)),
                    "external instance of {} has an interface or parameter override incompatible with its emitted stub",
                    core.name
                );
            }
        }
        let mut sv = String::new();
        for stub in stubs.values() {
            sv.push_str(&stub.emit_sv_stub().map_err(anyhow::Error::msg)?);
        }
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
        let mut package = Package {
            name: name.clone(),
            file_table: FileTable::new(),
            members: Vec::new(),
            top: None,
        };
        ensure!(
            !self.is_external_leaf(),
            "{name} is external; export a parent that instantiates it"
        );
        let mut blocks = IndexMap::new();
        self.collect_definitions(&mut blocks, leaves, &mut HashSet::new())?;
        let bindings = bind_externals(&mut package, &blocks, leaves)?;
        for (name, module) in blocks {
            module
                .build_block(&mut package, &bindings)
                .with_context(|| format!("exporting module {name}"))?;
        }
        package.set_top_block(&name).map_err(anyhow::Error::msg)?;
        Ok(package)
    }

    /// Rejects ports whose electrical semantics this exporter cannot represent.
    fn check_block_ir_ports(&self) -> Result<()> {
        let core = self.core.read();
        for (name, decl) in &core.ports {
            ensure!(
                !matches!(decl.io, IO::InOut(_)),
                "inout port {}.{name} is not supported by Block IR export; use the direct SystemVerilog emitter",
                core.name
            );
        }
        Ok(())
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
        visiting: &mut HashSet<String>,
    ) -> Result<()> {
        self.check_block_ir_ports()?;
        let name = self.get_name();
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
            if child.is_external_leaf() {
                child
                    .check_block_ir_ports()
                    .with_context(|| format!("exporting {name}.{}", inst.name()))?;
                leaves.entry(Arc::as_ptr(&child.core)).or_insert(child);
            } else {
                child
                    .collect_definitions(blocks, leaves, visiting)
                    .with_context(|| format!("exporting {name}.{}", inst.name()))?;
            }
        }
        visiting.remove(&name);
        blocks.insert(name, self.clone());
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
        let has_names = !self.core.read().specified_net_names.is_empty();
        for port in &ports {
            match port.io() {
                IO::Input(width) => {
                    let value = builder.input_port(port.name(), Type::Bits(width))?;
                    sources.insert(port.clone(), value);
                }
                IO::Output(_) => sinks.push((port.clone(), None)),
                IO::InOut(_) => unreachable!("inout ports are rejected before lowering"),
            }
        }
        for inst in self.get_instances() {
            let child = inst.get_mod_def();
            let external = bindings.get(&Arc::as_ptr(&child.core));
            let handle = if let Some(binding) = external {
                let PackageMember::Function(function) = &package.members[binding.function] else {
                    unreachable!()
                };
                builder.instantiate_extern(inst.name(), function)?
            } else {
                builder.instantiate(inst.name(), package.get_block(&child.get_name()).unwrap())?
            };
            for port in inst.get_ports(None) {
                if has_names && let Some(connections) = port.get_port_connections() {
                    for wire in connections.read().to_wires() {
                        requests.push((port.clone(), wire));
                    }
                }
                let ir_port = external.map_or(port.name(), |b| b.ports[port.name()].as_str());
                if port.is_driver() {
                    let value = builder.instantiation_output(handle, ir_port)?;
                    sources.insert(port, value);
                } else {
                    let ir_name = ir_port.to_owned();
                    sinks.push((port, Some((handle, ir_name))));
                }
            }
        }

        let names = NetNames::new(&self.core.read()).map_err(anyhow::Error::msg)?;
        // Driver ports must also have complete wiring or explicit unused markers.
        for port in sources.keys().filter(|p| p.is_driver()) {
            port_values(port, |net| net.driving_value())?;
        }
        // Define aliases from physical drivers before resolving sinks through the aliases.
        let mut wires = HashMap::new();
        for (port, wire) in requests {
            let value = if let Some(source) = sources.get(&port) {
                if wire.lsb == 0 && wire.msb + 1 == wire.width {
                    *source
                } else {
                    // Disjoint slices of one output may each request their own name.
                    builder.identity(*source)?
                }
            } else {
                let value = build_value(
                    &mut builder,
                    port_values(&port, |net| net.electrical_value())?,
                    &sources,
                    &wires,
                )?;
                // A receiver may be connected directly to an already named source.
                builder.identity(value)?
            };
            builder
                .set_name(value, &wire.name)
                .with_context(|| format!("naming {} as {}", port.debug_string(), wire.name))?;
            wires.insert(wire.name, value);
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
        reserve_wire_names(&mut block, names);
        // Source values must exist before output_port(), but positional callers see
        // the original port declaration order, including interleaved inputs/outputs.
        let mut order = std::mem::take(&mut block.ports);
        let core = self.core.read();
        order.sort_by_key(|p| {
            let name = match p {
                BlockPort::Input(node) | BlockPort::Output(node) => block.port_name(*node),
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
fn reserve_wire_names(block: &mut Block, mut names: NetNames) {
    for node in &mut block.graph.nodes {
        if node.name.is_some() {
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
