// SPDX-License-Identifier: Apache-2.0

use crate::{mod_def::ParameterSpec, ModDef, ParameterType, Usage, IO};
use indexmap::IndexMap;
use num_bigint::BigInt;

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
    pub fn to_mod_def(&self, width: usize) -> ModDef {
        let mod_def = ModDef::new("br_delay_nr");
        mod_def.add_port("clk", IO::Input(1));
        mod_def.add_port("in", IO::Input(width));
        mod_def.add_port("out", IO::Output(width));
        mod_def.add_port("out_stages", IO::Output(width * self.depth));

        let mut parameters = IndexMap::new();
        parameters.insert(
            "Width".to_string(),
            ParameterSpec {
                value: BigInt::from(width),
                ty: ParameterType::Unsigned(32),
            },
        );
        parameters.insert(
            "NumStages".to_string(),
            ParameterSpec {
                value: BigInt::from(self.depth),
                ty: ParameterType::Unsigned(32),
            },
        );

        mod_def.core.borrow_mut().parameters = parameters;

        mod_def.set_usage(Usage::EmitNothingAndStop);

        mod_def
    }
}

impl ModDef {
    /// Resolve the instance name to use for a pipeline given its configuration.
    /// Ensures uniqueness within this module definition and against a
    /// caller-provided set tracking names chosen during the current
    /// emission.
    pub(crate) fn resolve_pipeline_instance_name(&self, pipeline: &PipelineConfig) -> String {
        if let Some(inst_name) = pipeline.inst_name.as_ref() {
            // Explicit name provided: validate uniqueness
            let core = self.core.borrow();
            assert!(
                !core.instances.contains_key(inst_name),
                "Cannot use pipeline instance name {}, since that instance name is already used in module definition {}.",
                inst_name,
                core.name
            );
            inst_name.clone()
        } else {
            // Otherwise generate a unique name using the module-local counter
            let mut core = self.core.borrow_mut();
            loop {
                let name = format!("pipeline_conn_{}", core.pipeline_counter.next().unwrap());
                if !core.instances.contains_key(&name) {
                    break name;
                }
            }
        }
    }
}
