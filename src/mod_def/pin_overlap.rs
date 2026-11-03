// SPDX-License-Identifier: Apache-2.0

use super::{ModDefCore, OverlapPolygon};
use crate::ModDef;
use rstar::{RTree, primitives::GeomWithData};
use std::{collections::BTreeMap, fmt};

/// Two placed port bits whose polygons touch or overlap on one metal layer.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PinOverlap {
    pub layer: String,
    pins: [(String, usize); 2],
}

impl PinOverlap {
    /// Creates a collision on this layer. The order of the pins does not affect equality or hashing.
    pub fn new(layer: impl Into<String>, mut pins: [(String, usize); 2]) -> Self {
        pins.sort_unstable();
        Self {
            layer: layer.into(),
            pins,
        }
    }

    /// Returns the two pins ordered by port name, then bit index.
    pub fn pins(&self) -> &[(String, usize); 2] {
        &self.pins
    }
}

impl fmt::Display for PinOverlap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b] = &self.pins;
        write!(
            f,
            "{}[{}] and {}[{}] touch or overlap on {}",
            a.0, a.1, b.0, b.1, self.layer
        )
    }
}

impl std::error::Error for PinOverlap {}

impl ModDef {
    /// Returns the first pair of placed port bits on the same layer whose polygons share a point.
    /// Edge and corner contact both count. Unplaced bits and pins on child instances are excluded.
    pub fn check_pin_overlaps(&self) -> Result<(), PinOverlap> {
        self.core.read().check_pin_overlaps()
    }
}

impl ModDefCore {
    /// Checks the placed pins while the caller holds the module's read lock.
    pub(super) fn check_pin_overlaps(&self) -> Result<(), PinOverlap> {
        let mut layers = BTreeMap::<&str, Vec<_>>::new();
        for (port, bits) in &self.physical_pins {
            for (bit, pin) in bits.iter().enumerate() {
                if let Some(pin) = pin {
                    let polygon = OverlapPolygon::new(pin.transformed_polygon());
                    let placed = GeomWithData::new(polygon, (port.as_str(), bit));
                    layers.entry(&pin.layer).or_default().push(placed);
                }
            }
        }

        for (layer, pins) in layers {
            if pins.len() < 2 {
                continue;
            }
            let tree = RTree::bulk_load(pins);
            for (a, b) in tree.intersection_candidates_with_other_tree(&tree) {
                // The self-query yields each pin with itself and both orders of each pair.
                if a.data < b.data && a.geom().intersects_candidate(b.geom()) {
                    return Err(PinOverlap::new(
                        layer,
                        [a.data, b.data].map(|(port, bit)| (port.to_owned(), bit)),
                    ));
                }
            }
        }
        Ok(())
    }
}
