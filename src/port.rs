// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::io::IO;
use crate::mod_inst::HierPathElem;
use crate::{
    ConvertibleToPortSlice, Coordinate, ModDef, ModDefCore, ModInst, PhysicalPin, PortKey,
    PortSlice,
};

mod connect;
mod export;
mod feedthrough;
mod tieoff;

/// Represents a port on a module definition or a module instance.
#[derive(Clone, Debug)]
pub enum Port {
    ModDef {
        mod_def_core: Weak<RefCell<ModDefCore>>,
        name: String,
    },
    ModInst {
        hierarchy: Vec<HierPathElem>,
        port_name: String,
    },
}

impl Port {
    /// Returns the name this port has in its (parent) module definition.
    pub fn name(&self) -> &str {
        match self {
            Port::ModDef { name, .. } => name,
            Port::ModInst { port_name, .. } => port_name,
        }
    }

    /// Returns the IO enum associated with this Port.
    pub fn io(&self) -> IO {
        match self {
            Port::ModDef { mod_def_core, name } => {
                mod_def_core.upgrade().unwrap().borrow().ports[name].clone()
            }
            Port::ModInst {
                hierarchy,
                port_name,
                ..
            } => {
                let inst_frame = hierarchy
                    .last()
                    .expect("Port::ModInst hierarchy cannot be empty");
                inst_frame
                    .mod_def_core
                    .upgrade()
                    .unwrap()
                    .borrow()
                    .instances[inst_frame.inst_name.as_str()]
                .borrow()
                .ports[port_name.as_str()]
                .clone()
            }
        }
    }

    pub(crate) fn variant_name(&self) -> &str {
        match self {
            Port::ModDef { .. } => "ModDef",
            Port::ModInst { .. } => "ModInst",
        }
    }

    pub(crate) fn assign_to_inst(&self, inst: &ModInst) -> Port {
        match self {
            Port::ModDef { name, .. } => Port::ModInst {
                hierarchy: inst.hierarchy.clone(),
                port_name: name.clone(),
            },
            _ => panic!("Already assigned to an instance."),
        }
    }

    pub(crate) fn to_port_key(&self) -> PortKey {
        match self {
            Port::ModDef { name, .. } => PortKey::ModDefPort {
                mod_def_name: self.get_mod_def_core().borrow().name.clone(),
                port_name: name.clone(),
            },
            Port::ModInst { port_name, .. } => PortKey::ModInstPort {
                mod_def_name: self.get_mod_def_core().borrow().name.clone(),
                inst_name: self
                    .inst_name()
                    .expect("Port::ModInst hierarchy cannot be empty")
                    .to_string(),
                port_name: port_name.clone(),
            },
        }
    }

    pub(crate) fn is_driver(&self) -> bool {
        match self {
            Port::ModDef { .. } => matches!(self.io(), IO::Input(_)),
            Port::ModInst { .. } => matches!(self.io(), IO::Output(_)),
        }
    }

    pub(crate) fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            Port::ModDef { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
            Port::ModInst { hierarchy, .. } => hierarchy
                .last()
                .expect("Port::ModInst hierarchy cannot be empty")
                .mod_def_core
                .upgrade()
                .expect("Containing ModDefCore has been dropped"),
        }
    }

    pub(crate) fn get_mod_def_where_declared(&self) -> ModDef {
        match self {
            Port::ModDef { mod_def_core, .. } => ModDef {
                core: mod_def_core.upgrade().unwrap(),
            },
            Port::ModInst { hierarchy, .. } => {
                let last = hierarchy.last().unwrap();
                ModDef {
                    core: last
                        .mod_def_core
                        .upgrade()
                        .unwrap()
                        .borrow()
                        .instances
                        .get(last.inst_name.as_str())
                        .unwrap()
                        .clone(),
                }
            }
        }
    }

    pub fn get_mod_inst(&self) -> Option<ModInst> {
        match self {
            Port::ModInst { hierarchy, .. } => Some(ModInst {
                hierarchy: hierarchy.clone(),
            }),
            _ => None,
        }
    }

    pub(crate) fn inst_name(&self) -> Option<&str> {
        match self {
            Port::ModInst { hierarchy, .. } => {
                hierarchy.last().map(|frame| frame.inst_name.as_str())
            }
            _ => None,
        }
    }

    pub(crate) fn get_port_name(&self) -> String {
        match self {
            Port::ModDef { name, .. } => name.clone(),
            Port::ModInst { port_name, .. } => port_name.clone(),
        }
    }

    pub(crate) fn debug_string(&self) -> String {
        match self {
            Port::ModDef { name, mod_def_core } => {
                format!("{}.{}", mod_def_core.upgrade().unwrap().borrow().name, name)
            }
            Port::ModInst { port_name, .. } => {
                let inst = self
                    .get_mod_inst()
                    .expect("Port::ModInst hierarchy cannot be empty");
                format!("{}.{}", inst.debug_string(), port_name)
            }
        }
    }

    pub fn get_physical_pin(&self) -> PhysicalPin {
        self.to_port_slice().get_physical_pin()
    }

    pub fn get_coordinate(&self) -> Coordinate {
        self.get_physical_pin().position
    }

    pub(crate) fn debug_string_with_width(&self) -> String {
        format!("{}[{}:{}]", self.debug_string(), self.io().width() - 1, 0)
    }

    /// Returns a slice of this port from `msb` down to `lsb`, inclusive.
    pub fn slice(&self, msb: usize, lsb: usize) -> PortSlice {
        if msb >= self.io().width() || lsb > msb {
            panic!(
                "Invalid slice [{}:{}] of port {}",
                msb,
                lsb,
                self.debug_string_with_width()
            );
        }
        PortSlice {
            port: self.clone(),
            msb,
            lsb,
        }
    }

    /// Returns a single-bit slice of this port at the specified index.
    pub fn bit(&self, index: usize) -> PortSlice {
        self.slice(index, index)
    }

    /// Splits this port into `n` equal slices, returning a vector of port
    /// slices. For example, if this port is 8-bit wide and `n` is 4, this will
    /// return a vector of 4 port slices, each 2 bits wide: `[1:0]`, `[3:2]`,
    /// `[5:4]`, and `[7:6]`.
    pub fn subdivide(&self, n: usize) -> Vec<PortSlice> {
        self.to_port_slice().subdivide(n)
    }

    /// Returns an ordered list of `(port name, bit index)` pairs covering every
    /// bit of this port.
    pub fn to_bits(&self) -> Vec<(&str, usize)> {
        let mut bits = Vec::new();
        for i in 0..self.io().width() {
            bits.push((self.name(), i));
        }
        bits
    }
}

impl ConvertibleToPortSlice for Port {
    fn to_port_slice(&self) -> PortSlice {
        PortSlice {
            port: self.clone(),
            msb: self.io().width() - 1,
            lsb: 0,
        }
    }
}
