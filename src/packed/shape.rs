// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

use crate::EnumValue;
use indexmap::IndexMap;

/// The wiring shape after elaboration. Arrays share a single element shape regardless of length.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(super) width: usize,
    kind: Kind,
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Bits,
    Enum(Box<IndexMap<String, EnumValue>>),
    Struct(Vec<Member>),
    Array {
        left: i32,
        right: i32,
        element: Arc<Shape>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Member {
    pub(super) name: Arc<str>,
    pub(super) offset: usize,
    pub(super) shape: Arc<Shape>,
}

/// Fields and array elements occupy distinct positions even when their offsets match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Field(usize),
    Element(i32),
}

#[derive(Clone, Copy)]
pub(super) enum Select<'a> {
    Field(&'a str),
    Element(i32),
}

/// Counts both endpoints, including ranges that cross zero.
pub(crate) fn extent(left: i32, right: i32) -> usize {
    usize::try_from(u64::from(left.abs_diff(right)) + 1).expect("packed dimension too wide")
}

impl Shape {
    /// Constructs a leaf with the given number of wires.
    pub(crate) fn bits(width: usize) -> Arc<Self> {
        assert!(width > 0, "a packed value must have at least one bit");
        Arc::new(Self {
            width,
            kind: Kind::Bits,
        })
    }

    /// Builds an enum leaf with its member table in declaration order.
    pub(crate) fn enumeration(width: usize, members: IndexMap<String, EnumValue>) -> Arc<Self> {
        Arc::new(Self {
            width,
            kind: Kind::Enum(Box::new(members)),
        })
    }

    /// Returns the member table of this enum or an array's innermost enum.
    pub(crate) fn enum_members(&self) -> Option<&IndexMap<String, EnumValue>> {
        match &self.kind {
            Kind::Enum(members) => Some(members),
            Kind::Array { element, .. } => element.enum_members(),
            _ => None,
        }
    }

    /// Packs fields in declaration order, with the last field at bit zero.
    pub(crate) fn structure(fields: impl IntoIterator<Item = (Arc<str>, Arc<Self>)>) -> Arc<Self> {
        let mut width = 0usize;
        let mut names = HashSet::new();
        let mut members = Vec::new();
        for (name, shape) in fields {
            assert!(!name.is_empty(), "a packed field must have a name");
            assert!(names.insert(name.clone()), "duplicate packed field {name}");
            members.push(Member {
                name,
                offset: 0,
                shape,
            });
        }
        assert!(
            !members.is_empty(),
            "a packed struct must have at least one field"
        );
        for member in members.iter_mut().rev() {
            member.offset = width;
            width = width
                .checked_add(member.shape.width)
                .expect("packed struct width exceeds usize");
        }
        Arc::new(Self {
            width,
            kind: Kind::Struct(members),
        })
    }

    /// Wraps an element in one packed dimension without duplicating its members.
    pub(crate) fn array(self: &Arc<Self>, left: i32, right: i32) -> Arc<Self> {
        Arc::new(Self {
            width: self
                .width
                .checked_mul(extent(left, right))
                .expect("packed array width exceeds usize"),
            kind: Kind::Array {
                left,
                right,
                element: self.clone(),
            },
        })
    }

    /// Lists direct members; an array must be indexed before selecting a field.
    pub(super) fn members(&self, parent: &str) -> &[Member] {
        match &self.kind {
            Kind::Struct(members) => members,
            _ => panic!("{parent} must be a single packed struct to select its fields"),
        }
    }

    /// Looks up a member by name, preserving its ordinal for ancestor comparisons.
    pub(super) fn field(&self, parent: &str, name: &str) -> (usize, &Member) {
        self.members(parent)
            .iter()
            .enumerate()
            .find(|(_, m)| m.name.as_ref() == name)
            .unwrap_or_else(|| panic!("No field {name} in {parent}"))
    }

    /// Returns array extents, including the bit width of a multi-bit vector element.
    pub(crate) fn array_dimensions(&self) -> Vec<usize> {
        let mut dims = Vec::new();
        let mut shape = self;
        while let Kind::Array {
            left,
            right,
            element,
        } = &shape.kind
        {
            dims.push(extent(*left, *right));
            shape = element;
        }
        if matches!(shape.kind, Kind::Bits) && shape.width > 1 {
            dims.push(shape.width);
        }
        dims
    }

    /// Returns physical word ranges, rejecting indices that cannot match supported pin names.
    pub(crate) fn physical_word_layout(&self, parent: &str) -> (Vec<(i32, i32)>, usize) {
        let mut ranges = Vec::new();
        let mut shape = self;
        while let Kind::Array {
            left,
            right,
            element,
        } = &shape.kind
        {
            ranges.push((*left, *right));
            shape = element;
        }
        let word_width = if matches!(shape.kind, Kind::Bits) && shape.width == 1 {
            ranges.pop().map_or(1, |(left, right)| {
                assert!(
                    left >= 0 && right == 0,
                    "cannot split physical pins for {parent}: innermost vector range [{left}:{right}] must descend to zero"
                );
                extent(left, right)
            })
        } else {
            shape.width
        };
        for &(left, right) in &ranges {
            assert!(
                left >= 0 && right >= 0,
                "cannot split physical pins for {parent}: outer range [{left}:{right}] contains negative indices"
            );
        }
        (ranges, word_width)
    }

    /// Identifies source bit arrays whose packed dimensions can be preserved in a stub.
    pub(crate) fn has_bit_vector_dimensions(&self) -> bool {
        let Kind::Array { element, .. } = &self.kind else {
            return false;
        };
        let mut shape = element.as_ref();
        while let Kind::Array { element, .. } = &shape.kind {
            shape = element;
        }
        matches!(shape.kind, Kind::Bits)
    }

    /// Returns exact zero-based packed bit dimensions for an imported stub declaration.
    pub(crate) fn stub_packed_dimensions(&self, parent: &str) -> Option<Vec<usize>> {
        let mut ranges = Vec::new();
        let mut shape = self;
        while let Kind::Array {
            left,
            right,
            element,
        } = &shape.kind
        {
            ranges.push((*left, *right));
            shape = element;
        }
        if ranges.is_empty() || !matches!(shape.kind, Kind::Bits) {
            return None;
        }
        let mut dims = Vec::with_capacity(ranges.len() + usize::from(shape.width > 1));
        for (left, right) in ranges {
            assert!(
                left >= 0 && right == 0,
                "cannot emit packed stub port {parent}: range [{left}:{right}] must descend to zero"
            );
            dims.push(extent(left, right));
        }
        if shape.width > 1 {
            dims.push(shape.width);
        }
        Some(dims)
    }

    /// Returns the bounds of an explicit array or the normalized bounds of a bit vector.
    pub(super) fn index_bounds(&self, parent: &str) -> (i32, i32) {
        match &self.kind {
            Kind::Array { left, right, .. } => (*left, *right),
            Kind::Bits if self.width > 1 => (
                i32::try_from(self.width - 1).expect("packed bit vector index exceeds i32"),
                0,
            ),
            _ => panic!("{parent} is not a packed array or bit vector"),
        }
    }

    /// Computes an element's normalized bit offset from its declared SV index.
    pub(super) fn element(&self, parent: &str, index: i32) -> (usize, Arc<Self>) {
        let (left, right) = self.index_bounds(parent);
        assert!(
            left.min(right) <= index && index <= left.max(right),
            "Index {index} outside packed dimension [{left}:{right}] of {parent}"
        );
        let element = match &self.kind {
            Kind::Array { element, .. } => element.clone(),
            Kind::Bits => Self::bits(1),
            _ => unreachable!(),
        };
        (index.abs_diff(right) as usize * element.width, element)
    }

    /// Returns the wires and shape selected from either kind of container.
    pub(super) fn select(&self, parent: &str, select: Select<'_>) -> (usize, Arc<Self>) {
        match select {
            Select::Field(name) => {
                let (_, member) = self.field(parent, name);
                (member.offset, member.shape.clone())
            }
            Select::Element(index) => self.element(parent, index),
        }
    }

    /// Shares the layout for a port or typedef when it carries more than a width.
    pub(crate) fn retained(self: &Arc<Self>) -> Option<Arc<Self>> {
        (!matches!(self.kind, Kind::Bits)).then(|| self.clone())
    }
}
