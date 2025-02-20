// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::{Intf, ModDef};

impl Intf {
    /// Divides each signal in this interface into `n` equal slices, returning a
    /// vector of interfaces. For example, if this interface is `{"data":
    /// "a_data[31:0]", "valid": "a_valid[3:0]"}` and `n` is 4, this will return
    /// a vector of 4 interfaces, each with signals `{"data": "a_data[7:0]",
    /// "valid": "a_valid[0:0]"}`, `{"data": "a_data[15:8]", "valid":
    /// "a_valid[1:1]"}`, and so on. The names of the new interfaces are formed
    /// by appending "_0", "_1", "_2", and so on to the name of this interface;
    /// these names can be used to retrieve specific slices of the interface
    /// with `get_intf`.
    pub fn subdivide(&self, n: usize) -> Vec<Intf> {
        let mut result = Vec::new();

        let mut mappings: Vec<IndexMap<String, (String, usize, usize)>> = Vec::with_capacity(n);
        for _ in 0..n {
            mappings.push(IndexMap::new());
        }

        for (func_name, port_slice) in self.get_port_slices() {
            let slices = port_slice.subdivide(n);
            for (i, slice) in slices.into_iter().enumerate() {
                let port_name = port_slice.port.get_port_name();
                mappings[i].insert(func_name.clone(), (port_name.clone(), slice.msb, slice.lsb));
            }
        }

        for i in 0..n {
            let intf = match self {
                Intf::ModDef { name, .. } => {
                    let name = format!("{}_{}", name, i);
                    ModDef {
                        core: self.get_mod_def_core(),
                    }
                    .def_intf(&name, mappings.remove(0))
                }
                _ => panic!(
                    "Error subdividing {}: subdividing ModInst interfaces is not supported.",
                    self.debug_string()
                ),
            };
            result.push(intf);
        }

        result
    }
}
