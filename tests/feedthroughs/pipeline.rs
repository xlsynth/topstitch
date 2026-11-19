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
        c.emit(EmitOptions::default()),
        "\
module c(
  input wire clk_existing,
  input wire clk_new
);
  wire [170:0] a_i_out;
  logic [204:0][170:0] delay_stages;
  always_ff @ (posedge clk_existing) begin
    delay_stages <= {delay_stages[203:0], a_i_out};
  end
  wire [238:0] b_i_out;
  logic [254:0][238:0] delay_stages_1;
  always_ff @ (posedge clk_new) begin
    delay_stages_1 <= {delay_stages_1[253:0], b_i_out};
  end
  a a_i (
    .out(a_i_out),
    .in(delay_stages_1[254])
  );
  b b_i (
    .in(delay_stages[204]),
    .out(b_i_out)
  );
  d pipeline_conn_0 (

  );
  d pipeline_conn_2 (

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

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog).unwrap();
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
        top_module.emit(EmitOptions::default()),
        "\
module TopModule(
  input wire clk
);
  wire [31:0] inst_a_a_data;
  logic [204:0][31:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[203:0], inst_a_a_data};
  end
  wire inst_a_a_valid;
  logic [204:0] delay_stages_1;
  always_ff @ (posedge clk) begin
    delay_stages_1 <= {delay_stages_1[203:0], inst_a_a_valid};
  end
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid(inst_a_a_valid)
  );
  ModuleB inst_b (
    .b_data(delay_stages[204]),
    .b_valid(delay_stages_1[204])
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

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog).unwrap();
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

    let emitted = top_module.emit(EmitOptions::default());
    assert_eq!(
        emitted,
        "\
module TopModule(
  input wire clk
);
  wire [31:0] inst_a_a_data;
  logic [204:0][31:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[203:0], inst_a_a_data};
  end
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid()
  );
  ModuleB inst_b (
    .b_data(delay_stages[204]),
    .b_valid(1'h0)
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

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog).unwrap();
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
        top_module.emit(EmitOptions::default()),
        "\
module TopModule(
  input wire clk
);
  wire inst_a_a_tx;
  logic [204:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[203:0], inst_a_a_tx};
  end
  wire inst_b_b_tx;
  logic [204:0] delay_stages_1;
  always_ff @ (posedge clk) begin
    delay_stages_1 <= {delay_stages_1[203:0], inst_b_b_tx};
  end
  ModuleA inst_a (
    .a_tx(inst_a_a_tx),
    .a_rx(delay_stages_1[204])
  );
  ModuleB inst_b (
    .b_tx(inst_b_b_tx),
    .b_rx(delay_stages[204])
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

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    module_a.def_intf_from_name_underscore("intf");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog).unwrap();
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

    let emitted = top_module.emit(EmitOptions::default());
    assert_eq!(
        emitted,
        "\
module TopModule(
  input wire clk
);
  wire inst_a_intf_a_tx;
  logic [204:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[203:0], inst_a_intf_a_tx};
  end
  wire inst_b_intf_a_tx;
  logic [204:0] delay_stages_1;
  always_ff @ (posedge clk) begin
    delay_stages_1 <= {delay_stages_1[203:0], inst_b_intf_a_tx};
  end
  ModuleA inst_a (
    .intf_a_tx(inst_a_intf_a_tx),
    .intf_a_rx(delay_stages_1[204]),
    .intf_b_tx(),
    .intf_b_rx(1'h0)
  );
  ModuleB inst_b (
    .intf_a_tx(inst_b_intf_a_tx),
    .intf_a_rx(delay_stages[204]),
    .intf_b_tx(),
    .intf_b_rx(1'h0)
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
        mod_def.emit(EmitOptions::default()),
        "\
module TestModule(
  input wire [7:0] input_signal,
  output wire [7:0] output_signal,
  input wire clk
);
  logic [170:0][7:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[169:0], input_signal};
  end
  assign output_signal = delay_stages[170];
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

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    let a_intf = module_a.def_intf_from_name_underscore("a");

    let module_c = ModDef::from_verilog("ModuleC", module_c_verilog).unwrap();
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
        top_module.emit(EmitOptions::default()),
        "\
module ModuleB(
  input wire [7:0] ft_left_data_out,
  output wire [7:0] ft_right_data_out,
  input wire clk,
  input wire ft_left_valid_out,
  output wire ft_right_valid_out
);
  logic [170:0][7:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[169:0], ft_left_data_out};
  end
  assign ft_right_data_out = delay_stages[170];
  logic [170:0] delay_stages_1;
  always_ff @ (posedge clk) begin
    delay_stages_1 <= {delay_stages_1[169:0], ft_left_valid_out};
  end
  assign ft_right_valid_out = delay_stages_1[170];
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

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    module_a.def_intf_from_name_underscore("a");

    let module_e = ModDef::from_verilog("ModuleE", module_e_verilog).unwrap();
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
        top_module.emit(EmitOptions::default()),
        "\
module ModuleB(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire clk,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid
);
  logic [170:0][7:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[169:0], ft_flipped_a_data};
  end
  assign ft_original_a_data = delay_stages[170];
  logic [170:0] delay_stages_1;
  always_ff @ (posedge clk) begin
    delay_stages_1 <= {delay_stages_1[169:0], ft_flipped_a_valid};
  end
  assign ft_original_a_valid = delay_stages_1[170];
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
  logic [238:0][7:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[237:0], ft_flipped_a_data};
  end
  assign ft_original_a_data = delay_stages[238];
  logic [238:0] delay_stages_1;
  always_ff @ (posedge clk) begin
    delay_stages_1 <= {delay_stages_1[237:0], ft_flipped_a_valid};
  end
  assign ft_original_a_valid = delay_stages_1[238];
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

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    module_a.def_intf_from_name_underscore("a");

    let module_e = ModDef::from_verilog("ModuleE", module_e_verilog).unwrap();
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
        top_module.emit(EmitOptions::default()),
        "\
module ModuleB(
  input wire [7:0] ft_x_rx,
  output wire [7:0] ft_y_tx,
  input wire clk,
  output wire [7:0] ft_x_tx,
  input wire [7:0] ft_y_rx
);
  logic [170:0][7:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[169:0], ft_x_rx};
  end
  assign ft_y_tx = delay_stages[170];
  logic [170:0][7:0] delay_stages_1;
  always_ff @ (posedge clk) begin
    delay_stages_1 <= {delay_stages_1[169:0], ft_y_rx};
  end
  assign ft_x_tx = delay_stages_1[170];
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
  logic [238:0][7:0] delay_stages;
  always_ff @ (posedge clk) begin
    delay_stages <= {delay_stages[237:0], ft_x_rx};
  end
  assign ft_y_tx = delay_stages[238];
  logic [238:0][7:0] delay_stages_1;
  always_ff @ (posedge clk) begin
    delay_stages_1 <= {delay_stages_1[237:0], ft_y_rx};
  end
  assign ft_x_tx = delay_stages_1[238];
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

#[test]
fn interface_pipeline_rejects_a_later_bad_pair_before_connecting_the_first() {
    for (crossover, bad_io) in [
        (false, IO::Output(7)),
        (true, IO::Input(8)),
        (false, IO::InOut(8)),
    ] {
        let top = ModDef::new("Top");
        let tx = if crossover { "_tx" } else { "" };
        let rx = if crossover { "_rx" } else { "" };
        let valid = top.add_port(format!("tx_valid{tx}"), IO::Input(1));
        top.add_port(format!("tx_data{tx}"), IO::Input(8));
        top.add_port(format!("bad_valid{rx}"), IO::Output(1));
        top.add_port(format!("bad_data{rx}"), bad_io);
        top.add_port(format!("good_valid{rx}"), IO::Output(1));
        top.add_port(format!("good_data{rx}"), IO::Output(8));
        let source = top.def_intf_from_prefix("source", "tx_");
        let bad = top.def_intf_from_prefix("bad", "bad_");
        let good = top.def_intf_from_prefix("good", "good_");
        let connect = |target: &Intf| {
            let pipeline = PipelineConfig {
                depth: 2,
                ..Default::default()
            };
            if crossover {
                source.crossover_pipeline(target, "^(.*)_tx$", "^(.*)_rx$", pipeline);
            } else {
                source.connect_pipeline(target, pipeline, false);
            }
        };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| connect(&bad))).is_err());
        assert!(top.get_instances().is_empty() && !top.has_port("clk"));
        assert!(valid.get_connections().is_empty());
        connect(&good);
        assert_eq!(top.get_instances().len(), 2);
        assert_eq!(
            top.get_instance("pipeline_conn_0").get_port("in").io(),
            IO::Input(1)
        );
        assert!(!valid.get_connections().is_empty());
    }
}

#[test]
fn feedthrough_checks_the_proposed_clock_port_before_adding_either_side() {
    let route = ModDef::new("Route");
    for (width, clock) in [(8, "rx_data"), (1, "tx_data")] {
        let pipeline = PipelineConfig {
            depth: 2,
            clk: clock.into(),
            ..Default::default()
        };
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || route.feedthrough_pipeline("rx_data", "tx_data", width, pipeline)
            ))
            .is_err()
        );
        assert!(route.get_ports(None).is_empty() && route.get_instances().is_empty());
    }
    // A one-bit input can intentionally serve as both data and clock.
    route.feedthrough_pipeline(
        "rx_data",
        "tx_data",
        1,
        PipelineConfig {
            depth: 2,
            clk: "rx_data".into(),
            ..Default::default()
        },
    );
    assert_eq!(route.get_ports(None).len(), 2);
    assert_eq!(route.get_instances().len(), 1);
    assert!(route.emit(Default::default()).contains("posedge rx_data"));
}

#[test]
fn late_hop_clock_collision_leaves_every_route_definition_and_endpoint_untouched() {
    for api in ["port", "interface", "crossover"] {
        let top = ModDef::new("Top");
        let source = ModDef::new("Source");
        let target = ModDef::new("Target");
        source.add_port("data_tx", IO::Output(8)).tieoff(0);
        let target_pin = if api == "crossover" {
            "data_rx"
        } else {
            "data_tx"
        };
        target.add_port(target_pin, IO::Input(8)).unused();
        source.def_intf_from_prefix("src", "");
        target.def_intf_from_prefix("dst", "");
        let src = top.instantiate(&source, None, None);
        let dst = top.instantiate(&target, None, None);
        let first = ModDef::new("First");
        let last = ModDef::new("Last");
        let a = top.instantiate(&first, None, None);
        let b = top.instantiate(&last, None, None);
        let route = |last_clock: &str| {
            let pipe = PipelineConfig {
                depth: 2,
                ..Default::default()
            };
            let hops = [
                (&a, Some(pipe.clone())),
                (
                    &b,
                    Some(PipelineConfig {
                        clk: last_clock.into(),
                        ..pipe
                    }),
                ),
            ];
            match api {
                "port" => src.get_port("data_tx").connect_through_generic(
                    &dst.get_port(target_pin),
                    &hops,
                    "hop",
                ),
                "interface" => src.get_intf("src").connect_through_generic(
                    &dst.get_intf("dst"),
                    &hops,
                    "hop",
                    false,
                ),
                _ => src.get_intf("src").crossover_through_generic(
                    &dst.get_intf("dst"),
                    &hops,
                    "^(.*)_tx$",
                    "^(.*)_rx$",
                    "left",
                    "right",
                ),
            }
        };
        let colliding_port = match api {
            "port" => "hop_flipped",
            "interface" => "hop_flipped_src_data_tx",
            _ => "left_data_rx",
        };
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| route(colliding_port)))
                .is_err()
        );
        for hop in [&first, &last] {
            assert!(hop.get_ports(None).is_empty() && hop.get_instances().is_empty());
            assert!(!hop.has_intf("hop_flipped_src") && !hop.has_intf("hop_original_src"));
        }
        assert_eq!(top.get_instances().len(), 4);
        assert!(src.get_port("data_tx").get_connections().is_empty());
        assert!(dst.get_port(target_pin).get_connections().is_empty());
        route("clk");
        assert_eq!(first.get_instances().len(), 1);
        assert_eq!(last.get_instances().len(), 1);
        let clock = top.add_port("clk", IO::Input(1));
        for hop in [&a, &b] {
            hop.get_port("clk").connect(&clock);
        }
        assert!(top.emit(Default::default()).contains("module Last("));
    }
}

#[test]
fn interface_feedthrough_reserves_later_signal_and_interface_names_before_creation() {
    let source = ModDef::new("Source");
    source.add_port("bus_valid", IO::Output(1));
    source.add_port("bus_data", IO::Output(8));
    let bus = source.def_intf_from_prefix("bus", "bus_");
    for conflict in ["clock", "port", "interface"] {
        let route = ModDef::new("Route");
        let mut pipeline = PipelineConfig {
            depth: 2,
            ..Default::default()
        };
        match conflict {
            "clock" => pipeline.clk = "rx_data".into(),
            "port" => {
                route.add_port("tx_data", IO::Output(8));
            }
            _ => {
                route.add_port("existing_data", IO::Input(1));
                route.def_intf_from_prefix("tx", "existing_");
            }
        }
        let existing_ports = route.get_ports(None);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || bus.feedthrough_pipeline(&route, "rx", "tx", pipeline)
            ))
            .is_err()
        );
        assert_eq!(route.get_ports(None), existing_ports);
        assert!(route.get_instances().is_empty() && !route.has_intf("rx"));
        assert_eq!(route.has_intf("tx"), conflict == "interface");
        bus.feedthrough_pipeline(
            &route,
            "in",
            "out",
            PipelineConfig {
                depth: 2,
                ..Default::default()
            },
        );
        assert_eq!(route.get_instances().len(), 2);
        assert!(route.has_intf("in") && route.has_intf("out"));
    }
}

const PIPELINE_DFF: &str = "module Dff(input logic d, clk, output logic q);
    always_ff @(posedge clk) q <= d;
endmodule";

fn mapped_pipeline_module() -> ModDef {
    let top = ModDef::new("Top");
    let mut map = GateMap::new();
    map.flop(
        FlopFlavor::posedge(),
        ModDef::from_verilog("Dff", PIPELINE_DFF).unwrap(),
        FlopPins {
            d: "d",
            clk: "clk",
            q: "q",
            reset: None,
        },
    )
    .unwrap();
    top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    top
}

#[test]
fn concurrent_pipelines_share_their_implicit_clock() {
    use std::sync::Barrier;
    use std::thread;

    for top in [ModDef::new("Top"), mapped_pipeline_module()] {
        let channels: Vec<_> = (0..12)
            .map(|i| {
                (
                    top.add_port(format!("d{i}"), IO::Input(1)),
                    top.add_port(format!("q{i}"), IO::Output(1)),
                )
            })
            .collect();
        let start = Barrier::new(channels.len());
        thread::scope(|scope| {
            for (d, q) in channels {
                let start = &start;
                scope.spawn(move || {
                    start.wait();
                    d.connect_pipeline(
                        &q,
                        PipelineConfig {
                            depth: 1,
                            ..Default::default()
                        },
                    );
                });
            }
        });
        assert_eq!(top.get_instances().len(), 12);
        assert_eq!(top.get_port("clk").get_connections().len(), 12);
        let sv = top.emit(Default::default());
        ModDef::from_verilog("Top", format!("{PIPELINE_DFF}\n{sv}")).unwrap();
    }
}

#[test]
fn mapped_pipeline_rejects_explicit_names_before_adding_a_clock_or_a_connection() {
    let top = mapped_pipeline_module();
    let a = top.add_port("a", IO::Input(1));
    let y = top.add_port("y", IO::Output(1));
    top.add_port("existing_clk", IO::Input(1)).unused();
    let passthrough = ModDef::new("Pass");
    passthrough.feedthrough("a", "y", 1);
    let pass = top.instantiate(&passthrough, None, None);
    pass.get_port("a").connect(&a);
    let data = pass.get_port("y");
    data.specify_net_name("custom");
    let before = data.get_connections().len();
    for (clock, instance) in [
        ("clk", "clk"),
        ("existing_clk", "existing_clk"),
        ("clk", "a"),
        ("clk", "custom"),
    ] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            data.connect_pipeline(
                &y,
                PipelineConfig {
                    depth: 2,
                    clk: clock.into(),
                    inst_name: Some(instance.into()),
                },
            );
        }));
        assert!(
            result.is_err(),
            "accepted mapped instance {instance} with clock {clock}"
        );
        assert_eq!(top.get_instances().len(), 1);
        assert_eq!(data.get_connections().len(), before);
        assert!(y.get_connections().is_empty() && !top.has_port("clk"));
    }
    data.connect_pipeline(
        &y,
        PipelineConfig {
            depth: 2,
            inst_name: Some("stage".into()),
            ..Default::default()
        },
    );
    let sv = top.emit(Default::default());
    assert!(sv.contains(" stage ("));
    ModDef::from_verilog("Top", format!("{PIPELINE_DFF}\n{sv}")).unwrap();

    let inline = ModDef::new("Inline");
    inline.feedthrough_pipeline(
        "a",
        "q",
        1,
        PipelineConfig {
            depth: 2,
            inst_name: Some("clk".into()),
            ..Default::default()
        },
    );
    let sv = inline.emit(Default::default());
    assert!(sv.contains("posedge clk"));
    ModDef::from_verilog("Inline", sv).unwrap();
}

#[test]
fn automatic_mapped_pipeline_skips_current_names_and_yields_its_sv_name_to_later_ports() {
    let top = mapped_pipeline_module();
    let a = top.add_port("pipeline_conn_1", IO::Input(1));
    let y = top.add_port("y", IO::Output(1));
    let pass = ModDef::new("Pass");
    pass.feedthrough("a", "y", 1);
    let pass = top.instantiate(&pass, None, None);
    pass.get_port("a").connect(&a);
    let data = pass.get_port("y");
    data.specify_net_name("pipeline_conn_2");
    data.connect_pipeline(
        &y,
        PipelineConfig {
            depth: 2,
            clk: "pipeline_conn_0".into(),
            inst_name: None,
        },
    );
    let delay = top.get_instance("pipeline_conn_3");
    assert_eq!(delay.get_port("in").io(), IO::Input(1));
    top.add_port("pipeline_conn_3", IO::Input(1)).unused();
    let sv = top.emit(Default::default());
    assert!(sv.contains(" pipeline_conn_3_1 ("));
    ModDef::from_verilog("Top", format!("{PIPELINE_DFF}\n{sv}")).unwrap();
}

#[test]
fn mapped_interface_feedthrough_reserves_generated_signal_names() {
    let source = ModDef::new("Source");
    for name in ["tx_first", "tx_0", "tx_1"] {
        source.add_port(name, IO::Output(1));
    }
    let bus = source.def_intf_from_prefix("bus", "tx_");
    let top = mapped_pipeline_module();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bus.feedthrough_pipeline(
            &top,
            "pipeline_conn",
            "data",
            PipelineConfig {
                depth: 2,
                inst_name: Some("pipeline_conn_first".into()),
                ..Default::default()
            },
        );
    }));
    assert!(result.is_err());
    assert!(top.get_ports(None).is_empty() && top.get_instances().is_empty());
    assert!(top.get_intfs(None).is_empty());
    bus.feedthrough_pipeline(
        &top,
        "pipeline_conn",
        "data",
        PipelineConfig {
            depth: 2,
            ..Default::default()
        },
    );
    for pin in ["pipeline_conn_first", "pipeline_conn_0", "pipeline_conn_1"] {
        assert!(top.has_port(pin));
    }
    let sv = top.emit(Default::default());
    assert_eq!(top.get_instances().len(), 3);
    ModDef::from_verilog("Top", format!("{PIPELINE_DFF}\n{sv}")).unwrap();
}
