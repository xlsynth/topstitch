// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use std::rc::Rc;

use crate::{mod_inst::HierPathElem, ModDef, ModInst};

impl ModDef {
    /// Returns a vector of all module instances within this module definition.
    pub fn get_instances(&self) -> Vec<ModInst> {
        self.core
            .borrow()
            .instances
            .keys()
            .map(|name| ModInst {
                hierarchy: vec![HierPathElem {
                    mod_def_core: Rc::downgrade(&self.core),
                    inst_name: name.clone(),
                }],
            })
            .collect()
    }

    /// Returns the module instance within this module definition with the given
    /// name; panics if an instance with that name does not exist.
    pub fn get_instance(&self, name: impl AsRef<str>) -> ModInst {
        let inner = self.core.borrow();
        if inner.instances.contains_key(name.as_ref()) {
            ModInst {
                hierarchy: vec![HierPathElem {
                    mod_def_core: Rc::downgrade(&self.core),
                    inst_name: name.as_ref().to_string(),
                }],
            }
        } else {
            panic!("Instance {}.{} does not exist", inner.name, name.as_ref())
        }
    }

    /// Instantiate a module, using the provided instance name. `autoconnect` is
    /// an optional list of port names to automatically connect between the
    /// parent module and the instantiated module. This feature does not make
    /// any connections between module instances.
    ///
    /// As an example, suppose that the parent module has a port named `clk` and
    /// the instantiated module has a port named `clk`. Passing
    /// `autoconnect=Some(&["clk"])` will automatically connect the two ports.
    /// It will not automatically connect the `clk` port on this module
    /// instance to the `clk` port on any other module instances.
    ///
    /// It's OK if some or all of the `autoconnect` names do not exist in
    /// the parent module and/or instantiated module; TopStitch will not panic
    /// in this case.
    pub fn instantiate(
        &self,
        moddef: &ModDef,
        name: Option<&str>,
        autoconnect: Option<&[&str]>,
    ) -> ModInst {
        let name_default;
        let name = if let Some(name) = name {
            name
        } else {
            name_default = format!("{}_i", moddef.core.borrow().name);
            name_default.as_str()
        };

        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.borrow().name
            );
        }

        {
            let mut inner = self.core.borrow_mut();
            if inner.instances.contains_key(name) {
                panic!("Instance {}.{} already exists", inner.name, name);
            }
            inner
                .instances
                .insert(name.to_string(), moddef.core.clone());
        }

        // Create the ModInst
        let inst = ModInst {
            hierarchy: vec![HierPathElem {
                mod_def_core: Rc::downgrade(&self.core),
                inst_name: name.to_string(),
            }],
        };

        // autoconnect logic
        if let Some(port_names) = autoconnect {
            for &port_name in port_names {
                // Check if the instantiated module has this port
                if let Some(io) = moddef.core.borrow().ports.get(port_name) {
                    {
                        let mut inner = self.core.borrow_mut();
                        if !inner.ports.contains_key(port_name) {
                            inner.ports.insert(port_name.to_string(), io.clone());
                        }
                    }

                    // Connect the instance port to the parent module port
                    let parent_port = self.get_port(port_name);
                    let instance_port = inst.get_port(port_name);
                    parent_port.connect(&instance_port)
                }
            }
        }

        inst
    }

    /// Create one or more instances of a module, using the provided dimensions.
    /// For example, if `dimensions` is `&[3]`, TopStitch will create a 1D array
    /// of 3 instances, called `<mod_def_name>_i_0`, `<mod_def_name>_i_1`,
    /// `<mod_def_name>_i_2`. If `dimensions` is `&[2, 3]`, TopStitch will
    /// create a `2x3` array of instances, called `<mod_def_name>_i_0_0`,
    /// `<mod_def_name>_i_0_1`, `<mod_def_name>_i_0_2`, `<mod_def_name>_i_1_0`,
    /// etc. If provided, the optional `prefix` argument sets the prefix used in
    /// naming instances to something other than `<mod_def_name>_i_`.
    /// `autoconnect` has the same meaning as in `instantiate()`: if provided,
    /// it is a list of port names to automatically connect between the parent
    /// module and the instantiated module. For example, if the parent module
    /// has a port named `clk` and the instantiated module has a port named
    /// `clk`, passing `Some(&["clk"])` will automatically connect the two
    /// ports.
    pub fn instantiate_array(
        &self,
        moddef: &ModDef,
        dimensions: &[usize],
        prefix: Option<&str>,
        autoconnect: Option<&[&str]>,
    ) -> Vec<ModInst> {
        if dimensions.is_empty() {
            panic!(
                "Array instantiation of {} in {}: dimensions array cannot be empty.",
                moddef.get_name(),
                self.get_name()
            );
        }
        if dimensions.contains(&0) {
            panic!(
                "Array instantiation of {} in {}: dimension sizes must be greater than zero.",
                moddef.get_name(),
                self.get_name()
            );
        }

        // Create a vector of ranges based on dimensions
        let ranges: Vec<std::ops::Range<usize>> = dimensions.iter().map(|&d| 0..d).collect();

        // Generate all combinations of indices
        let index_combinations = ranges.into_iter().multi_cartesian_product();

        let mut instances = Vec::new();

        for indices in index_combinations {
            // Build instance name
            let indices_str = indices
                .iter()
                .map(|&i| i.to_string())
                .collect::<Vec<String>>()
                .join("_");

            let instance_name = match prefix {
                Some(pfx) => {
                    if indices_str.is_empty() {
                        pfx.to_string()
                    } else {
                        format!("{pfx}_{indices_str}")
                    }
                }
                None => {
                    let moddef_name = &moddef.core.borrow().name;
                    if indices_str.is_empty() {
                        format!("{moddef_name}_i")
                    } else {
                        format!("{moddef_name}_i_{indices_str}")
                    }
                }
            };

            // Instantiate the moddef
            let inst = self.instantiate(moddef, Some(&instance_name), autoconnect);
            instances.push(inst);
        }

        instances
    }
}
