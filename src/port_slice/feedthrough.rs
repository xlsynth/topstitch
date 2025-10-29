// SPDX-License-Identifier: Apache-2.0

use crate::connection::port_slice::Abutment;
use crate::{ConvertibleToModDef, PipelineConfig, Port, PortSlice};

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
        let flipped_port = mod_def_or_mod_inst
            .to_mod_def()
            .add_port(&flipped, self.port.io().with_width(self.width()).flip());
        let original_port = mod_def_or_mod_inst
            .to_mod_def()
            .add_port(&original, self.port.io().with_width(self.width()));
        flipped_port.connect_generic(&original_port, pipeline.clone(), Abutment::NA);
        (
            mod_def_or_mod_inst.get_port(&flipped),
            mod_def_or_mod_inst.get_port(&original),
        )
    }
}
