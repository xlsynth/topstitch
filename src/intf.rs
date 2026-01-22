// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use indexmap::IndexMap;

use crate::mod_def::ModDefCore;
use crate::{MetadataKey, MetadataValue, ModDef, PortSlice};

mod connect;
mod copy;
mod crossover;
mod debug;
mod export;
mod feedthrough;
mod subdivide;
mod tieoff;
mod width;

/// Represents an interface on a module definition or module instance.
/// Interfaces are used to connect modules together by function name.
#[derive(Clone)]
pub enum Intf {
    ModDef {
        name: String,
        mod_def_core: Weak<RefCell<ModDefCore>>,
    },
    ModInst {
        intf_name: String,
        inst_name: String,
        mod_def_core: Weak<RefCell<ModDefCore>>,
    },
}

impl Intf {
    pub(crate) fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            Intf::ModDef { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
            Intf::ModInst { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
        }
    }

    pub(crate) fn get_port_slices(&self) -> IndexMap<String, PortSlice> {
        match self {
            Intf::ModDef {
                mod_def_core, name, ..
            } => {
                let core = mod_def_core.upgrade().unwrap();
                let binding = core.borrow();
                let mod_def = ModDef { core: core.clone() };
                let mapping = binding.interfaces.get(name).unwrap();
                mapping
                    .iter()
                    .map(|(func_name, (port_name, msb, lsb))| {
                        (
                            func_name.clone(),
                            mod_def.get_port_slice(port_name, *msb, *lsb),
                        )
                    })
                    .collect()
            }
            Intf::ModInst {
                inst_name,
                intf_name,
                mod_def_core,
                ..
            } => {
                let core = mod_def_core.upgrade().unwrap();
                let binding = core.borrow();
                let mod_def = ModDef { core: core.clone() };
                let inst = mod_def.get_instance(inst_name);
                let inst_core = binding.instances.get(inst_name).unwrap();
                let inst_binding = inst_core.borrow();
                let inst_mapping = inst_binding.interfaces.get(intf_name).unwrap();
                inst_mapping
                    .iter()
                    .map(|(func_name, (port_name, msb, lsb))| {
                        (
                            func_name.clone(),
                            inst.get_port_slice(port_name, *msb, *lsb),
                        )
                    })
                    .collect()
            }
        }
    }

    /// Returns an iterator over the interface functions and their port slices.
    pub fn iter(&self) -> indexmap::map::IntoIter<String, PortSlice> {
        self.get_port_slices().into_iter()
    }

    /// Returns an iterator over the interface function names.
    pub fn keys(&self) -> indexmap::map::IntoKeys<String, PortSlice> {
        self.get_port_slices().into_keys()
    }

    /// Returns an iterator over the interface port slices.
    pub fn values(&self) -> indexmap::map::IntoValues<String, PortSlice> {
        self.get_port_slices().into_values()
    }

    pub(crate) fn get_intf_name(&self) -> String {
        match self {
            Intf::ModDef { name, .. } => name.clone(),
            Intf::ModInst { intf_name, .. } => intf_name.clone(),
        }
    }

    pub fn set_metadata(
        &self,
        key: impl Into<MetadataKey>,
        value: impl Into<MetadataValue>,
    ) -> Self {
        let key = key.into();
        let value = value.into();
        match self {
            Intf::ModDef { .. } => {
                let core_rc = self.get_mod_def_core();
                let mut core = core_rc.borrow_mut();
                core.mod_def_intf_metadata
                    .entry(self.get_intf_name())
                    .or_default()
                    .insert(key, value);
            }
            Intf::ModInst { inst_name, .. } => {
                let core_rc = self.get_mod_def_core();
                let mut core = core_rc.borrow_mut();
                core.mod_inst_intf_metadata
                    .entry(inst_name.clone())
                    .or_default()
                    .entry(self.get_intf_name())
                    .or_default()
                    .insert(key, value);
            }
        }
        self.clone()
    }

    pub fn get_metadata(&self, key: impl AsRef<str>) -> Option<MetadataValue> {
        match self {
            Intf::ModDef { .. } => {
                let core_rc = self.get_mod_def_core();
                let core = core_rc.borrow();
                core.mod_def_intf_metadata
                    .get(&self.get_intf_name())
                    .and_then(|metadata| metadata.get(key.as_ref()).cloned())
            }
            Intf::ModInst { inst_name, .. } => {
                let core_rc = self.get_mod_def_core();
                let core = core_rc.borrow();
                core.mod_inst_intf_metadata
                    .get(inst_name)
                    .and_then(|intfs| intfs.get(&self.get_intf_name()))
                    .and_then(|metadata| metadata.get(key.as_ref()).cloned())
            }
        }
    }

    pub fn clear_metadata(&self, key: impl AsRef<str>) -> Self {
        match self {
            Intf::ModDef { .. } => {
                let core_rc = self.get_mod_def_core();
                let mut core = core_rc.borrow_mut();
                if let Some(metadata) = core.mod_def_intf_metadata.get_mut(&self.get_intf_name()) {
                    metadata.remove(key.as_ref());
                    if metadata.is_empty() {
                        core.mod_def_intf_metadata.remove(&self.get_intf_name());
                    }
                }
            }
            Intf::ModInst { inst_name, .. } => {
                let core_rc = self.get_mod_def_core();
                let mut core = core_rc.borrow_mut();
                if let Some(intfs) = core.mod_inst_intf_metadata.get_mut(inst_name) {
                    let intf_name = self.get_intf_name();
                    if let Some(metadata) = intfs.get_mut(&intf_name) {
                        metadata.remove(key.as_ref());
                        if metadata.is_empty() {
                            intfs.remove(&intf_name);
                        }
                    }
                    if intfs.is_empty() {
                        core.mod_inst_intf_metadata.remove(inst_name);
                    }
                }
            }
        }
        self.clone()
    }
}
