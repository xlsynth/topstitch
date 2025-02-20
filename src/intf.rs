// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use indexmap::IndexMap;

use crate::mod_def::ModDefCore;
use crate::{ModDef, PortSlice};

mod connect;
mod copy;
mod crossover;
mod debug;
mod export;
mod feedthrough;
mod subdivide;
mod tieoff;

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

    pub(crate) fn get_intf_name(&self) -> String {
        match self {
            Intf::ModDef { name, .. } => name.clone(),
            Intf::ModInst { intf_name, .. } => intf_name.clone(),
        }
    }
}
