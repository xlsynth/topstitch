// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use std::sync::Arc;

use crate::{
    IO, ModDef, ModInst, PipelineConfig, mod_inst::HierPathElem, pipeline::PipelinePreflight,
};

/// Selects the implementation and the instance naming rules for an occurrence.
pub(super) enum Instantiation<'a> {
    Resolve,
    Exact,
    Pipeline(&'a PipelineConfig),
}

impl ModDef {
    /// Returns a vector of all module instances within this module definition.
    pub fn get_instances(&self) -> Vec<ModInst> {
        self.core
            .read()
            .instances
            .keys()
            .map(|name| ModInst {
                hierarchy: vec![HierPathElem {
                    mod_def_core: Arc::downgrade(&self.core),
                    inst_name: name.clone(),
                }],
            })
            .collect()
    }

    /// Returns the module instance within this module definition with the given
    /// name; panics if an instance with that name does not exist.
    pub fn get_instance(&self, name: impl AsRef<str>) -> ModInst {
        let inner = self.core.read();
        if inner.instances.contains_key(name.as_ref()) {
            ModInst {
                hierarchy: vec![HierPathElem {
                    mod_def_core: Arc::downgrade(&self.core),
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
    /// Names missing from the instance are skipped. If a matching parent port
    /// does not exist, it is created with the child's direction, width, and packed
    /// layout. An existing parent port keeps its own declaration.
    pub fn instantiate(
        &self,
        moddef: &ModDef,
        name: Option<&str>,
        autoconnect: Option<&[&str]>,
    ) -> ModInst {
        self.instantiate_definition(moddef, name, autoconnect, Instantiation::Resolve)
    }

    /// Instantiates a pipeline with a stable logical name and collision-free automatic SV naming.
    pub(crate) fn instantiate_pipeline(&self, pipeline: &PipelineConfig, width: usize) -> ModInst {
        self.core.read().assert_wiring_mutable();
        self.instantiate_definition(
            &pipeline.delay_def(self, width),
            pipeline.inst_name.as_deref(),
            None,
            Instantiation::Pipeline(pipeline),
        )
    }

    /// Constructs an occurrence, optionally resolving a logical primitive in the parent's policy.
    pub(super) fn instantiate_definition(
        &self,
        moddef: &ModDef,
        name: Option<&str>,
        autoconnect: Option<&[&str]>,
        mode: Instantiation,
    ) -> ModInst {
        if Arc::ptr_eq(&self.core, &moddef.core) {
            panic!(
                "Cannot instantiate a module within itself: {}",
                self.core.read().name
            );
        }

        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.read().name
            );
        }

        let primitive = {
            let def = moddef.core.read();
            def.primitive.as_ref().map(|kind| {
                let pin = match kind {
                    crate::Primitive::ReduceAnd
                    | crate::Primitive::ReduceOr
                    | crate::Primitive::ReduceXor => "a",
                    _ => kind.output(),
                };
                (kind.clone(), def.ports[pin].io.width())
            })
        };
        let (name, moddef) = loop {
            let (implementation, epoch) = match &primitive {
                Some((kind, width)) if !matches!(mode, Instantiation::Exact) => {
                    self.implementation(kind, *width)
                }
                _ => (moddef.clone(), self.core.read().primitives.policy_epoch),
            };
            let ordinary_name = (primitive.is_none() && name.is_none())
                .then(|| format!("{}_i", implementation.get_name()));
            let mapped_pipeline = matches!(mode, Instantiation::Pipeline(_))
                && implementation.core.read().primitives.mapped_body;
            let mut inner = self.core.write();
            if primitive.is_some() && inner.primitives.policy_epoch != epoch {
                continue;
            }
            if let Instantiation::Pipeline(pipeline) = mode {
                PipelinePreflight::default().check_delay_names(
                    self,
                    &inner,
                    pipeline,
                    mapped_pipeline,
                );
            }
            let name = if let Some(name) = name {
                name.to_owned()
            } else if let Instantiation::Pipeline(pipeline) = mode {
                loop {
                    let name = format!("pipeline_conn_{}", inner.pipeline_counter.next().unwrap());
                    if inner.instances.contains_key(&name)
                        || (mapped_pipeline
                            && (name == pipeline.clk
                                || inner.ports.contains_key(&name)
                                || inner.specified_net_names.contains(&name)))
                    {
                        continue;
                    }
                    if mapped_pipeline {
                        // A later port or wire can still claim this identifier before emission.
                        inner.primitives.anonymous.insert(name.clone());
                    }
                    break name;
                }
            } else if let Some((kind, _)) = &primitive {
                let mut serial = inner.instances.len();
                loop {
                    let key = format!("{}@{serial}", kind.label());
                    if !inner.instances.contains_key(&key) {
                        inner.primitives.anonymous.insert(key.clone());
                        break key;
                    }
                    serial += 1;
                }
            } else {
                ordinary_name.unwrap()
            };
            if inner.instances.contains_key(&name) {
                panic!("Instance {}.{} already exists", inner.name, name);
            }
            if let Instantiation::Pipeline(pipeline) = mode {
                inner
                    .ports
                    .entry(pipeline.clk.clone())
                    .or_insert_with(|| IO::Input(1).into());
            }
            if primitive
                .as_ref()
                .is_some_and(|(kind, _)| !kind.is_wiring())
            {
                inner.primitives.instantiated_logic = true;
            }
            inner
                .instances
                .insert(name.clone(), implementation.core.clone());
            break (name, implementation);
        };

        // Create the ModInst
        let inst = ModInst {
            hierarchy: vec![HierPathElem {
                mod_def_core: Arc::downgrade(&self.core),
                inst_name: name,
            }],
        };

        // autoconnect logic
        if let Some(port_names) = autoconnect {
            for &port_name in port_names {
                // Check if the instantiated module has this port
                if let Some(decl) = moddef.core.read().ports.get(port_name) {
                    {
                        let mut inner = self.core.write();
                        if !inner.ports.contains_key(port_name) {
                            inner.ports.insert(port_name.to_string(), decl.for_copy());
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
                    let moddef_name = &moddef.core.read().name;
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
