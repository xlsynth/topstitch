// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_pipeline() {
    let a = ModDef::new("a");
    a.add_port("out", IO::Output(0xab)).tieoff(0);
    a.add_port("in", IO::Input(0xef)).unused();
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("in", IO::Input(0xab)).unused();
    b.add_port("out", IO::Output(0xef)).tieoff(0);
    b.set_usage(Usage::EmitNothingAndStop);

    let d = ModDef::new("d");
    d.set_usage(Usage::EmitNothingAndStop);
    let c = ModDef::new("c");
    c.add_port("clk_existing", IO::Input(1));
    let a_inst = c.instantiate(&a, None, None);
    let b_inst = c.instantiate(&b, None, None);

    a_inst.get_port("out").connect_pipeline(
        &b_inst.get_port("in"),
        PipelineConfig {
            clk: "clk_existing".to_string(),
            depth: 0xcd,
            inst_name: Some("a_to_b_pipeline".to_string()),
        },
    );

    a_inst.get_port("in").connect_pipeline(
        &b_inst.get_port("out"),
        PipelineConfig {
            clk: "clk_new".to_string(),
            depth: 0xff,
            inst_name: None,
        },
    );

    // try to collide with the generated pipeline connection names
    c.instantiate(&d, Some("pipeline_conn_0"), None);
    c.instantiate(&d, Some("pipeline_conn_2"), None);

    assert_eq!(
        c.emit(true),
        "\
module c(
  input wire clk_existing,
  input wire clk_new
);
  wire [170:0] a_i_out;
  wire [238:0] a_i_in;
  wire [170:0] b_i_in;
  wire [238:0] b_i_out;
  a a_i (
    .out(a_i_out),
    .in(a_i_in)
  );
  b b_i (
    .in(b_i_in),
    .out(b_i_out)
  );
  d pipeline_conn_0 (
    
  );
  d pipeline_conn_2 (
    
  );
  br_delay_nr #(
    .Width(32'h0000_00ab),
    .NumStages(32'h0000_00cd)
  ) a_to_b_pipeline (
    .clk(clk_existing),
    .in(a_i_out[170:0]),
    .out(b_i_in[170:0]),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_00ef),
    .NumStages(32'h0000_00ff)
  ) pipeline_conn_1 (
    .clk(clk_new),
    .in(b_i_out[238:0]),
    .out(a_i_in[238:0]),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
fn test_intf_connect_pipeline() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid
    );
    endmodule
    ";

    let module_b_verilog = "
    module ModuleB (
        input [31:0] b_data,
        input b_valid
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    module_b.def_intf_from_prefix("b_intf", "b_");

    let top_module = ModDef::new("TopModule");

    let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);
    let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

    let a_intf = a_inst.get_intf("a_intf");
    let b_intf = b_inst.get_intf("b_intf");

    a_intf.connect_pipeline(
        &b_intf,
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 0xcd,
            ..Default::default()
        },
        false,
    );

    assert_eq!(
        top_module.emit(true),
        "\
module TopModule(
  input wire clk
);
  wire [31:0] inst_a_a_data;
  wire inst_a_a_valid;
  wire [31:0] inst_b_b_data;
  wire inst_b_b_valid;
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid(inst_a_a_valid)
  );
  ModuleB inst_b (
    .b_data(inst_b_b_data),
    .b_valid(inst_b_b_valid)
  );
  br_delay_nr #(
    .Width(32'h0000_0020),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(inst_a_a_data[31:0]),
    .out(inst_b_b_data[31:0]),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(inst_a_a_valid),
    .out(inst_b_b_valid),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
fn test_crossover_pipeline() {
    let module_a_verilog = "
        module ModuleA (
            output a_tx,
            input a_rx
        );
        endmodule
        ";

    let module_b_verilog = "
        module ModuleB (
          output b_tx,
          input b_rx
        );
        endmodule
        ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    module_b.def_intf_from_prefix("b_intf", "b_");

    let top_module = ModDef::new("TopModule");

    let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);
    let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

    let a_intf = a_inst.get_intf("a_intf");
    let b_intf = b_inst.get_intf("b_intf");

    a_intf.crossover_pipeline(
        &b_intf,
        "tx",
        "rx",
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 0xcd,
            ..Default::default()
        },
    );

    assert_eq!(
        top_module.emit(true),
        "\
module TopModule(
  input wire clk
);
  wire inst_a_a_tx;
  wire inst_a_a_rx;
  wire inst_b_b_tx;
  wire inst_b_b_rx;
  ModuleA inst_a (
    .a_tx(inst_a_a_tx),
    .a_rx(inst_a_a_rx)
  );
  ModuleB inst_b (
    .b_tx(inst_b_b_tx),
    .b_rx(inst_b_b_rx)
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(inst_a_a_tx),
    .out(inst_b_b_rx),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(inst_b_b_tx),
    .out(inst_a_a_rx),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
fn test_feedthrough_pipeline() {
    let mod_def = ModDef::new("TestModule");
    mod_def.feedthrough_pipeline(
        "input_signal",
        "output_signal",
        8,
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 0xab,
            ..Default::default()
        },
    );

    assert_eq!(
        mod_def.emit(true),
        "\
module TestModule(
  input wire [7:0] input_signal,
  output wire [7:0] output_signal,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(input_signal[7:0]),
    .out(output_signal[7:0]),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
fn test_intf_feedthrough_pipeline() {
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
    a_intf.feedthrough_pipeline(
        &module_b,
        "ft_left",
        "ft_right",
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 0xab,
            ..Default::default()
        },
    );

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    b_inst.get_port("clk").tieoff(0);
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
  input wire clk,
  input wire ft_left_valid_out,
  output wire ft_right_valid_out
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_left_data_out[7:0]),
    .out(ft_right_data_out[7:0]),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(ft_left_valid_out),
    .out(ft_right_valid_out),
    .out_stages()
  );
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  wire [7:0] ModuleB_i_ft_left_data_out;
  wire [7:0] ModuleB_i_ft_right_data_out;
  wire ModuleB_i_ft_left_valid_out;
  wire ModuleB_i_ft_right_valid_out;
  wire [7:0] ModuleC_i_c_data_in;
  wire ModuleC_i_c_valid_in;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out)
  );
  ModuleB ModuleB_i (
    .ft_left_data_out(ModuleB_i_ft_left_data_out),
    .ft_right_data_out(ModuleB_i_ft_right_data_out),
    .clk(1'h0),
    .ft_left_valid_out(ModuleB_i_ft_left_valid_out),
    .ft_right_valid_out(ModuleB_i_ft_right_valid_out)
  );
  ModuleC ModuleC_i (
    .c_data_in(ModuleC_i_c_data_in),
    .c_valid_in(ModuleC_i_c_valid_in)
  );
  assign ModuleB_i_ft_left_data_out[7:0] = ModuleA_i_a_data_out[7:0];
  assign ModuleB_i_ft_left_valid_out = ModuleA_i_a_valid_out;
  assign ModuleC_i_c_data_in[7:0] = ModuleB_i_ft_right_data_out[7:0];
  assign ModuleC_i_c_valid_in = ModuleB_i_ft_right_valid_out;
endmodule
"
    );
}

#[test]
fn test_intf_connect_through_generic() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data,
          output a_valid
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e_data,
          input e_valid
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_name_underscore("a");

    let module_e = ModDef::from_verilog("ModuleE", module_e_verilog, true, false);
    module_e.def_intf_from_name_underscore("e");

    let module_b = ModDef::new("ModuleB");
    let module_c = ModDef::new("ModuleC");
    let module_d = ModDef::new("ModuleD");

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);
    let d_inst = top_module.instantiate(&module_d, None, None);
    let e_inst = top_module.instantiate(&module_e, None, None);

    let cfg = |depth: usize| {
        Some(PipelineConfig {
            clk: "clk".to_string(),
            depth,
            ..Default::default()
        })
    };

    a_inst.get_intf("a").connect_through_generic(
        &e_inst.get_intf("e"),
        &[(&b_inst, cfg(0xab)), (&c_inst, None), (&d_inst, cfg(0xef))],
        "ft",
        false,
    );

    b_inst.get_port("clk").tieoff(0);
    d_inst.get_port("clk").tieoff(0);

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire clk,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_flipped_a_data[7:0]),
    .out(ft_original_a_data[7:0]),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(ft_flipped_a_valid),
    .out(ft_original_a_valid),
    .out_stages()
  );
endmodule
module ModuleC(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid
);
  assign ft_original_a_data[7:0] = ft_flipped_a_data[7:0];
  assign ft_original_a_valid = ft_flipped_a_valid;
endmodule
module ModuleD(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire clk,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ef)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_flipped_a_data[7:0]),
    .out(ft_original_a_data[7:0]),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00ef)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(ft_flipped_a_valid),
    .out(ft_original_a_valid),
    .out_stages()
  );
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data;
  wire ModuleA_i_a_valid;
  wire [7:0] ModuleB_i_ft_flipped_a_data;
  wire [7:0] ModuleB_i_ft_original_a_data;
  wire ModuleB_i_ft_flipped_a_valid;
  wire ModuleB_i_ft_original_a_valid;
  wire [7:0] ModuleC_i_ft_flipped_a_data;
  wire [7:0] ModuleC_i_ft_original_a_data;
  wire ModuleC_i_ft_flipped_a_valid;
  wire ModuleC_i_ft_original_a_valid;
  wire [7:0] ModuleD_i_ft_flipped_a_data;
  wire [7:0] ModuleD_i_ft_original_a_data;
  wire ModuleD_i_ft_flipped_a_valid;
  wire ModuleD_i_ft_original_a_valid;
  wire [7:0] ModuleE_i_e_data;
  wire ModuleE_i_e_valid;
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid)
  );
  ModuleB ModuleB_i (
    .ft_flipped_a_data(ModuleB_i_ft_flipped_a_data),
    .ft_original_a_data(ModuleB_i_ft_original_a_data),
    .clk(1'h0),
    .ft_flipped_a_valid(ModuleB_i_ft_flipped_a_valid),
    .ft_original_a_valid(ModuleB_i_ft_original_a_valid)
  );
  ModuleC ModuleC_i (
    .ft_flipped_a_data(ModuleC_i_ft_flipped_a_data),
    .ft_original_a_data(ModuleC_i_ft_original_a_data),
    .ft_flipped_a_valid(ModuleC_i_ft_flipped_a_valid),
    .ft_original_a_valid(ModuleC_i_ft_original_a_valid)
  );
  ModuleD ModuleD_i (
    .ft_flipped_a_data(ModuleD_i_ft_flipped_a_data),
    .ft_original_a_data(ModuleD_i_ft_original_a_data),
    .clk(1'h0),
    .ft_flipped_a_valid(ModuleD_i_ft_flipped_a_valid),
    .ft_original_a_valid(ModuleD_i_ft_original_a_valid)
  );
  ModuleE ModuleE_i (
    .e_data(ModuleE_i_e_data),
    .e_valid(ModuleE_i_e_valid)
  );
  assign ModuleB_i_ft_flipped_a_data[7:0] = ModuleA_i_a_data[7:0];
  assign ModuleB_i_ft_flipped_a_valid = ModuleA_i_a_valid;
  assign ModuleC_i_ft_flipped_a_data[7:0] = ModuleB_i_ft_original_a_data[7:0];
  assign ModuleC_i_ft_flipped_a_valid = ModuleB_i_ft_original_a_valid;
  assign ModuleD_i_ft_flipped_a_data[7:0] = ModuleC_i_ft_original_a_data[7:0];
  assign ModuleD_i_ft_flipped_a_valid = ModuleC_i_ft_original_a_valid;
  assign ModuleE_i_e_data[7:0] = ModuleD_i_ft_original_a_data[7:0];
  assign ModuleE_i_e_valid = ModuleD_i_ft_original_a_valid;
endmodule
"
    );
}

#[test]
fn test_intf_crossover_through_pipeline() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_tx,
          input [7:0] a_rx
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e_rx,
          output [7:0] e_tx
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_name_underscore("a");

    let module_e = ModDef::from_verilog("ModuleE", module_e_verilog, true, false);
    module_e.def_intf_from_name_underscore("e");

    let module_b = ModDef::new("ModuleB");
    let module_c = ModDef::new("ModuleC");
    let module_d = ModDef::new("ModuleD");

    let cfg = |depth: usize| {
        Some(PipelineConfig {
            clk: "clk".to_string(),
            depth,
            ..Default::default()
        })
    };

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);
    let d_inst = top_module.instantiate(&module_d, None, None);
    let e_inst = top_module.instantiate(&module_e, None, None);

    a_inst.get_intf("a").crossover_through_generic(
        &e_inst.get_intf("e"),
        &[(&b_inst, cfg(0xab)), (&c_inst, None), (&d_inst, cfg(0xef))],
        "tx",
        "rx",
        "ft_x",
        "ft_y",
    );

    b_inst.get_port("clk").tieoff(0);
    d_inst.get_port("clk").tieoff(0);

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [7:0] ft_x_rx,
  output wire [7:0] ft_y_tx,
  input wire clk,
  output wire [7:0] ft_x_tx,
  input wire [7:0] ft_y_rx
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_x_rx[7:0]),
    .out(ft_y_tx[7:0]),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(ft_y_rx[7:0]),
    .out(ft_x_tx[7:0]),
    .out_stages()
  );
endmodule
module ModuleC(
  input wire [7:0] ft_x_rx,
  output wire [7:0] ft_y_tx,
  output wire [7:0] ft_x_tx,
  input wire [7:0] ft_y_rx
);
  assign ft_y_tx[7:0] = ft_x_rx[7:0];
  assign ft_x_tx[7:0] = ft_y_rx[7:0];
endmodule
module ModuleD(
  input wire [7:0] ft_x_rx,
  output wire [7:0] ft_y_tx,
  input wire clk,
  output wire [7:0] ft_x_tx,
  input wire [7:0] ft_y_rx
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ef)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_x_rx[7:0]),
    .out(ft_y_tx[7:0]),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ef)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(ft_y_rx[7:0]),
    .out(ft_x_tx[7:0]),
    .out_stages()
  );
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_tx;
  wire [7:0] ModuleA_i_a_rx;
  wire [7:0] ModuleB_i_ft_x_rx;
  wire [7:0] ModuleB_i_ft_y_tx;
  wire [7:0] ModuleB_i_ft_x_tx;
  wire [7:0] ModuleB_i_ft_y_rx;
  wire [7:0] ModuleC_i_ft_x_rx;
  wire [7:0] ModuleC_i_ft_y_tx;
  wire [7:0] ModuleC_i_ft_x_tx;
  wire [7:0] ModuleC_i_ft_y_rx;
  wire [7:0] ModuleD_i_ft_x_rx;
  wire [7:0] ModuleD_i_ft_y_tx;
  wire [7:0] ModuleD_i_ft_x_tx;
  wire [7:0] ModuleD_i_ft_y_rx;
  wire [7:0] ModuleE_i_e_rx;
  wire [7:0] ModuleE_i_e_tx;
  ModuleA ModuleA_i (
    .a_tx(ModuleA_i_a_tx),
    .a_rx(ModuleA_i_a_rx)
  );
  ModuleB ModuleB_i (
    .ft_x_rx(ModuleB_i_ft_x_rx),
    .ft_y_tx(ModuleB_i_ft_y_tx),
    .clk(1'h0),
    .ft_x_tx(ModuleB_i_ft_x_tx),
    .ft_y_rx(ModuleB_i_ft_y_rx)
  );
  ModuleC ModuleC_i (
    .ft_x_rx(ModuleC_i_ft_x_rx),
    .ft_y_tx(ModuleC_i_ft_y_tx),
    .ft_x_tx(ModuleC_i_ft_x_tx),
    .ft_y_rx(ModuleC_i_ft_y_rx)
  );
  ModuleD ModuleD_i (
    .ft_x_rx(ModuleD_i_ft_x_rx),
    .ft_y_tx(ModuleD_i_ft_y_tx),
    .clk(1'h0),
    .ft_x_tx(ModuleD_i_ft_x_tx),
    .ft_y_rx(ModuleD_i_ft_y_rx)
  );
  ModuleE ModuleE_i (
    .e_rx(ModuleE_i_e_rx),
    .e_tx(ModuleE_i_e_tx)
  );
  assign ModuleB_i_ft_x_rx[7:0] = ModuleA_i_a_tx[7:0];
  assign ModuleC_i_ft_x_rx[7:0] = ModuleB_i_ft_y_tx[7:0];
  assign ModuleD_i_ft_x_rx[7:0] = ModuleC_i_ft_y_tx[7:0];
  assign ModuleE_i_e_rx[7:0] = ModuleD_i_ft_y_tx[7:0];
  assign ModuleA_i_a_rx[7:0] = ModuleB_i_ft_x_tx[7:0];
  assign ModuleB_i_ft_y_rx[7:0] = ModuleC_i_ft_x_tx[7:0];
  assign ModuleC_i_ft_y_rx[7:0] = ModuleD_i_ft_x_tx[7:0];
  assign ModuleD_i_ft_y_rx[7:0] = ModuleE_i_e_tx[7:0];
endmodule
"
    );
}
