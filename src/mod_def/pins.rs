// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;

use crate::mod_def::dtypes::{Coordinate, PhysicalPin, Polygon};
use crate::PortSlice;

impl PortSlice {
    /// Define a physical pin for this single-bit PortSlice, with an arbitrary
    /// polygon shape relative to `position` on the given `layer`.
    pub fn define_physical_pin(
        &self,
        layer: impl AsRef<str>,
        position: Coordinate,
        polygon: Polygon,
    ) {
        self.check_validity();
        assert!(
            self.width() == 1,
            "define_physical_pin must be called on a single bit slice"
        );
        // Only allowed on ModDef ports (not instance ports)
        assert!(
            matches!(self.port, crate::Port::ModDef { .. }),
            "define_physical_pin must be called on a ModDef port"
        );

        let port_name = self.port.get_port_name();
        let bit = self.lsb; // since width()==1

        // Validate port exists and bit in range
        let core_borrow = self.get_mod_def_core();
        let mut core = core_borrow.borrow_mut();
        let io = core.ports.get(&port_name).unwrap_or_else(|| {
            panic!(
                "Port {}.{} does not exist (adding physical pin)",
                core.name, port_name
            )
        });
        let width = io.width();
        if bit >= width {
            panic!(
                "Bit {} out of range for port {}.{} with width {}",
                bit, core.name, port_name, width
            );
        }

        // Ensure vector of appropriate width exists on first use
        let pins_for_port = match core.physical_pins.entry(port_name.to_string()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(v) => v.insert(vec![None; width]),
        };

        pins_for_port[bit] = Some(PhysicalPin {
            layer: layer.as_ref().to_string(),
            position,
            polygon,
        });
    }
}
