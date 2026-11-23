// SPDX-License-Identifier: Apache-2.0

use crate::pipeline::PipelinePreflight;
use crate::{ConvertibleToModDef, PipelineConfig, Port, PortDecl};

impl Port {
    /// Punches a feedthrough in the provided module definition. Both ports retain this port's packed layout.
    pub fn feedthrough(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Port, Port) {
        self.decl_for_copy()
            .feedthrough(&mod_def_or_mod_inst.to_mod_def(), flipped, original, None)
    }

    /// Punches a feedthrough in the provided module definition for this port,
    /// with a pipeline. Both ports retain this port's packed layout.
    pub fn feedthrough_pipeline(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) -> (Port, Port) {
        self.decl_for_copy().feedthrough(
            &mod_def_or_mod_inst.to_mod_def(),
            flipped,
            original,
            Some(pipeline),
        )
    }
}

impl PortDecl {
    /// Creates and connects a pair of feedthrough ports with this declaration's width and layout.
    pub(crate) fn feedthrough(
        self,
        target: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: Option<PipelineConfig>,
    ) -> (Port, Port) {
        let def = target.to_mod_def();
        if let Some(pipeline) = &pipeline {
            PipelinePreflight::default().feedthrough(
                &def,
                flipped.as_ref(),
                original.as_ref(),
                self.io.clone(),
                Some(pipeline),
            );
        }
        let flipped_port = def.add_port(&flipped, self.clone().flipped());
        let original_port = def.add_port(&original, self);
        flipped_port.connect_generic(&original_port, pipeline);
        (target.get_port(flipped), target.get_port(original))
    }
}
