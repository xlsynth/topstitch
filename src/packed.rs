// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::sync::Arc;

mod enumeration;
mod shape;
use shape::{Select, Step};
pub(crate) use shape::{Shape, extent};

use crate::EnumValue;
use crate::port::PortDecl;
use crate::{ConvertibleToPortSlice, IO, Port, PortSlice};
use indexmap::IndexMap;

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

/// A packed wiring layout or a selection within it. Layouts own their field names and
/// share their shapes across ports and selections.
#[derive(Debug, Clone)]
pub struct PackedLayout {
    name: Arc<str>,
    root: Arc<Shape>,
    pub(crate) shape: Arc<Shape>,
    path: Vec<(Step, usize)>,
}

impl PackedLayout {
    /// Creates a root with a diagnostic name, sharing the supplied shape.
    pub(crate) fn from_shape(name: impl Into<Arc<str>>, shape: Arc<Shape>) -> Self {
        Self {
            name: name.into(),
            root: shape.clone(),
            shape,
            path: Vec::new(),
        }
    }

    /// Creates a leaf of `width` bits. Use [`Self::array_of`] when you need to
    /// select elements by index. Panics if `width` is zero.
    pub fn bits(width: usize) -> Self {
        Self::from_shape("bits", Shape::bits(width))
    }

    /// Creates a packed struct with fields ordered from most to least significant.
    /// The name identifies the layout in diagnostics. Ports declared from the
    /// layout emit as flat bit vectors.
    ///
    /// An eight-bit header can be wired to two outputs by field name.
    ///
    /// ```
    /// use topstitch::{EmitOptions, IO, ModDef, PackedLayout};
    ///
    /// let layout = PackedLayout::structure(
    ///     "header",
    ///     [("opcode", PackedLayout::bits(3)), ("length", PackedLayout::bits(5))],
    /// );
    /// let unpack = ModDef::new("Unpack");
    /// let header = unpack.add_port("header", layout.input());
    /// let opcode = unpack.add_port("opcode", IO::Output(3));
    /// let length = unpack.add_port("length", IO::Output(5));
    /// header.field("opcode").connect(&opcode);
    /// header.field("length").connect(&length);
    ///
    /// let rtl = unpack.emit(EmitOptions::default());
    /// assert!(rtl.contains("assign opcode = header[7:5];"));
    /// assert!(rtl.contains("assign length = header[4:0];"));
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if there are no fields, a field name is empty or repeated, or the
    /// total width exceeds `usize`.
    pub fn structure<N: Into<Arc<str>>>(
        name: impl Into<Arc<str>>,
        fields: impl IntoIterator<Item = (N, Self)>,
    ) -> Self {
        let shape = Shape::structure(
            fields
                .into_iter()
                .map(|(name, field)| (name.into(), field.shape)),
        );
        Self::from_shape(name, shape)
    }

    /// Creates a new packed array layout of `count` elements indexed from `count - 1` down to 0.
    /// Element 0 occupies the least significant bits. The array is a new root for range queries.
    /// Panics if `count` is zero or its highest index does not fit in an `i32`.
    pub fn array_of(element: &Self, count: usize) -> Self {
        assert!(count > 0, "a packed array must have at least one element");
        let msb = i32::try_from(count - 1).expect("packed array index exceeds i32");
        Self::from_shape(element.name.clone(), element.shape.array(msb, 0))
    }

    /// Selects a direct member of a single struct. Index an array before selecting its fields.
    pub fn field(&self, name: &str) -> Self {
        let (i, m) = self.shape.field(&self.name, name);
        self.child(&m.name, Step::Field(i), m.offset, &m.shape)
    }

    /// Returns enum members in declaration order, including those of an array's element enum.
    pub fn enum_members(&self) -> Option<&IndexMap<String, EnumValue>> {
        self.shape.enum_members()
    }

    /// Lists direct members from most to least significant. Each name and field
    /// layout can outlive the parent layout.
    pub fn fields(&self) -> Vec<(Arc<str>, Self)> {
        self.shape
            .members(&self.name)
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.name.clone(),
                    self.child(&m.name, Step::Field(i), m.offset, &m.shape),
                )
            })
            .collect()
    }

    /// Selects one packed dimension by its declared SV index. The right bound is
    /// the least significant element for both ascending and descending ranges.
    /// Panics on a non-array or an out-of-range index.
    pub fn element(&self, index: impl TryInto<i32>) -> Self {
        let i = sv_index(index);
        let (offset, child) = self.shape.element(&self.name, i);
        self.child(&self.name, Step::Element(i), offset, child)
    }

    /// Retains the occurrence as well as the selected shape for ancestor-relative ranges.
    fn child(&self, name: &Arc<str>, step: Step, offset: usize, shape: &Arc<Shape>) -> Self {
        let mut path = self.path.clone();
        path.push((step, offset));
        Self {
            name: name.clone(),
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

    /// Returns this selection's bit range within its root layout.
    pub fn range_in_root(&self) -> BitRange {
        self.range_at(self.path.iter().map(|(_, offset)| offset).sum())
    }

    /// Returns this selection's bit range within an ancestor (or itself).
    /// Panics for a different root or a selection outside the ancestor's path.
    pub fn range_in(&self, container: &Self) -> BitRange {
        assert!(
            Arc::ptr_eq(&self.root, &container.root) && self.path.starts_with(&container.path),
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
    pub fn on(&self, port: &impl ConvertibleToPortSlice) -> PackedView {
        let slice = port.to_port_slice();
        assert_eq!(
            slice.width(),
            self.width(),
            "Width mismatch binding {}",
            self.name
        );
        PackedView {
            slice,
            shape: self.shape.clone(),
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

    /// Shares the selected shape with the port declaration.
    fn port_decl(&self, io: IO) -> PortDecl {
        PortDecl {
            shape: self.shape.retained(),
            ..io.into()
        }
    }
}

/// Validates Rust loop indices as well as signed SV indices without wrapping.
fn sv_index(index: impl TryInto<i32>) -> i32 {
    index
        .try_into()
        .unwrap_or_else(|_| panic!("packed array index does not fit in i32"))
}

/// A view of wires through a packed layout. Select a named field or an array
/// element to keep its layout, or use a numeric slice for the raw wires.
#[derive(Debug, Clone)]
pub struct PackedView {
    slice: PortSlice,
    shape: Arc<Shape>,
}

impl PackedView {
    /// Returns enum members for the selected wires, including those of an array's element enum.
    pub fn enum_members(&self) -> Option<&IndexMap<String, EnumValue>> {
        self.shape.enum_members()
    }

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
        let (offset, child) = self.shape.select(parent, select);
        Self {
            slice: self.slice.slice_with_offset_and_width(offset, child.width),
            shape: child.clone(),
        }
    }

    /// Exports the selected wires as a flat port with their selected layout.
    pub fn export_as(&self, name: impl AsRef<str>) -> Port {
        self.slice.export_with_shape(name, self.shape.retained())
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

impl Deref for PackedView {
    type Target = PortSlice;
    fn deref(&self) -> &Self::Target {
        &self.slice
    }
}

impl ConvertibleToPortSlice for PackedView {
    fn to_port_slice(&self) -> PortSlice {
        self.slice.clone()
    }
}

impl Port {
    /// Returns a view when the port retains struct fields, array elements, or enum members.
    /// Ports declared with [`PackedLayout::bits`] or plain [`IO`] return `None`.
    ///
    /// ```
    /// use topstitch::{ConvertibleToPortSlice, ModDef, PackedLayout};
    ///
    /// let top = ModDef::new("Top");
    /// let word = top.add_port("word", PackedLayout::bits(5).input());
    /// assert!(word.packed_view().is_none());
    ///
    /// let lanes = PackedLayout::array_of(&PackedLayout::bits(1), 5);
    /// let port = top.add_port("lanes", lanes.input());
    /// assert_eq!(port.packed_view().unwrap().element(3).to_port_slice(), port.bit(3));
    /// ```
    pub fn packed_view(&self) -> Option<PackedView> {
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
    pub fn field(&self, name: &str) -> PackedView {
        self.to_port_slice().field(name)
    }

    /// Selects one packed array element by its declared SV index.
    pub fn element(&self, index: impl TryInto<i32>) -> PackedView {
        self.to_port_slice().element(index)
    }
}

impl PortSlice {
    /// Returns a view if this slice covers a whole port with a known packed layout.
    /// Use [`PackedLayout::on`] to give an arbitrary slice an explicit interpretation.
    pub fn packed_view(&self) -> Option<PackedView> {
        Some(PackedView {
            slice: self.clone(),
            shape: self.whole_packed_shape()?,
        })
    }

    /// Returns the shape of a whole port; an arbitrary bit range carries no type.
    pub(crate) fn whole_packed_shape(&self) -> Option<Arc<Shape>> {
        let core = self.port.get_mod_def_core_where_declared();
        let core = core.read();
        let decl = core.ports.get(self.port.name())?;
        if self.lsb == 0 && self.width() == decl.io.width() {
            decl.shape.clone()
        } else {
            None
        }
    }

    /// Selects a direct struct member. Panics for partial slices or unknown fields.
    pub fn field(&self, name: &str) -> PackedView {
        self.require_view().field(name)
    }

    /// Selects one packed array dimension. Panics for partial slices or invalid indices.
    pub fn element(&self, index: impl TryInto<i32>) -> PackedView {
        self.require_view().element(index)
    }

    /// Requires a retained layout for named or indexed selection.
    fn require_view(&self) -> PackedView {
        self.packed_view().unwrap_or_else(|| {
            panic!(
                "{} is not a whole port with a packed layout",
                self.debug_string()
            )
        })
    }
}
