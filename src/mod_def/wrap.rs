// SPDX-License-Identifier: Apache-2.0

use crate::ModDef;

impl ModDef {
    /// Instantiates this module definition within a new module definition, and
    /// returns the new module definition. The new module definition has all of
    /// the same ports as the original module, which are connected directly to
    /// ports with the same names on the instance of the original module.
    pub fn wrap(&self, def_name: Option<&str>, inst_name: Option<&str>) -> ModDef {
        let original_name = &self.core.read().name;

        let def_name_default;
        let def_name = if let Some(name) = def_name {
            name
        } else {
            def_name_default = format!("{original_name}_wrapper");
            def_name_default.as_str()
        };

        let wrapper = ModDef::new(def_name);

        let inst = wrapper.instantiate(self, inst_name, None);

        // Copy interface definitions.
        {
            let original_core = self.core.read();
            let mut wrapper_core = wrapper.core.write();

            // Copy interface definitions
            for (intf_name, mapping) in &original_core.interfaces {
                wrapper_core
                    .interfaces
                    .insert(intf_name.clone(), mapping.clone());
            }
        }

        // For each port in the original module, add a corresponding port to the wrapper
        // and connect them.
        for (port_name, io) in self.core.read().ports.iter() {
            let wrapper_port = wrapper.add_port(port_name, io.clone());
            let inst_port = inst.get_port(port_name);
            wrapper_port.connect(&inst_port);
        }

        wrapper
    }
}
