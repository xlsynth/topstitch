// SPDX-License-Identifier: Apache-2.0

use crate::ParserConfig;
use std::collections::HashMap;
use std::error::Error;
use std::ops::Index;
use std::path::Path;
use std::str::FromStr;

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
            .map(|path| path.to_str().unwrap())
            .collect::<Vec<_>>(),
        ignore_unknown_modules,
        ..Default::default()
    };

    extract_packages_with_config(&cfg)
}

pub fn extract_packages_from_verilog(
    verilog: impl AsRef<str>,
    ignore_unknown_modules: bool,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    let verilog = slang_rs::str2tmpfile(verilog.as_ref()).unwrap();

    let cfg = ParserConfig {
        sources: &[verilog.path().to_str().unwrap()],
        ignore_unknown_modules,
        ..Default::default()
    };

    extract_packages_with_config(&cfg)
}

pub fn extract_packages_with_config(
    cfg: &ParserConfig,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    let pkgs = slang_rs::extract_packages(&cfg.to_slang_config())?;

    Ok(pkgs
        .into_iter()
        .map(|(name, pkg)| {
            (
                name.clone(),
                Package {
                    name: name.clone(),
                    parameters: pkg
                        .parameters
                        .into_iter()
                        .map(|(name, param)| {
                            (
                                name.clone(),
                                Parameter {
                                    name: name.clone(),
                                    value: param.value,
                                },
                            )
                        })
                        .collect(),
                },
            )
        })
        .collect())
}

#[derive(Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub value: String,
}

impl Parameter {
    /// Parse the parameter's `value` into any type that implements [`FromStr`].
    ///
    /// # Type Parameters
    ///
    /// * **`T`** – The numeric type you want (`i64`, `u128`,
    ///   [`num_bigint::BigInt`], [`num_bigint::BigUint`], `f64`, ...). `T` only
    ///   needs to satisfy `T: FromStr`.
    ///
    /// # Errors
    ///
    /// Returns `Err(T::Err)` if `value` is *not* a valid textual
    /// representation for `T`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use num_bigint::{BigInt, BigUint};
    /// use std::convert::TryFrom;
    ///
    /// # use topstitch::Parameter;
    /// let p = Parameter { name: "answer".into(), value: "42".into() };
    ///
    /// // Primitive integer (type inferred)
    /// let n: i32 = p.parse().unwrap();
    ///
    /// // Explicit turbofish when inference can’t decide
    /// let n128 = p.parse::<u128>().unwrap();
    ///
    /// // Big integers
    /// let big:  BigInt  = p.parse().unwrap();
    /// let ubig: BigUint = p.parse().unwrap();
    /// ```
    pub fn parse<T>(&self) -> Result<T, T::Err>
    where
        T: FromStr,
    {
        self.value.parse()
    }

    pub fn calc_type_width(&self) -> Result<usize, Box<dyn Error>> {
        let value = self.parse::<String>()?;
        match slang_rs::parse_type_definition(&value)?.width() {
            Ok(width) => Ok(width),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Package {
    pub name: String,
    pub parameters: HashMap<String, Parameter>,
}

impl Index<&str> for Package {
    type Output = Parameter;

    fn index(&self, key: &str) -> &Self::Output {
        &self.parameters[key]
    }
}

impl Package {
    /// Get a parameter from the package, returning `None` if it doesn't exist.
    pub fn get(&self, key: &str) -> Option<&Parameter> {
        self.parameters.get(key)
    }
}
