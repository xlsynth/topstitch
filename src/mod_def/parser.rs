// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use slang_rs::PackageLookup;

use crate::mod_def::parameterize::ImportContext;
use crate::mod_def::parameterize::ParameterBinding;
use crate::mod_def::parser_cfg::ParserConfig;
use crate::mod_def::{ParameterSpec, ParameterType};
use crate::{IO, ModDef, Source, SourceText, Usage, VerilogLibrary};

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

/// Select the override targets that an instance of this module can bind.
fn import_config_parameters(
    compilation: &slang_rs::Compilation,
    top: &str,
    overrides: &[(&str, &str)],
) -> Result<IndexMap<String, ParameterBinding>, slang_rs::Error> {
    let mut selected = Vec::new();
    let mut qualified = Vec::new();
    for &(path, _) in overrides {
        if let Some(name) = override_parameter(top, path).map_err(|message| slang_rs::Error {
            message,
            diagnostics: Vec::new(),
        })? {
            selected.push(name);
            if path != name {
                qualified.push(name);
            }
        }
    }
    let (bindings, _) = import_parameters(compilation, top, &selected, false)?;
    for name in qualified {
        if !bindings.contains_key(name) {
            return Err(slang_rs::Error {
                message: format!(
                    "Cannot bind qualified override {top}.{name}: no overridable value parameter of that name"
                ),
                diagnostics: Vec::new(),
            });
        }
    }
    Ok(bindings)
}

/// Resolve a bare override or a direct parameter on the selected top.
fn override_parameter<'a>(top: &str, path: &'a str) -> Result<Option<&'a str>, String> {
    let unsupported = || format!("Cannot resolve SV parameter override path {path:?} for {top}");
    let mut parts = path.split('.').map(str::trim);
    let mut head = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(unsupported)?;
    let rooted = head == "$root";
    if rooted {
        head = parts
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(unsupported)?;
    } else if parts.clone().next().is_none() {
        super::net_names::check_identifier(path, "parameter name", top)?;
        return Ok(Some(path));
    }
    super::net_names::check_identifier(head, "module name", top)?;
    let parameter = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(unsupported)?;
    if head != top {
        return Ok(None);
    }
    super::net_names::check_identifier(parameter, "parameter name", top)?;
    if parts.next().is_some() {
        return Err(format!(
            "Cannot instantiate {top} with hierarchical override {path}: an SV instance can bind only its own parameters"
        ));
    }
    Ok(Some(parameter))
}

/// Snapshot selected parameters by their elaborated symbol names.
pub(super) fn import_parameters(
    compilation: &slang_rs::Compilation,
    top: &str,
    names: &[&str],
    covers_state: bool,
) -> Result<
    (
        IndexMap<String, ParameterBinding>,
        Option<slang_rs::ModuleParameterState>,
    ),
    slang_rs::Error,
> {
    let error = |message: String| slang_rs::Error {
        message,
        diagnostics: Vec::new(),
    };
    if names.is_empty() {
        return Ok((IndexMap::new(), None));
    }
    let values = compilation
        .parameter_values_for(top, names)
        .map_err(|e| match e {
            slang_rs::LookupError::Extraction(error) => error,
            other => error(other.to_string()),
        })?;
    if !values.is_empty() {
        super::net_names::check_identifier(top, "module name", top).map_err(error)?;
    }
    // When every parameter was requested, this lookup already owns the complete
    // original state. An unrequested default is evaluated only if a stub needs it.
    let state =
        (covers_state && values.len() == names.len()).then(|| slang_rs::ModuleParameterState {
            values: values.clone(),
            types: IndexMap::new(),
        });
    let mut bindings = IndexMap::with_capacity(values.len());
    for (name, parameter) in values {
        super::net_names::check_identifier(&name, "parameter name", top).map_err(error)?;
        let sv_type = match &parameter.ty {
            Ok(slang_rs::Type::Enum {
                name: Some(type_name),
                packed_dimensions,
                unpacked_dimensions,
                ..
            }) if packed_dimensions.is_empty() && unpacked_dimensions.is_empty() => {
                Some(external_enum_type(compilation, type_name).ok_or_else(|| error(format!(
                    "Cannot represent parameter {top}.{name}: enum type {type_name} is not accessible from an SV instance"
                )))?)
            }
            Ok(slang_rs::Type::Enum { .. }) => {
                return Err(error(format!(
                    "Cannot represent parameter {top}.{name}: enum requires an accessible scalar typedef"
                )));
            }
            Err(err) if matches!(parameter.value, slang_rs::ConstantValue::Integer(_)) => {
                return Err(error(format!(
                    "Cannot represent parameter {top}.{name}: {err}"
                )));
            }
            _ => None,
        };
        if let Some(ty) = &sv_type {
            super::net_names::check_qualified_identifier(ty, "parameter cast type", top)
                .map_err(error)?;
        }
        let spec = match parameter.value {
            slang_rs::ConstantValue::Integer(integer) => {
                let ty = if integer.signed {
                    ParameterType::Signed(integer.width)
                } else {
                    ParameterType::Unsigned(integer.width)
                };
                if parameter.is_implicit_string && !integer.has_unknown() && integer.width % 8 == 0
                {
                    let mut bytes = integer.bits.to_bytes_be();
                    let byte_width = integer.width / 8;
                    if bytes.len() < byte_width {
                        let mut padded = vec![0; byte_width - bytes.len()];
                        padded.extend(bytes);
                        bytes = padded;
                    }
                    ParameterSpec::String(String::from_utf8(bytes).map_err(|_| {
                        error(format!(
                            "Cannot represent parameter {top}.{name}: implicit string is not UTF-8"
                        ))
                    })?)
                } else if integer.has_unknown() {
                    let digits = (0..integer.width as u64)
                        .rev()
                        .map(|bit| {
                            if integer.x_mask.bit(bit) {
                                'x'
                            } else if integer.z_mask.bit(bit) {
                                'z'
                            } else if integer.bits.bit(bit) {
                                '1'
                            } else {
                                '0'
                            }
                        })
                        .collect();
                    ParameterSpec::FourState { digits, ty }
                } else {
                    ParameterSpec::Integer {
                        value: integer.bits.into(),
                        ty,
                    }
                }
            }
            slang_rs::ConstantValue::String(bytes) => {
                let text = String::from_utf8(bytes).map_err(|_| {
                    error(format!(
                        "Cannot represent parameter {top}.{name}: string is not UTF-8"
                    ))
                })?;
                ParameterSpec::String(text)
            }
            _ => {
                return Err(error(format!(
                    "Cannot represent parameter {top}.{name}: expected a packed integer or string"
                )));
            }
        };
        bindings.insert(
            name,
            ParameterBinding {
                value: spec,
                sv_type,
                is_implicit_string: parameter.is_implicit_string,
            },
        );
    }
    Ok((bindings, state))
}

/// Resolve a package typedef without requiring a cast until a parameter is overridden.
fn external_enum_type(compilation: &slang_rs::Compilation, name: &str) -> Option<String> {
    let (package, member) = name.split_once("::")?;
    if package.is_empty() || member.is_empty() || member.contains("::") {
        return None;
    }
    compilation.package(package).ok()?.ty(member).ok()?;
    Some(name.to_owned())
}

/// Collect accessible SV types required to cast parameter overrides.
pub(super) fn import_parameter_cast_types(
    compilation: &slang_rs::Compilation,
    top: &str,
) -> Result<IndexMap<String, String>, slang_rs::Error> {
    let parameters = compilation
        .enum_parameters_for(top)
        .map_err(|err| slang_rs::Error {
            message: err.to_string(),
            diagnostics: Vec::new(),
        })?;
    Ok(parameters
        .into_iter()
        .filter_map(|parameter| {
            let slang_rs::Type::Enum {
                name: Some(name),
                packed_dimensions,
                unpacked_dimensions,
                ..
            } = parameter.ty
            else {
                return None;
            };
            if !packed_dimensions.is_empty() || !unpacked_dimensions.is_empty() {
                return None;
            }
            external_enum_type(compilation, &name).map(|ty| (parameter.name, ty))
        })
        .collect())
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
        parameters: IndexMap<String, ParameterBinding>,
        sv_parameter_cast_types: IndexMap<String, String>,
        hierarchy: Option<&slang_rs::Instance>,
    ) -> ModDef {
        let (ports, enum_ports) = import_ports(parser_ports, import.config.skip_unsupported);
        let mod_def = Self::new(mod_def_name);
        if let Some(inst) = hierarchy {
            for child in super::hierarchy::ImportedInstance::children_of(inst) {
                child.instantiate_in(&mod_def);
            }
        }
        {
            let mut core = mod_def.core.write();
            core.ports = ports;
            core.enum_ports = enum_ports;
            core.usage = if hierarchy.is_some() {
                Usage::EmitDefinitionAndDescend
            } else {
                Usage::EmitNothingAndStop
            };
            core.verilog_import = Some(import);
            core.parameters = parameters;
            core.sv_parameter_cast_types = sv_parameter_cast_types;
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
        source: Arc<super::ParserConfigOwned>,
    ) -> Result<Self, slang_rs::LookupError> {
        let ports = compilation.ports_for(name)?;
        let hierarchy = cfg
            .include_hierarchy
            .then(|| compilation.hierarchy_for(name))
            .transpose()?;
        Ok(Self::mod_def_from_parser_ports(
            name,
            &ports,
            Arc::new(ImportContext::new(
                source,
                super::sv_parameter_emission::capture(compilation, [name])?,
            )),
            import_config_parameters(compilation, name, cfg.parameters)?,
            import_parameter_cast_types(compilation, name)?,
            hierarchy.as_ref(),
        ))
    }

    /// Import module ports and optional hierarchy from an existing compilation.
    pub(crate) fn all_from_compilation(
        cfg: &ParserConfig,
        compilation: &slang_rs::Compilation,
        source: Arc<super::ParserConfigOwned>,
    ) -> Result<Vec<Self>, slang_rs::Error> {
        let ports = compilation.ports()?;
        let import = Arc::new(ImportContext::new(
            source,
            super::sv_parameter_emission::capture(compilation, ports.keys().map(String::as_str))?,
        ));
        let hierarchy = cfg
            .include_hierarchy
            .then(|| compilation.hierarchy())
            .transpose()?;
        ports
            .into_iter()
            .map(|(name, ports)| {
                Ok(Self::mod_def_from_parser_ports(
                    &name,
                    &ports,
                    import.clone(),
                    import_config_parameters(compilation, &name, cfg.parameters)?,
                    import_parameter_cast_types(compilation, &name)?,
                    hierarchy.as_ref().and_then(|h| h.get(&name)),
                ))
            })
            .collect()
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
