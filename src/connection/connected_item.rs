// SPDX-License-Identifier: Apache-2.0

use num_bigint::BigInt;
use std::fmt::{self, Debug};

use crate::connection::port_slice::PortSliceConnections;
use crate::PortSlice;

/// Represents what a PortSlice is connected to: another
/// PortSlice, a tieoff, an "unused" marker, or a wire.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectedItem {
    PortSlice(PortSlice),
    Tieoff(Tieoff),
    Unused(Unused),
    Wire(Wire),
}

/// Marker object used to indicate that a slice of a port is intentionally
/// left unconnected. This is distinct from an implicitly unconnected slice
/// (which is an error during validation). `Unused` values can be sliced and
/// merged without changing semantics.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Unused {}

impl Unused {
    /// Constructs a new marker for an intentionally unused slice.
    pub fn new() -> Self {
        Self {}
    }

    /// Returns a new `Unused` representing a subslice of this marker.
    /// Offsets and widths are ignored because an unused region remains unused
    /// regardless of geometry; the method exists to keep APIs uniform.
    pub fn slice_with_offset_and_width(&self, _offset: usize, _width: usize) -> Unused {
        Unused {}
    }

    /// Attempts to merge two adjacent `Unused` markers. Since unused markers
    /// carry no data, merges always succeed and return another `Unused`.
    pub fn try_merge(&self, _other: &Unused) -> Option<Unused> {
        Some(Unused {})
    }
}

/// Represents a tieoff connected to a PortSlice.
#[derive(Clone, PartialEq)]
pub struct Tieoff {
    pub value: BigInt,
    pub width: usize,
}

impl Tieoff {
    pub fn new<T: Into<BigInt>>(value: T, width: usize) -> Self {
        let value_as_big_int = value.into();

        let min_value = BigInt::from(0);
        assert!(
            value_as_big_int >= min_value,
            "Tieoff value must be non-negative"
        );

        let max_value = (BigInt::from(1u32) << width) - 1;
        assert!(
            value_as_big_int <= max_value,
            "Tieoff value must be less than or equal to the maximum value for the given width"
        );

        Self {
            value: value_as_big_int,
            width,
        }
    }

    /// Returns a new Tieoff value that is a slice of this one, with the given
    /// offset and width.
    pub fn slice_with_offset_and_width(&self, offset: usize, width: usize) -> Tieoff {
        let mask = (BigInt::from(1u32) << width) - 1;
        Tieoff::new((&self.value >> offset) & mask, width)
    }

    /// Attempts to concatenate two adjacent tieoffs. On success returns a new
    /// `Tieoff` whose width is the sum of the inputs and whose value is the
    /// bitwise concatenation (self as the upper bits).
    pub fn try_merge(&self, other: &Tieoff) -> Option<Tieoff> {
        Some(Tieoff::new(
            (&self.value << other.width) | &other.value,
            self.width + other.width,
        ))
    }
}

impl Debug for Tieoff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x({:X})", self.value)
    }
}

/// Represents a wire name specification for a PortSlice. `width` represents the
/// full width of the wire, while `msb` and `lsb` define a slice of the wire
/// that is connected to the PortSlice. `msb` - `lsb` + 1 <= `width`. Slicing a
/// wire multiple times changes `msb` and `lsb`, but not `width`. This is
/// convenient because any PortSlice connected to the wire has the full
/// information needed to declare the wire.
#[derive(Clone, PartialEq)]
pub struct Wire {
    pub name: String,
    pub width: usize,
    pub msb: usize,
    pub lsb: usize,
}

impl Wire {
    /// Attempts to merge two adjacent slices of the same named wire into a
    /// single wider slice. Succeeds only if names and widths match and the
    /// ranges are exactly contiguous (self.lsb == other.msb + 1).
    pub fn try_merge(&self, other: &Wire) -> Option<Wire> {
        if self.name == other.name && self.width == other.width && (self.lsb == (other.msb + 1)) {
            Some(Wire {
                name: self.name.clone(),
                width: self.width,
                msb: self.msb,
                lsb: other.lsb,
            })
        } else {
            None
        }
    }

    /// Returns a new Wire with the same `name` and `width`, but `msb` and `lsb`
    /// adjusted according to the given offset and width.
    pub fn slice_with_offset_and_width(&self, offset: usize, width: usize) -> Wire {
        let lsb = self.lsb + offset;
        let msb = lsb + width - 1;

        Wire {
            name: self.name.clone(),
            width: self.width,
            msb,
            lsb,
        }
    }
}

impl Debug for Wire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}[{}:{}]", self.name, self.msb, self.lsb)
    }
}

impl ConnectedItem {
    /// Returns a new `ConnectedItem` corresponding to the requested subslice.
    /// For each variant, the slicing operation is delegated to the underlying
    /// type and preserves the variant, keeping semantics consistent across
    /// `PortSlice`, `Tieoff`, `Unused`, and `Wire`.
    pub fn slice_with_offset_and_width(&self, offset: usize, width: usize) -> ConnectedItem {
        match self {
            ConnectedItem::PortSlice(port_slice) => {
                port_slice.slice_with_offset_and_width(offset, width).into()
            }
            ConnectedItem::Tieoff(tieoff) => {
                tieoff.slice_with_offset_and_width(offset, width).into()
            }
            ConnectedItem::Unused(unused) => {
                unused.slice_with_offset_and_width(offset, width).into()
            }
            ConnectedItem::Wire(wire) => wire.slice_with_offset_and_width(offset, width).into(),
        }
    }

    /// Attempts to merge two `ConnectedItem`s of the same variant when their
    /// slices are exactly adjacent. Returns `None` for mismatched variants or
    /// non-contiguous ranges. The exact merge rules are delegated to the
    /// variant-specific `try_merge` implementations.
    pub fn try_merge(&self, other: &ConnectedItem) -> Option<ConnectedItem> {
        match (self, other) {
            (ConnectedItem::PortSlice(this), ConnectedItem::PortSlice(other)) => {
                this.try_merge(other).map(ConnectedItem::PortSlice)
            }
            (ConnectedItem::Wire(this), ConnectedItem::Wire(other)) => {
                this.try_merge(other).map(ConnectedItem::Wire)
            }
            (ConnectedItem::Tieoff(this), ConnectedItem::Tieoff(other)) => {
                this.try_merge(other).map(ConnectedItem::Tieoff)
            }
            (ConnectedItem::Unused(this), ConnectedItem::Unused(other)) => {
                this.try_merge(other).map(ConnectedItem::Unused)
            }
            _ => None,
        }
    }
}

impl PortSliceConnections {
    /// Returns a vector of all tieoff connections
    pub fn to_tieoffs(&self) -> Vec<Tieoff> {
        let mut result = Vec::new();
        for connection in self {
            if let ConnectedItem::Tieoff(tieoff) = &connection.other {
                result.push(tieoff.clone());
            }
        }
        result
    }

    /// Returns a vector of all wire name specifications
    pub fn to_wires(&self) -> Vec<Wire> {
        let mut result = Vec::new();
        for connection in self {
            if let ConnectedItem::Wire(wire) = &connection.other {
                result.push(wire.clone());
            }
        }
        result
    }

    /// Returns a vector of all port slice connections
    pub fn to_port_slices(&self) -> Vec<PortSlice> {
        let mut result = Vec::new();
        if !self.is_empty() {
            result.push(self[0].this.clone());
        }
        for connection in self {
            if let ConnectedItem::PortSlice(port_slice) = &connection.other {
                result.push(port_slice.clone());
            }
        }
        result
    }

    /// Returns the number of "unused" connections
    pub fn to_unused_count(&self) -> usize {
        let mut result = 0;
        for connection in self {
            if let ConnectedItem::Unused(_) = &connection.other {
                result += 1;
            }
        }
        result
    }
}

impl PartialEq<PortSlice> for ConnectedItem {
    fn eq(&self, other: &PortSlice) -> bool {
        matches!(self, ConnectedItem::PortSlice(ps) if ps == other)
    }
}

impl PartialEq<ConnectedItem> for PortSlice {
    fn eq(&self, other: &ConnectedItem) -> bool {
        matches!(other, ConnectedItem::PortSlice(ps) if self == ps)
    }
}

// Ergonomic conversions to ConnectedItem
impl From<PortSlice> for ConnectedItem {
    fn from(value: PortSlice) -> Self {
        ConnectedItem::PortSlice(value)
    }
}

impl From<Tieoff> for ConnectedItem {
    fn from(value: Tieoff) -> Self {
        ConnectedItem::Tieoff(value)
    }
}

impl From<Wire> for ConnectedItem {
    fn from(value: Wire) -> Self {
        ConnectedItem::Wire(value)
    }
}

impl From<Unused> for ConnectedItem {
    fn from(value: Unused) -> Self {
        ConnectedItem::Unused(value)
    }
}
