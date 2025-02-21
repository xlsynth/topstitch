// SPDX-License-Identifier: Apache-2.0

use crate::Intf;

impl Intf {
    pub fn width(&self) -> usize {
        self.get_port_slices()
            .values()
            .map(|slice| slice.width())
            .sum()
    }
}
