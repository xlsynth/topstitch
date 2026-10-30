// SPDX-License-Identifier: Apache-2.0

use crate::port::{PortDecl, PortType};
use crate::{ModDef, Port, PortSlice};

impl PortSlice {
    /// Create a new port called `name` on the parent module and connects it to
    /// this port slice. A whole-port export retains its packed layout for
    /// field and element selection. A partial slice exports only bits. Both emit flat ports.
    ///
    /// The exact behavior depends on whether this is a port slice on a module
    /// definition or a module instance. If this is a port slice on a module
    /// definition, a new port is created on the same module definition, with
    /// the same width, but opposite direction. For example, suppose that this
    /// is a port slice `a` on a module definition that is an 8-bit input;
    /// calling `export_as("y")` will create an 8-bit output on the same
    /// module definition called `y`.
    ///
    /// If, on the other hand, this is a port slice on a module instance, a new
    /// port will be created on the module definition containing the
    /// instance, with the same width and direction. For example, if this is
    /// an 8-bit input port `x` on a module instance, calling
    /// `export_as("y")` will create a new 8-bit input port `y` on the
    /// module definition that contains the instance.
    pub fn export_as(&self, name: impl AsRef<str>) -> Port {
        let ty = self
            .whole_packed_shape()
            .map_or(PortType::Bits, PortType::Packed);
        self.export_with_type(name, ty)
    }

    /// Exports these wires with the layout selected by the caller.
    pub(crate) fn export_with_type(&self, name: impl AsRef<str>, ty: PortType) -> Port {
        let io = match self.port {
            Port::ModDef { .. } => self.port.io().with_width(self.width()).flip(),
            Port::ModInst { .. } => self.port.io().with_width(self.width()),
        };

        let core = self.get_mod_def_core();
        let moddef = ModDef { core };

        let new_port = moddef.add_port(name, PortDecl { io, ty });
        self.connect(&new_port);

        new_port
    }

    /// Same as export_as(), but the new port is created with the same name as
    /// the port being exported. As a result, this method can only be used with
    /// ports on module instances. The method will panic if called on a port
    /// slice on a module definition.
    pub fn export(&self) -> Port {
        self.export_as(self.default_export_name())
    }

    /// Returns the instance port name for an export without an explicit name.
    pub(crate) fn default_export_name(&self) -> &str {
        match &self.port {
            Port::ModDef { .. } => panic!(
                "Use export_as() to export {}, specifying the new name of the exported port.",
                self.debug_string()
            ),
            Port::ModInst { port_name, .. } => port_name,
        }
    }
}
