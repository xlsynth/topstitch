// SPDX-License-Identifier: Apache-2.0

use num_bigint::BigInt;

use crate::connection::{
    connected_item::{Tieoff, Unused},
    port_slice::Abutment,
};
use crate::port_slice::ConvertibleToPortSlice;
use crate::PortSlice;

impl PortSlice {
    /// Ties off this port slice to the given constant value, specified as a
    /// `BigInt` or type that can be converted to a `BigInt`.
    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        let big_int_value = value.into();

        self.port
            .get_port_connections_define_if_missing()
            .borrow_mut()
            .add(
                self.to_port_slice(),
                Tieoff::new(big_int_value, self.width()),
                Abutment::NA,
            );
    }

    /// Marks this port slice as unused, meaning that if it is an module
    /// instance output or module definition input, validation will not fail if
    /// the slice drives nothing. In fact, validation will fail if the slice
    /// drives anything.
    pub fn unused(&self) {
        self.port
            .get_port_connections_define_if_missing()
            .borrow_mut()
            .add(self.to_port_slice(), Unused::new(), Abutment::NA);
    }
}
