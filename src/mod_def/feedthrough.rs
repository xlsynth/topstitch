// SPDX-License-Identifier: Apache-2.0

use crate::{IO, ModDef, PipelineConfig};

impl ModDef {
    /// Punches a feedthrough through this module definition with the given
    /// input and output names and width. This will create two new ports on the
    /// module definition, `input_name[width-1:0]` and `output_name[width-1:0]`,
    /// and connect them together.
    pub fn feedthrough(
        &self,
        input_name: impl AsRef<str>,
        output_name: impl AsRef<str>,
        width: usize,
    ) {
        self.feedthrough_generic(input_name, output_name, width, None);
    }

    /// Adds an input and an output, each `width` bits wide, and connects
    /// `input_name` to `output_name` through a `br_delay_nr` instance with
    /// `pipeline.depth` stages. Reuses the port named by `pipeline.clk` or
    /// creates a one-bit clock input if that port is absent.
    pub fn feedthrough_pipeline(
        &self,
        input_name: impl AsRef<str>,
        output_name: impl AsRef<str>,
        width: usize,
        pipeline: PipelineConfig,
    ) {
        self.feedthrough_generic(input_name, output_name, width, Some(pipeline));
    }

    fn feedthrough_generic(
        &self,
        input_name: impl AsRef<str>,
        output_name: impl AsRef<str>,
        width: usize,
        pipeline: Option<PipelineConfig>,
    ) {
        let input_port = self.add_port(input_name, IO::Input(width));
        let output_port = self.add_port(output_name, IO::Output(width));
        input_port.connect_generic(&output_port, pipeline);
    }
}
