// SPDX-License-Identifier: Apache-2.0

use super::polygon_overlap::OverlapPolygon;
use crate::{ModDef, Polygon};
use std::fmt;

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
        struct PlacedPin<'a> {
            port: &'a str,
            bit: usize,
            layer: &'a str,
            polygon: OverlapPolygon<Polygon>,
        }

        let core = self.core.read();
        let mut pins = Vec::new();
        for (port, bits) in &core.physical_pins {
            for (bit, pin) in bits.iter().enumerate() {
                if let Some(pin) = pin {
                    pins.push(PlacedPin {
                        port,
                        bit,
                        layer: &pin.layer,
                        polygon: OverlapPolygon::new(pin.transformed_polygon()),
                    });
                }
            }
        }

        pins.sort_by_key(|pin| (pin.layer, pin.polygon.bbox.min_y));
        for (i, a) in pins.iter().enumerate() {
            for b in &pins[i + 1..] {
                if b.layer != a.layer || b.polygon.bbox.min_y >= a.polygon.bbox.max_y {
                    break;
                }
                if !a.polygon.overlaps_in_area(&b.polygon) {
                    continue;
                }
                return Err(PinOverlap::new(
                    a.layer,
                    [(a.port.to_owned(), a.bit), (b.port.to_owned(), b.bit)],
                ));
            }
        }
        Ok(())
    }
}
