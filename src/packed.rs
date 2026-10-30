// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;

use slang_rs::{Field, Type};

use crate::{ConvertibleToPortSlice, PortSlice};

// TODO(mkorbel): Revisit using BitRange for port declarations if we need to
// preserve nonzero lower bounds such as [15:8]. Ports currently start at bit zero.
/// An inclusive range of bit positions measured in a single layout or port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitRange {
    msb: usize,
    lsb: usize,
}

impl BitRange {
    /// Creates an inclusive `[msb:lsb]` range. Panics for reversed endpoints
    /// or a width too large to represent as a `usize`.
    pub const fn new(msb: usize, lsb: usize) -> Self {
        assert!(msb >= lsb, "bit range MSB must be at least its LSB");
        assert!(msb - lsb < usize::MAX, "bit range width exceeds usize");
        Self { msb, lsb }
    }

    /// Returns the least significant bit index.
    pub const fn lsb(self) -> usize {
        self.lsb
    }

    /// Returns the most significant bit index.
    pub const fn msb(self) -> usize {
        self.msb
    }

    /// Returns the number of bits, including both endpoints.
    pub const fn width(self) -> usize {
        self.msb - self.lsb + 1
    }
}

/// The layout of a packed struct or a member selected within it.
/// Use [`Self::on`] to view a port or slice through this layout.
#[derive(Debug, Clone)]
pub struct PackedLayout<'a> {
    name: &'a str,
    ty: &'a Type,
    root: &'a Type,
    // Each step names the member and measures its LSB within its direct parent.
    path: Vec<(&'a str, usize)>,
}

impl<'a> PackedLayout<'a> {
    /// Starts at bit zero of a packed struct typedef.
    pub(crate) fn from_struct(name: &'a str, ty: &'a Type) -> Self {
        let root = Self {
            name,
            ty,
            root: ty,
            path: Vec::new(),
        };
        root.declarations();
        root
    }

    /// Selects a direct member by name. Panics if there is no such member or
    /// the current type is not a single packed struct.
    pub fn field(&self, name: &str) -> Self {
        self.members_from_lsb()
            .find(|(member, _)| member.name == name)
            .map(|(member, offset)| self.child(member, offset))
            .unwrap_or_else(|| panic!("No field {name} in {}", self.name))
    }

    /// Lists direct members in SystemVerilog declaration order (MSB first).
    pub fn fields(&self) -> Vec<(&'a str, Self)> {
        let mut members: Vec<_> = self
            .members_from_lsb()
            .map(|(member, offset)| (member.name.as_str(), self.child(member, offset)))
            .collect();
        members.reverse();
        members
    }

    /// Returns the width in bits, including all packed dimensions.
    pub fn width(&self) -> usize {
        self.ty
            .width()
            .unwrap_or_else(|err| panic!("Cannot size {}: {err}", self.name))
    }

    /// Returns this field's bit range within its immediate container. Panics on the root.
    pub fn range_in_parent(&self) -> BitRange {
        let &(_, offset) = self
            .path
            .last()
            .unwrap_or_else(|| panic!("{} is the root and has no parent", self.name));
        self.range_at(offset)
    }

    /// Returns this field's bit range within the original packed struct typedef.
    pub fn range_in_root(&self) -> BitRange {
        self.range_at(self.path.iter().map(|(_, offset)| offset).sum())
    }

    /// Returns this field's bit range within a selected ancestor (or itself).
    /// Panics for a different root or a selection outside this field's path.
    pub fn range_in(&self, container: &Self) -> BitRange {
        assert!(
            std::ptr::eq(self.root, container.root) && self.path.starts_with(&container.path),
            "{ancestor} does not contain {field} in this packed layout",
            ancestor = container.name,
            field = self.name
        );
        let suffix = &self.path[container.path.len()..];
        self.range_at(suffix.iter().map(|(_, offset)| offset).sum())
    }

    /// Places this field's width at an offset in the requested coordinate system.
    fn range_at(&self, lsb: usize) -> BitRange {
        BitRange::new(lsb + (self.width() - 1), lsb)
    }

    /// Returns a view of a port or slice using this layout. The widths must match.
    pub fn on(&self, port: &impl ConvertibleToPortSlice) -> PackedView<'a> {
        let slice = port.to_port_slice();
        assert_eq!(
            slice.width(),
            self.width(),
            "Width mismatch binding {}",
            self.name
        );
        PackedView {
            slice,
            member: self.clone(),
        }
    }

    /// Extends the selection by one declaration, retaining its offset in this member.
    fn child(&self, member: &'a Field, offset: usize) -> Self {
        let mut path = self.path.clone();
        path.push((&member.name, offset));
        Self {
            name: &member.name,
            ty: &member.ty,
            root: self.root,
            path,
        }
    }

    /// Walks direct members with offsets from this struct's low bit.
    fn members_from_lsb(&self) -> impl Iterator<Item = (&'a Field, usize)> {
        self.declarations().iter().rev().scan(0, |next, member| {
            let offset = *next;
            *next += member
                .ty
                .width()
                .unwrap_or_else(|err| panic!("Cannot size {}: {err}", member.name));
            Some((member, offset))
        })
    }

    /// Returns the members of one struct, rejecting aggregates that need indexing.
    fn declarations(&self) -> &'a [Field] {
        match self.ty {
            Type::Struct {
                fields,
                is_packed: true,
                packed_dimensions,
                unpacked_dimensions,
                ..
            } if packed_dimensions.is_empty() && unpacked_dimensions.is_empty() => fields,
            _ => panic!(
                "{} must be a single packed struct to select its fields",
                self.name
            ),
        }
    }
}

/// A view of wires through an explicitly supplied [`PackedLayout`].
#[derive(Debug, Clone)]
pub struct PackedView<'a> {
    slice: PortSlice,
    member: PackedLayout<'a>,
}

impl<'a> PackedView<'a> {
    /// Selects the wires corresponding to a direct struct member.
    pub fn field(&self, name: &str) -> Self {
        let member = self.member.field(name);
        let range = member.range_in_parent();
        Self {
            slice: self
                .slice
                .slice_with_offset_and_width(range.lsb(), range.width()),
            member,
        }
    }

    /// Selects wires at inclusive offsets within this view, counting from its
    /// least significant bit as zero. If the view spans port bits [52:48],
    /// slicing (3, 1) selects port bits [51:49].
    /// Panics if the range is reversed or extends beyond the view.
    pub fn slice(&self, msb: usize, lsb: usize) -> PortSlice {
        assert!(
            lsb <= msb && msb < self.slice.width(),
            "Invalid slice [{msb}:{lsb}] of packed view {}",
            self.member.name
        );
        self.slice.slice_with_offset_and_width(lsb, msb - lsb + 1)
    }

    /// Returns a single wire by zero-based offset from this slice's low bit.
    pub fn bit(&self, offset: usize) -> PortSlice {
        self.slice(offset, offset)
    }
}

impl Deref for PackedView<'_> {
    type Target = PortSlice;
    fn deref(&self) -> &Self::Target {
        &self.slice
    }
}

impl ConvertibleToPortSlice for PackedView<'_> {
    fn to_port_slice(&self) -> PortSlice {
        self.slice.clone()
    }
}
