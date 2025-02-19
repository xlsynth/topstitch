// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToPortSlice, Port};

impl Port {
    /// Create a new port called `name` on the parent module and connects it to
    /// this port.
    ///
    /// The exact behavior depends on whether this is a port on a module
    /// definition or a module instance. If this is a port on a module
    /// definition, a new port is created on the same module definition, with
    /// the same width, but opposite direction. For example, suppose that this
    /// is a port `a` on a module definition that is an 8-bit input; calling
    /// `export_as("y")` will create an 8-bit output on the same module
    /// definition called `y`.
    ///
    /// If, on the other hand, this is a port on a module instance, a new port
    /// will be created on the module definition containing the instance, with
    /// the same width and direction. For example, if this is an 8-bit input
    /// port `x` on a module instance, calling `export_as("y")` will create a
    /// new 8-bit input port `y` on the module definition that contains the
    /// instance.
    pub fn export_as(&self, name: impl AsRef<str>) -> Port {
        self.to_port_slice().export_as(name)
    }

    /// Same as export_as(), but the new port is created with the same name as
    /// the port being exported. As a result, this method can only be used with
    /// ports on module instances. The method will panic if called on a port on
    /// a module definition.
    pub fn export(&self) -> Port {
        self.to_port_slice().export()
    }
}
