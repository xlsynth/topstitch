// SPDX-License-Identifier: Apache-2.0

use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use indexmap::map::Entry;
use xlsynth_vast::{Expr, LiteralFormat, LogicRef, VastFile, VastFileType, VastModule};

use super::net_names::NetNames;
use crate::connection::connected_item::ConnectedItem;
use crate::connection::expression_source::merge_expression_sources;
use crate::connection::validate::check_for_gaps;
use crate::{IO, ModDef, ModDefCore, Port, PortSlice, Usage};

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

#[derive(Eq, Hash, PartialEq)]
enum WireSource {
    Requested(String),
    InstancePort(String, String),
}

struct NetCollection {
    ports: HashMap<String, LogicRef>,
    wires: HashMap<WireSource, LogicRef>,
    names: NetNames,
}

impl NetCollection {
    /// Declares each wire once, keeping distinct sources separate even if their names collide.
    fn wire(
        &mut self,
        source: WireSource,
        width: usize,
        file: &mut VastFile,
        module: &mut VastModule,
    ) -> &LogicRef {
        self.wires.entry(source).or_insert_with_key(|source| {
            let name = match source {
                WireSource::Requested(name) => name.clone(),
                WireSource::InstancePort(instance, port) => {
                    let base = format!("{instance}_{port}");
                    self.names.reserve_generated(&base).unwrap_or(base)
                }
            };
            let data_type = file.make_bit_vector_type(width as i64, false);
            file.add_wire(*module, &name, &data_type)
        })
    }

    /// Selects the same named port or instance wire for every reference to a slice.
    fn port_slice_expr(
        &mut self,
        slice: &PortSlice,
        file: &mut VastFile,
        module: &mut VastModule,
        options: &EmitOptions,
    ) -> Expr {
        let port = &slice.port;
        let width = port.io().width();
        let net = match port.inst_name() {
            Some(instance) => self.wire(
                WireSource::InstancePort(instance.to_owned(), port.name().to_owned()),
                width,
                file,
                module,
            ),
            None => &self.ports[port.name()],
        };
        slice_net(net, width, slice.msb, slice.lsb, file, options)
    }
}

impl ModDef {
    /// Writes Verilog code for this module definition to the given file path.
    pub fn emit_to_file(&self, path: &Path, options: EmitOptions) {
        let err_msg = format!("emitting ModDef to file at path: {path:?}");
        std::fs::write(path, self.emit(options)).expect(&err_msg);
    }

    /// Returns Verilog code for this module definition as a string.
    ///
    /// Emitted names, including references to external modules and their ports, must
    /// match `[A-Za-z_][A-Za-z0-9_]*` and cannot be SystemVerilog keywords.
    /// An imported black box may use escaped identifiers internally.
    pub fn emit(&self, options: EmitOptions) -> String {
        self.check_emission_names()
            .unwrap_or_else(|e| panic!("{e}"));
        if options.validate {
            self.validate();
        }
        let parameter_headers = super::sv_parameter_emission::plan(self);
        let mut emitted_module_names = IndexMap::new();
        let mut output = String::new();
        self.emit_recursive(
            &mut emitted_module_names,
            &parameter_headers,
            &mut output,
            &options,
        );
        output
    }

    fn emit_recursive(
        &self,
        emitted_module_names: &mut IndexMap<String, Arc<RwLock<ModDefCore>>>,
        parameter_headers: &super::sv_parameter_emission::ParameterHeaders,
        output: &mut String,
        options: &EmitOptions,
    ) {
        let core = self.core.read();

        if core.usage == Usage::EmitNothingAndStop || core.usage == Usage::EmitDefinitionAndStop {
            return;
        }

        match emitted_module_names.entry(core.name.clone()) {
            Entry::Occupied(entry) => {
                let existing_moddef = entry.get();
                if !Arc::ptr_eq(existing_moddef, &self.core) {
                    panic!("Two distinct modules with the same name: {}", core.name);
                } else {
                    return;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(self.core.clone());
            }
        }

        // Recursively emit instances

        if core.usage == Usage::EmitDefinitionAndDescend {
            for inst in core.instances.values() {
                ModDef { core: inst.clone() }.emit_recursive(
                    emitted_module_names,
                    parameter_headers,
                    output,
                    options,
                );
            }
        }

        let mut file = VastFile::new(VastFileType::SystemVerilog);
        let declarations = parameter_headers.get(&core.name);
        let names = declarations.map_or(&[][..], |decl| decl.names.as_slice());
        self.add_module_to(&core, &mut file, options, names);
        let rtl = file.emit();
        if let Some(header) = declarations {
            // VAST models the ports and body. Its parameter AST requires an initializer
            // and cannot express the source declaration of every imported value parameter.
            let prefix = format!("module {}", core.name);
            let rest = rtl.strip_prefix(&prefix).expect("VAST module header");
            output.push_str(&prefix);
            output.push_str(&header.header);
            if !core.ports.is_empty() {
                output.push(' ');
            } else if header.header.trim_end().ends_with(';') {
                // An import clause needs a parameter or port list after its semicolon.
                output.push_str(" ()");
            }
            if header.body.is_empty() {
                output.push_str(rest);
            } else {
                let (ports, body) = rest.split_once(";\n").expect("VAST port list terminator");
                output.push_str(ports);
                output.push_str(";\n");
                output.push_str(&header.body);
                output.push('\n');
                output.push_str(body);
            }
        } else {
            output.push_str(&rtl);
        }
    }

    /// Build this module's ports and body in a single-module VAST file.
    fn add_module_to(
        &self,
        core: &ModDefCore,
        file: &mut VastFile,
        options: &EmitOptions,
        declaration_names: &[String],
    ) {
        // Start the module declaration.

        let mut module = file.add_module(&core.name);
        let mut ports = HashMap::with_capacity(core.ports.len());
        for (name, io) in &core.ports {
            let data_type = file.make_bit_vector_type(io.width() as i64, false);
            let port = match io {
                IO::Input(_) => file.add_input(module, name, &data_type),
                IO::Output(_) => file.add_output(module, name, &data_type),
                IO::InOut(_) => file.add_inout(module, name, &data_type),
            };
            ports.insert(name.clone(), port);
        }
        if core.usage == Usage::EmitStubAndStop {
            if let Some(name) = declaration_names
                .iter()
                .find(|name| core.ports.contains_key(*name))
            {
                panic!(
                    "Name collision in module {}: parameter {name} conflicts with a port.",
                    core.name
                );
            }
            return;
        }
        let mut names = NetNames::new(core).unwrap_or_else(|err| panic!("{err}"));
        names
            .reserve_declarations(declaration_names, &core.name)
            .unwrap_or_else(|err| panic!("{err}"));
        let mut nets = NetCollection {
            ports,
            wires: HashMap::new(),
            names,
        };

        // Create module instances
        for (inst_name, inst) in core.instances.iter() {
            let core_borrowed = self.core.read();
            let empty_connections = IndexMap::new();
            let mod_inst_connections = match core_borrowed.mod_inst_connections.get(inst_name) {
                Some(mod_inst_connections) => mod_inst_connections,
                None => &empty_connections,
            };

            let module_name = inst.read().name.clone();
            let mut parameter_port_names: Vec<String> = Vec::new();
            let mut parameter_expr_vals: Vec<Expr> = Vec::new();
            let mut connection_port_names = Vec::new();
            let mut connection_expressions = Vec::new();

            for (port_name, io) in inst.read().ports.iter() {
                connection_port_names.push(port_name.clone());

                let enum_t = inst
                    .read()
                    .enum_ports
                    .get(port_name)
                    .map(|enum_t| file.make_extern_type(enum_t));

                let port_slice_connections = match mod_inst_connections.get(port_name) {
                    Some(port_slice_connections) => port_slice_connections,
                    None => {
                        panic!("{}.{}.{} is unconnected", core.name, inst_name, port_name);
                    }
                };

                // break into non-overlapping chunks
                let mut non_overlapping =
                    port_slice_connections.read().trace().make_non_overlapping();

                non_overlapping.retain(|c| !c.is_empty());
                non_overlapping.sort_by_key(|c| -(c[0].this.msb as isize));

                // make sure there aren't gaps between connections for this port
                check_for_gaps(
                    &non_overlapping,
                    io,
                    &format!("{}.{}.{}", core.name, inst_name, port_name),
                );

                let expression_sources = non_overlapping
                    .iter()
                    .map(|c| c.to_expression_source().unwrap())
                    .collect::<Vec<_>>();

                let merged = merge_expression_sources(expression_sources);

                if (merged.len() == 1) && matches!(merged[0].other, ConnectedItem::Unused(_)) {
                    connection_expressions.push(None);
                    continue;
                }

                let mut concat_entries = merged
                    .into_iter()
                    .map(|c| {
                        connected_item_to_expression(
                            &c.this,
                            &c.other,
                            file,
                            &mut module,
                            &mut nets,
                            options,
                        )
                    })
                    .collect::<Vec<_>>();

                let connection_expression = match concat_entries.len() {
                    0 => None,
                    1 => Some(concat_entries.remove(0)),
                    _ => {
                        let slice_references: Vec<&Expr> = concat_entries.iter().collect();
                        Some(file.make_concat(&slice_references))
                    }
                };

                connection_expressions.push(connection_expression.map(|expr| {
                    if let Some(enum_t) = enum_t {
                        file.make_type_cast(&enum_t, &expr)
                    } else {
                        expr
                    }
                }));
            }

            // Build parameter override expressions, if any
            if !inst.read().parameters.is_empty() {
                let param_core = inst.read();
                for (param_name, spec) in param_core.parameters.iter() {
                    parameter_port_names.push(param_name.clone());
                    let expr = spec.sv_expression(file).unwrap_or_else(|e| {
                        panic!("invalid parameter {}.{param_name}: {e}", param_core.name)
                    });
                    parameter_expr_vals.push(expr);
                }
            }

            let parameter_expressions: Vec<&Expr> = parameter_expr_vals.iter().collect();
            let parameter_port_name_refs: Vec<&str> =
                parameter_port_names.iter().map(|s| s.as_str()).collect();
            let instantiation = file.make_instantiation(
                &module_name,
                inst_name,
                &parameter_port_name_refs,
                &parameter_expressions,
                &connection_port_names
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>(),
                &connection_expressions
                    .iter()
                    .map(|o| o.as_ref())
                    .collect::<Vec<_>>(),
            );
            file.add_member_instantiation(module, instantiation);
        }

        // Emit assign statements for ModDef ports if necessary
        for port_name in core.ports.keys() {
            let core_borrowed = self.core.read();
            let port_slice_connections = match core_borrowed.mod_def_connections.get(port_name) {
                Some(port_slice_connections) => port_slice_connections,
                None => panic!("{}.{} is unconnected", core.name, port_name),
            };

            // break into non-overlapping chunks
            let mut non_overlapping = port_slice_connections.read().trace().make_non_overlapping();

            non_overlapping.retain(|c| !c.is_empty());
            non_overlapping.sort_by_key(|c| -(c[0].this.msb as isize));

            // make sure there aren't gaps between connections for this port
            check_for_gaps(
                &non_overlapping,
                core.ports.get(port_name).unwrap(),
                &format!("{}.{}", core.name, port_name),
            );

            let expression_sources = non_overlapping
                .iter()
                .map(|c| c.to_expression_source().unwrap())
                .collect::<Vec<_>>();

            let merged = merge_expression_sources(expression_sources);

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

                // Expand the assignment from MSB to LSB by slicing both sides
                // at the same relative offset. ConnectedItem slicing keeps
                // this mapping uniform for ports, wires, and constants.
                let assignments = if options.bitblast_assignments {
                    (0..width)
                        .rev()
                        .map(|offset| {
                            (
                                expression_source
                                    .this
                                    .slice_with_offset_and_width(offset, 1),
                                expression_source
                                    .other
                                    .slice_with_offset_and_width(offset, 1),
                            )
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![(expression_source.this, expression_source.other)]
                };

                for (this, other) in assignments {
                    let lhs = slice_net(
                        &nets.ports[port_name],
                        this.port.io().width(),
                        this.msb,
                        this.lsb,
                        file,
                        options,
                    );

                    let rhs = connected_item_to_expression(
                        &this,
                        &other,
                        file,
                        &mut module,
                        &mut nets,
                        options,
                    );

                    let assignment = file.make_continuous_assignment(&lhs, &rhs);
                    file.add_member_continuous_assignment(module, assignment);
                }
            }
        }
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
            nets.port_slice_expr(port_slice, file, module, options)
        }
        ConnectedItem::Wire(wire) => {
            let width = wire.width;
            let source = WireSource::Requested(wire.name.clone());
            let net = nets.wire(source, width, file, module);
            slice_net(net, width, wire.msb, wire.lsb, file, options)
        }
        ConnectedItem::Tieoff(tieoff) => {
            let literal_str = format!("bits[{}]:{}", tieoff.width, tieoff.value);
            file.make_literal(&literal_str, &LiteralFormat::Hex)
                .unwrap()
        }
        ConnectedItem::Unused(_) => nets.port_slice_expr(this, file, module, options),
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
    if !options.preserve_full_width_slices && width == (msb - lsb + 1) {
        net.to_expr()
    } else if !options.preserve_single_bit_slices && msb == lsb {
        file.make_index(&net.to_indexable_expr(), msb as i64)
            .to_expr()
    } else {
        file.make_slice(&net.to_indexable_expr(), msb as i64, lsb as i64)
            .to_expr()
    }
}
