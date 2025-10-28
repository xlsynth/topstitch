// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::{Rc, Weak};

use crate::connection::PortSliceConnections;
use crate::io::IO;
use crate::mod_inst::HierPathElem;
use crate::{
    ConvertibleToPortSlice, Coordinate, ModDef, ModDefCore, ModInst, PhysicalPin, PortSlice,
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

impl PartialEq for Port {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Port::ModDef {
                    mod_def_core: a_core,
                    name: a_name,
                },
                Port::ModDef {
                    mod_def_core: b_core,
                    name: b_name,
                },
            ) => match (a_core.upgrade(), b_core.upgrade()) {
                (Some(a_rc), Some(b_rc)) => Rc::ptr_eq(&a_rc, &b_rc) && (a_name == b_name),
                _ => false,
            },
            (
                Port::ModInst {
                    hierarchy: a_hier,
                    port_name: a_port,
                },
                Port::ModInst {
                    hierarchy: b_hier,
                    port_name: b_port,
                },
            ) => a_hier == b_hier && (a_port == b_port),
            _ => false,
        }
    }
}

impl Eq for Port {}

impl Hash for Port {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Port::ModDef { name, mod_def_core } => {
                // Hash pointer identity and name
                if let Some(rc) = mod_def_core.upgrade() {
                    Rc::as_ptr(&rc).hash(state);
                } else {
                    (0usize).hash(state);
                }
                name.hash(state);
            }
            Port::ModInst {
                port_name,
                hierarchy,
            } => {
                // Hash the chain of instance frame pointer identities and names, then port name
                for frame in hierarchy {
                    if let Some(rc) = frame.mod_def_core.upgrade() {
                        Rc::as_ptr(&rc).hash(state);
                    } else {
                        (0usize).hash(state);
                    }
                    frame.inst_name.hash(state);
                }
                port_name.hash(state);
            }
        }
    }
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

    pub(crate) fn assign_to_inst(&self, inst: &ModInst) -> Port {
        match self {
            Port::ModDef { name, .. } => Port::ModInst {
                hierarchy: inst.hierarchy.clone(),
                port_name: name.clone(),
            },
            _ => panic!("Already assigned to an instance."),
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

    pub(crate) fn get_port_connections_define_if_missing(
        &self,
    ) -> Rc<RefCell<PortSliceConnections>> {
        let core_rc = self.get_mod_def_core();
        let mut core = core_rc.borrow_mut();
        match self {
            Port::ModDef { .. } => core
                .mod_def_connections
                .entry(self.name().to_string())
                .or_default()
                .clone(),
            Port::ModInst { .. } => core
                .mod_inst_connections
                .entry(self.inst_name().unwrap().to_string())
                .or_default()
                .entry(self.name().to_string())
                .or_default()
                .clone(),
        }
    }

    pub(crate) fn get_port_connections(&self) -> Option<Rc<RefCell<PortSliceConnections>>> {
        let core_rc = self.get_mod_def_core();
        let core = core_rc.borrow();
        match self {
            Port::ModDef { .. } => core
                .mod_def_connections
                .get(&self.name().to_string())
                .cloned(),
            Port::ModInst { .. } => core
                .mod_inst_connections
                .get(&self.inst_name().unwrap().to_string())
                .and_then(|connections| connections.get(&self.name().to_string()).cloned()),
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

    /// Returns the default net name for this port.
    pub fn default_net_name(&self) -> String {
        match self {
            Port::ModDef { name, .. } => name.clone(),
            Port::ModInst { port_name, .. } => {
                default_net_name_for_inst_port(self.inst_name().unwrap(), port_name)
            }
        }
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

pub(crate) fn default_net_name_for_inst_port(
    inst_name: impl AsRef<str>,
    port_name: impl AsRef<str>,
) -> String {
    format!("{}_{}", inst_name.as_ref(), port_name.as_ref())
}
