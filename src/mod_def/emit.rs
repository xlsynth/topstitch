// SPDX-License-Identifier: Apache-2.0

mod module_names;
mod primitive;
#[cfg(test)]
mod tests;

use module_names::{EmissionHierarchy, ModuleBody};

use std::collections::{HashMap, HashSet};
use std::path::Path;

use indexmap::IndexMap;
use indexmap::map::Entry;
use xlsynth_vast::{Expr, LiteralFormat, LogicRef, VastFile, VastFileType, VastModule};

use crate::connection::PortSliceConnections;
use crate::connection::connected_item::{ConnectedItem, Tieoff};
use crate::connection::expression_source::merge_expression_sources;
use crate::connection::validate::check_for_gaps;
use crate::{IO, ModDef, Port, PortSlice, Usage};

#[derive(Debug, PartialEq)]
enum NetNameSource {
    ManuallySpecified(String),
    ModDefPort(String),
    ModInstPort((String, String)),
}

use std::fmt;

/// Options controlling Verilog emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitOptions {
    /// Validate the module definition before emitting Verilog.
    pub validate: bool,
    /// Emit each bit of a continuous assignment as a separate assignment.
    pub bitblast_assignments: bool,
    /// Preserve a single-bit slice such as `x[2:2]` instead of simplifying it to `x[2]`.
    pub preserve_single_bit_slices: bool,
    /// Preserve a full-width slice such as `x[3:0]` instead of simplifying it to `x`.
    pub preserve_full_width_slices: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            validate: true,
            bitblast_assignments: false,
            preserve_single_bit_slices: false,
            preserve_full_width_slices: false,
        }
    }
}

impl fmt::Display for NetNameSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetNameSource::ManuallySpecified(name) => write!(f, "{name}"),
            NetNameSource::ModDefPort(name) => write!(f, "{name}"),
            NetNameSource::ModInstPort((inst_name, port_name)) => {
                write!(f, "{}_{}", inst_name, port_name)
            }
        }
    }
}

struct NetCollection {
    sources: IndexMap<String, NetNameSource>,
    logic_refs: IndexMap<String, LogicRef>,
    reserved: HashSet<String>,
    next_suffix: HashMap<String, usize>,
    physical_names: HashMap<String, String>,
    primitive_wires: HashMap<(String, String), String>,
    inline_values: HashMap<(String, String), Expr>,
    /// Maximum expression depth arriving at each consumer from its inlined inputs.
    inline_input_depths: HashMap<String, usize>,
}

impl NetCollection {
    pub fn new() -> Self {
        Self {
            sources: IndexMap::new(),
            logic_refs: IndexMap::new(),
            reserved: HashSet::new(),
            next_suffix: HashMap::new(),
            physical_names: HashMap::new(),
            primitive_wires: HashMap::new(),
            inline_values: HashMap::new(),
            inline_input_depths: HashMap::new(),
        }
    }

    pub fn declare_mod_def_port(
        &mut self,
        name: &str,
        io: &IO,
        file: &mut VastFile,
        module: &mut VastModule,
    ) {
        // Declare the port in the sources map.
        let name_as_string = name.to_string();
        if let Some(existing_source) = self.sources.insert(
            name_as_string.clone(),
            NetNameSource::ModDefPort(name.to_string()),
        ) {
            panic!(
                "Error while declaring ModDef port {name} in a NetCollection: a net name source for this port has already been declared ({existing_source:?})"
            );
        }

        let width = match io {
            IO::Input(width) | IO::Output(width) | IO::InOut(width) => *width,
        };
        let data_type = file.make_bit_vector_type(width as i64, false);
        let logic_ref = match io {
            IO::Input(_) => file.add_input(*module, name, &data_type),
            IO::Output(_) => file.add_output(*module, name, &data_type),
            IO::InOut(_) => file.add_inout(*module, name, &data_type),
        };

        if self.logic_refs.insert(name_as_string, logic_ref).is_some() {
            panic!(
                "NetCollection out of sync: net \"{name}\" is declared in the refs map but not the sources map."
            );
        }
    }

    pub fn get_logic_ref_create_if_necessary(
        &mut self,
        source: NetNameSource,
        width: usize,
        file: &mut VastFile,
        module: &mut VastModule,
    ) -> &LogicRef {
        let name = match &source {
            NetNameSource::ModInstPort(key) if self.primitive_wires.contains_key(key) => {
                self.primitive_wires[key].clone()
            }
            NetNameSource::ModInstPort((key, port)) if self.physical_names.contains_key(key) => {
                format!("{}_{port}", self.physical_names[key])
            }
            _ => source.to_string(),
        };
        match self.sources.entry(name) {
            Entry::Occupied(source_entry) => {
                let existing_ref = self.logic_refs.get(source_entry.key());
                let existing_source = source_entry.get();
                if existing_source == &source {
                    existing_ref.unwrap()
                } else {
                    panic!(
                        "Net name collision for {source:?} and {:?}: both resolve to \"{}\". If you have used specify_net_name() on one of the ports involved, you may need to remove it or change the net name. If not, you may need to use specify_net_name() to override the net name for one of the ModInst ports involved.",
                        existing_source,
                        &source_entry.key()
                    );
                }
            }
            Entry::Vacant(source_entry) => {
                let net_name = source_entry.key().clone();
                let data_type = file.make_bit_vector_type(width as i64, false);
                let wire = match source {
                    NetNameSource::ManuallySpecified(_) | NetNameSource::ModInstPort(_) => {
                        file.add_wire(*module, &net_name, &data_type)
                    }
                    NetNameSource::ModDefPort(_) => {
                        panic!(
                            "ModDef ports should be added to NetCollection using declare_mod_def_port()"
                        )
                    }
                };
                source_entry.insert(source);
                match self.logic_refs.entry(net_name.clone()) {
                    Entry::Vacant(ref_entry) => ref_entry.insert(wire),
                    Entry::Occupied(_) => {
                        panic!(
                            "NetCollection out of sync: net \"{}\" is declared in the refs map but not the sources map.",
                            net_name
                        );
                    }
                }
            }
        }
    }
}

impl ModDef {
    /// Writes Verilog code for this module definition to the given file path.
    pub fn emit_to_file(&self, path: &Path, options: EmitOptions) {
        let err_msg = format!("emitting ModDef to file at path: {path:?}");
        std::fs::write(path, self.emit(options)).expect(&err_msg);
    }

    /// Returns Verilog for this module. Use `wrap()` to emit a primitive as its own module.
    ///
    /// Each definition's instances and interface are captured once per emission. An instance
    /// added after its parent is captured appears on the next call. Finish wiring connections
    /// before emitting a design that another thread is building.
    pub fn emit(&self, options: EmitOptions) -> String {
        assert!(
            self.primitive_kind().is_none(),
            "Call wrap() before emitting a primitive as a module."
        );
        if options.validate {
            self.validate();
        }
        EmissionHierarchy::new(self).emit(&options)
    }
}

impl ModuleBody {
    /// Emits the instances and port bindings selected during hierarchy collection.
    fn emit(self, hierarchy: &EmissionHierarchy, file: &mut VastFile, options: &EmitOptions) {
        let interface = hierarchy.interface(&self.definition);
        let mut module = file.add_module(&interface.name);
        let mut nets = self.nets;
        for (port_name, io) in &interface.ports {
            nets.declare_mod_def_port(port_name, io, file, &mut module);
        }

        if interface.usage == Usage::EmitStubAndStop {
            return;
        }

        // Emit behavioral operations in construction order before their structural consumers.
        let inline_ops = &self.inline_ops;
        let ops = inline_ops.iter().map(|name| (name, &self.instances[name]));
        let children = self
            .instances
            .iter()
            .filter(|(name, _)| !inline_ops.contains(*name));
        let mut inout_tieoffs = Vec::new();
        for (inst_name, (inst, connections)) in ops.chain(children) {
            let def = hierarchy.interface(inst);
            let connections = match &def.primitive {
                Some(kind) => kind.connections_for_emission(
                    &self.definition,
                    inst_name,
                    &def.ports,
                    connections,
                ),
                None => std::borrow::Cow::Borrowed(connections),
            };
            let pins = def.ports.iter().map(|(name, io)| {
                let pin = connections.get(name).unwrap_or_else(|| {
                    panic!("{}.{}.{} is unconnected", interface.name, inst_name, name)
                });
                let sources = port_sources(
                    pin,
                    io,
                    &format!("{}.{}.{}", interface.name, inst_name, name),
                    &mut inout_tieoffs,
                );
                (name.as_str(), io, sources)
            });
            if inline_ops.contains(inst_name) {
                let kind = def.primitive.as_ref().unwrap();
                kind.emit_inline(
                    inst_name,
                    inline_ops,
                    def.ports[kind.output()].width(),
                    pins,
                    file,
                    &mut module,
                    &mut nets,
                    options,
                );
                continue;
            }

            let mut parameter_port_names: Vec<String> = Vec::new();
            let mut parameter_expr_vals: Vec<Expr> = Vec::new();
            let mut connection_port_names = Vec::new();
            let mut connection_expressions = Vec::new();
            for (name, _, sources) in pins {
                connection_port_names.push(name);
                if let [source] = sources.as_slice()
                    && matches!(source.other, ConnectedItem::Unused(_))
                {
                    connection_expressions.push(None);
                    continue;
                }
                let value = port_expression(sources, file, &mut module, &mut nets, options);
                connection_expressions.push(value.map(|expr| match def.enum_ports.get(name) {
                    Some(t) => {
                        let ty = file.make_extern_type(t);
                        file.make_type_cast(&ty, &expr)
                    }
                    None => expr,
                }));
            }

            // Build parameter override expressions, if any
            if !def.parameters.is_empty() {
                for (param_name, spec) in def.parameters.iter() {
                    parameter_port_names.push(param_name.clone());
                    let expr = match spec {
                        crate::mod_def::ParameterSpec::Integer { value, ty } => {
                            if value.sign() == num_bigint::Sign::Minus {
                                // TODO(sherbst) 2025-10-29: Support negative parameter values
                                panic!("Negative parameter values not yet supported");
                            }
                            let literal_str = format!("bits[{}]:{}", ty.width(), value);
                            file.make_literal(&literal_str, &LiteralFormat::Hex)
                                .unwrap()
                        }
                        crate::mod_def::ParameterSpec::String(value) => {
                            file.make_string_literal(value)
                        }
                    };
                    parameter_expr_vals.push(expr);
                }
            }

            let parameter_expressions: Vec<&Expr> = parameter_expr_vals.iter().collect();
            let parameter_port_name_refs: Vec<&str> =
                parameter_port_names.iter().map(|s| s.as_str()).collect();
            let instantiation = file.make_instantiation(
                &def.name,
                nets.physical_names.get(inst_name).unwrap_or(inst_name),
                &parameter_port_name_refs,
                &parameter_expressions,
                &connection_port_names,
                &connection_expressions
                    .iter()
                    .map(|o| o.as_ref())
                    .collect::<Vec<_>>(),
            );
            file.add_member_instantiation(module, instantiation);
        }

        // Emit assign statements for ModDef ports if necessary
        for port_name in interface.ports.keys() {
            let port_slice_connections = match self.port_connections.get(port_name) {
                Some(port_slice_connections) => port_slice_connections,
                None => panic!("{}.{} is unconnected", interface.name, port_name),
            };

            let merged = port_sources(
                port_slice_connections,
                &interface.ports[port_name],
                &format!("{}.{}", interface.name, port_name),
                &mut inout_tieoffs,
            );

            for expression_source in merged {
                let other_width = match &expression_source.other {
                    ConnectedItem::PortSlice(port_slice) => {
                        if let Port::ModDef {
                            name: port_slice_port_name,
                            ..
                        } = &port_slice.port
                            && port_slice_port_name == port_name
                        {
                            // If the expression source for this port is the
                            // port itself, skip it.
                            continue;
                        }
                        port_slice.width()
                    }
                    ConnectedItem::Tieoff(tieoff) => tieoff.width,
                    ConnectedItem::Unused(_) => {
                        continue;
                    }
                    ConnectedItem::Wire(wire) => wire.msb - wire.lsb + 1,
                };

                let width = expression_source.this.width();

                // Connection construction and slicing should have caught any
                // width mismatch before emission, but double-check here.
                assert_eq!(
                    other_width,
                    width,
                    "Width mismatch while emitting an assignment to {}",
                    expression_source.this.debug_string(),
                );

                let port = ConnectedItem::PortSlice(expression_source.this.clone());
                let other = &expression_source.other;
                // A named wire on an inout reads the resolved port, including external drives.
                let (lhs, rhs) = if matches!(interface.ports[port_name], IO::InOut(_))
                    && matches!(other, ConnectedItem::Wire(_))
                {
                    (other, &port)
                } else {
                    (&port, other)
                };
                emit_net_assignment(
                    &expression_source.this,
                    lhs,
                    rhs,
                    file,
                    &mut module,
                    &mut nets,
                    options,
                );
            }
        }

        for (net, tieoff) in inout_tieoffs {
            emit_net_assignment(
                &net.this,
                &net.other,
                &ConnectedItem::Tieoff(tieoff),
                file,
                &mut module,
                &mut nets,
                options,
            );
        }

        // Ports, wires and structural instances share one SystemVerilog namespace.
        for name in self
            .instances
            .keys()
            .filter(|name| !inline_ops.contains(*name))
        {
            let emitted = nets.physical_names.get(name).unwrap_or(name);
            if let Some(source) = nets.sources.get(emitted) {
                panic!(
                    "SV name collision in {}: instance {emitted} and {source:?} use the same identifier",
                    interface.name
                );
            }
        }
    }
}

/// Resolves a port's connections to non-overlapping expression sources in MSB-first order.
fn port_sources(
    connections: &parking_lot::RwLock<PortSliceConnections>,
    io: &IO,
    name: &str,
    inout_tieoffs: &mut Vec<(crate::PortSliceConnection, Tieoff)>,
) -> Vec<crate::PortSliceConnection> {
    // Tracing can revisit this pin through another connection.
    let connections = connections.read().clone();
    let mut chunks = connections.trace().make_non_overlapping();
    chunks.retain(|chunk| !chunk.is_empty());
    chunks.sort_by_key(|chunk| std::cmp::Reverse(chunk[0].this.msb));
    check_for_gaps(&chunks, io, name);
    merge_expression_sources(
        chunks
            .iter()
            .map(|chunk| chunk.resolve_expression_source(inout_tieoffs).unwrap())
            .collect(),
    )
}

/// Drives a resolved net or module port, applying the same slicing to both sides.
fn emit_net_assignment(
    this: &PortSlice,
    lhs: &ConnectedItem,
    rhs: &ConnectedItem,
    file: &mut VastFile,
    module: &mut VastModule,
    nets: &mut NetCollection,
    options: &EmitOptions,
) {
    let mut assign = |this: &PortSlice, lhs: &ConnectedItem, rhs: &ConnectedItem| {
        let lhs = connected_item_to_expression(this, lhs, file, module, nets, options);
        let rhs = connected_item_to_expression(this, rhs, file, module, nets, options);
        let statement = file.make_continuous_assignment(&lhs, &rhs);
        file.add_member_continuous_assignment(*module, statement);
    };
    if options.bitblast_assignments {
        for offset in (0..this.width()).rev() {
            assign(
                &this.slice_with_offset_and_width(offset, 1),
                &lhs.slice_with_offset_and_width(offset, 1),
                &rhs.slice_with_offset_and_width(offset, 1),
            );
        }
    } else {
        assign(this, lhs, rhs);
    }
}

/// Assembles one whole port expression from its non-overlapping sources in MSB-first order.
fn port_expression(
    sources: Vec<crate::PortSliceConnection>,
    file: &mut VastFile,
    module: &mut VastModule,
    nets: &mut NetCollection,
    options: &EmitOptions,
) -> Option<Expr> {
    let parts: Vec<_> = sources
        .into_iter()
        .map(|c| connected_item_to_expression(&c.this, &c.other, file, module, nets, options))
        .collect();
    match parts.as_slice() {
        [] => None,
        [value] => Some(*value),
        _ => Some(file.make_concat(&parts.iter().collect::<Vec<_>>())),
    }
}

fn connected_item_to_expression(
    this: &PortSlice,
    item: &ConnectedItem,
    file: &mut VastFile,
    module: &mut VastModule,
    nets: &mut NetCollection,
    options: &EmitOptions,
) -> Expr {
    match item {
        ConnectedItem::PortSlice(port_slice) => {
            let port = &port_slice.port;
            let width = port.io().width();

            let inst_name = port.inst_name();
            let port_name = port.get_port_name();
            let source = match inst_name {
                Some(inst_name) => NetNameSource::ModInstPort((inst_name.to_string(), port_name)),
                None => NetNameSource::ModDefPort(port_name),
            };

            if let NetNameSource::ModInstPort(key) = &source
                && let Some(value) = nets.inline_values.get(key)
            {
                debug_assert_eq!(width, port_slice.width());
                return *value;
            }
            let net = nets.get_logic_ref_create_if_necessary(source, width, file, module);

            slice_net(net, width, port_slice.msb, port_slice.lsb, file, options)
        }
        ConnectedItem::Wire(wire) => {
            let width = wire.width;
            let source = NetNameSource::ManuallySpecified(wire.name.clone());
            let net = nets.get_logic_ref_create_if_necessary(source, width, file, module);
            slice_net(net, width, wire.msb, wire.lsb, file, options)
        }
        ConnectedItem::Tieoff(tieoff) => {
            let literal_str = format!("bits[{}]:{}", tieoff.width, tieoff.value);
            file.make_literal(&literal_str, &LiteralFormat::Hex)
                .unwrap()
        }
        ConnectedItem::Unused(_) => {
            // TODO(sherbst) 2025-12-01: Reduce code duplication for
            // ConnectedItem::PortSlice and ConnectedItem::Unused cases?

            let port = &this.port;
            let width = port.io().width();

            let inst_name = port.inst_name();
            let port_name = port.get_port_name();
            let source = match inst_name {
                Some(inst_name) => NetNameSource::ModInstPort((inst_name.to_string(), port_name)),
                None => NetNameSource::ModDefPort(port_name),
            };

            let net = nets.get_logic_ref_create_if_necessary(source, width, file, module);

            slice_net(net, width, this.msb, this.lsb, file, options)
        }
    }
}

fn slice_net(
    net: &LogicRef,
    width: usize,
    msb: usize,
    lsb: usize,
    file: &mut VastFile,
    options: &EmitOptions,
) -> Expr {
    slice_bits(net.to_indexable_expr(), width, msb, lsb, file, options)
}

/// Selects an inclusive range within one packed vector.
fn slice_bits(
    net: xlsynth_vast::IndexableExpr,
    width: usize,
    msb: usize,
    lsb: usize,
    file: &mut VastFile,
    options: &EmitOptions,
) -> Expr {
    // A selected word can itself be scalar, for example a stage of a one-bit delay.
    if width == 1 || (!options.preserve_full_width_slices && width == msb - lsb + 1) {
        net.to_expr()
    } else if !options.preserve_single_bit_slices && msb == lsb {
        file.make_index(&net, msb as i64).to_expr()
    } else {
        file.make_slice(&net, msb as i64, lsb as i64).to_expr()
    }
}
