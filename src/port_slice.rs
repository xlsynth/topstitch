// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::fmt::{self, Debug};
use std::rc::Rc;

use crate::connection::port_slice::PortSliceConnections;
use crate::mod_inst::HierPathElem;
use crate::port::PortDirectionality;
use crate::{Coordinate, EdgeOrientation, Mat3, ModDef, ModDefCore, PhysicalPin, Port};

mod connect;
mod export;
mod feedthrough;
mod tieoff;
mod trace;

/// Represents a slice of a port, which may be on a module definition or on a
/// module instance.
///
/// A slice is a defined as a contiguous range of bits from `msb` down to `lsb`,
/// inclusive. A slice can be a single bit on the port (`msb` equal to `lsb`),
/// the entire port, or any range in between.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PortSlice {
    pub(crate) port: Port,
    pub(crate) msb: usize,
    pub(crate) lsb: usize,
}

impl Debug for PortSlice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.debug_string())
    }
}

impl PortSlice {
    /// Places this port based on what has been connected to it.
    pub fn place_abutted(&self) {
        for bit_index in self.lsb..=self.msb {
            let bit = self.port.bit(bit_index);
            bit.place_abutted_to(bit.trace_through_hierarchy().unwrap());
        }
    }

    /// Place this single-bit slice by deriving coordinates from `source`,
    /// snapping to the nearest usable edge and track.
    pub fn place_abutted_to<T: ConvertibleToPortSlice>(&self, source: T) {
        self.check_validity();

        let source_slice = source.to_port_slice();
        source_slice.check_validity();

        let width = self.width();
        let source_width = source_slice.width();

        if width != source_width {
            panic!(
                "Width mismatch when placing {} with respect to {}",
                self.debug_string(),
                source_slice.debug_string()
            );
        }

        if width == 1 {
            let source_pin = source_slice.get_physical_pin();
            self.place_from_physical_pin(&source_pin);
        } else {
            for i in 0..width {
                let self_bit = self.slice_with_offset_and_width(i, 1);
                let source_bit = source_slice.slice_with_offset_and_width(i, 1);
                self_bit.place_abutted_to(source_bit);
            }
        }
    }

    /// Place this single-bit slice on the edge opposite `source`, preserving
    /// the layer and track index.
    pub fn place_across_from<T: ConvertibleToPortSlice>(&self, source: T) {
        self.check_validity();

        let source_slice = source.to_port_slice();
        source_slice.check_validity();

        let width = self.width();
        let source_width = source_slice.width();

        if width != source_width {
            panic!(
                "Width mismatch when placing {} with respect to {}",
                self.debug_string(),
                source_slice.debug_string()
            );
        }

        if width == 1 {
            let src_mod_def = source_slice.get_mod_def_where_declared();
            let src_mod_def_core = src_mod_def.core;
            let dst_mod_def = self.get_mod_def_where_declared();
            let dst_mod_def_core = dst_mod_def.core;
            if !Rc::ptr_eq(&src_mod_def_core, &dst_mod_def_core) {
                panic!(
                    "place_across_from requires source and target slices to belong to the same module definition"
                );
            }

            let core = src_mod_def_core.borrow();
            let src_pin = core.get_physical_pin(source_slice.port.name(), source_slice.lsb);

            let dst_edge_idx = core
                .shape
                .as_ref()
                .unwrap()
                .find_opposite_edge(&src_pin.position)
                .unwrap_or_else(|err| panic!("{}", err));
            let dst_edge = core.shape.as_ref().unwrap().get_edge(dst_edge_idx);
            drop(core);

            let dst_coordinate = match dst_edge.orientation() {
                Some(EdgeOrientation::North | EdgeOrientation::South) => {
                    src_pin.position.with_x(dst_edge.a.x)
                }
                Some(EdgeOrientation::East | EdgeOrientation::West) => {
                    src_pin.position.with_y(dst_edge.a.y)
                }
                None => panic!("Edge is not axis-aligned; only rectilinear edges are supported"),
            };

            PortSlice {
                port: Port::ModDef {
                    mod_def_core: Rc::downgrade(&dst_mod_def_core),
                    name: self.port.name().to_string(),
                },
                msb: self.msb,
                lsb: self.lsb,
            }
            .place_from_physical_pin(&PhysicalPin {
                layer: src_pin.layer,
                position: dst_coordinate,
                polygon: src_pin.polygon,
            });
        } else {
            for i in 0..width {
                let self_bit = self.slice_with_offset_and_width(i, 1);
                let source_bit = source_slice.slice_with_offset_and_width(i, 1);
                self_bit.place_across_from(source_bit);
            }
        }
    }

    /// Place this single-bit slice using the provided physical pin. The caller
    /// is responsible for ensuring the pin is in the appropriate coordinate
    /// space.
    pub fn place_from_physical_pin(&self, source_pin: &PhysicalPin) {
        // TODO: should this take into account the polygon in the PhysicalPin object?

        self.check_validity();
        assert!(self.width() == 1, "place_from requires single-bit slices");

        let (target_mod_def, target_transform) = match &self.port {
            Port::ModDef { .. } => (self.get_mod_def(), Mat3::identity()),
            Port::ModInst { .. } => {
                let inst = self
                    .port
                    .get_mod_inst()
                    .expect("Port::ModInst hierarchy cannot be empty");
                (inst.get_mod_def(), inst.get_transform())
            }
        };

        let inverse_transform = target_transform.inverse();
        let source_coord_local = source_pin.position.apply_transform(&inverse_transform);
        let layer_name = &source_pin.layer;

        let shape = target_mod_def
            .get_shape()
            .expect("Target module must have a defined shape");
        let track = target_mod_def
            .get_track(layer_name)
            .unwrap_or_else(|| panic!("Unknown track layer '{}'", layer_name));

        let edge_index = shape
            .closest_edge_index_where(&source_coord_local, |edge| {
                edge.get_index_range(&track).is_some()
            })
            .expect("No compatible edge found for placement");

        let relative_track_index = target_mod_def
            .nearest_relative_track_index(edge_index, layer_name, &source_coord_local)
            .expect("Track range must exist for selected edge");

        let port_name = self.port.get_port_name();
        target_mod_def.place_pin_on_edge_index(
            port_name,
            self.lsb,
            edge_index,
            layer_name,
            relative_track_index,
        );
    }

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

    pub(crate) fn slice_with_offset_and_width(&self, offset: usize, width: usize) -> Self {
        assert!(offset + width <= self.width());

        PortSlice {
            port: self.port.clone(),
            msb: self.lsb + offset + width - 1,
            lsb: self.lsb + offset,
        }
    }

    /// Returns the width of the port slice.
    pub fn width(&self) -> usize {
        self.msb - self.lsb + 1
    }

    pub(crate) fn debug_string(&self) -> String {
        format!("{}[{}:{}]", self.port.debug_string(), self.msb, self.lsb)
    }

    pub(crate) fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        self.port.get_mod_def_core()
    }

    pub(crate) fn get_mod_def(&self) -> ModDef {
        ModDef {
            core: self.get_mod_def_core(),
        }
    }

    pub(crate) fn get_mod_def_where_declared(&self) -> ModDef {
        self.port.get_mod_def_where_declared()
    }

    /// Return the `PortSliceConnections` associated with this port slice, if
    /// any.
    pub(crate) fn get_port_connections(&self) -> Option<PortSliceConnections> {
        let connections = self.port.get_port_connections()?;
        let connections_borrowed = connections.borrow();
        Some(connections_borrowed.slice(self.msb, self.lsb))
    }

    /// Returns a new `PortSlice` with the same port name, MSB, and LSB, but the
    /// provided hierarchy.
    pub(crate) fn as_mod_inst_port_slice(&self, hierarchy: Vec<HierPathElem>) -> PortSlice {
        PortSlice {
            port: self.port.as_mod_inst_port(hierarchy),
            msb: self.msb,
            lsb: self.lsb,
        }
    }

    /// Returns a new `PortSlice` with the same port name, MSB, and LSB, but a
    /// module definition pointer that corresponds to the module where the port
    /// is declared. For example, if `top` instantiates `a` as `a_inst` and
    /// `a_inst` has a port `x`, calling this function on `top.a_inst.x\[3:2\]`
    /// will effectively return `a.x\[3:2\]` (i.e., the port slice on the
    /// module definition of `a`).
    pub(crate) fn as_mod_def_port_slice(&self) -> PortSlice {
        PortSlice {
            port: self.port.as_mod_def_port(),
            msb: self.msb,
            lsb: self.lsb,
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
        self.port.inst_name().map(|name| name.to_string())
    }

    /// Returns `(port name, bit index)` pairs describing every bit covered by
    /// this slice, ordered from LSB to MSB.
    pub fn to_bits(&self) -> Vec<(&str, usize)> {
        (self.lsb..=self.msb)
            .map(|i| (self.port.name(), i))
            .collect()
    }

    /// Returns the physical pin for this slice. For ModDef ports, this is in
    /// the local coordinate space, whereas for ModInst ports, this is with
    /// respect to the coordinate space of the parent module.
    pub fn get_physical_pin(&self) -> PhysicalPin {
        if self.width() != 1 {
            panic!(
                "Port slice {} must be a single bit to compute a pin position",
                self.debug_string()
            );
        }

        match &self.port {
            Port::ModDef { mod_def_core, name } => mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .get_physical_pin(name, self.lsb),
            Port::ModInst { .. } => {
                let mod_inst = self
                    .port
                    .get_mod_inst()
                    .expect("Port::ModInst hierarchy cannot be empty");
                let transform = mod_inst.get_transform();
                let pin = mod_inst
                    .get_mod_def()
                    .get_physical_pin(self.port.name(), self.lsb);

                let position = pin.position.apply_transform(&transform);
                let polygon = pin.polygon.apply_transform(&transform);
                PhysicalPin {
                    layer: pin.layer,
                    position,
                    polygon,
                }
            }
        }
    }

    /// Returns the physical coordinate of this single-bit port slice.
    pub fn get_coordinate(&self) -> Coordinate {
        self.get_physical_pin().position
    }

    pub(crate) fn try_merge(&self, other: &PortSlice) -> Option<PortSlice> {
        if self.port == other.port && (self.lsb == (other.msb + 1)) {
            Some(PortSlice {
                port: self.port.clone(),
                msb: self.msb,
                lsb: other.lsb,
            })
        } else {
            None
        }
    }

    pub(crate) fn get_directionality(&self) -> PortDirectionality {
        self.port.get_directionality()
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
