// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToPortSlice, Port, PortSlice};

impl Port {
    /// See documentation for [`PortSlice::trace_through_hierarchy()`].
    pub fn trace_through_hierarchy(&self) -> Option<PortSlice> {
        self.to_port_slice().trace_through_hierarchy()
    }

    /// Returns `true` if any part of this port ultimately traces to a tieoff.
    pub fn has_tieoff_connection(&self) -> bool {
        self.to_port_slice().has_tieoff_connection()
    }
}
