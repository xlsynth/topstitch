// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use indexmap::IndexMap;

use crate::packed::Shape;
use crate::{IntegerValue, PackedLayout};

/// The width, selectable wiring layout, and enum members of a hardware value.
/// Equality compares these properties. For example, two scalar typedefs
/// can compare equal even when their names or signedness differ.
#[derive(Debug, Clone, PartialEq)]
pub struct DataType {
    pub(crate) width: Option<usize>,
    pub(crate) shape: Option<Arc<Shape>>,
    pub(crate) enum_members: Option<IndexMap<String, IntegerValue>>,
}

impl DataType {
    /// Returns the packed width in bits, or an error for a nonnumeric type.
    pub fn width(&self) -> Result<usize, &'static str> {
        self.width.ok_or("Type is not numeric")
    }

    /// Returns the selectable layout of a packed struct or array.
    pub fn packed_layout(&self) -> Option<PackedLayout> {
        self.layout_with_label("value")
    }

    /// Returns enum members in declaration order, including those of an array's element enum.
    pub fn enum_members(&self) -> Option<&IndexMap<String, IntegerValue>> {
        self.enum_members.as_ref()
    }

    /// Uses the caller's label for layout diagnostics.
    pub(crate) fn layout_with_label(&self, label: &str) -> Option<PackedLayout> {
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
            shape: layout.shape.has_children().then(|| layout.shape.clone()),
            enum_members: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract_packages_from_verilog;

    #[test]
    fn native_layouts_match_imported_type_descriptions() {
        let packages = extract_packages_from_verilog(
            "package p;
                typedef logic flag_t;
                typedef struct packed { logic valid; logic [2:0] tag; } header_t;
             endpackage",
            false,
        )
        .unwrap();
        let p = &packages["p"];
        let flag = PackedLayout::bits(1);
        let tag = PackedLayout::array_of(&flag, 3);
        let header = PackedLayout::structure("header", [("valid", flag.clone()), ("tag", tag)]);

        assert_eq!(&DataType::from(&flag), p.typedef("flag_t"));
        assert_eq!(&DataType::from(&header), p.typedef("header_t"));
    }
}
