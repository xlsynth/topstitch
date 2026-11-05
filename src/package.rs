// SPDX-License-Identifier: Apache-2.0

use slang_rs::{IntegerValue, Type};

/// Returns the declared value of a member of this resolved SystemVerilog enum.
/// Use this when membership in a particular enum matters, including through a
/// typedef alias. To look up a name declared directly in a package without
/// specifying an enum type, use [`slang_rs::PackageLookup::enumerator`].
///
/// For an array of enums, the member belongs to the element enum, regardless
/// of packed or unpacked dimensions. No array index is needed. Returns an error
/// if the type is not an enum or array of enums, or the member is missing.
/// Struct fields are not searched.
///
/// ```
/// use topstitch::{enum_member, PackageLookup, ParserConfig, Source, VerilogLibrary};
/// let sources = [Source::text(
///     "package p; typedef enum logic [2:0] { A = 1, B = 2 } [1:0] pair_t; endpackage",
/// )];
/// let lib = VerilogLibrary::from_config(&ParserConfig { sources: &sources, ..Default::default() })?;
/// let pkg = lib.package("p")?;
/// let pair = pkg.ty("pair_t")?;
/// assert_eq!(pair.width()?, 6); // Two 3-bit elements.
/// assert_eq!(u8::try_from(enum_member(&pair, "B")?)?, 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn enum_member<'a>(ty: &'a Type, member: &str) -> Result<&'a IntegerValue, String> {
    let Type::Enum { name, variants, .. } = ty else {
        return Err("expected a SystemVerilog enum or array of enums".into());
    };
    variants
        .iter()
        .find(|v| v.name == member)
        .map(|v| &v.value)
        .ok_or_else(|| {
            format!(
                "no member {member} in enum {}",
                name.as_deref().unwrap_or("<anonymous>")
            )
        })
}
