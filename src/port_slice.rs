// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::rc::Rc;

use crate::{ModDefCore, Port};

mod connect;
mod export;
mod feedthrough;
mod tieoff;
/// Represents a slice of a port, which may be on a module definition or on a
/// module instance.
///
/// A slice is a defined as a contiguous range of bits from `msb` down to `lsb`,
/// inclusive. A slice can be a single bit on the port (`msb` equal to `lsb`),
/// the entire port, or any range in between.
#[derive(Clone, Debug)]
pub struct PortSlice {
    pub(crate) port: Port,
    pub(crate) msb: usize,
    pub(crate) lsb: usize,
}

impl PortSlice {
    /// Divides a port slice into `n` parts of equal bit width, return a vector
    /// of `n` port slices. For example, if a port is 8 bits wide and `n` is 2,
    /// the port will be divided into 2 slices of 4 bits each: `port[3:0]` and
    /// `port[7:4]`. This method panics if the port width is not divisible by
    /// `n`.
    pub fn subdivide(&self, n: usize) -> Vec<Self> {
        let width = self.msb - self.lsb + 1;
        if width % n != 0 {
            panic!(
                "Cannot subdivide {} into {} equal parts.",
                self.debug_string(),
                n
            );
        }
        (0..n)
            .map(move |i| {
                let sub_width = width / n;
                PortSlice {
                    port: self.port.clone(),
                    msb: ((i + 1) * sub_width) - 1 + self.lsb,
                    lsb: (i * sub_width) + self.lsb,
                }
            })
            .collect()
    }

    pub(crate) fn slice_relative(&self, offset: usize, width: usize) -> Self {
        assert!(offset + width <= self.width());

        PortSlice {
            port: self.port.clone(),
            msb: self.lsb + offset + width - 1,
            lsb: self.lsb + offset,
        }
    }

    pub(crate) fn width(&self) -> usize {
        self.msb - self.lsb + 1
    }

    pub(crate) fn debug_string(&self) -> String {
        format!("{}[{}:{}]", self.port.debug_string(), self.msb, self.lsb)
    }

    pub(crate) fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            PortSlice {
                port: Port::ModDef { mod_def_core, .. },
                ..
            } => mod_def_core.upgrade().unwrap(),
            PortSlice {
                port: Port::ModInst { mod_def_core, .. },
                ..
            } => mod_def_core.upgrade().unwrap(),
        }
    }

    pub(crate) fn check_validity(&self) {
        if self.msb >= self.port.io().width() {
            panic!(
                "Port slice {} is invalid: msb must be less than the width of the port.",
                self.debug_string()
            );
        } else if self.lsb > self.msb {
            panic!(
                "Port slice {} is invalid: lsb must be less than or equal to msb.",
                self.debug_string()
            );
        }
    }

    /// Returns the instance name corresponding to the port slice, if this is
    /// a port slice on an instance. Otherwise, returns `None`.
    pub(crate) fn get_inst_name(&self) -> Option<String> {
        match &self.port {
            Port::ModInst { inst_name, .. } => Some(inst_name.clone()),
            _ => None,
        }
    }
}

/// Indicates that a type can be converted to a `PortSlice`. `Port` and
/// `PortSlice` both implement this trait, which makes it easier to perform the
/// same operations on both.
pub trait ConvertibleToPortSlice {
    fn to_port_slice(&self) -> PortSlice;
}

impl ConvertibleToPortSlice for PortSlice {
    fn to_port_slice(&self) -> PortSlice {
        self.clone()
    }
}
