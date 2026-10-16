// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use num_bigint::{BigInt, BigUint, Sign};
use parking_lot::RwLock;
use std::collections::HashMap;

use super::parser::{import_ports, parser_param_to_param};
use super::parser_cfg::ParserConfigOwned;
use crate::{IO, ModDef, Usage};

#[derive(Clone, Debug)]
struct ParameterizeCacheEntry {
    ports: IndexMap<String, IO>,
    enum_ports: IndexMap<String, String>,
    parameter_types: IndexMap<String, ParameterType>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ParameterizeCacheKey {
    module_name: String,
    parameters: Vec<(String, ParameterValue)>,
}

/// Shared by modules from one import and all their parameterized variants.
/// Cached interface data has no references back to the modules or this context.
#[derive(Debug)]
pub(crate) struct ImportContext {
    pub(crate) config: ParserConfigOwned,
    cache: RwLock<HashMap<ParameterizeCacheKey, ParameterizeCacheEntry>>,
}

impl ImportContext {
    pub(crate) fn new(config: ParserConfigOwned) -> Self {
        Self {
            config,
            cache: RwLock::new(HashMap::new()),
        }
    }
}

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
pub enum ParameterSpec {
    Integer { value: BigInt, ty: ParameterType },
    String(String),
}

impl ParameterSpec {
    fn value(&self) -> ParameterValue {
        match self {
            Self::Integer { value, .. } => ParameterValue::Integer(value.clone()),
            Self::String(value) => ParameterValue::String(value.clone()),
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
    /// Overrides supplied through [`ParserConfig::parameters`](super::ParserConfig::parameters)
    /// during the initial import are ignored by `parameterize()`.
    /// Only overrides from earlier `parameterize()` calls are retained. For
    /// example, if `A` defaults to `12` and the `ModDef` is imported with `A=34`,
    /// chaining `parameterize(&[("B", 56)])` and `parameterize(&[("C", 78)])`
    /// overrides only `B=56` and `C=78`. `A` is not explicitly overridden, so it
    /// uses its default of `12`.
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

        // TODO(sherbst) 2026-10-13: Decide how parameterize() should handle overrides
        // in ParserConfig.parameters from the initial import, or disallow them.
        // E.g. A defaults to 12, but the ModDef is imported with A=34. Chaining
        // parameterize() calls with B=56 and then C=78 overrides only B=56 and
        // C=78. A is not explicitly overridden, so it uses its default of 12.
        let mut merged_parameters: IndexMap<String, ParameterValue> = core
            .parameters
            .iter()
            .map(|(k, v)| (k.clone(), v.value()))
            .collect();
        for (name, value) in parameters {
            merged_parameters.insert((*name).to_string(), value.clone().into());
        }

        let cache_key = ParameterizeCacheKey {
            module_name: core.name.clone(),
            parameters: merged_parameters
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
        };

        let cached_entry = import.cache.read().get(&cache_key).cloned();
        let (ports, enum_ports, parameter_types) = if let Some(entry) = cached_entry {
            (entry.ports, entry.enum_ports, entry.parameter_types)
        } else {
            let overrides: Vec<(String, String)> = merged_parameters
                .iter()
                .map(|(name, value)| {
                    let str_value = match value {
                        ParameterValue::Integer(value) => {
                            // Preserve a minimum 32-bit width for untyped SV integers.
                            // TODO(sherbst) 2025-10-29: Support negative parameter values
                            let width = value.bits().max(32);
                            match value.sign() {
                                Sign::Plus | Sign::NoSign => format!("{width}'d{value}"),
                                Sign::Minus => panic!("Negative parameter values not yet supported"),
                            }
                        }
                        ParameterValue::String(value) => {
                            assert!(
                                value.bytes().all(|byte| {
                                    matches!(byte, b' '..=b'~')
                                        && !matches!(byte, b'\'' | b'"' | b'\\')
                                }),
                                "String parameter values must contain only printable ASCII characters and may not contain single quotes, double quotes, or backslashes: {value:?}"
                            );
                            format!("\"{value}\"")
                        }
                    };
                    (name.clone(), str_value)
                })
                .collect();
            let compilation = import.config.compile(&core.name, &overrides).unwrap();
            let parser_ports = compilation.ports().unwrap();
            // TODO: Look up parameters by name and extract type metadata only when needed.
            // Unrelated unsupported types should not require skip_unsupported.
            let parser_parameters = compilation.parameters().unwrap();

            let (ports, enum_ports) =
                import_ports(&parser_ports[&core.name], import.config.skip_unsupported);

            // Only integer overrides need type metadata to size emitted literals.
            let mut parameter_types = IndexMap::new();
            for parser_param in parser_parameters[&core.name].iter() {
                if !matches!(
                    merged_parameters.get(&parser_param.name),
                    Some(ParameterValue::Integer(_))
                ) {
                    continue;
                }
                match parser_param_to_param(parser_param) {
                    Ok((name, param_type)) => {
                        parameter_types.insert(name, param_type);
                    }
                    Err(e) if !import.config.skip_unsupported => panic!("{e}"),
                    Err(_) => {}
                }
            }

            import.cache.write().insert(
                cache_key,
                ParameterizeCacheEntry {
                    ports: ports.clone(),
                    enum_ports: enum_ports.clone(),
                    parameter_types: parameter_types.clone(),
                },
            );

            (ports, enum_ports, parameter_types)
        };

        let mut final_parameter_specs = IndexMap::new();
        for (name, value) in merged_parameters.into_iter() {
            let spec = match value {
                ParameterValue::Integer(value) => {
                    let ty = parameter_types
                        .get(&name)
                        .unwrap_or_else(|| {
                            panic!(
                                "Parameter type for '{}' not found when parameterizing module '{}'.",
                                name, core.name
                            )
                        })
                        .clone();
                    // TODO: Reject overflowing overrides early or use Slang-coerced values.
                    // E.g. `parameter logic [3:0] WIDTH = 4` overridden with 17 is truncated to 1
                    // in Slang. parameterize() succeeds, but emit() panics.
                    ParameterSpec::Integer { value, ty }
                }
                ParameterValue::String(value) => {
                    // TODO: Validate string override names. E.g. a module declares MODE,
                    // but parameterize(&[("MDOE", "FAST")]) misspells the name. Import,
                    // parameterize(), and emit() succeed, but the generated `.MDOE("FAST")`
                    // is invalid SystemVerilog.
                    ParameterSpec::String(value)
                }
            };
            final_parameter_specs.insert(name, spec);
        }

        let mod_def = Self::new(&core.name);
        {
            let mut new_core = mod_def.core.write();
            new_core.ports = ports;
            new_core.enum_ports = enum_ports;
            new_core.usage = Usage::EmitNothingAndStop;
            new_core.verilog_import = core.verilog_import.clone();
            new_core.parameters = final_parameter_specs;
        }
        mod_def
    }
}

#[cfg(test)]
mod tests;
