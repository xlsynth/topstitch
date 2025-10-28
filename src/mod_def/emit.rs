// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use indexmap::map::Entry;
use indexmap::IndexMap;
use xlsynth::vast::{Expr, LogicRef, VastFile, VastFileType, VastModule};

use crate::connection::connected_item::ConnectedItem;
use crate::connection::expression_source::merge_expression_sources;
use crate::connection::validate::check_for_gaps;
use crate::port::default_net_name_for_inst_port;
use crate::{ModDef, ModDefCore, Port, PortSlice, Usage, IO};

impl ModDef {
    /// Writes Verilog code for this module definition to the given file path.
    /// If `validate` is `true`, validate the module definition before emitting
    /// Verilog.
    pub fn emit_to_file(&self, path: &Path, validate: bool) {
        let err_msg = format!("emitting ModDef to file at path: {path:?}");
        std::fs::write(path, self.emit(validate)).expect(&err_msg);
    }

    /// Returns Verilog code for this module definition as a string. If
    /// `validate` is `true`, validate the module definition before emitting
    /// Verilog.
    pub fn emit(&self, validate: bool) -> String {
        if validate {
            self.validate();
        }
        let mut emitted_module_names = IndexMap::new();
        let mut file = VastFile::new(VastFileType::SystemVerilog);
        let mut leaf_text = Vec::new();
        let mut enum_remapping = IndexMap::new();
        self.emit_recursive(
            &mut emitted_module_names,
            &mut file,
            &mut leaf_text,
            &mut enum_remapping,
        );
        let emit_result = file.emit();
        if !emit_result.is_empty() {
            leaf_text.push(emit_result);
        }
        let result = leaf_text.join("\n");
        let result = crate::inout::rename_inout(result);
        crate::enum_type::remap_enum_types(result, &enum_remapping)
    }

    fn emit_recursive(
        &self,
        emitted_module_names: &mut IndexMap<String, Rc<RefCell<ModDefCore>>>,
        file: &mut VastFile,
        leaf_text: &mut Vec<String>,
        enum_remapping: &mut IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
    ) {
        let core = self.core.borrow();

        match emitted_module_names.entry(core.name.clone()) {
            Entry::Occupied(entry) => {
                let existing_moddef = entry.get();
                if !Rc::ptr_eq(existing_moddef, &self.core) {
                    panic!("Two distinct modules with the same name: {}", core.name);
                } else {
                    return;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(self.core.clone());
            }
        }

        if core.usage == Usage::EmitNothingAndStop {
            return;
        } else if core.usage == Usage::EmitDefinitionAndStop {
            leaf_text.push(core.generated_verilog.clone().unwrap());
            return;
        }

        // Recursively emit instances

        if core.usage == Usage::EmitDefinitionAndDescend {
            for inst in core.instances.values() {
                ModDef { core: inst.clone() }.emit_recursive(
                    emitted_module_names,
                    file,
                    leaf_text,
                    enum_remapping,
                );
            }
        }

        // Start the module declaration.

        let mut collision_detection = core.specified_net_names.clone();
        let mut module = file.add_module(&core.name);

        // Create nets for each module port

        let mut nets: IndexMap<String, LogicRef> = IndexMap::new();

        for port_name in core.ports.keys() {
            if !collision_detection.insert(port_name.to_string()) {
                panic!(
                    "Net \"{port_name}\" is already declared in module {}.",
                    core.name
                );
            }

            let io = core.ports.get(port_name).unwrap();
            let logic_ref =
                match io {
                    IO::Input(width) => module
                        .add_input(port_name, &file.make_bit_vector_type(*width as i64, false)),
                    IO::Output(width) => module
                        .add_output(port_name, &file.make_bit_vector_type(*width as i64, false)),
                    // TODO(sherbst) 11/18/24: Replace with VAST API call
                    IO::InOut(width) => module.add_input(
                        &format!("{}{}", port_name, crate::inout::INOUT_MARKER),
                        &file.make_bit_vector_type(*width as i64, false),
                    ),
                };

            if nets.insert(port_name.clone(), logic_ref).is_some() {
                panic!(
                    "Net {port_name} is already declared in module {}",
                    core.name
                );
            }
        }

        if core.usage == Usage::EmitStubAndStop {
            return;
        }

        // Create module instances
        for (inst_name, inst) in core.instances.iter() {
            let core_borrowed = self.core.borrow();
            let empty_connections = IndexMap::new();
            let mod_inst_connections = match core_borrowed.mod_inst_connections.get(inst_name) {
                Some(mod_inst_connections) => mod_inst_connections,
                None => &empty_connections,
            };

            let module_name = inst.borrow().name.clone();
            let parameter_port_names: Vec<&str> = Vec::new();
            let parameter_expressions: Vec<&Expr> = Vec::new();
            let mut connection_port_names = Vec::new();
            let mut connection_expressions = Vec::new();

            for (port_name, io) in inst.borrow().ports.iter() {
                if !collision_detection.insert(default_net_name_for_inst_port(inst_name, port_name))
                {
                    // TODO(sherbst) 2025-10-27: Don't add a net to the collision detection set if
                    // the default net name is never used. This would provide a convenient way to
                    // work around collisions on a per-port basis.
                    panic!(
                        "Net \"{}\" is already declared in module {}.",
                        default_net_name_for_inst_port(inst_name, port_name),
                        core.name
                    );
                }

                connection_port_names.push(port_name.clone());

                if inst.borrow().enum_ports.contains_key(port_name) {
                    enum_remapping
                        .entry(core.name.clone())
                        .or_default()
                        .entry(inst_name.clone())
                        .or_default()
                        .insert(
                            port_name.clone(),
                            inst.borrow().enum_ports.get(port_name).unwrap().clone(),
                        );
                }

                let port_slice_connections = match mod_inst_connections.get(port_name) {
                    Some(port_slice_connections) => port_slice_connections,
                    None => {
                        panic!("{}.{}.{} is unconnected", core.name, inst_name, port_name);
                    }
                };

                // break into non-overlapping chunks
                let mut non_overlapping = port_slice_connections
                    .borrow()
                    .trace()
                    .make_non_overlapping();

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
                        )
                    })
                    .collect::<Vec<_>>();

                match concat_entries.len() {
                    0 => {
                        connection_expressions.push(None);
                    }
                    1 => {
                        connection_expressions.push(Some(concat_entries.remove(0)));
                    }
                    _ => {
                        let slice_references: Vec<&Expr> = concat_entries.iter().collect();
                        connection_expressions.push(Some(file.make_concat(&slice_references)));
                    }
                }
            }

            let instantiation = file.make_instantiation(
                &module_name,
                inst_name,
                &parameter_port_names,
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
            module.add_member_instantiation(instantiation);
        }

        // Emit assign statements for ModDef ports if necessary
        for port_name in core.ports.keys() {
            let core_borrowed = self.core.borrow();
            let port_slice_connections = match core_borrowed.mod_def_connections.get(port_name) {
                Some(port_slice_connections) => port_slice_connections,
                None => panic!("{}.{} is unconnected", core.name, port_name),
            };

            // break into non-overlapping chunks
            let mut non_overlapping = port_slice_connections
                .borrow()
                .trace()
                .make_non_overlapping();

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
                match &expression_source.other {
                    ConnectedItem::PortSlice(port_slice) => {
                        if let Port::ModDef {
                            name: port_slice_port_name,
                            ..
                        } = &port_slice.port
                        {
                            if port_slice_port_name == port_name {
                                continue;
                            }
                        }
                    }
                    ConnectedItem::Unused(_) => {
                        continue;
                    }
                    _ => {}
                }

                let lhs = slice_net(
                    nets.get(port_name).unwrap(),
                    expression_source.this.port.io().width(),
                    expression_source.this.msb,
                    expression_source.this.lsb,
                    file,
                );

                let rhs = connected_item_to_expression(
                    &expression_source.this,
                    &expression_source.other,
                    file,
                    &mut module,
                    &mut nets,
                );

                let assignment = file.make_continuous_assignment(&lhs, &rhs);
                module.add_member_continuous_assignment(assignment);
            }
        }
    }
}

fn connected_item_to_expression(
    this: &PortSlice,
    item: &ConnectedItem,
    file: &mut VastFile,
    module: &mut VastModule,
    nets: &mut IndexMap<String, LogicRef>,
) -> Expr {
    match item {
        ConnectedItem::PortSlice(port_slice) => {
            let name = port_slice.port.default_net_name();
            let width = port_slice.port.io().width();
            let net = get_net_define_if_necessary(&name, width, file, module, nets);
            slice_net(net, width, port_slice.msb, port_slice.lsb, file)
        }
        ConnectedItem::Wire(wire) => {
            let name = wire.name.clone();
            let width = wire.width;
            let net = get_net_define_if_necessary(&name, width, file, module, nets);
            slice_net(net, width, wire.msb, wire.lsb, file)
        }
        ConnectedItem::Tieoff(tieoff) => {
            let literal_str = format!("bits[{}]:{}", tieoff.width, tieoff.value);
            file.make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex)
                .unwrap()
        }
        ConnectedItem::Unused(_) => {
            let name = this.port.default_net_name();
            let width = this.port.io().width();
            let net = get_net_define_if_necessary(&name, width, file, module, nets);
            slice_net(net, width, this.msb, this.lsb, file)
        }
    }
}

fn get_net_define_if_necessary<'a>(
    name: &str,
    width: usize,
    file: &mut VastFile,
    module: &mut VastModule,
    nets: &'a mut IndexMap<String, LogicRef>,
) -> &'a LogicRef {
    nets.entry(name.to_string())
        .or_insert_with(|| module.add_wire(name, &file.make_bit_vector_type(width as i64, false)))
}

fn slice_net(net: &LogicRef, width: usize, msb: usize, lsb: usize, file: &mut VastFile) -> Expr {
    if width == (msb - lsb + 1) {
        net.to_expr()
    } else if msb == lsb {
        file.make_index(&net.to_indexable_expr(), msb as i64)
            .to_expr()
    } else {
        file.make_slice(&net.to_indexable_expr(), msb as i64, lsb as i64)
            .to_expr()
    }
}
