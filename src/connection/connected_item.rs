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
    Unused,
    Wire(Wire),
}

/// Represents a tieoff connected to a PortSlice.
#[derive(Clone, PartialEq)]
pub struct Tieoff {
    pub value: BigInt,
}

impl Tieoff {
    pub fn new<T: Into<BigInt>>(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Returns a new Tieoff value that is a slice of this one, with the given
    /// offset and width.
    pub fn slice_with_offset_and_width(&self, offset: usize, width: usize) -> Tieoff {
        let mask = (BigInt::from(1u32) << width) - 1;
        Tieoff::new(self.value.clone() >> offset & mask)
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
    pub fn slice_with_offset_and_width(&self, offset: usize, width: usize) -> ConnectedItem {
        match self {
            ConnectedItem::PortSlice(port_slice) => {
                port_slice.slice_with_offset_and_width(offset, width).into()
            }
            ConnectedItem::Tieoff(tieoff) => {
                tieoff.slice_with_offset_and_width(offset, width).into()
            }
            ConnectedItem::Unused => ConnectedItem::Unused,
            ConnectedItem::Wire(wire) => wire.slice_with_offset_and_width(offset, width).into(),
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
            if let ConnectedItem::Unused = &connection.other {
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
