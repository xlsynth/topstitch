// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::mod_def::ParameterType;
use crate::mod_def::parameterize::ImportContext;
use crate::mod_def::parser_cfg::ParserConfig;
use crate::{IO, ModDef, Source, Usage, VerilogLibrary};

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
    /// Import one named module from a Verilog file with default parser settings.
    /// The named module is elaborated as the top. For multiple imports from the
    /// same sources or custom parser settings, use [`VerilogLibrary`].
    pub fn from_verilog_file(name: &str, path: impl AsRef<Path>) -> Result<Self, slang_rs::Error> {
        let path = path.as_ref();
        let path = path.to_str().ok_or_else(|| slang_rs::Error {
            message: format!("Verilog path is not valid UTF-8: {}", path.display()),
            diagnostics: Vec::new(),
        })?;
        let config = ParserConfig {
            sources: &[Source::file(path)],
            tops: &[name],
            ..Default::default()
        };
        let library = VerilogLibrary::from_config(&config).map_err(|err| slang_rs::Error {
            message: format!("Cannot import module '{name}' from {path}: {}", err.message),
            ..err
        })?;
        library.module(name)?.ok_or_else(|| slang_rs::Error {
            message: format!("Module '{name}' not found in {path}"),
            diagnostics: Vec::new(),
        })
    }

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

    /// Import one elaborated module without snapshotting unrelated ports or hierarchy.
    pub(crate) fn from_compilation(
        name: &str,
        cfg: &ParserConfig,
        compilation: &slang_rs::Compilation,
    ) -> Result<Option<Self>, slang_rs::Error> {
        let Some(ports) = compilation.ports_for(name)? else {
            return Ok(None);
        };
        let module = Self::mod_def_from_parser_ports(
            name,
            &ports,
            Arc::new(ImportContext::new(cfg.to_owned_config())),
        );
        if cfg.include_hierarchy {
            let stub = module.stub(name);
            if let Some(inst) = compilation.hierarchy_for(name)? {
                crate::mod_def::hierarchy::populate_hierarchy(&stub, &inst);
            }
            Ok(Some(stub))
        } else {
            Ok(Some(module))
        }
    }

    /// Import module ports and optional hierarchy from an existing compilation.
    pub(crate) fn all_from_compilation(
        cfg: &ParserConfig,
        compilation: &slang_rs::Compilation,
    ) -> Result<Vec<Self>, slang_rs::Error> {
        let parser_ports = compilation.ports()?;
        let import = Arc::new(ImportContext::new(cfg.to_owned_config()));

        let mod_defs: Vec<ModDef> = parser_ports
            .iter()
            .map(|(name, ports)| Self::mod_def_from_parser_ports(name, ports, import.clone()))
            .collect();

        if cfg.include_hierarchy {
            let mut mod_defs_with_hierarchy = Vec::new();
            let hierarchy = compilation.hierarchy()?;
            for mod_def in mod_defs.iter() {
                let mod_def_name = mod_def.get_name();
                if let Some(inst) = hierarchy.get(&mod_def_name) {
                    let stubbed_mod_def = mod_def.stub(&mod_def_name);
                    crate::mod_def::hierarchy::populate_hierarchy(&stubbed_mod_def, inst);
                    mod_defs_with_hierarchy.push(stubbed_mod_def);
                }
            }
            Ok(mod_defs_with_hierarchy)
        } else {
            Ok(mod_defs)
        }
    }
}
