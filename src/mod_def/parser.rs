// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use indexmap::IndexMap;
use slang_rs::{self, extract_ports, str2tmpfile, SlangConfig};

use crate::mod_def::dtypes::VerilogImport;
use crate::{ModDef, ModDefCore, Usage, IO};

pub(crate) fn parser_port_to_port(parser_port: &slang_rs::Port) -> Result<(String, IO), String> {
    let size = parser_port.ty.width().unwrap();
    let port_name = parser_port.name.clone();

    match parser_port.dir {
        slang_rs::PortDir::Input => Ok((port_name, IO::Input(size))),
        slang_rs::PortDir::Output => Ok((port_name, IO::Output(size))),
        slang_rs::PortDir::InOut => Ok((port_name, IO::InOut(size))),
    }
}

impl ModDef {
    fn mod_def_from_parser_ports(
        mod_def_name: &str,
        parser_ports: &[slang_rs::Port],
        cfg: &SlangConfig,
        skip_unsupported: bool,
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
                    {
                        if packed_dimensions.is_empty() && unpacked_dimensions.is_empty() {
                            if let IO::Input(_) = io {
                                enum_ports.insert(name.clone(), enum_name.clone());
                            }
                        }
                    }
                }
                Err(e) => {
                    if !skip_unsupported {
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
                generated_verilog: None,
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                whole_port_tieoffs: IndexMap::new(),
                whole_port_unused: IndexMap::new(),
                verilog_import: Some(VerilogImport {
                    sources: cfg.sources.iter().map(|s| s.to_string()).collect(),
                    incdirs: cfg.incdirs.iter().map(|s| s.to_string()).collect(),
                    defines: cfg
                        .defines
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    skip_unsupported,
                    ignore_unknown_modules: cfg.ignore_unknown_modules,
                }),
                inst_connections: IndexMap::new(),
                reserved_net_definitions: IndexMap::new(),
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
        let cfg = SlangConfig {
            sources: &verilog
                .iter()
                .map(|path| path.to_str().unwrap())
                .collect::<Vec<_>>(),
            ignore_unknown_modules,
            ..Default::default()
        };

        Self::from_verilog_using_slang(name, &cfg, skip_unsupported)
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
        let verilog = str2tmpfile(verilog.as_ref()).unwrap();

        let cfg = SlangConfig {
            sources: &[verilog.path().to_str().unwrap()],
            ignore_unknown_modules,
            ..Default::default()
        };

        Self::from_verilog_using_slang(name, &cfg, skip_unsupported)
    }

    /// Creates a new module definition from Verilog sources. The `name`
    /// parameter is the name of the module to extract from Verilog code, and
    /// `cfg` is a `SlangConfig` struct specifying source files, include
    /// directories, etc. If `skip_unsupported` is `true`, do not panic if the
    /// interface of module `name` contains unsupported features; simply skip
    /// these ports. This is occasionally useful when prototyping.
    pub fn from_verilog_using_slang(
        name: impl AsRef<str>,
        cfg: &SlangConfig,
        skip_unsupported: bool,
    ) -> Self {
        let parser_ports = extract_ports(cfg, skip_unsupported);

        let selected = parser_ports.get(name.as_ref()).unwrap_or_else(|| {
            panic!(
                "Module definition '{}' not found in Verilog sources.",
                name.as_ref()
            )
        });

        Self::mod_def_from_parser_ports(name.as_ref(), selected, cfg, skip_unsupported)
    }

    pub fn all_from_verilog_using_slang(cfg: &SlangConfig, skip_unsupported: bool) -> Vec<Self> {
        let parser_ports = extract_ports(cfg, skip_unsupported);
        parser_ports
            .keys()
            .map(|name| {
                Self::mod_def_from_parser_ports(name, &parser_ports[name], cfg, skip_unsupported)
            })
            .collect()
    }
}
