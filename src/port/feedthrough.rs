// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToModDef, ConvertibleToPortSlice, PipelineConfig, Port};

impl Port {
    /// Punches a feedthrough in the provided module definition for this port.
    pub fn feedthrough(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Port, Port) {
        self.to_port_slice()
            .feedthrough(&mod_def_or_mod_inst.to_mod_def(), flipped, original)
    }

    /// Punches a feedthrough in the provided module definition for this port,
    /// with a pipeline.
    pub fn feedthrough_pipeline(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) -> (Port, Port) {
        self.to_port_slice().feedthrough_pipeline(
            &mod_def_or_mod_inst.to_mod_def(),
            flipped,
            original,
            pipeline,
        )
    }
}
