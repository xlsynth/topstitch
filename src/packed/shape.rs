// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use slang_rs::{Range, Type};

/// The wiring shape after elaboration. Arrays share a single element shape regardless of length.
#[derive(Debug)]
pub(crate) struct Shape<N> {
    pub(super) width: usize,
    kind: Kind<N>,
}

#[derive(Debug)]
enum Kind<N> {
    Bits,
    Struct(Vec<Member<N>>),
    Array {
        left: i32,
        right: i32,
        element: Arc<Shape<N>>,
    },
}

#[derive(Debug)]
pub(super) struct Member<N> {
    pub(super) name: N,
    pub(super) offset: usize,
    pub(super) shape: Arc<Shape<N>>,
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

/// Packed structs have named members; any declared packed dimension has indices.
pub(crate) fn has_packed_layout(ty: &Type) -> bool {
    ty.width().is_ok()
        && (matches!(
            ty,
            Type::Struct {
                is_packed: true,
                ..
            }
        ) || !dimensions(ty).is_empty())
}

/// Slang reports packed dimensions in declaration order, including those from aliases.
fn dimensions(ty: &Type) -> &[Range] {
    match ty {
        Type::Struct {
            packed_dimensions, ..
        }
        | Type::Integral {
            packed_dimensions, ..
        }
        | Type::Enum {
            packed_dimensions, ..
        }
        | Type::Union {
            packed_dimensions, ..
        } => packed_dimensions,
    }
}

/// Counts both endpoints, including ranges that cross zero.
fn extent(left: i32, right: i32) -> usize {
    usize::try_from(u64::from(left.abs_diff(right)) + 1).expect("packed dimension too wide")
}

impl<N: AsRef<str>> Shape<N> {
    /// Extracts only wiring geometry from Slang, in declaration order.
    pub(crate) fn from_type<'a>(ty: &'a Type, name: impl Copy + Fn(&'a str) -> N) -> Arc<Self> {
        let width = ty.width().expect("expected a packed type");
        let dims = dimensions(ty);
        let base_width = dims.iter().fold(width, |w, d| w / extent(d.left, d.right));
        let kind = if let Type::Struct {
            fields,
            is_packed: true,
            ..
        } = ty
        {
            let mut end = base_width;
            Kind::Struct(
                fields
                    .iter()
                    .map(|field| {
                        let shape = Self::from_type(&field.ty, name);
                        end -= shape.width;
                        Member {
                            name: name(&field.name),
                            offset: end,
                            shape,
                        }
                    })
                    .collect(),
            )
        } else {
            Kind::Bits
        };
        let mut node = Arc::new(Self {
            width: base_width,
            kind,
        });
        for dim in dims.iter().rev() {
            node = node.array(dim.left, dim.right);
        }
        node
    }

    /// Wraps an element in one packed dimension without duplicating its members.
    pub(super) fn array(self: &Arc<Self>, left: i32, right: i32) -> Arc<Self> {
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
    pub(super) fn members(&self, parent: &str) -> &[Member<N>] {
        match &self.kind {
            Kind::Struct(members) => members,
            _ => panic!("{parent} must be a single packed struct to select its fields"),
        }
    }

    /// Looks up a member by name, preserving its ordinal for ancestor comparisons.
    pub(super) fn field(&self, parent: &str, name: &str) -> (usize, &Member<N>) {
        self.members(parent)
            .iter()
            .enumerate()
            .find(|(_, m)| m.name.as_ref() == name)
            .unwrap_or_else(|| panic!("No field {name} in {parent}"))
    }

    /// Computes an element's normalized bit offset from its declared SV index.
    pub(super) fn element(&self, parent: &str, index: i32) -> (usize, &Arc<Self>) {
        let Kind::Array {
            left,
            right,
            element,
        } = &self.kind
        else {
            panic!("{parent} is not a packed array");
        };
        assert!(
            (*left).min(*right) <= index && index <= (*left).max(*right),
            "Index {index} outside packed dimension [{left}:{right}] of {parent}"
        );
        (index.abs_diff(*right) as usize * element.width, element)
    }

    /// Returns the wires and shape selected from either kind of container.
    pub(super) fn select(&self, parent: &str, select: Select<'_>) -> (usize, &Arc<Self>) {
        match select {
            Select::Field(name) => {
                let (_, member) = self.field(parent, name);
                (member.offset, &member.shape)
            }
            Select::Element(index) => self.element(parent, index),
        }
    }

    /// Copies field names when a new port must outlive a borrowed package.
    pub(super) fn owned(&self) -> Arc<Shape<String>> {
        let kind = match &self.kind {
            Kind::Bits => Kind::Bits,
            Kind::Struct(fields) => Kind::Struct(
                fields
                    .iter()
                    .map(|f| Member {
                        name: f.name.as_ref().to_owned(),
                        offset: f.offset,
                        shape: f.shape.owned(),
                    })
                    .collect(),
            ),
            Kind::Array {
                left,
                right,
                element,
            } => Kind::Array {
                left: *left,
                right: *right,
                element: element.owned(),
            },
        };
        Arc::new(Shape {
            width: self.width,
            kind,
        })
    }
}

impl Shape<String> {
    /// A named or indexed selection retains its shape when exported; scalar bits need no metadata.
    pub(crate) fn into_port_type(self: Arc<Self>) -> crate::port::PortType {
        if matches!(self.kind, Kind::Bits) {
            crate::port::PortType::Bits
        } else {
            crate::port::PortType::Packed(self)
        }
    }
}
