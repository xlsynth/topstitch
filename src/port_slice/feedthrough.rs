// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToModDef, PipelineConfig, Port, PortDecl, PortSlice};

impl PortSlice {
    /// Punches a feedthrough in the provided module definition for this port
    /// slice.
    pub fn feedthrough(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Port, Port) {
        self.feedthrough_generic(&mod_def_or_mod_inst.to_mod_def(), flipped, original, None)
    }

    /// Punches a feedthrough in the provided module definition for this port
    /// slice, with a pipeline.
    pub fn feedthrough_pipeline(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) -> (Port, Port) {
        self.feedthrough_generic(mod_def_or_mod_inst, flipped, original, Some(pipeline))
    }

    pub(crate) fn feedthrough_generic(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: Option<PipelineConfig>,
    ) -> (Port, Port) {
        self.bits_decl()
            .feedthrough(mod_def_or_mod_inst, flipped, original, pipeline)
    }

    /// Gives the slice a flat declaration of its own width and its port's direction.
    pub(crate) fn bits_decl(&self) -> PortDecl {
        self.port.io().with_width(self.width()).into()
    }
}
