// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::{Intf, ModDef};

impl Intf {
    /// Creates a new interface on the parent module and connects it to this
    /// interface. The new interface will have the same functions as this
    /// interface; signal names are formed by concatenating the given prefix and
    /// the function name. For example, if this interface is `{"data": "a_data",
    /// "valid": "a_valid"}` and the prefix is "b_", the new interface will be
    /// `{"data": "b_data", "valid": "b_valid"}`. The `name` argument specifies
    /// the name of the new interface, which is used to retrieve the interface
    /// with `get_intf`.
    pub fn export_with_prefix(&self, name: impl AsRef<str>, prefix: impl AsRef<str>) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let mod_def_port_name = format!("{}{}", prefix.as_ref(), func_name);
            port_slice.export_as(&mod_def_port_name);
            mapping.insert(func_name, (mod_def_port_name, port_slice.width() - 1, 0));
        }
        ModDef {
            core: self.get_mod_def_core(),
        }
        .def_intf(name, mapping)
    }

    /// Export an interface using the given name, with a signal prefix of the
    /// name followed by an underscore. For example, if a block has an interface
    /// called "a" with signals "a_data" and "a_valid", calling
    /// export_with_name_underscore("b") will create a new interface called "b"
    /// with signals "b_data" and "b_valid".
    pub fn export_with_name_underscore(&self, name: impl AsRef<str>) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.export_with_prefix(name, prefix)
    }

    /// Exports an interface from a module instance to the parent module
    /// definition, returning a new interface. The new interface has the same
    /// name as the original interface, as well as the same signal names and
    /// signal functions. For example, calling this method on an interface on an
    /// intance called "a" with signals "a_data" and "a_valid" will create a new
    /// interface called "a" on the parent module definition with signals
    /// "a_data" and "a_valid".
    pub fn export(&self) -> Intf {
        if matches!(self, Intf::ModDef { .. }) {
            panic!(
                "Cannot export() {}; must use export_with_prefix() or export_with_name_underscore() instead.",
                self.debug_string()
            );
        }

        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let exported_port = port_slice.export();
            mapping.insert(
                func_name,
                (exported_port.get_port_name(), port_slice.width() - 1, 0),
            );
        }
        ModDef {
            core: self.get_mod_def_core(),
        }
        .def_intf(self.get_intf_name(), mapping)
    }
}
