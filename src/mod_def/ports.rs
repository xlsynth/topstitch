// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use std::sync::Arc;

use crate::port::PortDecl;
use crate::{ModDef, Port, PortSlice};

impl ModDef {
    /// Adds a port to this module. Pass [`IO`](crate::IO) for plain bits or a
    /// declaration from a [`PackedLayout`](crate::PackedLayout) to keep its
    /// fields and array indices selectable. Both emit a flat port.
    ///
    /// ```
    /// use topstitch::{extract_packages_from_verilog, ConvertibleToPortSlice, IO, ModDef, PackedLayout};
    /// let packages = extract_packages_from_verilog(
    ///     "package p; typedef struct packed { logic valid; logic [2:0] data; } item_t; endpackage",
    ///     false,
    /// ).unwrap();
    /// let item = packages["p"].packed_layout("item_t");
    /// let top = ModDef::new("Top");
    /// let enable = top.add_port("enable", IO::Input(1));
    /// let inputs = top.add_port("items", PackedLayout::array_of(&item, 4).input());
    /// assert_eq!(inputs.element(2).field("valid").to_port_slice(), inputs.bit(11));
    /// ```
    pub fn add_port(&self, name: impl AsRef<str>, decl: impl Into<PortDecl>) -> Port {
        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.read().name
            );
        }

        let decl = decl.into();
        let mut core = self.core.write();

        match core.ports.entry(name.as_ref().to_string()) {
            Entry::Occupied(_) => {
                panic!("Port {}.{} already exists.", core.name, name.as_ref(),)
            }
            Entry::Vacant(entry) => {
                entry.insert(decl);
                Port::ModDef {
                    name: name.as_ref().to_string(),
                    mod_def_core: Arc::downgrade(&self.core),
                }
            }
        }
    }

    /// Returns `true` if this module definition has a port with the given name.
    pub fn has_port(&self, name: impl AsRef<str>) -> bool {
        self.core.read().ports.contains_key(name.as_ref())
    }

    /// Returns the port on this module definition with the given name; panics
    /// if a port with that name does not exist.
    pub fn get_port(&self, name: impl AsRef<str>) -> Port {
        let inner = self.core.read();
        if inner.ports.contains_key(name.as_ref()) {
            Port::ModDef {
                name: name.as_ref().to_string(),
                mod_def_core: Arc::downgrade(&self.core),
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
        let inner = self.core.read();
        let mut result = Vec::new();
        for name in inner.ports.keys() {
            if prefix.is_none_or(|pfx| name.starts_with(pfx)) {
                result.push(Port::ModDef {
                    name: name.clone(),
                    mod_def_core: Arc::downgrade(&self.core),
                });
            }
        }
        result
    }
}
