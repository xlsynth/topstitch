// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::{ConvertibleToModDef, Intf, PipelineConfig};

impl Intf {
    pub fn feedthrough(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Intf, Intf) {
        self.feedthrough_generic(mod_def_or_mod_inst, flipped, original, None)
    }

    pub fn feedthrough_pipeline(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) -> (Intf, Intf) {
        self.feedthrough_generic(mod_def_or_mod_inst, flipped, original, Some(pipeline))
    }

    pub(crate) fn feedthrough_generic(
        &self,
        mod_def_or_mod_inst: &impl ConvertibleToModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: Option<PipelineConfig>,
    ) -> (Intf, Intf) {
        let mut flipped_mapping = IndexMap::new();
        let mut original_mapping = IndexMap::new();

        for (func_name, port_slice) in self.get_port_slices() {
            let flipped_func = format!("{}_{}", flipped.as_ref(), func_name);
            let original_func = format!("{}_{}", original.as_ref(), func_name);

            let (flipped_port, original_port) = port_slice.feedthrough_generic(
                mod_def_or_mod_inst,
                flipped_func,
                original_func,
                pipeline.clone(),
            );

            flipped_mapping.insert(
                func_name.clone(),
                (flipped_port.get_port_name(), port_slice.width() - 1, 0),
            );
            original_mapping.insert(
                func_name.clone(),
                (original_port.get_port_name(), port_slice.width() - 1, 0),
            );
        }

        mod_def_or_mod_inst
            .to_mod_def()
            .def_intf(&flipped, flipped_mapping);
        mod_def_or_mod_inst
            .to_mod_def()
            .def_intf(&original, original_mapping);

        (
            mod_def_or_mod_inst.get_intf(&flipped),
            mod_def_or_mod_inst.get_intf(&original),
        )
    }
}
