// SPDX-License-Identifier: Apache-2.0

use xlsynth::vast::{Expr, VastFile, VastModule};

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

// TODO(sherbst) 2025-10-24: remove this once the pipeline implementation is
// updated
#[allow(dead_code)]
pub struct PipelineDetails<'a> {
    pub file: &'a mut VastFile,
    pub module: &'a mut VastModule,
    pub inst_name: &'a str,
    pub clk: &'a Expr,
    pub width: usize,
    pub depth: usize,
    pub pipe_in: &'a Expr,
    pub pipe_out: &'a Expr,
}

// TODO(sherbst) 2025-10-24: remove this once the pipeline implementation is
// updated
#[allow(dead_code)]
pub fn add_pipeline(params: PipelineDetails) {
    let width_str = format!("bits[{}]:{}", 32, params.width);
    let width_expr = params
        .file
        .make_literal(&width_str, &xlsynth::ir_value::IrFormatPreference::Hex)
        .unwrap();

    let num_stages_str = format!("bits[{}]:{}", 32, params.depth);
    let num_stages_expr = params
        .file
        .make_literal(&num_stages_str, &xlsynth::ir_value::IrFormatPreference::Hex)
        .unwrap();

    let instantiation = params.file.make_instantiation(
        "br_delay_nr",
        params.inst_name,
        &["Width", "NumStages"],
        &[&width_expr, &num_stages_expr],
        &["clk", "in", "out", "out_stages"],
        &[
            Some(params.clk),
            Some(params.pipe_in),
            Some(params.pipe_out),
            None,
        ],
    );
    params.module.add_member_instantiation(instantiation);
}

#[cfg(test)]
mod tests {
    use super::{add_pipeline, PipelineDetails};
    use xlsynth::vast::{VastFile, VastFileType};

    #[test]
    fn test_pipeline() {
        let mut file = VastFile::new(VastFileType::SystemVerilog);
        let mut module = file.add_module("test");
        let clk_data_type = file.make_bit_vector_type(1, false);
        let pipe_data_type = file.make_bit_vector_type(171, false);
        let clk_wire = module.add_wire("clk", &clk_data_type);
        let in_wire = module.add_wire("pipe_in", &pipe_data_type);
        let out_wire = module.add_wire("pipe_out", &pipe_data_type);

        let params = PipelineDetails {
            file: &mut file,
            module: &mut module,
            inst_name: "br_delay_nr_i",
            clk: &clk_wire.to_expr(),
            width: 0xab,
            depth: 0xcd,
            pipe_in: &in_wire.to_expr(),
            pipe_out: &out_wire.to_expr(),
        };

        add_pipeline(params);

        assert_eq!(
            file.emit(),
            "\
module test;
  wire clk;
  wire [170:0] pipe_in;
  wire [170:0] pipe_out;
  br_delay_nr #(
    .Width(32'h0000_00ab),
    .NumStages(32'h0000_00cd)
  ) br_delay_nr_i (
    .clk(clk),
    .in(pipe_in),
    .out(pipe_out),
    .out_stages()
  );
endmodule
"
        );
    }
}
