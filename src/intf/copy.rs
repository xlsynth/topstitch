// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::{ConvertibleToModDef, Intf};

impl Intf {
    pub fn flip_to(&self, mod_def_or_mod_inst: &impl ConvertibleToModDef) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let port = mod_def_or_mod_inst
                .to_mod_def()
                .add_port(port_slice.port.name(), port_slice.port.io().flip());
            mapping.insert(func_name, (port.get_port_name(), port_slice.width() - 1, 0));
        }
        mod_def_or_mod_inst
            .to_mod_def()
            .def_intf(self.get_intf_name(), mapping);
        mod_def_or_mod_inst.get_intf(self.get_intf_name())
    }

    pub fn copy_to(&self, mod_def_or_mod_inst: &impl ConvertibleToModDef) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let port = mod_def_or_mod_inst.to_mod_def().add_port(
                port_slice.port.name(),
                port_slice.port.io().with_width(port_slice.width()),
            );
            mapping.insert(func_name, (port.get_port_name(), port_slice.width() - 1, 0));
        }
        mod_def_or_mod_inst
            .to_mod_def()
            .def_intf(self.get_intf_name(), mapping);
        mod_def_or_mod_inst.get_intf(self.get_intf_name())
    }

    pub fn copy_to_with_prefix(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        name: impl AsRef<str>,
        prefix: impl AsRef<str>,
    ) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let port_name = format!("{}{}", prefix.as_ref(), func_name);
            mod_def_or_mod_inst.to_mod_def().add_port(
                &port_name,
                port_slice.port.io().with_width(port_slice.width()),
            );
            mapping.insert(func_name, (port_name, port_slice.width() - 1, 0));
        }
        mod_def_or_mod_inst.to_mod_def().def_intf(&name, mapping);
        mod_def_or_mod_inst.get_intf(&name)
    }

    pub fn copy_to_with_name_underscore(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        name: impl AsRef<str>,
    ) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.copy_to_with_prefix(mod_def_or_mod_inst, name, prefix)
    }

    pub fn flip_to_with_prefix(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        name: impl AsRef<str>,
        prefix: impl AsRef<str>,
    ) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let port_name = format!("{}{}", prefix.as_ref(), func_name);
            mod_def_or_mod_inst.to_mod_def().add_port(
                &port_name,
                port_slice.port.io().with_width(port_slice.width()).flip(),
            );
            mapping.insert(func_name, (port_name, port_slice.width() - 1, 0));
        }
        mod_def_or_mod_inst.to_mod_def().def_intf(&name, mapping);
        mod_def_or_mod_inst.get_intf(&name)
    }

    pub fn flip_to_with_name_underscore(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        name: impl AsRef<str>,
    ) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.flip_to_with_prefix(mod_def_or_mod_inst, name, prefix)
    }
}
