// SPDX-License-Identifier: Apache-2.0

use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use indexmap::IndexMap;
use regex::Regex;

use crate::{ModDef, ModDefCore, Usage};

impl ModDef {
    /// Returns a new module definition with the given name, using the same
    /// ports and interfaces as the original module. The new module has no
    /// instantiations or internal connections. Wrap a primitive before stubbing it.
    pub fn stub(&self, name: impl AsRef<str>) -> ModDef {
        let core = self.core.read();
        assert!(
            core.primitive.is_none(),
            "Call wrap() before stubbing primitive {}.",
            core.name
        );
        ModDef {
            core: Arc::new(RwLock::new(ModDefCore {
                name: name.as_ref().to_string(),
                ports: core.ports.clone(),
                // TODO(sherbst): 12/08/2024 should enum_ports be copied when stubbing?
                // The implication is that modules that instantiate this stub will
                // use casting to connect to enum input ports, even though they appear
                // as flat buses in the stub.
                enum_ports: core.enum_ports.clone(),
                clocks: core.clocks.clone(),
                interfaces: core.interfaces.clone(),
                instances: IndexMap::new(),
                primitive: None,
                primitives: Default::default(),
                usage: Default::default(),
                verilog_import: None,
                parameters: IndexMap::new(),
                mod_inst_connections: IndexMap::new(),
                high_fanout_drivers: Default::default(),
                mod_def_connections: IndexMap::new(),
                mod_def_metadata: HashMap::new(),
                mod_def_port_metadata: HashMap::new(),
                mod_def_intf_metadata: HashMap::new(),
                mod_inst_metadata: HashMap::new(),
                mod_inst_port_metadata: HashMap::new(),
                mod_inst_intf_metadata: HashMap::new(),
                shape: self.core.read().shape.clone(),
                keepout: self.core.read().keepout.clone(),
                layer: self.core.read().layer.clone(),
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

    /// Sets each descendant definition whose name matches the regex to `Usage::EmitStubAndStop`.
    /// Stops descending at matches. Primitive definitions are traversed but cannot be stubbed.
    pub fn stub_recursive(&self, regex: impl AsRef<str>) {
        let regex_compiled = Regex::new(regex.as_ref()).unwrap();
        let mut visited = HashSet::new();
        self.stub_recursive_helper(&regex_compiled, &mut visited);
    }

    /// Tracks each distinct definition even when generated helpers share a logical name.
    fn stub_recursive_helper(
        &self,
        regex: &Regex,
        visited: &mut HashSet<*const RwLock<ModDefCore>>,
    ) {
        for inst in self.get_instances() {
            let mod_def = inst.get_mod_def();
            if !visited.insert(Arc::as_ptr(&mod_def.core)) {
                continue;
            }
            if mod_def.primitive_kind().is_none() && regex.is_match(&mod_def.get_name()) {
                mod_def.set_usage(Usage::EmitStubAndStop);
            } else {
                mod_def.stub_recursive_helper(regex, visited);
            }
        }
    }
}
