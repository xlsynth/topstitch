// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::{ConvertibleToModDef, Intf};

impl Intf {
    /// Copies the interface and its port names with each port's direction reversed.
    /// Whole ports keep their packed dimensions. Partial port slices become flat ports.
    pub fn flip_to(&self, mod_def_or_mod_inst: &impl ConvertibleToModDef) -> Intf {
        self.copy_to_impl(mod_def_or_mod_inst, self.name(), None, true)
    }

    /// Copies the interface and its port names. Whole ports keep their packed dimensions.
    /// Partial port slices become flat ports of the selected width.
    pub fn copy_to(&self, mod_def_or_mod_inst: &impl ConvertibleToModDef) -> Intf {
        self.copy_to_impl(mod_def_or_mod_inst, self.name(), None, false)
    }

    /// Like [`Self::copy_to`], using the given interface name and prefixing each function name
    /// to form its new port name.
    pub fn copy_to_with_prefix(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        name: impl AsRef<str>,
        prefix: impl AsRef<str>,
    ) -> Intf {
        self.copy_to_impl(
            mod_def_or_mod_inst,
            name.as_ref(),
            Some(prefix.as_ref()),
            false,
        )
    }

    /// Like [`Self::copy_to_with_prefix`], using the interface name followed by an underscore as the prefix.
    pub fn copy_to_with_name_underscore(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        name: impl AsRef<str>,
    ) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.copy_to_with_prefix(mod_def_or_mod_inst, name, prefix)
    }

    /// Like [`Self::flip_to`], using the given interface name and prefixing each function name
    /// to form its new port name.
    pub fn flip_to_with_prefix(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        name: impl AsRef<str>,
        prefix: impl AsRef<str>,
    ) -> Intf {
        self.copy_to_impl(
            mod_def_or_mod_inst,
            name.as_ref(),
            Some(prefix.as_ref()),
            true,
        )
    }

    /// Like [`Self::flip_to_with_prefix`], using the interface name followed by an underscore as the prefix.
    pub fn flip_to_with_name_underscore(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        name: impl AsRef<str>,
    ) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.flip_to_with_prefix(mod_def_or_mod_inst, name, prefix)
    }

    /// Copies the member declarations and returns the interface on the requested definition or instance.
    fn copy_to_impl(
        &self,
        target: &impl ConvertibleToModDef,
        name: &str,
        prefix: Option<&str>,
        flip: bool,
    ) -> Intf {
        let module = target.to_mod_def();
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let decl = Self::member_decl(&port_slice);
            let port_name = match prefix {
                Some(prefix) => format!("{prefix}{func_name}"),
                None => port_slice.port.name().to_owned(),
            };
            module.add_port(&port_name, if flip { decl.flipped() } else { decl });
            mapping.insert(func_name, (port_name, port_slice.width() - 1, 0));
        }
        module.def_intf(name, mapping);
        target.get_intf(name)
    }
}
