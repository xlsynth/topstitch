// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToModDef, Port, PortDecl};

impl Port {
    /// Adds an unconnected port with the same name, direction, width and packed layout.
    /// Passing an instance creates the port on its definition and returns the instance port.
    pub fn copy_to(&self, target: &impl ConvertibleToModDef) -> Port {
        target
            .to_mod_def()
            .add_port(self.name(), self.decl_for_copy());
        target.get_port(self.name())
    }

    /// Like [`Self::copy_to`], with the opposite direction. An inout stays an inout.
    pub fn flip_to(&self, target: &impl ConvertibleToModDef) -> Port {
        target
            .to_mod_def()
            .add_port(self.name(), self.decl_for_copy().flipped());
        target.get_port(self.name())
    }

    /// Reads the direction, width and packed layout to use when creating another port.
    pub(crate) fn decl_for_copy(&self) -> PortDecl {
        self.get_mod_def_core_where_declared().read().ports[self.name()].for_copy()
    }
}
