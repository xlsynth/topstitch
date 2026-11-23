// SPDX-License-Identifier: Apache-2.0

use crate::{IO, ModDef, ModDefCore, PortSlice, Primitive};
use indexmap::IndexMap;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub clk: String,
    pub depth: usize,
    pub inst_name: Option<String>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 0,
            inst_name: None,
        }
    }
}

impl PipelineConfig {
    /// Returns the default word-wide RTL delay definition. A zero-depth pipeline is a direct connection.
    pub fn to_mod_def(&self, width: usize) -> ModDef {
        self.delay_def(&ModDef::new("Pipeline"), width)
    }

    /// Checks all selected interface pairs before connecting the first function.
    pub(crate) fn preflight_connections<'a>(
        &self,
        pairs: impl IntoIterator<Item = (&'a PortSlice, &'a PortSlice)>,
    ) {
        let mut plan = PipelinePreflight::default();
        for (source, target) in pairs {
            source.check_connection(target, self.depth > 0);
            plan.delay(&source.get_mod_def(), source.width(), self);
        }
    }

    /// Describes the sequential primitive shared by expressions and structural pipelines.
    fn operation(&self) -> Primitive {
        Primitive::Delay {
            cycles: self.depth,
            falling: false,
            reset: None,
        }
    }

    /// Shares the parent's delay definition cache with expression-based registers.
    pub(crate) fn delay_def(&self, parent: &ModDef, width: usize) -> ModDef {
        parent.primitive_def(self.operation(), width)
    }
}

/// Names and IO that a batch of pipelines will add, in construction order.
#[derive(Default)]
pub(crate) struct PipelinePreflight {
    ports: HashMap<(*const RwLock<ModDefCore>, String), IO>,
    explicit_instances: HashMap<(*const RwLock<ModDefCore>, String), bool>,
    interfaces: HashSet<(*const RwLock<ModDefCore>, String)>,
}

impl PipelinePreflight {
    /// Checks the synthesized delay and its clock, including ports proposed by prior requests.
    pub(crate) fn delay(&mut self, parent: &ModDef, width: usize, pipeline: &PipelineConfig) {
        if pipeline.depth == 0 {
            return;
        }
        parent.core.read().assert_wiring_mutable();
        let (implementation, _) = parent.implementation(&pipeline.operation(), width);
        let mapped = implementation.core.read().primitives.mapped_body;
        self.check_delay_names(parent, &parent.core.read(), pipeline, mapped);
    }

    /// Checks SV names and clock IO against the locked parent and earlier requests in this batch.
    pub(crate) fn check_delay_names(
        &mut self,
        parent: &ModDef,
        core: &ModDefCore,
        pipeline: &PipelineConfig,
        mapped: bool,
    ) {
        let id = Arc::as_ptr(&parent.core);
        if !core.ports.contains_key(&pipeline.clk) {
            self.check_port_name(core, id, &pipeline.clk);
        }
        let clock = core
            .ports
            .get(&pipeline.clk)
            .map(|decl| &decl.io)
            .unwrap_or_else(|| {
                self.ports
                    .entry((id, pipeline.clk.clone()))
                    .or_insert(IO::Input(1))
            });
        assert_eq!(
            *clock,
            IO::Input(1),
            "pipeline clock must be a one-bit source"
        );
        if let Some(name) = &pipeline.inst_name {
            assert!(
                !core.instances.contains_key(name),
                "Pipeline instance {name} already exists in module {}",
                core.name
            );
            if mapped {
                assert!(
                    !core.ports.contains_key(name)
                        && !core.specified_net_names.contains(name)
                        && !self.ports.contains_key(&(id, name.clone())),
                    "Mapped pipeline instance {}.{name} collides with a port or named net",
                    core.name
                );
            }
            assert!(
                self.explicit_instances
                    .insert((id, name.clone()), mapped)
                    .is_none(),
                "Pipeline instance name {name} is requested more than once in module {}. An interface pipeline with multiple signals needs automatic instance names.",
                core.name
            );
        }
    }

    /// A new port occupies the same SV namespace as named wires and structural instances.
    fn check_port_name(&self, core: &ModDefCore, id: *const RwLock<ModDefCore>, name: &str) {
        let fixed_instance = core.instances.contains_key(name)
            && !core.primitives.anonymous.contains(name)
            && !core.inlines_primitive(name);
        assert!(
            !fixed_instance
                && !core.specified_net_names.contains(name)
                && self.explicit_instances.get(&(id, name.to_owned())) != Some(&true),
            "Port {}.{name} collides with a structural instance or named net",
            core.name
        );
    }

    /// Reserves both sides of a feedthrough before its optional delay creates a clock.
    pub(crate) fn feedthrough(
        &mut self,
        parent: &ModDef,
        flipped: &str,
        original: &str,
        io: IO,
        pipeline: Option<&PipelineConfig>,
    ) {
        assert!(
            !parent.frozen(),
            "Module {} is frozen. wrap() first if modifications are needed.",
            parent.get_name()
        );
        assert!(io.width() > 0, "feedthrough width must be positive");
        for (name, side) in [(flipped, io.flip()), (original, io.clone())] {
            self.check_port_name(&parent.core.read(), Arc::as_ptr(&parent.core), name);
            assert!(
                !parent.has_port(name)
                    && self
                        .ports
                        .insert((Arc::as_ptr(&parent.core), name.to_owned()), side)
                        .is_none(),
                "Port {}.{name} already exists or is requested twice",
                parent.get_name()
            );
        }
        if let Some(pipeline) = pipeline {
            assert!(
                pipeline.depth == 0 || !matches!(io, IO::InOut(_)),
                "Cannot pipeline InOut port {original}"
            );
            self.delay(parent, io.width(), pipeline);
        }
    }

    /// Reserves the interface names and all function ports for one feedthrough.
    pub(crate) fn interface(
        &mut self,
        parent: &ModDef,
        ports: &IndexMap<String, PortSlice>,
        flipped: &str,
        original: &str,
        pipeline: Option<&PipelineConfig>,
    ) {
        for name in [flipped, original] {
            assert!(
                !parent.has_intf(name)
                    && self
                        .interfaces
                        .insert((Arc::as_ptr(&parent.core), name.to_owned())),
                "Interface {}.{name} already exists or is requested twice",
                parent.get_name()
            );
        }
        for (function, port) in ports {
            self.feedthrough(
                parent,
                &format!("{flipped}_{function}"),
                &format!("{original}_{function}"),
                port.port.io().with_width(port.width()),
                pipeline,
            );
        }
    }
}
