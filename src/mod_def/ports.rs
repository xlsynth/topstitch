// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use std::rc::Rc;

use crate::{ModDef, Port, PortSlice, IO};

impl ModDef {
    /// Adds a port to the module definition with the given name. The direction
    /// and width are specfied via the `io` parameter.
    pub fn add_port(&self, name: impl AsRef<str>, io: IO) -> Port {
        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.borrow().name
            );
        }

        let mut core = self.core.borrow_mut();
        match core.ports.entry(name.as_ref().to_string()) {
            Entry::Occupied(_) => {
                panic!("Port {}.{} already exists.", core.name, name.as_ref(),)
            }
            Entry::Vacant(entry) => {
                entry.insert(io);
                Port::ModDef {
                    name: name.as_ref().to_string(),
                    mod_def_core: Rc::downgrade(&self.core),
                }
            }
        }
    }

    /// Returns `true` if this module definition has a port with the given name.
    pub fn has_port(&self, name: impl AsRef<str>) -> bool {
        self.core.borrow().ports.contains_key(name.as_ref())
    }

    /// Returns the port on this module definition with the given name; panics
    /// if a port with that name does not exist.
    pub fn get_port(&self, name: impl AsRef<str>) -> Port {
        let inner = self.core.borrow();
        if inner.ports.contains_key(name.as_ref()) {
            Port::ModDef {
                name: name.as_ref().to_string(),
                mod_def_core: Rc::downgrade(&self.core),
            }
        } else {
            panic!("Port {}.{} does not exist", inner.name, name.as_ref())
        }
    }

    /// Returns a slice of the port on this module definition with the given
    /// name, from `msb` down to `lsb`, inclusive; panics if a port with that
    /// name does not exist.
    pub fn get_port_slice(&self, name: impl AsRef<str>, msb: usize, lsb: usize) -> PortSlice {
        self.get_port(name).slice(msb, lsb)
    }

    /// Returns a vector of all ports on this module definition with the given
    /// prefix. If `prefix` is `None`, returns all ports.
    pub fn get_ports(&self, prefix: Option<&str>) -> Vec<Port> {
        let inner = self.core.borrow();
        let mut result = Vec::new();
        for name in inner.ports.keys() {
            if prefix.map_or(true, |pfx| name.starts_with(pfx)) {
                result.push(Port::ModDef {
                    name: name.clone(),
                    mod_def_core: Rc::downgrade(&self.core),
                });
            }
        }
        result
    }
}
