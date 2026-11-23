// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToPortSlice, Port};

impl Port {
    /// Create a new port called `name` on the parent module and connects it to
    /// this port. The new port keeps the original layout and packed declaration dimensions.
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
        self.to_port_slice()
            .export_with_decl(name, self.decl_for_copy())
    }

    /// Same as [`Self::export_as`], but the new port is created with the same name as
    /// the port being exported. As a result, this method can only be used with
    /// ports on module instances. The method will panic if called on a port on
    /// a module definition.
    pub fn export(&self) -> Port {
        self.export_as(self.default_export_name())
    }

    /// Returns the instance port name for an export without an explicit name.
    pub(crate) fn default_export_name(&self) -> &str {
        match self {
            Port::ModDef { .. } => panic!(
                "Use export_as() to export {}, specifying the new name of the exported port.",
                self.debug_string()
            ),
            Port::ModInst { port_name, .. } => port_name,
        }
    }
}
