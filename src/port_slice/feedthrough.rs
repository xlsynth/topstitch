// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToModDef, PipelineConfig, Port, PortDecl, PortSlice};

impl PortSlice {
    /// Creates a flat feedthrough of this slice's width in the target's definition.
    /// Passing an instance returns its instance ports.
    pub fn feedthrough(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Port, Port) {
        self.feedthrough_generic(mod_def_or_mod_inst, flipped, original, None)
    }

    /// Like [`Self::feedthrough`], with a pipeline between the two ports.
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
