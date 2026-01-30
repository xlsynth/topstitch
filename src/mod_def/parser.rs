// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::mod_def::ParameterType;
use crate::mod_def::dtypes::VerilogImport;
use crate::mod_def::parser_cfg::ParserConfig;
use crate::{IO, ModDef, ModDefCore, Usage};

pub(crate) fn parser_port_to_port(parser_port: &slang_rs::Port) -> Result<(String, IO), String> {
    let size = parser_port.ty.width().unwrap();
    let port_name = parser_port.name.clone();

    match parser_port.dir {
        slang_rs::PortDir::Input => Ok((port_name, IO::Input(size))),
        slang_rs::PortDir::Output => Ok((port_name, IO::Output(size))),
        slang_rs::PortDir::InOut => Ok((port_name, IO::InOut(size))),
    }
}

pub(crate) fn parser_param_to_param(
    parser_param: &slang_rs::ParameterDef,
) -> Result<(String, ParameterType), String> {
    match &parser_param.ty {
        slang_rs::Type::Logic {
            signed,
            unpacked_dimensions,
            packed_dimensions,
        } => {
            if !unpacked_dimensions.is_empty() {
                return Err(
                    "Parameters with unpacked dimensions are not currently supported".to_string(),
                );
            }
            let width = parser_param.ty.width()?;
            // Packed arrays of signed integers should be treated as unsigned
            // TODO(zhemao): Proper support for packed array types
            let is_array = packed_dimensions.len() > 1;
            let param_type = if *signed && !is_array {
                ParameterType::Signed(width)
            } else {
                ParameterType::Unsigned(width)
            };
            Ok((parser_param.name.clone(), param_type))
        }
        _ => {
            // TODO(zhemao): Proper support for struct, union, and enum types
            // For now, we can just treat them as flat unsigned integers
            let width = parser_param.ty.width()?;
            Ok((parser_param.name.clone(), ParameterType::Unsigned(width)))
        }
    }
}

impl ModDef {
    fn mod_def_from_parser_ports(
        mod_def_name: &str,
        parser_ports: &[slang_rs::Port],
        cfg: &ParserConfig,
    ) -> ModDef {
        let mut ports = IndexMap::new();
        let mut enum_ports = IndexMap::new();

        for parser_port in parser_ports {
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
                        && packed_dimensions.is_empty()
                        && unpacked_dimensions.is_empty()
                        && let IO::Input(_) = io
                    {
                        enum_ports.insert(name.clone(), enum_name.clone());
                    }
                }
                Err(e) => {
                    if !cfg.skip_unsupported {
                        panic!("{e}");
                    } else {
                        continue;
                    }
                }
            }
        }

        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: mod_def_name.to_string(),
                ports,
                enum_ports,
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Usage::EmitNothingAndStop,
                verilog_import: Some(VerilogImport {
                    sources: cfg.sources.iter().map(|s| s.to_string()).collect(),
                    incdirs: cfg.incdirs.iter().map(|s| s.to_string()).collect(),
                    defines: cfg
                        .defines
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    skip_unsupported: cfg.skip_unsupported,
                    ignore_unknown_modules: cfg.ignore_unknown_modules,
                }),
                parameters: IndexMap::new(),
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

    /// Creates a new module definition from a Verilog file. The `name`
    /// parameter is the name of the module to extract from the Verilog file,
    /// and `verilog` is the path to the Verilog file. If
    /// `ignore_unknown_modules` is `true`, do not panic if the Verilog file
    /// instantiates modules whose definitions cannot be found. This is often
    /// useful because only the interface of module `name` needs to be
    /// extracted; its contents do not need to be interpreted. If
    /// `skip_unsupported` is `true`, do not panic if the interface of module
    /// `name` contains unsupported features; simply skip these ports. This is
    /// occasionally useful when prototyping.
    pub fn from_verilog_file(
        name: impl AsRef<str>,
        verilog: &Path,
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        Self::from_verilog_files(name, &[verilog], ignore_unknown_modules, skip_unsupported)
    }

    /// Creates a new module definition from a list of Verilog files. The `name`
    /// parameter is the name of the module to extract from the Verilog sources,
    /// and `verilog` is an array of paths of Verilog sources. If
    /// `ignore_unknown_modules` is `true`, do not panic if the Verilog file
    /// instantiates modules whose definitions cannot be found. This is often
    /// useful because only the interface of module `name` needs to be
    /// extracted; its contents do not need to be interpreted. If
    /// `skip_unsupported` is `true`, do not panic if the interface of module
    /// `name` contains unsupported features; simply skip these ports. This is
    /// occasionally useful when prototyping.
    pub fn from_verilog_files(
        name: impl AsRef<str>,
        verilog: &[&Path],
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        let cfg = ParserConfig {
            sources: &verilog
                .iter()
                .map(|path| path.to_str().unwrap())
                .collect::<Vec<_>>(),
            ignore_unknown_modules,
            skip_unsupported,
            ..Default::default()
        };

        Self::from_verilog_with_config(name, &cfg)
    }

    /// Creates a new module definition from Verilog source code. The `name`
    /// parameter is the name of the module to extract from the Verilog code,
    /// and `verilog` is a string containing Verilog code. If
    /// `ignore_unknown_modules` is `true`, do not panic if the Verilog file
    /// instantiates modules whose definitions cannot be found. This is often
    /// useful because only the interface of module `name` needs to be
    /// extracted; its contents do not need to be interpreted. If
    /// `skip_unsupported` is `true`, do not panic if the interface of module
    /// `name` contains unsupported features; simply skip these ports. This is
    /// occasionally useful when prototyping.
    pub fn from_verilog(
        name: impl AsRef<str>,
        verilog: impl AsRef<str>,
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        let verilog = slang_rs::str2tmpfile(verilog.as_ref()).unwrap();

        let cfg = ParserConfig {
            sources: &[verilog.path().to_str().unwrap()],
            ignore_unknown_modules,
            skip_unsupported,
            ..Default::default()
        };

        Self::from_verilog_with_config(name, &cfg)
    }

    /// Creates a new module definition from Verilog sources. The `name`
    /// parameter is the name of the module to extract from Verilog code, and
    /// `cfg` is a `ParserConfig` struct specifying source files, include
    /// directories, etc.
    pub fn from_verilog_with_config(name: impl AsRef<str>, cfg: &ParserConfig) -> Self {
        let value = slang_rs::run_slang(&cfg.to_slang_config()).unwrap();
        let parser_ports = slang_rs::extract_ports_from_value(&value, cfg.skip_unsupported);

        let selected_ports = parser_ports.get(name.as_ref()).unwrap_or_else(|| {
            panic!(
                "Module definition '{}' not found in Verilog sources.",
                name.as_ref()
            )
        });

        let mod_def = Self::mod_def_from_parser_ports(name.as_ref(), selected_ports, cfg);

        if cfg.include_hierarchy {
            let mod_def_with_hierarchy = mod_def.stub(&name);
            let hierarchy = slang_rs::extract_hierarchy_from_value(&value);
            if let Some(inst) = hierarchy.get(name.as_ref()) {
                crate::mod_def::hierarchy::populate_hierarchy(&mod_def_with_hierarchy, inst);
            }
            mod_def_with_hierarchy
        } else {
            mod_def
        }
    }

    pub fn all_from_verilog_with_config(cfg: &ParserConfig) -> Vec<Self> {
        let value = slang_rs::run_slang(&cfg.to_slang_config()).unwrap();
        let parser_ports = slang_rs::extract_ports_from_value(&value, cfg.skip_unsupported);

        let mod_defs: Vec<ModDef> = parser_ports
            .keys()
            .map(|name| Self::mod_def_from_parser_ports(name, &parser_ports[name], cfg))
            .collect();

        if cfg.include_hierarchy {
            let mut mod_defs_with_hierarchy = Vec::new();
            let hierarchy = slang_rs::extract_hierarchy_from_value(&value);
            for mod_def in mod_defs.iter() {
                let mod_def_name = mod_def.get_name();
                if let Some(inst) = hierarchy.get(&mod_def_name) {
                    let stubbed_mod_def = mod_def.stub(&mod_def_name);
                    crate::mod_def::hierarchy::populate_hierarchy(&stubbed_mod_def, inst);
                    mod_defs_with_hierarchy.push(stubbed_mod_def);
                }
            }
            mod_defs_with_hierarchy
        } else {
            mod_defs
        }
    }
}
