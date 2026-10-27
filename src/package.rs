// SPDX-License-Identifier: Apache-2.0

use crate::{PackedField, ParserConfig, Source, SourceText};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::ops::Index;
use std::path::Path;

use slang_rs::{ConstantValue, Error as SlangError, IntegerValue, Type};

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
    /// Reads a parameter or localparam as the requested Rust type. Panics if
    /// the name is missing or the value cannot be converted. Use `pkg.parameters.get(name)`
    /// to inspect the raw value or handle an absent parameter.
    pub fn parameter<'a, T>(&'a self, name: &str) -> T
    where
        T: TryFrom<&'a ConstantValue>,
        T::Error: Display,
    {
        self.parameters
            .get(name)
            .unwrap_or_else(|| panic!("Missing parameter {}::{name}", self.name))
            .try_into()
            .unwrap_or_else(|err| panic!("Cannot convert {}::{name}: {err}", self.name))
    }

    /// Reads a member of a named enum typedef as the requested Rust type.
    /// Panics for a missing type or member, a non-enum, or a failed conversion.
    pub fn enum_value<'a, T>(&'a self, name: &str, member: &str) -> T
    where
        T: TryFrom<&'a IntegerValue>,
        T::Error: Display,
    {
        let variants = match self.typedef_entry(name).1 {
            Type::Enum {
                variants,
                packed_dimensions,
                unpacked_dimensions,
                ..
            } if packed_dimensions.is_empty() && unpacked_dimensions.is_empty() => variants,
            _ => panic!("{}::{name} is not an enum typedef", self.name),
        };
        let entry = variants
            .iter()
            .find(|v| v.name == member)
            .unwrap_or_else(|| panic!("Missing enum member {}::{name}::{member}", self.name));
        (&entry.value)
            .try_into()
            .unwrap_or_else(|err| panic!("Cannot convert {}::{name}::{member}: {err}", self.name))
    }

    /// Returns the bit width of a numeric typedef. Panics if the type is missing
    /// or does not have a packed width.
    pub fn type_width(&self, name: &str) -> usize {
        self.typedef_entry(name)
            .1
            .width()
            .unwrap_or_else(|err| panic!("Cannot size {}::{name}: {err}", self.name))
    }

    /// Looks up a single packed struct for named field selection. Panics for
    /// missing types, arrays of structs, and other kinds of typedef.
    ///
    /// ```
    /// use topstitch::{extract_packages_from_verilog, ModDef, IO};
    /// let packages = extract_packages_from_verilog(
    ///     "package display; typedef struct packed { logic [7:0] r, g, b; } rgb; endpackage",
    ///     false,
    /// ).unwrap();
    /// let rgb = packages["display"].packed_struct("rgb");
    /// let green_bits = rgb.field("g").range_in_root();
    /// assert_eq!((green_bits.msb(), green_bits.lsb()), (15, 8));
    /// let module = ModDef::new("screen");
    /// let pixels = module.add_port("pixels", IO::Input(32));
    /// let green = rgb.on(&pixels.slice(27, 4)).field("g");
    /// assert_eq!(green.bit(0), pixels.bit(12));
    /// ```
    pub fn packed_struct(&self, name: &str) -> PackedField<'_> {
        let (name, ty) = self.typedef_entry(name);
        PackedField::from_struct(name, ty)
    }

    /// Borrows a resolved typedef and its name from this package.
    fn typedef_entry(&self, name: &str) -> (&str, &Type) {
        let (key, result) = self
            .types
            .get_key_value(name)
            .unwrap_or_else(|| panic!("Missing typedef {}::{name}", self.name));
        (
            key,
            result
                .as_ref()
                .unwrap_or_else(|err| panic!("Cannot resolve {}::{name}: {err}", self.name)),
        )
    }
}
