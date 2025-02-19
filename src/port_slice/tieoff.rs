// SPDX-License-Identifier: Apache-2.0

use num_bigint::BigInt;

use crate::{Port, PortSlice};

impl PortSlice {
    /// Ties off this port slice to the given constant value, specified as a
    /// `BigInt` or type that can be converted to a `BigInt`.
    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        let mod_def_core = self.get_mod_def_core();

        let big_int_value = value.into();

        mod_def_core
            .borrow_mut()
            .tieoffs
            .push(((*self).clone(), big_int_value.clone()));

        if let Port::ModInst {
            inst_name,
            port_name,
            ..
        } = &self.port
        {
            if self.port.io().width() == self.width() {
                // whole port tieoff
                mod_def_core
                    .borrow_mut()
                    .whole_port_tieoffs
                    .entry(inst_name.clone())
                    .or_default()
                    .insert(port_name.clone(), big_int_value);
            }
        }
    }

    /// Marks this port slice as unused, meaning that if it is an module
    /// instance output or module definition input, validation will not fail if
    /// the slice drives nothing. In fact, validation will fail if the slice
    /// drives anything.
    pub fn unused(&self) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core.borrow_mut().unused.push((*self).clone());

        if let Port::ModInst {
            inst_name,
            port_name,
            ..
        } = &self.port
        {
            if self.port.io().width() == self.width() {
                // the whole port is unnused
                mod_def_core
                    .borrow_mut()
                    .whole_port_unused
                    .entry(inst_name.clone())
                    .or_default()
                    .insert(port_name.clone());
            }
        }
    }
}
