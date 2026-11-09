// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::mod_def::ParameterType;
use crate::mod_def::parameterize::ImportContext;
use crate::mod_def::parser_cfg::ParserConfig;
use crate::port::{PortDecl, PortType};
use crate::{IO, ModDef, Source, SourceText, Usage, VerilogLibrary};

/// Resolves the wiring interface and type information for one imported port.
fn parser_port_to_port(parser_port: slang_rs::Port) -> Result<(String, PortDecl), String> {
    if parser_port.name.is_empty() {
        return Err("Unnamed Verilog ports are not supported by topstitch".to_owned());
    }
    let size = parser_port.ty.width().map_err(|_| {
        format!(
            "Port '{}' has an unsupported unpacked type",
            parser_port.name
        )
    })?;
    let io = match parser_port.dir {
        slang_rs::PortDir::Input => IO::Input(size),
        slang_rs::PortDir::Output => IO::Output(size),
        slang_rs::PortDir::InOut => IO::InOut(size),
    };
    let ty = match parser_port.ty {
        ty if crate::packed::has_packed_layout(&ty) => {
            PortType::Packed(crate::packed::Shape::from_type(&ty, str::to_owned))
        }
        // TODO: Handle anonymous enum inputs, e.g. `input enum logic [1:0] {A, B} x`.
        // Import and emission succeed, but `.x(x)` connects a bit vector to an enum
        // without a cast, producing invalid SystemVerilog.
        slang_rs::Type::Enum {
            name: Some(name),
            packed_dimensions,
            unpacked_dimensions,
            ..
        } if matches!(io, IO::Input(_))
            && packed_dimensions.is_empty()
            && unpacked_dimensions.is_empty() =>
        {
            PortType::EnumInputCast(name)
        }
        _ => PortType::Bits,
    };
    Ok((parser_port.name, PortDecl { io, ty }))
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

/// Extracts port declarations from one module elaboration.
pub(super) fn import_ports(
    parser_ports: slang_rs::PortList,
    skip_unsupported: bool,
) -> IndexMap<String, PortDecl> {
    let mut result = IndexMap::new();
    for parser_port in parser_ports {
        match parser_port_to_port(parser_port) {
            Ok((name, decl)) => {
                result.insert(name, decl);
            }
            Err(e) if !skip_unsupported => panic!("{e}"),
            Err(_) => {}
        }
    }
    result
}

impl ModDef {
    fn mod_def_from_parser_ports(
        mod_def_name: &str,
        parser_ports: slang_rs::PortList,
        import: Arc<ImportContext>,
    ) -> ModDef {
        let ports = import_ports(parser_ports, import.config.skip_unsupported);
        let mod_def = Self::new(mod_def_name);
        {
            let mut core = mod_def.core.write();
            core.ports = ports;
            core.usage = Usage::EmitNothingAndStop;
            core.verilog_import = Some(import);
        }
        mod_def
    }

    /// Import a named module from one Verilog file with default parser settings.
    pub fn from_verilog_file(name: &str, path: impl AsRef<Path>) -> Result<Self, slang_rs::Error> {
        Self::from_verilog_files(name, [path])
    }

    /// Import a named module from Verilog files in the supplied order. Each
    /// file retains its path for diagnostics and relative includes.
    pub fn from_verilog_files(
        name: &str,
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Self, slang_rs::Error> {
        let sources = paths
            .into_iter()
            .map(|path| {
                let path = path.as_ref();
                path.to_str()
                    .map(Source::file)
                    .ok_or_else(|| slang_rs::Error {
                        message: format!("Verilog path is not valid UTF-8: {}", path.display()),
                        diagnostics: Vec::new(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::import_verilog_top(name, &sources)
    }

    /// Import a named module from in-memory Verilog, elaborating it as the top
    /// with default parser settings. Use [`VerilogLibrary`] for custom settings
    /// or to import several modules or packages from one compilation.
    pub fn from_verilog(name: &str, text: impl Into<SourceText>) -> Result<Self, slang_rs::Error> {
        Self::import_verilog_top(name, &[Source::text(text)])
    }

    /// Import one elaborated module without snapshotting unrelated ports or hierarchy.
    pub(crate) fn from_compilation(
        name: &str,
        cfg: &ParserConfig,
        compilation: &slang_rs::Compilation,
    ) -> Result<Self, slang_rs::LookupError> {
        let ports = compilation.ports_for(name)?;
        let module = Self::mod_def_from_parser_ports(
            name,
            ports,
            Arc::new(ImportContext::new(cfg.to_owned_config())),
        );
        if cfg.include_hierarchy {
            let stub = module.stub(name);
            let inst = compilation.hierarchy_for(name)?;
            crate::mod_def::hierarchy::populate_hierarchy(&stub, &inst);
            Ok(stub)
        } else {
            Ok(module)
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
            .into_iter()
            .map(|(name, ports)| Self::mod_def_from_parser_ports(&name, ports, import.clone()))
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

    /// Compile the inputs with the named module as the sole top.
    fn import_verilog_top(name: &str, sources: &[Source]) -> Result<Self, slang_rs::Error> {
        let config = ParserConfig {
            sources,
            tops: &[name],
            ..Default::default()
        };
        VerilogLibrary::from_config(&config)
            .map_err(slang_rs::LookupError::from)
            .and_then(|library| library.module(name))
            .map_err(|err| {
                let paths: Vec<_> = sources
                    .iter()
                    .filter_map(|source| match source {
                        Source::File(path) => Some(path.as_str()),
                        _ => None,
                    })
                    .collect();
                let origin = if paths.is_empty() {
                    String::new()
                } else {
                    format!(" from {}", paths.join(", "))
                };
                match err {
                    slang_rs::LookupError::Extraction(err) => slang_rs::Error {
                        message: format!("Cannot import module '{name}'{origin}: {}", err.message),
                        ..err
                    },
                    slang_rs::LookupError::NotFound { .. } => slang_rs::Error {
                        message: format!("Module '{name}' not found{origin}"),
                        diagnostics: Vec::new(),
                    },
                }
            })
    }
}
