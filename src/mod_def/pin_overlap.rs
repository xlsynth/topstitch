// SPDX-License-Identifier: Apache-2.0

use super::polygon_overlap::OverlapPolygon;
use crate::{ModDef, Polygon};
use rstar::{AABB, RTree, RTreeObject};
use std::{collections::BTreeMap, fmt};

/// Two placed port bits whose polygons overlap on one metal layer.
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
            "{}[{}] and {}[{}] overlap on {}",
            a.0, a.1, b.0, b.1, self.layer
        )
    }
}

impl std::error::Error for PinOverlap {}

impl ModDef {
    /// Checks the placed port bits of this module for positive-area overlap on the same layer.
    /// Returns the first collision. Unplaced bits, pins on child instances, and pins that
    /// touch only at an edge or corner are excluded.
    pub fn check_pin_overlaps(&self) -> Result<(), PinOverlap> {
        let core = self.core.read();
        let mut layers = BTreeMap::<&str, Vec<PlacedPin>>::new();
        for (port, bits) in &core.physical_pins {
            for (bit, pin) in bits.iter().enumerate() {
                if let Some(pin) = pin {
                    layers.entry(&pin.layer).or_default().push(PlacedPin {
                        port,
                        bit,
                        polygon: OverlapPolygon::new(pin.transformed_polygon()),
                    });
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
                if (a.port, a.bit) < (b.port, b.bit) && a.polygon.overlaps_in_area(&b.polygon) {
                    return Err(PinOverlap::new(
                        layer,
                        [(a.port.to_owned(), a.bit), (b.port.to_owned(), b.bit)],
                    ));
                }
            }
        }
        Ok(())
    }
}

struct PlacedPin<'a> {
    port: &'a str,
    bit: usize,
    polygon: OverlapPolygon<Polygon>,
}

impl RTreeObject for PlacedPin<'_> {
    type Envelope = AABB<[i64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let bbox = &self.polygon.bbox;
        AABB::from_corners([bbox.min_x, bbox.min_y], [bbox.max_x, bbox.max_y])
    }
}
