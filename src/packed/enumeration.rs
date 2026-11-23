// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

use indexmap::IndexMap;
use num_bigint::BigUint;

use super::{PackedLayout, Shape};
use crate::EnumValue;

impl PackedLayout {
    /// Defines an unsigned enum with explicit encodings in declaration order.
    /// Panics if an encoding is negative or does not fit in `width` bits, or if
    /// the enum fails the checks described in [`Self::from_enum_values`].
    ///
    /// ```
    /// use topstitch::{ModDef, PackedLayout};
    ///
    /// let mode = PackedLayout::enumeration("Mode", 2, [("Idle", 0), ("Busy", 2)]);
    /// let header = PackedLayout::structure("Header", [("mode", mode)]);
    /// let top = ModDef::new("Controller");
    /// let input = top.add_port("header", header.input());
    /// let field = input.field("mode");
    /// assert_eq!(u8::try_from(&field.enum_members().unwrap()["Busy"]), Ok(2));
    /// ```
    pub fn enumeration<N: Into<String>, V: TryInto<BigUint>>(
        name: impl Into<Arc<str>>,
        width: usize,
        members: impl IntoIterator<Item = (N, V)>,
    ) -> Self {
        Self::from_enum_values(
            name,
            members.into_iter().map(|(name, value)| {
                let bits = value
                    .try_into()
                    .unwrap_or_else(|_| panic!("invalid unsigned enum encoding"));
                (
                    name,
                    EnumValue::from_bits(
                        width,
                        false,
                        bits,
                        BigUint::default(),
                        BigUint::default(),
                    ),
                )
            }),
        )
    }

    /// Defines an enum from exact encodings, including signed and four-state values.
    /// The members must agree on width and signedness. Panics for an empty enum,
    /// empty or repeated member names, or repeated encodings.
    pub fn from_enum_values<N: Into<String>>(
        name: impl Into<Arc<str>>,
        members: impl IntoIterator<Item = (N, EnumValue)>,
    ) -> Self {
        let mut table = IndexMap::new();
        for (name, value) in members {
            let name = name.into();
            assert!(!name.is_empty(), "an enum member must have a name");
            assert!(
                table.insert(name, value).is_none(),
                "duplicate enum member name"
            );
        }
        let first = table
            .first()
            .expect("an enum must have at least one member")
            .1;
        let width = first.width();
        let mut encodings = HashSet::new();
        for (name, value) in &table {
            assert!(
                value.width() == width && value.is_signed() == first.is_signed(),
                "enum member {name} has a different width or signedness"
            );
            assert!(
                encodings.insert((value.bits(), value.x_mask(), value.z_mask())),
                "duplicate enum encoding for {name}"
            );
        }
        Self::from_shape(name, Shape::enumeration(width, table))
    }
}
