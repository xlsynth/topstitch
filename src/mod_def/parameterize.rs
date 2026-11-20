// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use num_bigint::{BigInt, BigUint, Sign};
use parking_lot::RwLock;
use std::collections::HashMap;
use xlsynth_vast::{VastFile, VastFileType};

use super::hierarchy::ImportedInstance;
use super::parser::{import_parameters, import_ports};
use super::parser_cfg::ParserConfigOwned;
use crate::{IO, ModDef};
use std::sync::Arc;

/// Build the SV literals used for elaboration and parameterized declarations.
pub(super) fn expressions(
    bindings: &IndexMap<String, ParameterBinding>,
) -> IndexMap<String, String> {
    let mut file = VastFile::new(VastFileType::SystemVerilog);
    bindings
        .iter()
        .map(|(n, v)| {
            let expr = v
                .sv_expression(&mut file)
                .unwrap_or_else(|err| panic!("cannot emit parameter {n}: {err}"));
            (n.clone(), file.emit_expression(&expr))
        })
        .collect()
}

/// A specialized real top's input requests and its parameter state for emission checks.
#[derive(Debug)]
pub(crate) struct Specialization {
    requests: IndexMap<String, ParameterBinding>,
    state: std::sync::OnceLock<slang_rs::ModuleParameterState>,
}

impl Specialization {
    /// Read the real source top at its original requests only when a stub needs verification.
    pub(super) fn state(
        &self,
        config: &ParserConfigOwned,
        name: &str,
    ) -> &slang_rs::ModuleParameterState {
        self.state.get_or_init(|| {
            let overrides = expressions(&self.requests).into_iter().collect::<Vec<_>>();
            let compilation = config
                .compile(name, &overrides)
                .unwrap_or_else(|e| panic!("cannot reproduce original module {name}: {e}"));
            compilation
                .parameter_state_for(name)
                .unwrap_or_else(|e| panic!("cannot read parameter state of {name}: {e}"))
        })
    }
}

#[derive(Clone, Debug)]
struct ParameterizeCacheEntry {
    specialization: Arc<Specialization>,
    hierarchy: Arc<[ImportedInstance]>,
    ports: IndexMap<String, IO>,
    enum_ports: IndexMap<String, String>,
    sv_parameter_cast_types: IndexMap<String, String>,
    parameters: IndexMap<String, ParameterBinding>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ParameterizeCacheKey {
    module_name: String,
    parameters: Vec<(String, ParameterBinding)>,
}

/// Shared by modules from one import and all their parameterized variants.
/// Cached interface data has no references back to the modules or this context.
#[derive(Debug)]
pub(crate) struct ImportContext {
    pub(crate) config: Arc<ParserConfigOwned>,
    pub(super) headers: super::sv_parameter_emission::ImportedHeaders,
    cache: RwLock<HashMap<ParameterizeCacheKey, ParameterizeCacheEntry>>,
}

impl ImportContext {
    pub(super) fn new(
        config: Arc<ParserConfigOwned>,
        headers: super::sv_parameter_emission::ImportedHeaders,
    ) -> Self {
        Self {
            config,
            headers,
            cache: RwLock::new(HashMap::new()),
        }
    }
}

// Represents the type of a parameter
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ParameterSpec {
    Integer {
        value: BigInt,
        ty: ParameterType,
    },
    String(String),
    /// Packed four-state bits in most-significant-bit order.
    FourState {
        digits: String,
        ty: ParameterType,
    },
}

/// An instance binding and the external SV type required to emit it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ParameterBinding {
    pub(crate) value: ParameterSpec,
    pub(crate) sv_type: Option<String>,
    pub(crate) is_implicit_string: bool,
}

impl From<ParameterSpec> for ParameterBinding {
    fn from(value: ParameterSpec) -> Self {
        Self {
            is_implicit_string: matches!(value, ParameterSpec::String(_)),
            value,
            sv_type: None,
        }
    }
}

impl ParameterBinding {
    /// Build the typed expression shared by SV emission and re-elaboration.
    pub(super) fn sv_expression(
        &self,
        file: &mut xlsynth_vast::VastFile,
    ) -> Result<xlsynth_vast::Expr, String> {
        let carriage_return =
            matches!(&self.value, ParameterSpec::String(value) if value.contains('\r'));
        let mut value = if carriage_return {
            let ParameterSpec::String(text) = &self.value else {
                unreachable!()
            };
            let bits = num_bigint::BigUint::from_bytes_be(text.as_bytes());
            file.make_literal(
                &format!("bits[{}]:{bits}", text.len() * 8),
                &xlsynth_vast::LiteralFormat::Hex,
            )
            .map_err(|err| err.to_string())?
        } else {
            self.value.sv_expression(file)?
        };
        if matches!(self.value, ParameterSpec::String(_)) && !self.is_implicit_string {
            // Without a cast, an untyped parameter infers a packed integer from the literal.
            let ty = file.make_extern_type("string");
            value = file.make_type_cast(&ty, &value);
        }
        if self.is_implicit_string
            && (carriage_return || !matches!(self.value, ParameterSpec::String(_)))
        {
            // The unselected string branch preserves SV's implicit-string conversion rules.
            let condition = file.make_unsized_zero_literal();
            let string = file.make_string_literal("\0");
            value = file.make_ternary(&condition, &string, &value);
        }
        Ok(match &self.sv_type {
            Some(name) => {
                let ty = file.make_extern_type(name);
                file.make_type_cast(&ty, &value)
            }
            None => value,
        })
    }
}

impl ParameterSpec {
    /// Build the SV expression used for re-elaboration and instance emission.
    pub(super) fn sv_expression(
        &self,
        file: &mut xlsynth_vast::VastFile,
    ) -> Result<xlsynth_vast::Expr, String> {
        match self {
            Self::Integer { value, ty } => {
                let format = if ty.signed() {
                    xlsynth_vast::LiteralFormat::SignedDecimal
                } else {
                    xlsynth_vast::LiteralFormat::Hex
                };
                file.make_literal(&format!("bits[{}]:{value}", ty.width()), &format)
                    .map_err(|err| err.to_string())
            }
            Self::FourState { digits, ty } => {
                // VAST spells Z as ? in a binary literal and provides X as an unsized literal.
                let mut parts = Vec::new();
                let mut rest = digits.as_str();
                while !rest.is_empty() {
                    let unknown = rest.starts_with('x');
                    let end = rest.find(|c| (c == 'x') != unknown).unwrap_or(rest.len());
                    let (run, remaining) = rest.split_at(end);
                    parts.push(if unknown {
                        let count = i32::try_from(run.len()).map_err(|e| e.to_string())?;
                        let x = file.make_unsized_x_literal();
                        let width = file.make_unsized_decimal_literal(count);
                        file.make_width_cast(&width, &x)
                    } else {
                        file.make_binary_pattern(&run.replace('z', "?"))
                            .map_err(|e| e.to_string())?
                    });
                    rest = remaining;
                }
                let expr = file.make_concat(&parts.iter().collect::<Vec<_>>());
                Ok(if ty.signed() {
                    file.make_function_call("$signed", &[&expr])
                } else {
                    expr
                })
            }
            Self::String(value) => Ok(file.make_string_literal(value)),
        }
    }

    /// Convert a caller's override into the self-contained literal used for elaboration.
    fn from_override(value: ParameterValue) -> Self {
        match value {
            ParameterValue::Integer(value) => {
                assert!(
                    value.sign() != Sign::Minus,
                    "Negative parameter values not yet supported"
                );
                let width =
                    usize::try_from(value.bits().max(32)).expect("integer width exceeds usize");
                Self::Integer {
                    value,
                    ty: ParameterType::Unsigned(width),
                }
            }
            ParameterValue::String(value) => {
                assert!(
                    value
                        .bytes()
                        .all(|b| matches!(b, b' '..=b'~') && !matches!(b, b'\'' | b'"' | b'\\')),
                    "String parameter values must contain only printable ASCII characters and may not contain single quotes, double quotes, or backslashes: {value:?}"
                );
                Self::String(value)
            }
        }
    }
}

/// An integer or string value supplied to [`ModDef::parameterize`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ParameterValue {
    /// An arbitrary-precision integer parameter value.
    Integer(BigInt),
    /// A printable-ASCII string parameter value without quotes or backslashes.
    String(String),
}

macro_rules! impl_parameter_value {
    ($variant:ident: $($type:ty),+ $(,)?) => {
        $(
            impl From<$type> for ParameterValue {
                fn from(value: $type) -> Self {
                    Self::$variant(value.into())
                }
            }
        )+
    };
}

impl_parameter_value!(Integer:
    bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, BigInt, BigUint,
);
impl_parameter_value!(String: String, &str, &String);

impl ModDef {
    /// Returns a new variant of an imported module with the given parameter
    /// overrides and the resulting port widths. Existing overrides from earlier
    /// calls are retained unless replaced by name. Instantiating the returned
    /// module emits these overrides; use [`ModDef::wrap`] to create a wrapper.
    ///
    /// Supply the parameter's symbol name without SystemVerilog escape syntax.
    /// Enum overrides require a scalar typedef accessible from a package.
    ///
    /// Overrides supplied through [`ParserConfig::parameters`](super::ParserConfig::parameters)
    /// are retained at their elaborated values. A later override replaces only
    /// the named binding. Unspecified defaults are elaborated again.
    ///
    /// With [`ParserConfig::include_hierarchy`](super::ParserConfig::include_hierarchy),
    /// the variant is a fresh editable hierarchy view reconstructed from the source.
    /// It inherits the caller's [usage](crate::Usage). Hierarchy edits belong to the
    /// particular view on which they were made.
    ///
    /// The original import and its variants share a cache of parsed interfaces.
    /// Source files must remain unchanged while using that import; import again
    /// to pick up edits. Separate imports have separate caches.
    ///
    /// Parameter values can be integers or strings. String values must contain
    /// only printable ASCII characters, including spaces, and cannot contain
    /// single quotes, double quotes, or backslashes. Mixed-type overrides can
    /// be supplied together as [`ParameterValue`], for example
    /// `parameterize::<ParameterValue>(&[("WIDTH", 32.into()),
    /// ("MODE", "FAST".into())])`.
    pub fn parameterize<T: Into<ParameterValue> + Clone>(
        &self,
        parameters: &[(&str, T)],
    ) -> ModDef {
        let core = self.core.read();
        let import = core.verilog_import.as_deref().unwrap_or_else(|| {
            panic!(
                "Error parameterizing {}: can only parameterize a module defined in external Verilog sources.",
                core.name
            );
        });

        let mut bindings = core
            .specialization
            .as_ref()
            .map_or(&core.parameters, |s| &s.requests)
            .clone();
        for (name, value) in parameters {
            super::net_names::check_identifier(name, "parameter name", &core.name)
                .unwrap_or_else(|e| panic!("{e}"));
            let sv_type = core.sv_parameter_cast_types.get(*name).cloned();
            if let Some(ty) = &sv_type {
                super::net_names::check_qualified_identifier(ty, "parameter cast type", &core.name)
                    .unwrap_or_else(|e| panic!("{e}"));
            }
            let mut binding =
                ParameterBinding::from(ParameterSpec::from_override(value.clone().into()));
            binding.sv_type = sv_type;
            bindings.insert((*name).to_owned(), binding);
        }
        let key = ParameterizeCacheKey {
            module_name: core.name.clone(),
            parameters: bindings
                .iter()
                .map(|(name, binding)| (name.clone(), binding.clone()))
                .collect(),
        };
        let cached = import.cache.read().get(&key).cloned();
        let entry = if let Some(entry) = cached {
            entry
        } else {
            let overrides = expressions(&bindings).into_iter().collect::<Vec<_>>();
            let compilation = import.config.compile(&core.name, &overrides).unwrap();
            let parser_ports = compilation.ports_for(&core.name).unwrap();
            let (ports, enum_ports) = import_ports(&parser_ports, import.config.skip_unsupported);
            let names: Vec<_> = overrides.iter().map(|(name, _)| name.as_str()).collect();
            let sv_parameter_cast_types =
                super::parser::import_parameter_cast_types(&compilation, &core.name)
                    .unwrap_or_else(|e| panic!("{e}"));
            let (parameters, state) = import_parameters(
                &compilation,
                &core.name,
                &names,
                import.headers.covers_state(&core.name, names.len()),
            )
            .unwrap_or_else(|e| panic!("{e}"));
            for name in names {
                assert!(
                    parameters.contains_key(name),
                    "Unknown parameter {}.{name}",
                    core.name
                );
            }
            let hierarchy = if import.config.include_hierarchy {
                ImportedInstance::children_of(&compilation.hierarchy_for(&core.name).unwrap())
            } else {
                Vec::new()
            };
            let entry = ParameterizeCacheEntry {
                specialization: Arc::new(Specialization {
                    requests: bindings,
                    state: state.map(std::sync::OnceLock::from).unwrap_or_default(),
                }),
                hierarchy: hierarchy.into(),
                ports,
                enum_ports,
                sv_parameter_cast_types,
                parameters,
            };
            import.cache.write().insert(key, entry.clone());
            entry
        };

        let mod_def = Self::new(&core.name);
        for child in entry.hierarchy.iter() {
            child.instantiate_in(&mod_def);
        }
        {
            let mut new_core = mod_def.core.write();
            new_core.ports = entry.ports;
            new_core.enum_ports = entry.enum_ports;
            new_core.sv_parameter_cast_types = entry.sv_parameter_cast_types;
            new_core.usage = core.usage.clone();
            new_core.verilog_import = core.verilog_import.clone();
            new_core.parameters = entry.parameters;
            new_core.specialization = Some(entry.specialization);
        }
        mod_def
    }
}

#[cfg(test)]
mod tests;
