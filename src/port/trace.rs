// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToPortSlice, Port, PortSlice};

impl Port {
    /// See documentation for [`PortSlice::trace_through_hierarchy()`].
    pub fn trace_through_hierarchy(&self) -> Option<PortSlice> {
        self.to_port_slice().trace_through_hierarchy()
    }
}
