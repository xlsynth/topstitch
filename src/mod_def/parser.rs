// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::mod_def::ParameterType;
use crate::mod_def::parameterize::ImportContext;
use crate::mod_def::parser_cfg::ParserConfig;
use crate::{IO, ModDef, Source, SourceText, Usage};

fn parser_port_to_port(parser_port: &slang_rs::Port) -> Result<(String, IO), String> {
    if parser_port.name.is_empty() {
        return Err("Unnamed Verilog ports are not supported by topstitch".to_owned());
    }
    let size = parser_port.ty.width().map_err(|_| {
        format!(
            "Port '{}' has an unsupported unpacked type",
            parser_port.name
        )
    })?;
    let port_name = parser_port.name.clone();

    match parser_port.dir {
        slang_rs::PortDir::Input => Ok((port_name, IO::Input(size))),
        slang_rs::PortDir::Output => Ok((port_name, IO::Output(size))),
        slang_rs::PortDir::InOut => Ok((port_name, IO::InOut(size))),
    }
}

pub(super) fn parser_param_to_param(
    parser_param: &slang_rs::Parameter,
) -> Result<(String, ParameterType), String> {
    let width = parser_param.ty.width().map_err(|_| {
        format!(
            "Parameter '{}' has an unsupported unpacked type",
            parser_param.name
        )
    })?;
    let signed = match &parser_param.ty {
        slang_rs::Type::Integral { signed, .. }
        | slang_rs::Type::Struct { signed, .. }
        | slang_rs::Type::Union { signed, .. }
        | slang_rs::Type::Enum { signed, .. } => *signed,
    };
    let ty = if signed {
        ParameterType::Signed(width)
    } else {
        ParameterType::Unsigned(width)
    };
    Ok((parser_param.name.clone(), ty))
}

pub(super) fn import_ports(
    parser_ports: &[slang_rs::Port],
    skip_unsupported: bool,
) -> (IndexMap<String, IO>, IndexMap<String, String>) {
    let mut ports = IndexMap::new();
    let mut enum_ports = IndexMap::new();
    for parser_port in parser_ports {
        match parser_port_to_port(parser_port) {
            Ok((name, io)) => {
                // Track which input ports have named scalar enum types so their
                // connections can be emitted with the required explicit casts.
                // TODO: Handle anonymous enum inputs, e.g. `input enum logic [1:0] {A, B} x`.
                // Import and emission succeed, but `.x(x)` connects a bit vector to an enum
                // without a cast, producing invalid SystemVerilog.
                if let slang_rs::Type::Enum {
                    name: Some(enum_name),
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
                ports.insert(name, io);
            }
            Err(e) if !skip_unsupported => panic!("{e}"),
            Err(_) => {}
        }
    }
    (ports, enum_ports)
}

impl ModDef {
    fn mod_def_from_parser_ports(
        mod_def_name: &str,
        parser_ports: &[slang_rs::Port],
        import: Arc<ImportContext>,
    ) -> ModDef {
        let (ports, enum_ports) = import_ports(parser_ports, import.config.skip_unsupported);
        let mod_def = Self::new(mod_def_name);
        {
            let mut core = mod_def.core.write();
            core.ports = ports;
            core.enum_ports = enum_ports;
            core.usage = Usage::EmitNothingAndStop;
            core.verilog_import = Some(import);
        }
        mod_def
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
                .map(|path| Source::file(path.to_str().unwrap()))
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
        verilog: impl Into<SourceText>,
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        let cfg = ParserConfig {
            sources: &[Source::text(verilog)],
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
        let compilation = cfg.compile().unwrap();
        let parser_ports = compilation.ports().unwrap();

        let selected_ports = parser_ports.get(name.as_ref()).unwrap_or_else(|| {
            panic!(
                "Module definition '{}' not found in Verilog sources.",
                name.as_ref()
            )
        });

        let mod_def = Self::mod_def_from_parser_ports(
            name.as_ref(),
            selected_ports,
            Arc::new(ImportContext::new(cfg.to_owned_config())),
        );

        if cfg.include_hierarchy {
            let mod_def_with_hierarchy = mod_def.stub(&name);
            let hierarchy = compilation.hierarchy().unwrap();
            if let Some(inst) = hierarchy.get(name.as_ref()) {
                crate::mod_def::hierarchy::populate_hierarchy(&mod_def_with_hierarchy, inst);
            }
            mod_def_with_hierarchy
        } else {
            mod_def
        }
    }

    pub fn all_from_verilog_with_config(cfg: &ParserConfig) -> Vec<Self> {
        let compilation = cfg.compile().unwrap();
        let parser_ports = compilation.ports().unwrap();
        let import = Arc::new(ImportContext::new(cfg.to_owned_config()));

        let mod_defs: Vec<ModDef> = parser_ports
            .iter()
            .map(|(name, ports)| Self::mod_def_from_parser_ports(name, ports, import.clone()))
            .collect();

        if cfg.include_hierarchy {
            let mut mod_defs_with_hierarchy = Vec::new();
            let hierarchy = compilation.hierarchy().unwrap();
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
