// SPDX-License-Identifier: Apache-2.0

use crate::{ConstantValue, IntegerValue, Package, PackedLayout, Type};
use std::fmt::Display;

/// Typed value and layout queries on an owned package snapshot.
pub trait PackageExt {
    /// Reads a parameter or localparam as the requested Rust type. Panics if
    /// the name is missing or the value cannot be converted. Use [`slang_rs::PackageLookup::parameter`] for a fallible raw lookup.
    fn parameter_as<'a, T>(&'a self, name: &str) -> T
    where
        T: TryFrom<&'a ConstantValue>,
        T::Error: Display;

    /// Reads an enumerator declared directly in this package. Imported and
    /// re-exported names are excluded; look them up in the declaring package.
    /// Use [`Self::enum_member`] to look up a member of a specific enum, including
    /// through a typedef that aliases another package's enum.
    /// Panics if the name is absent or the value cannot be converted.
    fn enumerator_as<'a, T>(&'a self, name: &str) -> T
    where
        T: TryFrom<&'a IntegerValue>,
        T::Error: Display;

    /// Reads `member` from the enum identified by typedef `name`, following
    /// aliases even when the enum is declared in another package. Use
    /// [`Self::enumerator_as`] for a name declared directly in this package.
    ///
    /// For arrays, lookup follows all packed and unpacked dimensions to the
    /// element enum. No index is needed: this reads the enumerator's declared
    /// value, not the contents of an array element. Struct fields are not searched.
    ///
    /// ```
    /// use topstitch::{PackageExt, ParserConfig, Source, VerilogLibrary};
    /// let sources = [Source::text(
    ///     "package p; typedef enum logic [2:0] { A = 1, B = 2 } [1:0] pair_t; endpackage",
    /// )];
    /// let lib = VerilogLibrary::from_config(&ParserConfig { sources: &sources, ..Default::default() })?;
    /// let p = lib.package("p")?.snapshot()?;
    /// assert_eq!(p.type_width("pair_t"), 6);          // Two 3-bit elements.
    /// assert_eq!(p.enum_member::<u8>("pair_t", "B"), 2); // B in the element enum.
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// Panics for a missing typedef or member, a non-enum element type, or a failed conversion.
    fn enum_member<'a, T>(&'a self, name: &str, member: &str) -> T
    where
        T: TryFrom<&'a IntegerValue>,
        T::Error: Display;

    /// Returns the bit width of a numeric typedef. Panics if the type is missing
    /// or does not have a packed width.
    fn type_width(&self, name: &str) -> usize;

    /// Returns the layout of a single packed struct typedef for named field selection. Panics for
    /// missing types, arrays of structs, and other kinds of typedef.
    ///
    /// ```
    /// use topstitch::{IO, ModDef, PackageExt, ParserConfig, Source, VerilogLibrary};
    /// let sources = [Source::text(
    ///     "package display; typedef struct packed { logic [7:0] r, g, b; } rgb; endpackage",
    /// )];
    /// let lib = VerilogLibrary::from_config(&ParserConfig { sources: &sources, ..Default::default() })?;
    /// let display = lib.package("display")?.snapshot()?;
    /// let rgb = display.packed_layout("rgb");
    /// let green_bits = rgb.field("g").range_in_root();
    /// assert_eq!((green_bits.msb(), green_bits.lsb()), (15, 8));
    /// let module = ModDef::new("screen");
    /// let pixels = module.add_port("pixels", IO::Input(32));
    /// let green = rgb.on(&pixels.slice(27, 4)).field("g");
    /// assert_eq!(green.bit(0), pixels.bit(12));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn packed_layout(&self, name: &str) -> PackedLayout<'_>;
}

impl PackageExt for Package {
    fn parameter_as<'a, T>(&'a self, name: &str) -> T
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

    fn enumerator_as<'a, T>(&'a self, name: &str) -> T
    where
        T: TryFrom<&'a IntegerValue>,
        T::Error: Display,
    {
        self.enumerators
            .get(name)
            .unwrap_or_else(|| panic!("Missing enum constant {}::{name}", self.name))
            .try_into()
            .unwrap_or_else(|err| panic!("Cannot convert {}::{name}: {err}", self.name))
    }

    fn enum_member<'a, T>(&'a self, name: &str, member: &str) -> T
    where
        T: TryFrom<&'a IntegerValue>,
        T::Error: Display,
    {
        let variants = match typedef_entry(self, name).1 {
            Type::Enum { variants, .. } => variants,
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

    fn type_width(&self, name: &str) -> usize {
        typedef_entry(self, name)
            .1
            .width()
            .unwrap_or_else(|err| panic!("Cannot size {}::{name}: {err}", self.name))
    }

    fn packed_layout(&self, name: &str) -> PackedLayout<'_> {
        let (name, ty) = typedef_entry(self, name);
        PackedLayout::from_struct(name, ty)
    }
}

/// Borrows a resolved typedef and its name from this package.
fn typedef_entry<'a>(pkg: &'a Package, name: &str) -> (&'a str, &'a Type) {
    let (key, result) = pkg
        .types
        .get_key_value(name)
        .unwrap_or_else(|| panic!("Missing typedef {}::{name}", pkg.name));
    (
        key,
        result
            .as_ref()
            .unwrap_or_else(|err| panic!("Cannot resolve {}::{name}: {err}", pkg.name)),
    )
}
