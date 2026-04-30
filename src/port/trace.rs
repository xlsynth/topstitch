// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToPortSlice, Port, PortSlice};

impl Port {
    /// See documentation for [`PortSlice::trace_through_hierarchy()`].
    pub fn trace_through_hierarchy(&self) -> Option<PortSlice> {
        self.to_port_slice().trace_through_hierarchy()
    }

    /// See documentation for [`PortSlice::get_connection_distance()`]. This
    /// currently supports only single-bit ports.
    pub fn get_connection_distance(&self) -> Option<i64> {
        self.to_port_slice().get_connection_distance()
    }

    /// See documentation for
    /// [`PortSlice::get_connected_port_slice_and_distance()`]. This currently
    /// supports only single-bit ports.
    pub fn get_connected_port_slice_and_distance(&self) -> Option<(PortSlice, i64)> {
        self.to_port_slice().get_connected_port_slice_and_distance()
    }

    /// Returns `true` if any part of this port ultimately traces to a tieoff.
    pub fn has_tieoff_connection(&self) -> bool {
        self.to_port_slice().has_tieoff_connection()
    }
}
