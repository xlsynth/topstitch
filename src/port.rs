// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::{Rc, Weak};

use crate::connection::PortSliceConnections;
use crate::io::IO;
use crate::mod_inst::HierPathElem;
use crate::{
    ConvertibleToPortSlice, Coordinate, MetadataKey, MetadataValue, ModDef, ModDefCore, ModInst,
    PhysicalPin, PortSlice,
};

mod connect;
mod export;
mod feedthrough;
mod tieoff;
mod trace;

#[derive(Clone, Debug)]
pub enum PortDirectionality {
    Driver,
    Receiver,
    InOut,
}

impl PortDirectionality {
    pub(crate) fn compatible_with(&self, other: &PortDirectionality) -> bool {
        matches!(
            (self, other),
            (PortDirectionality::InOut, _)
                | (_, PortDirectionality::InOut)
                | (PortDirectionality::Driver, PortDirectionality::Receiver)
                | (PortDirectionality::Receiver, PortDirectionality::Driver)
        )
    }
}

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

    pub fn set_metadata(
        &self,
        key: impl Into<MetadataKey>,
        value: impl Into<MetadataValue>,
    ) -> Self {
        let key = key.into();
        let value = value.into();
        match self {
            Port::ModDef { .. } => {
                let core_rc = self.get_mod_def_core_where_declared();
                let mut core = core_rc.borrow_mut();
                core.mod_def_port_metadata
                    .entry(self.name().to_string())
                    .or_default()
                    .insert(key, value);
            }
            Port::ModInst { .. } => {
                let inst_name = self
                    .inst_name()
                    .expect("Port::ModInst hierarchy cannot be empty")
                    .to_string();
                let core_rc = self.get_mod_def_core();
                let mut core = core_rc.borrow_mut();
                core.mod_inst_port_metadata
                    .entry(inst_name)
                    .or_default()
                    .entry(self.name().to_string())
                    .or_default()
                    .insert(key, value);
            }
        }
        self.clone()
    }

    pub fn get_metadata(&self, key: impl AsRef<str>) -> Option<MetadataValue> {
        match self {
            Port::ModDef { .. } => self
                .get_mod_def_core_where_declared()
                .borrow()
                .mod_def_port_metadata
                .get(self.name())
                .and_then(|metadata| metadata.get(key.as_ref()).cloned()),
            Port::ModInst { .. } => {
                let inst_name = self
                    .inst_name()
                    .expect("Port::ModInst hierarchy cannot be empty");
                self.get_mod_def_core()
                    .borrow()
                    .mod_inst_port_metadata
                    .get(inst_name)
                    .and_then(|ports| ports.get(self.name()))
                    .and_then(|metadata| metadata.get(key.as_ref()).cloned())
            }
        }
    }

    pub fn clear_metadata(&self, key: impl AsRef<str>) -> Self {
        match self {
            Port::ModDef { .. } => {
                let core_rc = self.get_mod_def_core_where_declared();
                let mut core = core_rc.borrow_mut();
                if let Some(metadata) = core.mod_def_port_metadata.get_mut(self.name()) {
                    metadata.remove(key.as_ref());
                    if metadata.is_empty() {
                        core.mod_def_port_metadata.remove(self.name());
                    }
                }
            }
            Port::ModInst { .. } => {
                let inst_name = self
                    .inst_name()
                    .expect("Port::ModInst hierarchy cannot be empty")
                    .to_string();
                let core_rc = self.get_mod_def_core();
                let mut core = core_rc.borrow_mut();
                if let Some(ports) = core.mod_inst_port_metadata.get_mut(&inst_name) {
                    if let Some(metadata) = ports.get_mut(self.name()) {
                        metadata.remove(key.as_ref());
                        if metadata.is_empty() {
                            ports.remove(self.name());
                        }
                    }
                    if ports.is_empty() {
                        core.mod_inst_port_metadata.remove(&inst_name);
                    }
                }
            }
        }
        self.clone()
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
        ModDef {
            core: self.get_mod_def_core_where_declared(),
        }
    }

    pub(crate) fn get_mod_def_core_where_declared(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            Port::ModDef { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
            Port::ModInst { hierarchy, .. } => {
                let last = hierarchy.last().unwrap();
                last.mod_def_core
                    .upgrade()
                    .unwrap()
                    .borrow()
                    .instances
                    .get(last.inst_name.as_str())
                    .unwrap()
                    .clone()
            }
        }
    }

    /// Returns a new [`Port::ModInst`] with the same port name, but the
    /// provided hierarchy.
    pub(crate) fn as_mod_inst_port(&self, hierarchy: Vec<HierPathElem>) -> Port {
        Port::ModInst {
            hierarchy,
            port_name: self.name().to_string(),
        }
    }

    /// Returns a new [`Port::ModDef`] with the same port name, but a module
    /// definition pointer that corresponds to the module where the port is
    /// declared. For example, if `top` instantiates `a` as `a_inst` and
    /// `a_inst` has a port `x`, calling this function on `top.a_inst.x` will
    /// return effectively return `a.x` (i.e., the port on the module definition
    /// of `a`).
    pub(crate) fn as_mod_def_port(&self) -> Port {
        Port::ModDef {
            mod_def_core: Rc::downgrade(&self.get_mod_def_core_where_declared()),
            name: self.name().to_string(),
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
        self.get_physical_pin().translation()
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

    pub(crate) fn get_directionality(&self) -> PortDirectionality {
        match self {
            Port::ModDef { .. } => match self.io() {
                IO::Input(_) => PortDirectionality::Driver,
                IO::Output(_) => PortDirectionality::Receiver,
                IO::InOut(_) => PortDirectionality::InOut,
            },
            Port::ModInst { .. } => match self.io() {
                IO::Input(_) => PortDirectionality::Receiver,
                IO::Output(_) => PortDirectionality::Driver,
                IO::InOut(_) => PortDirectionality::InOut,
            },
        }
    }

    /// Places this port based on what has been connected to it.
    pub fn place_abutted(&self) {
        self.to_port_slice().place_abutted();
    }

    /// Places this port abutted to the specified port or port slice.
    pub fn place_abutted_to<T: ConvertibleToPortSlice>(&self, other: T) {
        self.to_port_slice().place_abutted_to(other);
    }

    /// For each bit in this port, trace its connectivity to determine what
    /// existing pin it is connected to, and then place a new pin for the
    /// port bit that overlaps the connected pin.
    pub fn place_overlapped(&self, pin: &PhysicalPin) {
        self.to_port_slice().place_overlapped(pin);
    }

    /// For each bit `i` in this port, place a new pin that overlaps the `i`-th
    /// bit of `other`.
    pub fn place_overlapped_with<T: ConvertibleToPortSlice>(&self, other: T, pin: &PhysicalPin) {
        self.to_port_slice().place_overlapped_with(other, pin);
    }

    /// Places this port across from the specified port or port slice.
    pub fn place_across_from<T: ConvertibleToPortSlice>(&self, other: T) {
        self.to_port_slice().place_across_from(other);
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
