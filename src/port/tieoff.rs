// SPDX-License-Identifier: Apache-2.0

use num_bigint::BigInt;

use crate::{ConvertibleToPortSlice, Port};

impl Port {
    /// Ties off this port to the given constant value, specified as a `BigInt`
    /// or type that can be converted to a `BigInt`.
    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        self.to_port_slice().tieoff(value);
    }

    /// Marks this port as unused, meaning that if it is a module instance
    /// output or module definition input, validation will not fail if the port
    /// drives nothing. In fact, validation will fail if the port drives
    /// anything.
    pub fn unused(&self) {
        self.to_port_slice().unused();
    }
}
