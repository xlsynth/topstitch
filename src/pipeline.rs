// SPDX-License-Identifier: Apache-2.0

use crate::{IO, ModDef, ParameterType, Usage, mod_def::ParameterSpec};
use indexmap::IndexMap;
use num_bigint::BigInt;

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub clk: String,
    pub depth: usize,
    /// Exact instance name for a port or port slice, or a prefix for an interface.
    /// Interface pipelines append the function name, so `align` becomes
    /// `align_data` for function `data`, even for a one-signal interface.
    /// Crossovers use the function on the calling interface. `None` uses automatic naming.
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
    /// Expand an interface pipeline prefix for one function.
    pub(crate) fn for_function(&self, function: &str) -> Self {
        Self {
            clk: self.clk.clone(),
            depth: self.depth,
            inst_name: self
                .inst_name
                .as_ref()
                .map(|prefix| format!("{prefix}_{function}")),
        }
    }

    pub fn to_mod_def(&self, width: usize) -> ModDef {
        let mod_def = ModDef::new("br_delay_nr");
        mod_def.add_port("clk", IO::Input(1));
        mod_def.add_port("in", IO::Input(width));
        mod_def.add_port("out", IO::Output(width));
        mod_def.add_port("out_stages", IO::Output(width * self.depth));

        let mut parameters = IndexMap::new();
        parameters.insert(
            "Width".to_string(),
            ParameterSpec::Integer {
                value: BigInt::from(width),
                ty: ParameterType::Unsigned(32),
            },
        );
        parameters.insert(
            "NumStages".to_string(),
            ParameterSpec::Integer {
                value: BigInt::from(self.depth),
                ty: ParameterType::Unsigned(32),
            },
        );

        mod_def.core.write().parameters = parameters;

        mod_def.set_usage(Usage::EmitNothingAndStop);

        mod_def
    }
}

impl ModDef {
    /// Resolve an exact instance name or allocate one that avoids occupied declarations and the clock.
    pub(crate) fn resolve_pipeline_instance_name(&self, pipeline: &PipelineConfig) -> String {
        if let Some(inst_name) = pipeline.inst_name.as_ref() {
            // Explicit name provided: validate uniqueness
            let core = self.core.read();
            assert!(
                !core.instances.contains_key(inst_name),
                "Cannot use pipeline instance name {}, since that instance name is already used in module definition {}.",
                inst_name,
                core.name
            );
            inst_name.clone()
        } else {
            // Otherwise generate a unique name using the module-local counter
            let mut core = self.core.write();
            loop {
                let name = format!("pipeline_conn_{}", core.pipeline_counter.next().unwrap());
                let occupied = core
                    .instances
                    .keys()
                    .chain(core.ports.keys())
                    .chain(core.specified_net_names.iter())
                    .any(|declaration| sv_identifier(declaration) == name);
                if !occupied && sv_identifier(&pipeline.clk) != name {
                    break name;
                }
            }
        }
    }
}

/// Return the identifier represented by a simple or escaped SystemVerilog name.
fn sv_identifier(name: &str) -> &str {
    match name.strip_prefix('\\') {
        Some(escaped) => escaped
            .split(|ch: char| ch.is_ascii_whitespace())
            .next()
            .unwrap_or_default(),
        None => name,
    }
}
