// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use indexmap::IndexMap;
use regex::Regex;

use crate::{ModDef, ModDefCore, Usage};

impl ModDef {
    /// Returns a new module definition with the given name, using the same
    /// ports and interfaces as the original module. The new module has no
    /// instantiations or internal connections.
    pub fn stub(&self, name: impl AsRef<str>) -> ModDef {
        let core = self.core.borrow();
        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: name.as_ref().to_string(),
                ports: core.ports.clone(),
                // TODO(sherbst): 12/08/2024 should enum_ports be copied when stubbing?
                // The implication is that modules that instantiate this stub will
                // use casting to connect to enum input ports, even though they appear
                // as flat buses in the stub.
                enum_ports: core.enum_ports.clone(),
                interfaces: core.interfaces.clone(),
                instances: IndexMap::new(),
                usage: Default::default(),
                verilog_import: None,
                parameters: IndexMap::new(),
                mod_inst_connections: IndexMap::new(),
                mod_def_connections: IndexMap::new(),
                mod_def_metadata: HashMap::new(),
                mod_def_port_metadata: HashMap::new(),
                mod_def_intf_metadata: HashMap::new(),
                mod_inst_metadata: HashMap::new(),
                mod_inst_port_metadata: HashMap::new(),
                mod_inst_intf_metadata: HashMap::new(),
                shape: self.core.borrow().shape.clone(),
                layer: self.core.borrow().layer.clone(),
                inst_placements: IndexMap::new(),
                physical_pins: IndexMap::new(),
                port_max_distances: IndexMap::new(),
                track_definitions: None,
                track_occupancies: None,
                default_connection_max_distance: core.default_connection_max_distance,
                specified_net_names: HashSet::new(),
                pipeline_counter: 0..,
            })),
        }
    }

    /// Walk through all instances within this module definition, marking those
    /// whose names match the given regex with the usage
    /// `Usage::EmitStubAndStop`. Repeat recursively for all instances whose
    /// names do not match this regex.
    pub fn stub_recursive(&self, regex: impl AsRef<str>) {
        let regex_compiled = Regex::new(regex.as_ref()).unwrap();
        let mut visited = HashSet::new();
        self.stub_recursive_helper(&regex_compiled, &mut visited);
    }

    fn stub_recursive_helper(&self, regex: &Regex, visited: &mut HashSet<String>) {
        for inst in self.get_instances() {
            let mod_def = inst.get_mod_def();
            let mod_def_name = mod_def.get_name();
            if regex.is_match(mod_def_name.as_str()) {
                mod_def.set_usage(Usage::EmitStubAndStop);
            } else if !visited.contains(&mod_def_name) {
                visited.insert(mod_def_name);
                mod_def.stub_recursive_helper(regex, visited);
            }
        }
    }
}
