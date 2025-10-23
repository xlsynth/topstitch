// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_feedthrough() {
    let mod_def = ModDef::new("TestModule");
    mod_def.feedthrough("input_signal", "output_signal", 8);
    assert_eq!(
        mod_def.emit(true),
        "\
module TestModule(
  input wire [7:0] input_signal,
  output wire [7:0] output_signal
);
  assign output_signal = input_signal;
endmodule
"
    );
}

#[test]
fn test_intf_feedthrough() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data_out,
          output a_valid_out
      );
      endmodule
      ";

    let module_c_verilog = "
      module ModuleC (
          input [7:0] c_data_in,
          input c_valid_in
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    let a_intf = module_a.def_intf_from_name_underscore("a");

    let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);
    module_c.def_intf_from_name_underscore("c");

    let module_b = ModDef::new("ModuleB");
    a_intf.feedthrough(&module_b, "ft_left", "ft_right");

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);

    a_inst
        .get_intf("a")
        .connect(&b_inst.get_intf("ft_left"), false);
    c_inst
        .get_intf("c")
        .crossover(&b_inst.get_intf("ft_right"), "(.*)_in", "(.*)_out");

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [7:0] ft_left_data_out,
  output wire [7:0] ft_right_data_out,
  input wire ft_left_valid_out,
  output wire ft_right_valid_out
);
  assign ft_right_data_out = ft_left_data_out;
  assign ft_right_valid_out = ft_left_valid_out;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out)
  );
  wire [7:0] ModuleB_i_ft_right_data_out;
  wire ModuleB_i_ft_right_valid_out;
  ModuleB ModuleB_i (
    .ft_left_data_out(ModuleA_i_a_data_out),
    .ft_right_data_out(ModuleB_i_ft_right_data_out),
    .ft_left_valid_out(ModuleA_i_a_valid_out),
    .ft_right_valid_out(ModuleB_i_ft_right_valid_out)
  );
  ModuleC ModuleC_i (
    .c_data_in(ModuleB_i_ft_right_data_out),
    .c_valid_in(ModuleB_i_ft_right_valid_out)
  );
endmodule
"
    );
}

#[test]
fn test_port_feedthrough() {
    let a = ModDef::new("A");
    a.add_port("a", IO::Input(8)).unused();

    let b = ModDef::new("B");
    a.get_port("a").feedthrough(&b, "flipped", "original");

    assert_eq!(
        b.emit(true),
        "\
module B(
  output wire [7:0] flipped,
  input wire [7:0] original
);
  assign flipped = original;
endmodule
"
    );
}

#[test]
fn test_port_slice_feedthrough() {
    let a = ModDef::new("A");
    a.add_port("a", IO::Input(8)).unused();

    let b = ModDef::new("B");
    a.get_port("a")
        .slice(7, 4)
        .feedthrough(&b, "flipped", "original");

    assert_eq!(
        b.emit(true),
        "\
module B(
  output wire [3:0] flipped,
  input wire [3:0] original
);
  assign flipped = original;
endmodule
"
    );
}

#[test]
#[ignore = "skipped until the pipeline implementation is updated"]
fn test_port_feedthrough_pipeline() {
    let a = ModDef::new("A");
    a.add_port("a", IO::Input(8)).unused();

    let b = ModDef::new("B");
    a.get_port("a").feedthrough_pipeline(
        &b,
        "flipped",
        "original",
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 1,
            ..Default::default()
        },
    );

    assert_eq!(
        b.emit(true),
        "\
module B(
  output wire [7:0] flipped,
  input wire [7:0] original,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_0001)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(original[7:0]),
    .out(flipped[7:0]),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
#[ignore = "skipped until the pipeline implementation is updated"]
fn test_port_slice_feedthrough_pipeline() {
    let a = ModDef::new("A");
    a.add_port("a", IO::Input(8)).unused();

    let b = ModDef::new("B");
    a.get_port("a").slice(7, 4).feedthrough_pipeline(
        &b,
        "flipped",
        "original",
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 1,
            ..Default::default()
        },
    );

    assert_eq!(
        b.emit(true),
        "\
module B(
  output wire [3:0] flipped,
  input wire [3:0] original,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0004),
    .NumStages(32'h0000_0001)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(original[3:0]),
    .out(flipped[3:0]),
    .out_stages()
  );
endmodule
"
    );
}
