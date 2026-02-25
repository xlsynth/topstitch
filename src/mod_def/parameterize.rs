// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use num_bigint::{BigInt, Sign};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use crate::{
    IO, ModDef, ModDefCore, ParserConfig, Usage, mod_def::parser_cfg::ParserConfigOwnedKey,
    mod_def::parser_param_to_param, mod_def::parser_port_to_port,
};

#[derive(Clone, Debug)]
struct ParameterizeCacheEntry {
    ports: IndexMap<String, IO>,
    enum_ports: IndexMap<String, String>,
    parameter_types: IndexMap<String, ParameterType>,
}

static PARAMETERIZE_CACHE: LazyLock<RwLock<HashMap<ParserConfigOwnedKey, ParameterizeCacheEntry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// Represents the type of a parameter
#[derive(Clone, Debug)]
pub enum ParameterType {
    Signed(usize),
    Unsigned(usize),
}

impl ParameterType {
    pub fn width(&self) -> usize {
        match self {
            ParameterType::Signed(width) => *width,
            ParameterType::Unsigned(width) => *width,
        }
    }

    pub fn signed(&self) -> bool {
        match self {
            ParameterType::Signed(_) => true,
            ParameterType::Unsigned(_) => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ParameterSpec {
    pub value: BigInt,
    pub ty: ParameterType,
}

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
    pub fn parameterize<T: Into<BigInt> + Clone>(&self, parameters: &[(&str, T)]) -> ModDef {
        let core = self.core.read();
        let bigint_params: Vec<(&str, BigInt)> = parameters
            .iter()
            .map(|(name, val)| (*name, val.clone().into()))
            .collect();

        if core.verilog_import.is_none() {
            panic!(
                "Error parameterizing {}: can only parameterize a module defined in external Verilog sources.",
                core.name
            );
        }

        // Merge parameter overrides with any existing ones
        let mut merged_parameters: IndexMap<String, BigInt> = self
            .core
            .read()
            .parameters
            .iter()
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect();
        for (k, v) in bigint_params.into_iter() {
            merged_parameters.insert(k.to_string(), v);
        }

        // Convert the merged bigint parameters to their systemverilog representation
        // for the parser configuration.
        let parameters_with_string_values: Vec<(String, String)> = merged_parameters
            .iter()
            .map(|(name, value)| {
                // Get the width from the bigint itself, not from the parameter definition.
                // TODO(sherbst) 2025-10-29: Support negative parameter values
                let width = value.bits();
                let str_value = match value.sign() {
                    Sign::Plus | Sign::NoSign => format!("{width}'d{value}"),
                    Sign::Minus => panic!("Negative parameter values not yet supported"),
                };
                (name.clone(), str_value)
            })
            .collect();

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

        let cfg = ParserConfig {
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

        let cache_key = cfg.to_owned_key();
        let cached_entry = PARAMETERIZE_CACHE.read().get(&cache_key).cloned();
        let (ports, enum_ports, parameter_types) = if let Some(entry) = cached_entry {
            (entry.ports, entry.enum_ports, entry.parameter_types)
        } else {
            let parser_result = slang_rs::run_slang(&cfg.to_slang_config()).unwrap();
            let parser_ports = slang_rs::extract_ports_from_value(&parser_result, true);
            let parser_parameters =
                slang_rs::extract_parameter_defs_from_value(&parser_result, true);

            // Build new ports and enum port info based on the parameterized interface
            let mut ports = IndexMap::new();
            let mut enum_ports = IndexMap::new();
            for parser_port in parser_ports[&core.name].iter() {
                match parser_port_to_port(parser_port) {
                    Ok((name, io)) => {
                        ports.insert(name.clone(), io.clone());
                        if let slang_rs::Type::Enum {
                            name: enum_name,
                            packed_dimensions,
                            unpacked_dimensions,
                            ..
                        } = &parser_port.ty
                            && packed_dimensions.is_empty()
                            && unpacked_dimensions.is_empty()
                            && let IO::Input(_) = io
                        {
                            enum_ports.insert(name.clone(), enum_name.clone());
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

            // Parameter types for building literals during emission
            let mut parameter_types = IndexMap::new();
            for parser_param in parser_parameters[&core.name].iter() {
                match parser_param_to_param(parser_param) {
                    Ok((name, param_type)) => {
                        parameter_types.insert(name, param_type);
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

            PARAMETERIZE_CACHE.write().insert(
                cache_key,
                ParameterizeCacheEntry {
                    ports: ports.clone(),
                    enum_ports: enum_ports.clone(),
                    parameter_types: parameter_types.clone(),
                },
            );

            (ports, enum_ports, parameter_types)
        };

        // Build final parameter specs combining values and types (types must exist)
        let mut final_parameter_specs: IndexMap<String, crate::mod_def::ParameterSpec> =
            IndexMap::new();
        for (name, value) in merged_parameters.into_iter() {
            let ty = parameter_types
                .get(&name)
                .unwrap_or_else(|| {
                    panic!(
                        "Parameter type for '{}' not found when parameterizing module '{}'.",
                        name, core.name
                    )
                })
                .clone();
            final_parameter_specs.insert(name, crate::mod_def::ParameterSpec { value, ty });
        }

        ModDef {
            core: Arc::new(RwLock::new(ModDefCore {
                name: core.name.clone(),
                ports,
                enum_ports,
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Usage::EmitNothingAndStop,
                verilog_import: core.verilog_import.clone(),
                parameters: final_parameter_specs,
                mod_inst_connections: IndexMap::new(),
                mod_def_connections: IndexMap::new(),
                mod_def_metadata: HashMap::new(),
                mod_def_port_metadata: HashMap::new(),
                mod_def_intf_metadata: HashMap::new(),
                mod_inst_metadata: HashMap::new(),
                mod_inst_port_metadata: HashMap::new(),
                mod_inst_intf_metadata: HashMap::new(),
                shape: None,
                layer: None,
                inst_placements: IndexMap::new(),
                physical_pins: IndexMap::new(),
                port_max_distances: IndexMap::new(),
                track_definitions: None,
                track_occupancies: None,
                default_connection_max_distance: Some(0),
                specified_net_names: HashSet::new(),
                pipeline_counter: 0..,
            })),
        }
    }
}
