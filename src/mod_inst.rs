// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::rc::Weak;

use crate::{ConvertibleToModDef, Intf, ModDef, ModDefCore, Port, PortSlice};

/// Represents an instance of a module definition, like `<mod_def_name>
/// <mod_inst_name> ( ... );` in Verilog.
#[derive(Clone)]
pub struct ModInst {
    pub(crate) name: String,
    pub(crate) mod_def_core: Weak<RefCell<ModDefCore>>,
}

impl ModInst {
    /// Returns `true` if this module instance has an interface with the given
    /// name.
    pub fn has_intf(&self, name: impl AsRef<str>) -> bool {
        ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .has_intf(name)
    }

    /// Returns `true` if this module instance has a port with the given name.
    pub fn has_port(&self, name: impl AsRef<str>) -> bool {
        ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .has_port(name)
    }

    /// First, get the module definition for this instance. Then, return the
    /// module instance with the given name in that module defintion.
    pub fn get_instance(&self, name: impl AsRef<str>) -> ModInst {
        self.get_mod_def().get_instance(name)
    }

    /// Returns the port on this instance with the given name. Panics if no such
    /// port exists.
    pub fn get_port(&self, name: impl AsRef<str>) -> Port {
        ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .get_port(name)
        .assign_to_inst(self)
    }

    /// Returns a slice of the port on this instance with the given name, from
    /// `msb` down to `lsb`, inclusive. Panics if no such port exists.
    pub fn get_port_slice(&self, name: impl AsRef<str>, msb: usize, lsb: usize) -> PortSlice {
        self.get_port(name).slice(msb, lsb)
    }

    /// Returns a vector of ports on this instance with the given prefix, or all
    /// ports if `prefix` is `None`.
    pub fn get_ports(&self, prefix: Option<&str>) -> Vec<Port> {
        let result = ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .get_ports(prefix);
        result
            .into_iter()
            .map(|port| port.assign_to_inst(self))
            .collect()
    }

    /// Returns the interface on this instance with the given name. Panics if no
    /// such interface exists.
    pub fn get_intf(&self, name: impl AsRef<str>) -> Intf {
        let mod_def_core = self.mod_def_core.upgrade().unwrap();
        let instances = &mod_def_core.borrow().instances;

        let inst_core = match instances.get(&self.name) {
            Some(inst_core) => inst_core.clone(),
            None => panic!(
                "Interface '{}' does not exist on module definition '{}'",
                name.as_ref(),
                mod_def_core.borrow().name
            ),
        };

        let inst_core_borrowed = inst_core.borrow();

        if inst_core_borrowed.interfaces.contains_key(name.as_ref()) {
            Intf::ModInst {
                intf_name: name.as_ref().to_string(),
                inst_name: self.name.clone(),
                mod_def_core: self.mod_def_core.clone(),
            }
        } else {
            panic!(
                "Interface '{}' does not exist in instance '{}'",
                name.as_ref(),
                self.debug_string()
            );
        }
    }

    /// Returns the ModDef that this is an instance of.
    pub fn get_mod_def(&self) -> ModDef {
        ModDef {
            core: self
                .mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .instances
                .get(&self.name)
                .unwrap_or_else(|| panic!("Instance named {} not found", self.name))
                .clone(),
        }
    }

    pub(crate) fn debug_string(&self) -> String {
        format!(
            "{}.{}",
            self.mod_def_core.upgrade().unwrap().borrow().name,
            self.name
        )
    }
}

impl ConvertibleToModDef for ModInst {
    fn to_mod_def(&self) -> ModDef {
        self.get_mod_def()
    }
    fn get_port(&self, name: impl AsRef<str>) -> Port {
        self.get_port(name)
    }
    fn get_intf(&self, name: impl AsRef<str>) -> Intf {
        self.get_intf(name)
    }
}
