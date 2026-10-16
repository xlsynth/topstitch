// SPDX-License-Identifier: Apache-2.0

use crate::{ParserConfig, Source, SourceText};
use std::collections::HashMap;
use std::error::Error;
use std::ops::Index;
use std::path::Path;

use slang_rs::{ConstantValue, Error as SlangError, Type};

pub fn extract_packages_from_verilog_file(
    verilog: &Path,
    ignore_unknown_modules: bool,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    extract_packages_from_verilog_files(&[verilog], ignore_unknown_modules)
}

pub fn extract_packages_from_verilog_files(
    verilog: &[&Path],
    ignore_unknown_modules: bool,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    let cfg = ParserConfig {
        sources: &verilog
            .iter()
            .map(|path| Source::file(path.to_str().unwrap()))
            .collect::<Vec<_>>(),
        ignore_unknown_modules,
        ..Default::default()
    };

    extract_packages_with_config(&cfg)
}

/// Extract package constants and typedefs from SystemVerilog source text.
///
/// Integer constants support checked `TryFrom` and `TryInto` conversions that
/// respect SystemVerilog signedness and reject X/Z bits or values outside the
/// requested type's range.
///
/// # Examples
///
/// ```rust
/// use topstitch::extract_packages_from_verilog;
/// let packages = extract_packages_from_verilog(
///     "package p; localparam int answer = 42; endpackage",
///     false,
/// ).unwrap();
/// let answer = i32::try_from(&packages["p"]["answer"]).unwrap();
/// assert_eq!(answer, 42);
/// let answer: i32 = (&packages["p"]["answer"]).try_into().unwrap();
/// assert_eq!(answer, 42);
/// ```
pub fn extract_packages_from_verilog(
    verilog: impl Into<SourceText>,
    ignore_unknown_modules: bool,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    let cfg = ParserConfig {
        sources: &[Source::text(verilog)],
        ignore_unknown_modules,
        ..Default::default()
    };

    extract_packages_with_config(&cfg)
}

pub fn extract_packages_with_config(
    cfg: &ParserConfig,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    let pkgs = cfg.compile()?.packages()?;

    Ok(pkgs
        .into_iter()
        .map(|(name, pkg)| {
            (
                name,
                Package {
                    name: pkg.name,
                    parameters: pkg.parameters,
                    types: pkg.types,
                },
            )
        })
        .collect())
}

#[derive(Debug, PartialEq)]
pub struct Package {
    pub name: String,
    /// Evaluated parameters and localparams, keyed by name. Typedefs are in
    /// `types`. Integer values support checked `TryFrom` conversions.
    pub parameters: HashMap<String, ConstantValue>,
    /// Resolved typedefs. An unsupported type has its own error so it does not
    /// prevent access to the package's other members.
    pub types: HashMap<String, Result<Type, SlangError>>,
}

impl Index<&str> for Package {
    type Output = ConstantValue;

    fn index(&self, key: &str) -> &Self::Output {
        &self.parameters[key]
    }
}

impl Package {
    /// Get a parameter's value. Returns `None` for a missing name or typedef.
    pub fn get(&self, key: &str) -> Option<&ConstantValue> {
        self.parameters.get(key)
    }
}
