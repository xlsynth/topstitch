// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToModDef, PipelineConfig, Port, PortDecl};

impl Port {
    /// Creates a feedthrough with this port's layout in the target's definition.
    /// Passing an instance returns its instance ports.
    pub fn feedthrough(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Port, Port) {
        self.decl_for_copy()
            .feedthrough(mod_def_or_mod_inst, flipped, original, None)
    }

    /// Like [`Self::feedthrough`], with a pipeline between the two ports.
    pub fn feedthrough_pipeline(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) -> (Port, Port) {
        self.decl_for_copy()
            .feedthrough(mod_def_or_mod_inst, flipped, original, Some(pipeline))
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
        let flipped_port = def.add_port(&flipped, self.clone().flipped());
        let original_port = def.add_port(&original, self);
        flipped_port.connect_generic(&original_port, pipeline);
        (target.get_port(flipped), target.get_port(original))
    }
}
