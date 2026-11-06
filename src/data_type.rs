// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use indexmap::IndexMap;

use crate::packed::Shape;
use crate::{EnumValue, PackedLayout};

/// The width, selectable wiring layout, and enum members of a hardware value.
/// Equality compares these properties. For example, two scalar typedefs
/// can compare equal even when their names or signedness differ.
#[derive(Debug, Clone, PartialEq)]
pub struct DataType {
    pub(crate) width: Option<usize>,
    /// For an unpacked array of enums, keeps its element enum for member lookup.
    pub(crate) shape: Option<Arc<Shape>>,
}

impl DataType {
    /// Returns the packed width in bits, or an error for a nonnumeric type.
    pub fn width(&self) -> Result<usize, &'static str> {
        self.width.ok_or("Type is not numeric")
    }

    /// Returns the layout of a packed struct, array, or enum. Layouts from the
    /// same descriptor share a root for [`PackedLayout::range_in`].
    pub fn packed_layout(&self) -> Option<PackedLayout> {
        self.layout_with_label("value")
    }

    /// Returns enum members in declaration order, including those of an array's element enum.
    pub fn enum_members(&self) -> Option<&IndexMap<String, EnumValue>> {
        self.shape.as_ref()?.enum_members()
    }

    /// Uses the caller's label for layout diagnostics.
    pub(crate) fn layout_with_label(&self, label: &str) -> Option<PackedLayout> {
        self.width?;
        self.shape
            .as_ref()
            .map(|shape| PackedLayout::from_shape(label, shape.clone()))
    }
}

#[cfg(test)]
impl From<&PackedLayout> for DataType {
    /// Shares the selected wiring shape, which can outlive the original layout.
    fn from(layout: &PackedLayout) -> Self {
        Self {
            width: Some(layout.width()),
            shape: layout.shape.retained(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackageLookup, ParserConfig, Source, VerilogLibrary};

    #[test]
    fn native_layouts_match_imported_type_descriptions() {
        let sources = [Source::text(
            "package p;
                typedef logic flag_t;
                typedef struct packed { logic valid; logic [2:0] tag; } header_t;
             endpackage",
        )];
        let lib = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            ..Default::default()
        })
        .unwrap();
        let p = lib.package("p").unwrap();
        let flag = PackedLayout::bits(1);
        let tag = PackedLayout::array_of(&flag, 3);
        let header = PackedLayout::structure("header", [("valid", flag.clone()), ("tag", tag)]);

        assert_eq!(
            DataType::from(&flag),
            DataType::from_sv_type(p.ty("flag_t").unwrap().into_owned())
        );
        assert_eq!(
            DataType::from(&header),
            DataType::from_sv_type(p.ty("header_t").unwrap().into_owned())
        );
    }
}
