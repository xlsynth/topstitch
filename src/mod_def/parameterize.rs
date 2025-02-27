// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use slang_rs::{self, extract_ports, SlangConfig};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use xlsynth::vast::{VastFile, VastFileType};

use crate::{Usage, IO};

use crate::mod_def::parser_port_to_port;
use crate::{ModDef, ModDefCore};

impl ModDef {
    /// Returns a new module definition that is a variant of this module
    /// definition, where the given parameters have been overridden from their
    /// default values. For example, if the module definition has a parameter
    /// `WIDTH` with a default value of `32`, calling `parameterize(&[("WIDTH",
    /// 64)])` will return a new module definition with the same ports and
    /// instances, but with the parameter `WIDTH` set to `64`. This is
    /// implemented by creating a wrapper module that instantiates the original
    /// module with the given parameters. The name of the wrapper module
    /// defaults to
    /// `<original_mod_def_name>_<param_name_0>_<param_value_0>_<param_name_1>_<param_value_1>_.
    /// ..`; this can be overridden via the optional `def_name` argument. The
    /// instance name of the original module within the wrapper is
    /// `<original_mod_def_name>_i`; this can be overridden via the optional
    /// `inst_name` argument.
    pub fn parameterize(
        &self,
        parameters: &[(&str, i32)],
        def_name: Option<&str>,
        inst_name: Option<&str>,
    ) -> ModDef {
        let core = self.core.borrow();

        if core.verilog_import.is_none() {
            panic!("Error parameterizing {}: can only parameterize a module defined in external Verilog sources.", core.name);
        }

        // Determine the name of the definition if not provided.
        let original_name = &self.core.borrow().name;
        let mut def_name_default = original_name.clone();
        for (param_name, param_value) in parameters {
            def_name_default.push_str(&format!("_{}_{}", param_name, param_value));
        }
        let def_name = def_name.unwrap_or(&def_name_default);

        // Determine the name of the instance inside the wrapper if not provided.
        let inst_name_default = format!("{}_i", original_name);
        let inst_name = inst_name.unwrap_or(&inst_name_default);

        // Determine the I/O for the module.
        let parameters_with_string_values = parameters
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect::<Vec<(String, String)>>();

        let sources: Vec<&str> = core
            .verilog_import
            .as_ref()
            .unwrap()
            .sources
            .iter()
            .map(|s| s.as_str())
            .collect();

        let incdirs: Vec<&str> = core
            .verilog_import
            .as_ref()
            .unwrap()
            .incdirs
            .iter()
            .map(|s| s.as_str())
            .collect();

        let defines: Vec<(&str, &str)> = core
            .verilog_import
            .as_ref()
            .unwrap()
            .defines
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let cfg = SlangConfig {
            sources: sources.as_slice(),
            incdirs: incdirs.as_slice(),
            parameters: &parameters_with_string_values
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            tops: &[&core.name],
            defines: defines.as_slice(),
            ignore_unknown_modules: core.verilog_import.as_ref().unwrap().ignore_unknown_modules,
            ..Default::default()
        };

        let parser_ports = extract_ports(&cfg, true);

        // Generate a wrapper that sets the parameters to the given values.
        let mut file = VastFile::new(VastFileType::Verilog);

        let mut wrapped_module = file.add_module(def_name);
        let mut connection_port_names = Vec::new();
        let mut connection_logic_refs = Vec::new();
        let mut connection_expressions = Vec::new();
        for parser_port in parser_ports[&core.name].iter() {
            match parser_port_to_port(parser_port) {
                Ok((name, io)) => {
                    let logic_expr = match io {
                        IO::Input(width) => wrapped_module.add_input(
                            name.as_str(),
                            &file.make_bit_vector_type(width as i64, false),
                        ),
                        IO::Output(width) => wrapped_module.add_output(
                            name.as_str(),
                            &file.make_bit_vector_type(width as i64, false),
                        ),
                        // TODO(sherbst) 11/18/24: Replace with VAST API call
                        IO::InOut(width) => wrapped_module.add_input(
                            &format!("{}{}", name, crate::inout::INOUT_MARKER),
                            &file.make_bit_vector_type(width as i64, false),
                        ),
                    };
                    connection_port_names.push(name.clone());
                    connection_expressions.push(Some(logic_expr.to_expr()));
                    connection_logic_refs.push(logic_expr);
                }
                Err(e) => {
                    if !core.verilog_import.as_ref().unwrap().skip_unsupported {
                        panic!("{e}");
                    } else {
                        continue;
                    }
                }
            }
        }

        let mut parameter_port_names = Vec::new();
        let mut parameter_port_expressions = Vec::new();

        for (name, value) in parameters {
            parameter_port_names.push(name);
            // TODO(sherbst) 09/24/2024: support parameter values other than 32-bit
            // integers.
            let literal_str = format!("bits[{}]:{}", 32, value);
            let expr = file
                .make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex)
                .unwrap();
            parameter_port_expressions.push(expr);
        }

        wrapped_module.add_member_instantiation(
            file.make_instantiation(
                core.name.as_str(),
                inst_name,
                &parameter_port_names
                    .iter()
                    .map(|&&s| s)
                    .collect::<Vec<&str>>(),
                &parameter_port_expressions.iter().collect::<Vec<_>>(),
                &connection_port_names
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>(),
                &connection_expressions
                    .iter()
                    .map(|o| o.as_ref())
                    .collect::<Vec<_>>(),
            ),
        );

        let verilog = file.emit();

        let mut ports = IndexMap::new();
        let mut enum_remapping: IndexMap<String, IndexMap<String, IndexMap<String, String>>> =
            IndexMap::new();
        for parser_port in parser_ports[&core.name].iter() {
            match parser_port_to_port(parser_port) {
                Ok((name, io)) => {
                    ports.insert(name.clone(), io.clone());
                    // Enum input ports that are not a packed array require special handling
                    // They need to have casting to be valid Verilog.
                    if let slang_rs::Type::Enum {
                        name: enum_name,
                        packed_dimensions,
                        unpacked_dimensions,
                        ..
                    } = &parser_port.ty
                    {
                        if packed_dimensions.is_empty() && unpacked_dimensions.is_empty() {
                            if let IO::Input(_) = io {
                                enum_remapping
                                    .entry(def_name.to_string())
                                    .or_default()
                                    .entry(inst_name.to_string())
                                    .or_default()
                                    .insert(name.clone(), enum_name.clone());
                            }
                        }
                    }
                }
                Err(e) => {
                    if !core.verilog_import.as_ref().unwrap().skip_unsupported {
                        panic!("{e}");
                    } else {
                        continue;
                    }
                }
            }
        }

        let verilog = crate::enum_type::remap_enum_types(verilog, &enum_remapping);

        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: def_name.to_string(),
                ports,
                enum_ports: IndexMap::new(),
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Usage::EmitDefinitionAndStop,
                generated_verilog: Some(verilog.to_string()),
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                whole_port_tieoffs: IndexMap::new(),
                whole_port_unused: IndexMap::new(),
                verilog_import: None,
                inst_connections: IndexMap::new(),
                reserved_net_definitions: IndexMap::new(),
                adjacency_matrix: HashMap::new(),
                ignore_adjacency: HashSet::new(),
            })),
        }
    }
}
