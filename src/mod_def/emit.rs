// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use indexmap::map::Entry;
use indexmap::IndexMap;
use xlsynth::vast::{Expr, LogicRef, VastFile, VastFileType};

use crate::mod_def::{Assignment, PortSliceOrWire};
use crate::{ModDef, ModDefCore, Port, PortSlice, Usage, IO};

use crate::pipeline::add_pipeline;
use crate::pipeline::PipelineDetails;

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
        let mut pipeline_counter = 0usize..;

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

        let mut module = file.add_module(&core.name);

        let mut ports: IndexMap<String, LogicRef> = IndexMap::new();

        for port_name in core.ports.keys() {
            let io = core.ports.get(port_name).unwrap();
            if ports.contains_key(port_name) {
                panic!("Port {}.{} is already declared", core.name, port_name);
            }
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
            ports.insert(port_name.clone(), logic_ref);
        }

        if core.usage == Usage::EmitStubAndStop {
            return;
        }

        // List out the wires to be used for internal connections.
        let mut nets: IndexMap<String, LogicRef> = IndexMap::new();
        for (inst_name, inst) in core.instances.iter() {
            for (port_name, io) in inst.borrow().ports.iter() {
                if self
                    .core
                    .borrow()
                    .whole_port_tieoffs
                    .contains_key(inst_name)
                    && self.core.borrow().whole_port_tieoffs[inst_name].contains_key(port_name)
                {
                    // skip whole port tieoffs; they are handled in the instantiation
                    continue;
                }
                if self.core.borrow().whole_port_unused.contains_key(inst_name)
                    && self.core.borrow().whole_port_unused[inst_name].contains(port_name)
                {
                    // skip ports that are completely unused; they are handled in the instantiation
                    continue;
                }
                if core.inst_connections.contains_key(inst_name)
                    && core
                        .inst_connections
                        .get(inst_name)
                        .unwrap()
                        .contains_key(port_name)
                {
                    // Don't create a wire for a port that is directly connected to a module
                    // definition port
                    continue;
                }
                let net_name = format!("{inst_name}_{port_name}");
                if ports.contains_key(&net_name) {
                    panic!("Generated net name for instance port {}.{} collides with a port name on module definition {}: \
both are called {}. Altering the instance name will likely fix this problem. connect_to_net() could also be used to \
specify an alternate net name for this instance port, although that may be more labor-intensive since all connectivity \
on that net will need to be updated.",
                        inst_name, port_name, core.name, net_name
                    );
                }
                let data_type = file.make_bit_vector_type(io.width() as i64, false);
                if nets
                    .insert(net_name.clone(), module.add_wire(&net_name, &data_type))
                    .is_some()
                {
                    panic!("Generated net name for instance port {}.{} collides with another generated net name within \
module definition {}: both are called {}. Altering the instance name will likely fix this problem. connect_to_net() could \
also be used to specify an alternate net name for this instance port, although that may be more labor-intensive since all \
connectivity on that net will need to be updated.",
                        inst_name, port_name, core.name, net_name);
                }

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
            }
        }

        // Create wires for reserved net definitions.
        for wire in core.reserved_net_definitions.values() {
            if nets
                .insert(
                    wire.name.clone(),
                    module.add_wire(
                        &wire.name,
                        &file.make_bit_vector_type(wire.width as i64, false),
                    ),
                )
                .is_some()
            {
                panic!("connect_to_net()-specified net name {} already exists in module definition {}. \
This is likely due to a collision with a generated net name, which has the form {{instance name}}_{{port name}}. \
Two possible solutions: 1) change the instance name corresponding to the generated net name, or 2) provide an \
alternate net name to connect_to_net().",
                    wire.name, core.name
                );
            }
        }

        // Instantiate modules.
        for (inst_name, inst) in core.instances.iter() {
            let module_name = &inst.borrow().name;
            let instance_name = inst_name;
            let parameter_port_names: Vec<&str> = Vec::new();
            let parameter_expressions: Vec<&Expr> = Vec::new();
            let mut connection_port_names = Vec::new();
            let mut connection_expressions = Vec::new();

            for (port_name, io) in inst.borrow().ports.iter() {
                connection_port_names.push(port_name.clone());

                if core.inst_connections.contains_key(inst_name)
                    && core
                        .inst_connections
                        .get(inst_name)
                        .unwrap()
                        .contains_key(port_name)
                {
                    let mut port_slices = core
                        .inst_connections
                        .get(inst_name)
                        .unwrap()
                        .get(port_name)
                        .unwrap()
                        .clone();
                    port_slices.sort_by(|a, b| b.inst_port_slice.msb.cmp(&a.inst_port_slice.msb));

                    let mut concat_entries = Vec::new();
                    let mut msb_expected: i64 = (io.width() as i64) - 1;

                    for port_slice in port_slices {
                        // create a filler if needed
                        if port_slice.inst_port_slice.msb as i64 > msb_expected {
                            panic!(
                                "Instance port slice index {} is out of bounds for instance port {}.{} in module {}, \
since the width of that port is {}. Check the slice indices for this instance port.",
                                port_slice.inst_port_slice.msb, inst_name, port_name, core.name, io.width()
                            );
                        }

                        if (port_slice.inst_port_slice.msb as i64) < msb_expected {
                            let filler_msb = msb_expected;
                            let filler_lsb = (port_slice.inst_port_slice.msb as i64) + 1;
                            let net_name =
                                format!("UNUSED_{inst_name}_{port_name}_{filler_msb}_{filler_lsb}");
                            let data_type =
                                file.make_bit_vector_type(filler_msb - filler_lsb + 1, false);
                            let wire = module.add_wire(&net_name, &data_type);
                            concat_entries.push(wire.to_expr());
                            if nets.insert(net_name.clone(), wire).is_some() {
                                panic!("Generated net name {} for instance port {}.{} already exists in module definition \
{}. If possible, changing the instance name will likely resolve this issue.", net_name, inst_name, port_name, core.name);
                            }
                        }

                        msb_expected = (port_slice.inst_port_slice.lsb as i64) - 1;

                        match &port_slice.connected_to {
                            PortSliceOrWire::PortSlice(port_slice) => concat_entries.push(
                                file.make_slice(
                                    &ports
                                        .get(&port_slice.port.get_port_name())
                                        .unwrap()
                                        .to_indexable_expr(),
                                    port_slice.msb as i64,
                                    port_slice.lsb as i64,
                                )
                                .to_expr(),
                            ),
                            PortSliceOrWire::Wire(wire) => {
                                concat_entries.push(nets.get(&wire.name).unwrap().to_expr());
                            }
                        }
                    }

                    if msb_expected > -1 {
                        let filler_msb = msb_expected;
                        let filler_lsb = 0;
                        let net_name =
                            format!("UNUSED_{inst_name}_{port_name}_{filler_msb}_{filler_lsb}");
                        let data_type =
                            file.make_bit_vector_type(filler_msb - filler_lsb + 1, false);
                        let wire = module.add_wire(&net_name, &data_type);
                        concat_entries.push(wire.to_expr());
                        if nets.insert(net_name.clone(), wire).is_some() {
                            panic!("Generated net name {} for instance port {}.{} already exists in module definition \
{}. If possible, changing the instance name will likely resolve this issue.", net_name, inst_name, port_name, core.name);
                        }
                    }

                    if concat_entries.len() == 1 {
                        connection_expressions.push(Some(concat_entries.remove(0)));
                    } else {
                        let slice_references: Vec<&Expr> = concat_entries.iter().collect();
                        connection_expressions.push(Some(file.make_concat(&slice_references)));
                    }
                } else if self
                    .core
                    .borrow()
                    .whole_port_tieoffs
                    .contains_key(inst_name)
                    && self.core.borrow().whole_port_tieoffs[inst_name].contains_key(port_name)
                {
                    let value = self.core.borrow().whole_port_tieoffs[inst_name][port_name].clone();
                    let literal_str = format!("bits[{}]:{}", io.width(), value);
                    let value_expr = file
                        .make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex)
                        .unwrap();
                    connection_expressions.push(Some(value_expr));
                } else if self.core.borrow().whole_port_unused.contains_key(inst_name)
                    && self.core.borrow().whole_port_unused[inst_name].contains(port_name)
                {
                    connection_expressions.push(None);
                } else {
                    let net_name = format!("{inst_name}_{port_name}");
                    connection_expressions.push(Some(nets.get(&net_name).unwrap().to_expr()));
                }
            }

            let instantiation = file.make_instantiation(
                module_name,
                instance_name,
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

        // Emit assign statements for connections.
        let mut pipeline_inst_names = HashSet::new();
        for Assignment {
            lhs, rhs, pipeline, ..
        } in &core.assignments
        {
            let lhs_slice = match lhs {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => file.make_slice(
                    &ports.get(name).unwrap().to_indexable_expr(),
                    *msb as i64,
                    *lsb as i64,
                ),
                PortSlice {
                    port:
                        Port::ModInst {
                            inst_name,
                            port_name,
                            ..
                        },
                    msb,
                    lsb,
                } => {
                    let net_name = format!("{inst_name}_{port_name}");
                    file.make_slice(
                        &nets.get(&net_name).unwrap().to_indexable_expr(),
                        *msb as i64,
                        *lsb as i64,
                    )
                }
            };
            let rhs_slice = match rhs {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => file.make_slice(
                    &ports.get(name).unwrap().to_indexable_expr(),
                    *msb as i64,
                    *lsb as i64,
                ),
                PortSlice {
                    port:
                        Port::ModInst {
                            inst_name,
                            port_name,
                            ..
                        },
                    msb,
                    lsb,
                } => {
                    let net_name = format!("{inst_name}_{port_name}");
                    file.make_slice(
                        &nets.get(&net_name).unwrap().to_indexable_expr(),
                        *msb as i64,
                        *lsb as i64,
                    )
                }
            };
            match pipeline {
                None => {
                    let assignment =
                        file.make_continuous_assignment(&lhs_slice.to_expr(), &rhs_slice.to_expr());
                    module.add_member_continuous_assignment(assignment);
                }
                Some(pipeline) => {
                    // Find a unique name for the pipeline instance
                    let pipeline_inst_name = if let Some(inst_name) = pipeline.inst_name.as_ref() {
                        assert!(
                            (!core.instances.contains_key(inst_name)) && (!pipeline_inst_names.contains(inst_name)),
                            "Cannot use pipeline instance name {}, since that instance name is already used in module definition {}.",
                            inst_name,
                            core.name
                        );
                        pipeline_inst_names.insert(inst_name.clone());
                        inst_name.clone()
                    } else {
                        loop {
                            let name =
                                format!("pipeline_conn_{}", pipeline_counter.next().unwrap());
                            if !core.instances.contains_key(&name) {
                                break name;
                            }
                        }
                    };
                    let pipeline_details = PipelineDetails {
                        file,
                        module: &mut module,
                        inst_name: &pipeline_inst_name,
                        clk: &ports
                            .get(&pipeline.clk)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Pipeline clock {} is not defined as a port of module {}.",
                                    pipeline.clk, core.name
                                )
                            })
                            .to_expr(),
                        width: lhs.width(),
                        depth: pipeline.depth,
                        pipe_in: &rhs_slice.to_expr(),
                        pipe_out: &lhs_slice.to_expr(),
                    };
                    add_pipeline(pipeline_details);
                }
            };
        }

        // Emit assign statements for tieoffs.
        for (dst, value) in &core.tieoffs {
            if let Port::ModInst { .. } = &dst.port {
                if dst.port.io().width() == dst.width() {
                    // skip whole port tieoffs; they are handled in the instantiation
                    continue;
                }
            }
            let (dst_expr, width) = match dst {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => (
                    file.make_slice(
                        &ports.get(name).unwrap().to_indexable_expr(),
                        *msb as i64,
                        *lsb as i64,
                    ),
                    msb - lsb + 1,
                ),
                PortSlice {
                    port:
                        Port::ModInst {
                            inst_name,
                            port_name,
                            ..
                        },
                    msb,
                    lsb,
                } => {
                    let net_name = format!("{inst_name}_{port_name}");
                    (
                        file.make_slice(
                            &nets.get(&net_name).unwrap().to_indexable_expr(),
                            *msb as i64,
                            *lsb as i64,
                        ),
                        msb - lsb + 1,
                    )
                }
            };
            let literal_str = format!("bits[{width}]:{value}");
            let value_expr =
                file.make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex);
            let assignment =
                file.make_continuous_assignment(&dst_expr.to_expr(), &value_expr.unwrap());
            module.add_member_continuous_assignment(assignment);
        }
    }
}
