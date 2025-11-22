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
use crate::{ModDef, ModDefCore, Port, PortSlice, Usage, IO};

#[derive(Debug, PartialEq)]
enum NetNameSource {
    ManuallySpecified(String),
    ModDefPort(String),
    ModInstPort((String, String)),
}

use std::fmt;

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
}

impl NetCollection {
    pub fn new() -> Self {
        Self {
            sources: IndexMap::new(),
            logic_refs: IndexMap::new(),
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
            panic!("Error while declaring ModDef port {name} in a NetCollection: a net name source for this port has already been declared ({existing_source:?})");
        }

        let logic_ref = match io {
            IO::Input(width) => {
                module.add_input(name, &file.make_bit_vector_type(*width as i64, false))
            }
            IO::Output(width) => {
                module.add_output(name, &file.make_bit_vector_type(*width as i64, false))
            }
            IO::InOut(width) => {
                module.add_inout(name, &file.make_bit_vector_type(*width as i64, false))
            }
        };

        if self.logic_refs.insert(name_as_string, logic_ref).is_some() {
            panic!("NetCollection out of sync: net \"{name}\" is declared in the refs map but not the sources map.");
        }
    }

    pub fn get_logic_ref<'a>(&'a self, name: &str) -> Option<&'a LogicRef> {
        self.logic_refs.get(name)
    }

    pub fn get_logic_ref_create_if_necessary<'a>(
        &'a mut self,
        source: NetNameSource,
        width: usize,
        file: &mut VastFile,
        module: &mut VastModule,
    ) -> &'a LogicRef {
        match self.sources.entry(source.to_string()) {
            Entry::Occupied(source_entry) => {
                let existing_ref = self.logic_refs.get(source_entry.key());
                let existing_source = source_entry.get();
                if existing_source == &source {
                    existing_ref.unwrap()
                } else {
                    panic!("Net name collision for {source:?} and {:?}: both resolve to \"{}\". If you have used specify_net_name() on one of the ports involved, you may need to remove it or change the net name. If not, you may need to use specify_net_name() to override the net name for one of the ModInst ports involved.", existing_source, &source_entry.key());
                }
            }
            Entry::Vacant(source_entry) => {
                let net_name = source.to_string();
                let data_type = file.make_bit_vector_type(width as i64, false);
                let wire = match source {
                    NetNameSource::ManuallySpecified(_) | NetNameSource::ModInstPort(_) => {
                        module.add_wire(&net_name, &data_type)
                    }
                    NetNameSource::ModDefPort(_) => {
                        panic!("ModDef ports should be added to NetCollection using declare_mod_def_port()")
                    }
                };
                source_entry.insert(source);
                match self.logic_refs.entry(net_name.clone()) {
                    Entry::Vacant(ref_entry) => ref_entry.insert(wire),
                    Entry::Occupied(_) => {
                        panic!("NetCollection out of sync: net \"{}\" is declared in the refs map but not the sources map.", net_name);
                    }
                }
            }
        }
    }
}

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
        self.emit_recursive(&mut emitted_module_names, &mut file);
        let emit_result = file.emit();
        let mut leaf_text = Vec::new();
        if !emit_result.is_empty() {
            leaf_text.push(emit_result);
        }
        leaf_text.join("\n")
    }

    fn emit_recursive(
        &self,
        emitted_module_names: &mut IndexMap<String, Rc<RefCell<ModDefCore>>>,
        file: &mut VastFile,
    ) {
        let core = self.core.borrow();

        if core.usage == Usage::EmitNothingAndStop || core.usage == Usage::EmitDefinitionAndStop {
            return;
        }

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

        // Recursively emit instances

        if core.usage == Usage::EmitDefinitionAndDescend {
            for inst in core.instances.values() {
                ModDef { core: inst.clone() }.emit_recursive(emitted_module_names, file);
            }
        }

        // Start the module declaration.

        let mut module = file.add_module(&core.name);

        // Create nets for each module port

        let mut nets = NetCollection::new();

        for port_name in core.ports.keys() {
            let io = core.ports.get(port_name).unwrap();
            nets.declare_mod_def_port(port_name, io, file, &mut module);
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
            let mut parameter_port_names: Vec<String> = Vec::new();
            let mut parameter_expr_vals: Vec<Expr> = Vec::new();
            let mut connection_port_names = Vec::new();
            let mut connection_expressions = Vec::new();

            for (port_name, io) in inst.borrow().ports.iter() {
                connection_port_names.push(port_name.clone());

                let enum_t = inst
                    .borrow()
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
            if !inst.borrow().parameters.is_empty() {
                let param_core = inst.borrow();
                for (param_name, spec) in param_core.parameters.iter() {
                    parameter_port_names.push(param_name.clone());
                    if spec.value.sign() == num_bigint::Sign::Minus {
                        // TODO(sherbst) 2025-10-29: Support negative parameter values
                        panic!("Negative parameter values not yet supported");
                    }
                    let literal_str = format!("bits[{}]:{}", spec.ty.width(), spec.value);
                    let expr = file
                        .make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex)
                        .unwrap();
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
                    nets.get_logic_ref(port_name).unwrap(),
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
    nets: &mut NetCollection,
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

            let net = nets.get_logic_ref_create_if_necessary(source, width, file, module);

            slice_net(net, width, port_slice.msb, port_slice.lsb, file)
        }
        ConnectedItem::Wire(wire) => {
            let width = wire.width;
            let source = NetNameSource::ManuallySpecified(wire.name.clone());
            let net = nets.get_logic_ref_create_if_necessary(source, width, file, module);
            slice_net(net, width, wire.msb, wire.lsb, file)
        }
        ConnectedItem::Tieoff(tieoff) => {
            let literal_str = format!("bits[{}]:{}", tieoff.width, tieoff.value);
            file.make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex)
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

            slice_net(net, width, this.msb, this.lsb, file)
        }
    }
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
