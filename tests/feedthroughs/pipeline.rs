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
    // try to collide with the generated pipeline connection names
    c.instantiate(&d, Some("pipeline_conn_0"), None);
    c.instantiate(&d, Some("pipeline_conn_2"), None);

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

    assert_eq!(
        c.emit(true),
        "\
module c(
  input wire clk_existing,
  input wire clk_new
);
  wire [170:0] a_i_out;
  wire [238:0] pipeline_conn_1_out;
  a a_i (
    .out(a_i_out),
    .in(pipeline_conn_1_out)
  );
  wire [170:0] a_to_b_pipeline_out;
  wire [238:0] b_i_out;
  b b_i (
    .in(a_to_b_pipeline_out),
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
    .in(a_i_out),
    .out(a_to_b_pipeline_out),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_00ef),
    .NumStages(32'h0000_00ff)
  ) pipeline_conn_1 (
    .clk(clk_new),
    .in(b_i_out),
    .out(pipeline_conn_1_out),
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
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid(inst_a_a_valid)
  );
  wire [31:0] pipeline_conn_0_out;
  wire pipeline_conn_1_out;
  ModuleB inst_b (
    .b_data(pipeline_conn_0_out),
    .b_valid(pipeline_conn_1_out)
  );
  br_delay_nr #(
    .Width(32'h0000_0020),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(inst_a_a_data),
    .out(pipeline_conn_0_out),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(inst_a_a_valid),
    .out(pipeline_conn_1_out),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
fn test_intf_connect_pipeline_except() {
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

    a_intf.connect_pipeline_except(
        &b_intf,
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 0xcd,
            ..Default::default()
        },
        Some(&["valid"]),
    );
    a_intf.get("valid").unwrap().unused_or_tieoff(0);
    b_intf.get("valid").unwrap().unused_or_tieoff(0);

    let emitted = top_module.emit(true);
    assert_eq!(
        emitted,
        "\
module TopModule(
  input wire clk
);
  wire [31:0] inst_a_a_data;
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid()
  );
  wire [31:0] pipeline_conn_0_out;
  ModuleB inst_b (
    .b_data(pipeline_conn_0_out),
    .b_valid(1'h0)
  );
  br_delay_nr #(
    .Width(32'h0000_0020),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(inst_a_a_data),
    .out(pipeline_conn_0_out),
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
  wire pipeline_conn_1_out;
  ModuleA inst_a (
    .a_tx(inst_a_a_tx),
    .a_rx(pipeline_conn_1_out)
  );
  wire inst_b_b_tx;
  wire pipeline_conn_0_out;
  ModuleB inst_b (
    .b_tx(inst_b_b_tx),
    .b_rx(pipeline_conn_0_out)
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(inst_a_a_tx),
    .out(pipeline_conn_0_out),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(inst_b_b_tx),
    .out(pipeline_conn_1_out),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
fn test_crossover_pipeline_except() {
    let module_a_verilog = "
        module ModuleA (
            output intf_a_tx,
            input intf_a_rx,
            output intf_b_tx,
            input intf_b_rx
        );
        endmodule
        ";

    let module_b_verilog = "
        module ModuleB (
          output intf_a_tx,
          input intf_a_rx,
          output intf_b_tx,
          input intf_b_rx
        );
        endmodule
        ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_name_underscore("intf");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    module_b.def_intf_from_name_underscore("intf");

    let top_module = ModDef::new("TopModule");

    let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);
    let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

    let a_intf = a_inst.get_intf("intf");
    let b_intf = b_inst.get_intf("intf");

    a_intf.crossover_pipeline_except(
        &b_intf,
        "^(.*)_tx$",
        "^(.*)_rx$",
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 0xcd,
            ..Default::default()
        },
        Some(&["b_tx"]),
    );

    a_intf.get("b_tx").unwrap().unused_or_tieoff(0);
    a_intf.get("b_rx").unwrap().unused_or_tieoff(0);
    b_intf.get("b_rx").unwrap().unused_or_tieoff(0);
    b_intf.get("b_tx").unwrap().unused_or_tieoff(0);

    let emitted = top_module.emit(true);
    assert_eq!(
        emitted,
        "\
module TopModule(
  input wire clk
);
  wire inst_a_intf_a_tx;
  wire pipeline_conn_1_out;
  ModuleA inst_a (
    .intf_a_tx(inst_a_intf_a_tx),
    .intf_a_rx(pipeline_conn_1_out),
    .intf_b_tx(),
    .intf_b_rx(1'h0)
  );
  wire inst_b_intf_a_tx;
  wire pipeline_conn_0_out;
  ModuleB inst_b (
    .intf_a_tx(inst_b_intf_a_tx),
    .intf_a_rx(pipeline_conn_0_out),
    .intf_b_tx(),
    .intf_b_rx(1'h0)
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(inst_a_intf_a_tx),
    .out(pipeline_conn_0_out),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0001),
    .NumStages(32'h0000_00cd)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(inst_b_intf_a_tx),
    .out(pipeline_conn_1_out),
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
    .in(input_signal),
    .out(output_signal),
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
    .in(ft_left_data_out),
    .out(ft_right_data_out),
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
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out)
  );
  wire [7:0] ModuleB_i_ft_right_data_out;
  wire ModuleB_i_ft_right_valid_out;
  ModuleB ModuleB_i (
    .ft_left_data_out(ModuleA_i_a_data_out),
    .ft_right_data_out(ModuleB_i_ft_right_data_out),
    .clk(1'h0),
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
    .in(ft_flipped_a_data),
    .out(ft_original_a_data),
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
  assign ft_original_a_data = ft_flipped_a_data;
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
    .in(ft_flipped_a_data),
    .out(ft_original_a_data),
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
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid)
  );
  wire [7:0] ModuleB_i_ft_original_a_data;
  wire ModuleB_i_ft_original_a_valid;
  ModuleB ModuleB_i (
    .ft_flipped_a_data(ModuleA_i_a_data),
    .ft_original_a_data(ModuleB_i_ft_original_a_data),
    .clk(1'h0),
    .ft_flipped_a_valid(ModuleA_i_a_valid),
    .ft_original_a_valid(ModuleB_i_ft_original_a_valid)
  );
  wire [7:0] ModuleC_i_ft_original_a_data;
  wire ModuleC_i_ft_original_a_valid;
  ModuleC ModuleC_i (
    .ft_flipped_a_data(ModuleB_i_ft_original_a_data),
    .ft_original_a_data(ModuleC_i_ft_original_a_data),
    .ft_flipped_a_valid(ModuleB_i_ft_original_a_valid),
    .ft_original_a_valid(ModuleC_i_ft_original_a_valid)
  );
  wire [7:0] ModuleD_i_ft_original_a_data;
  wire ModuleD_i_ft_original_a_valid;
  ModuleD ModuleD_i (
    .ft_flipped_a_data(ModuleC_i_ft_original_a_data),
    .ft_original_a_data(ModuleD_i_ft_original_a_data),
    .clk(1'h0),
    .ft_flipped_a_valid(ModuleC_i_ft_original_a_valid),
    .ft_original_a_valid(ModuleD_i_ft_original_a_valid)
  );
  ModuleE ModuleE_i (
    .e_data(ModuleD_i_ft_original_a_data),
    .e_valid(ModuleD_i_ft_original_a_valid)
  );
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
    .in(ft_x_rx),
    .out(ft_y_tx),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(ft_y_rx),
    .out(ft_x_tx),
    .out_stages()
  );
endmodule
module ModuleC(
  input wire [7:0] ft_x_rx,
  output wire [7:0] ft_y_tx,
  output wire [7:0] ft_x_tx,
  input wire [7:0] ft_y_rx
);
  assign ft_y_tx = ft_x_rx;
  assign ft_x_tx = ft_y_rx;
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
    .in(ft_x_rx),
    .out(ft_y_tx),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ef)
  ) pipeline_conn_1 (
    .clk(clk),
    .in(ft_y_rx),
    .out(ft_x_tx),
    .out_stages()
  );
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_tx;
  wire [7:0] ModuleB_i_ft_x_tx;
  ModuleA ModuleA_i (
    .a_tx(ModuleA_i_a_tx),
    .a_rx(ModuleB_i_ft_x_tx)
  );
  wire [7:0] ModuleB_i_ft_y_tx;
  wire [7:0] ModuleC_i_ft_x_tx;
  ModuleB ModuleB_i (
    .ft_x_rx(ModuleA_i_a_tx),
    .ft_y_tx(ModuleB_i_ft_y_tx),
    .clk(1'h0),
    .ft_x_tx(ModuleB_i_ft_x_tx),
    .ft_y_rx(ModuleC_i_ft_x_tx)
  );
  wire [7:0] ModuleC_i_ft_y_tx;
  wire [7:0] ModuleD_i_ft_x_tx;
  ModuleC ModuleC_i (
    .ft_x_rx(ModuleB_i_ft_y_tx),
    .ft_y_tx(ModuleC_i_ft_y_tx),
    .ft_x_tx(ModuleC_i_ft_x_tx),
    .ft_y_rx(ModuleD_i_ft_x_tx)
  );
  wire [7:0] ModuleD_i_ft_y_tx;
  wire [7:0] ModuleE_i_e_tx;
  ModuleD ModuleD_i (
    .ft_x_rx(ModuleC_i_ft_y_tx),
    .ft_y_tx(ModuleD_i_ft_y_tx),
    .clk(1'h0),
    .ft_x_tx(ModuleD_i_ft_x_tx),
    .ft_y_rx(ModuleE_i_e_tx)
  );
  ModuleE ModuleE_i (
    .e_rx(ModuleD_i_ft_y_tx),
    .e_tx(ModuleE_i_e_tx)
  );
endmodule
"
    );
}
