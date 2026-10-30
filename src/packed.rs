// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::sync::Arc;

use slang_rs::Type;

mod shape;
use shape::{Select, Step};
pub(crate) use shape::{Shape, has_packed_layout};

use crate::port::{PortDecl, PortType};
use crate::{ConvertibleToPortSlice, IO, Port, PortSlice};

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

/// Identifies a typedef across lookups, or a particular Rust-created array across selections.
#[derive(Debug, Clone)]
enum LayoutRoot<'a> {
    Typedef(&'a Type),
    Array(Arc<Shape<&'a str>>),
}

impl LayoutRoot<'_> {
    /// Tests occurrence identity, independent of whether two layouts have the same widths.
    fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Typedef(a), Self::Typedef(b)) => std::ptr::eq(*a, *b),
            (Self::Array(a), Self::Array(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// The layout of a packed typedef or a selection within it.
#[derive(Debug, Clone)]
pub struct PackedLayout<'a> {
    name: &'a str,
    root: LayoutRoot<'a>,
    shape: Arc<Shape<&'a str>>,
    path: Vec<(Step, usize)>,
}

impl<'a> PackedLayout<'a> {
    /// Starts at bit zero of a packed struct or packed array typedef.
    pub(crate) fn from_type(name: &'a str, ty: &'a Type) -> Self {
        assert!(
            has_packed_layout(ty),
            "{name} must be a packed struct or packed array"
        );
        Self {
            name,
            root: LayoutRoot::Typedef(ty),
            shape: Shape::from_type(ty, |s| s),
            path: Vec::new(),
        }
    }

    /// Creates a new packed array layout of `count` elements indexed from `count - 1` down to 0.
    /// Element 0 occupies the least significant bits. The array is a new root for range queries.
    /// Panics if `count` is zero or its highest index does not fit in an `i32`.
    pub fn array_of(element: &Self, count: usize) -> Self {
        assert!(count > 0, "a packed array must have at least one element");
        let msb = i32::try_from(count - 1).expect("packed array index exceeds i32");
        let shape = element.shape.array(msb, 0);
        Self {
            name: element.name,
            root: LayoutRoot::Array(shape.clone()),
            shape,
            path: Vec::new(),
        }
    }

    /// Selects a direct member of a single struct. Index an array before selecting its fields.
    pub fn field(&self, name: &str) -> Self {
        let (i, m) = self.shape.field(self.name, name);
        self.child(m.name, Step::Field(i), m.offset, &m.shape)
    }

    /// Lists direct members in SystemVerilog declaration order (MSB first).
    pub fn fields(&self) -> Vec<(&'a str, Self)> {
        self.shape
            .members(self.name)
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.name,
                    self.child(m.name, Step::Field(i), m.offset, &m.shape),
                )
            })
            .collect()
    }

    /// Selects one packed dimension by its declared SV index. The right bound is
    /// the least significant element for both ascending and descending ranges.
    /// Panics on a non-array or an out-of-range index.
    pub fn element(&self, index: impl TryInto<i32>) -> Self {
        let i = sv_index(index);
        let (offset, child) = self.shape.element(self.name, i);
        self.child(self.name, Step::Element(i), offset, child)
    }

    /// Retains the occurrence as well as the selected shape for ancestor-relative ranges.
    fn child(&self, name: &'a str, step: Step, offset: usize, shape: &Arc<Shape<&'a str>>) -> Self {
        let mut path = self.path.clone();
        path.push((step, offset));
        Self {
            name,
            root: self.root.clone(),
            shape: shape.clone(),
            path,
        }
    }

    /// Returns the selected width in bits, including any remaining packed dimensions.
    pub fn width(&self) -> usize {
        self.shape.width
    }

    /// Returns this selection's bit range within its immediate container. Panics on the root.
    pub fn range_in_parent(&self) -> BitRange {
        let &(_, offset) = self
            .path
            .last()
            .unwrap_or_else(|| panic!("{} is the root and has no parent", self.name));
        self.range_at(offset)
    }

    /// Returns this selection's bit range within the original typedef.
    pub fn range_in_root(&self) -> BitRange {
        self.range_at(self.path.iter().map(|(_, offset)| offset).sum())
    }

    /// Returns this selection's bit range within an ancestor (or itself).
    /// Panics for a different root or a selection outside the ancestor's path.
    pub fn range_in(&self, container: &Self) -> BitRange {
        assert!(
            self.root.same_as(&container.root) && self.path.starts_with(&container.path),
            "{ancestor} does not contain {field} in this packed layout",
            ancestor = container.name,
            field = self.name
        );
        self.range_at(
            self.path[container.path.len()..]
                .iter()
                .map(|(_, offset)| offset)
                .sum(),
        )
    }

    /// Places this selection's width at an offset in the requested coordinate system.
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
            shape: ViewShape::Package(self.shape.clone()),
        }
    }

    /// Declares an input whose width and shape come from this layout.
    pub fn input(&self) -> PortDecl {
        self.port_decl(IO::Input(self.width()))
    }

    /// Declares an output whose width and shape come from this layout.
    pub fn output(&self) -> PortDecl {
        self.port_decl(IO::Output(self.width()))
    }

    /// Declares a bidirectional port whose width and shape come from this layout.
    pub fn inout(&self) -> PortDecl {
        self.port_decl(IO::InOut(self.width()))
    }

    /// Gives the declaration its own copy of the selected layout so it can outlive the package.
    fn port_decl(&self, io: IO) -> PortDecl {
        PortDecl {
            io,
            ty: self.shape.owned().into_port_type(),
        }
    }
}

/// Validates Rust loop indices as well as signed SV indices without wrapping.
fn sv_index(index: impl TryInto<i32>) -> i32 {
    index
        .try_into()
        .unwrap_or_else(|_| panic!("packed array index does not fit in i32"))
}

#[derive(Debug, Clone)]
enum ViewShape<'a> {
    Package(Arc<Shape<&'a str>>),
    Port(Arc<Shape<String>>),
}

/// A view of wires through a packed layout. Select a named field or an array
/// element to keep its layout, or use a numeric slice for the raw wires.
#[derive(Debug, Clone)]
pub struct PackedView<'a> {
    slice: PortSlice,
    shape: ViewShape<'a>,
}

impl PackedView<'_> {
    /// Selects the wires corresponding to a direct struct member.
    pub fn field(&self, name: &str) -> Self {
        self.select(Select::Field(name))
    }

    /// Selects the wires for one element by its declared SV index. Chain calls
    /// for multiple dimensions. Panics on a non-array or an out-of-range index.
    pub fn element(&self, index: impl TryInto<i32>) -> Self {
        self.select(Select::Element(sv_index(index)))
    }

    /// Applies the child's offset to the current wires, even on a nonzero port slice.
    fn select(&self, select: Select<'_>) -> Self {
        let parent = self.slice.port.name();
        let (shape, offset, width) = match &self.shape {
            ViewShape::Package(s) => {
                let (offset, child) = s.select(parent, select);
                (ViewShape::Package(child.clone()), offset, child.width)
            }
            ViewShape::Port(s) => {
                let (offset, child) = s.select(parent, select);
                (ViewShape::Port(child.clone()), offset, child.width)
            }
        };
        Self {
            slice: self.slice.slice_with_offset_and_width(offset, width),
            shape,
        }
    }

    /// Exports the selected wires as a flat port with their selected layout.
    pub fn export_as(&self, name: impl AsRef<str>) -> Port {
        let shape = match &self.shape {
            ViewShape::Package(s) => s.owned(),
            ViewShape::Port(s) => s.clone(),
        };
        self.slice.export_with_type(name, shape.into_port_type())
    }

    /// Like [`Self::export_as`], using the instance port's name.
    pub fn export(&self) -> Port {
        self.export_as(self.slice.default_export_name())
    }

    /// Selects wires at inclusive offsets within this view, counting from its
    /// least significant bit as zero. If the view spans port bits `[52:48]`,
    /// `slice(3, 1)` selects port bits `[51:49]`.
    /// Panics if the range is reversed or extends beyond the view.
    pub fn slice(&self, msb: usize, lsb: usize) -> PortSlice {
        assert!(
            lsb <= msb && msb < self.slice.width(),
            "Invalid slice [{msb}:{lsb}] of packed view {}",
            self.slice.debug_string()
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

impl Port {
    /// Returns a view of this port's packed layout, if it has one. A port added
    /// with a [`PackedLayout`] retains its layout; one added with plain [`IO`]
    /// has no layout, even if its width matches a known packed type.
    pub fn packed_view(&self) -> Option<PackedView<'static>> {
        self.to_port_slice().packed_view()
    }

    /// Selects a direct struct member from this port's layout.
    ///
    /// ```
    /// use topstitch::{ConvertibleToPortSlice, ModDef};
    /// let source = "module Sender(output struct packed { logic valid; logic [3:0] tag; } word);
    ///               endmodule";
    /// let sender = ModDef::from_verilog("Sender", source, false, false);
    /// let top = ModDef::new("Top");
    /// let word = top.instantiate(&sender, None, None).get_port("word");
    /// assert_eq!(word.field("valid").to_port_slice(), word.bit(4));
    /// let view = word.packed_view().unwrap();
    /// assert_eq!(view.field("tag").to_port_slice(), word.slice(3, 0));
    /// ```
    pub fn field(&self, name: &str) -> PackedView<'static> {
        self.to_port_slice().field(name)
    }

    /// Selects one packed array element by its declared SV index.
    pub fn element(&self, index: impl TryInto<i32>) -> PackedView<'static> {
        self.to_port_slice().element(index)
    }
}

impl PortSlice {
    /// Returns a view if this slice covers a whole port with a known packed layout.
    /// Use [`PackedLayout::on`] to give an arbitrary slice an explicit interpretation.
    pub fn packed_view(&self) -> Option<PackedView<'static>> {
        Some(PackedView {
            slice: self.clone(),
            shape: ViewShape::Port(self.whole_packed_shape()?),
        })
    }

    /// Returns the shape of a whole port; an arbitrary bit range carries no type.
    pub(crate) fn whole_packed_shape(&self) -> Option<Arc<Shape<String>>> {
        let core = self.port.get_mod_def_core_where_declared();
        let core = core.read();
        let decl = core.ports.get(self.port.name())?;
        match &decl.ty {
            PortType::Packed(shape) if self.lsb == 0 && self.width() == decl.io.width() => {
                Some(shape.clone())
            }
            _ => None,
        }
    }

    /// Selects a direct struct member. Panics for partial slices or unknown fields.
    pub fn field(&self, name: &str) -> PackedView<'static> {
        self.require_view().field(name)
    }

    /// Selects one packed array dimension. Panics for partial slices or invalid indices.
    pub fn element(&self, index: impl TryInto<i32>) -> PackedView<'static> {
        self.require_view().element(index)
    }

    /// Requires a retained layout for named or indexed selection.
    fn require_view(&self) -> PackedView<'static> {
        self.packed_view().unwrap_or_else(|| {
            panic!(
                "{} is not a whole port with a packed layout",
                self.debug_string()
            )
        })
    }
}
