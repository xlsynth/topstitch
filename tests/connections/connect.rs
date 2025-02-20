// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_intf_connect_through() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data,
          output a_valid,
          input a_ready
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e_data,
          input e_valid,
          output e_ready
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

    a_inst.get_intf("a").connect_through(
        &e_inst.get_intf("e"),
        &[&b_inst, &c_inst, &d_inst],
        "ft",
        false,
    );

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid,
  output wire ft_flipped_a_ready,
  input wire ft_original_a_ready
);
  assign ft_original_a_data[7:0] = ft_flipped_a_data[7:0];
  assign ft_original_a_valid = ft_flipped_a_valid;
  assign ft_flipped_a_ready = ft_original_a_ready;
endmodule
module ModuleC(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid,
  output wire ft_flipped_a_ready,
  input wire ft_original_a_ready
);
  assign ft_original_a_data[7:0] = ft_flipped_a_data[7:0];
  assign ft_original_a_valid = ft_flipped_a_valid;
  assign ft_flipped_a_ready = ft_original_a_ready;
endmodule
module ModuleD(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid,
  output wire ft_flipped_a_ready,
  input wire ft_original_a_ready
);
  assign ft_original_a_data[7:0] = ft_flipped_a_data[7:0];
  assign ft_original_a_valid = ft_flipped_a_valid;
  assign ft_flipped_a_ready = ft_original_a_ready;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data;
  wire ModuleA_i_a_valid;
  wire ModuleA_i_a_ready;
  wire [7:0] ModuleB_i_ft_flipped_a_data;
  wire [7:0] ModuleB_i_ft_original_a_data;
  wire ModuleB_i_ft_flipped_a_valid;
  wire ModuleB_i_ft_original_a_valid;
  wire ModuleB_i_ft_flipped_a_ready;
  wire ModuleB_i_ft_original_a_ready;
  wire [7:0] ModuleC_i_ft_flipped_a_data;
  wire [7:0] ModuleC_i_ft_original_a_data;
  wire ModuleC_i_ft_flipped_a_valid;
  wire ModuleC_i_ft_original_a_valid;
  wire ModuleC_i_ft_flipped_a_ready;
  wire ModuleC_i_ft_original_a_ready;
  wire [7:0] ModuleD_i_ft_flipped_a_data;
  wire [7:0] ModuleD_i_ft_original_a_data;
  wire ModuleD_i_ft_flipped_a_valid;
  wire ModuleD_i_ft_original_a_valid;
  wire ModuleD_i_ft_flipped_a_ready;
  wire ModuleD_i_ft_original_a_ready;
  wire [7:0] ModuleE_i_e_data;
  wire ModuleE_i_e_valid;
  wire ModuleE_i_e_ready;
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid),
    .a_ready(ModuleA_i_a_ready)
  );
  ModuleB ModuleB_i (
    .ft_flipped_a_data(ModuleB_i_ft_flipped_a_data),
    .ft_original_a_data(ModuleB_i_ft_original_a_data),
    .ft_flipped_a_valid(ModuleB_i_ft_flipped_a_valid),
    .ft_original_a_valid(ModuleB_i_ft_original_a_valid),
    .ft_flipped_a_ready(ModuleB_i_ft_flipped_a_ready),
    .ft_original_a_ready(ModuleB_i_ft_original_a_ready)
  );
  ModuleC ModuleC_i (
    .ft_flipped_a_data(ModuleC_i_ft_flipped_a_data),
    .ft_original_a_data(ModuleC_i_ft_original_a_data),
    .ft_flipped_a_valid(ModuleC_i_ft_flipped_a_valid),
    .ft_original_a_valid(ModuleC_i_ft_original_a_valid),
    .ft_flipped_a_ready(ModuleC_i_ft_flipped_a_ready),
    .ft_original_a_ready(ModuleC_i_ft_original_a_ready)
  );
  ModuleD ModuleD_i (
    .ft_flipped_a_data(ModuleD_i_ft_flipped_a_data),
    .ft_original_a_data(ModuleD_i_ft_original_a_data),
    .ft_flipped_a_valid(ModuleD_i_ft_flipped_a_valid),
    .ft_original_a_valid(ModuleD_i_ft_original_a_valid),
    .ft_flipped_a_ready(ModuleD_i_ft_flipped_a_ready),
    .ft_original_a_ready(ModuleD_i_ft_original_a_ready)
  );
  ModuleE ModuleE_i (
    .e_data(ModuleE_i_e_data),
    .e_valid(ModuleE_i_e_valid),
    .e_ready(ModuleE_i_e_ready)
  );
  assign ModuleB_i_ft_flipped_a_data[7:0] = ModuleA_i_a_data[7:0];
  assign ModuleB_i_ft_flipped_a_valid = ModuleA_i_a_valid;
  assign ModuleA_i_a_ready = ModuleB_i_ft_flipped_a_ready;
  assign ModuleC_i_ft_flipped_a_data[7:0] = ModuleB_i_ft_original_a_data[7:0];
  assign ModuleC_i_ft_flipped_a_valid = ModuleB_i_ft_original_a_valid;
  assign ModuleB_i_ft_original_a_ready = ModuleC_i_ft_flipped_a_ready;
  assign ModuleD_i_ft_flipped_a_data[7:0] = ModuleC_i_ft_original_a_data[7:0];
  assign ModuleD_i_ft_flipped_a_valid = ModuleC_i_ft_original_a_valid;
  assign ModuleC_i_ft_original_a_ready = ModuleD_i_ft_flipped_a_ready;
  assign ModuleE_i_e_data[7:0] = ModuleD_i_ft_original_a_data[7:0];
  assign ModuleE_i_e_valid = ModuleD_i_ft_original_a_valid;
  assign ModuleD_i_ft_original_a_ready = ModuleE_i_e_ready;
endmodule
"
    );
}

#[test]
fn test_connect_through() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    let module_e = ModDef::from_verilog("ModuleE", module_e_verilog, true, false);

    let module_b = ModDef::new("ModuleB");
    let module_c = ModDef::new("ModuleC");
    let module_d = ModDef::new("ModuleD");

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);
    let d_inst = top_module.instantiate(&module_d, None, None);
    let e_inst = top_module.instantiate(&module_e, None, None);

    a_inst
        .get_port("a")
        .connect_through(&e_inst.get_port("e"), &[&b_inst, &c_inst, &d_inst], "ft");

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original
);
  assign ft_original[7:0] = ft_flipped[7:0];
endmodule
module ModuleC(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original
);
  assign ft_original[7:0] = ft_flipped[7:0];
endmodule
module ModuleD(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original
);
  assign ft_original[7:0] = ft_flipped[7:0];
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a;
  wire [7:0] ModuleB_i_ft_flipped;
  wire [7:0] ModuleB_i_ft_original;
  wire [7:0] ModuleC_i_ft_flipped;
  wire [7:0] ModuleC_i_ft_original;
  wire [7:0] ModuleD_i_ft_flipped;
  wire [7:0] ModuleD_i_ft_original;
  wire [7:0] ModuleE_i_e;
  ModuleA ModuleA_i (
    .a(ModuleA_i_a)
  );
  ModuleB ModuleB_i (
    .ft_flipped(ModuleB_i_ft_flipped),
    .ft_original(ModuleB_i_ft_original)
  );
  ModuleC ModuleC_i (
    .ft_flipped(ModuleC_i_ft_flipped),
    .ft_original(ModuleC_i_ft_original)
  );
  ModuleD ModuleD_i (
    .ft_flipped(ModuleD_i_ft_flipped),
    .ft_original(ModuleD_i_ft_original)
  );
  ModuleE ModuleE_i (
    .e(ModuleE_i_e)
  );
  assign ModuleB_i_ft_flipped[7:0] = ModuleA_i_a[7:0];
  assign ModuleC_i_ft_flipped[7:0] = ModuleB_i_ft_original[7:0];
  assign ModuleD_i_ft_flipped[7:0] = ModuleC_i_ft_original[7:0];
  assign ModuleE_i_e[7:0] = ModuleD_i_ft_original[7:0];
endmodule
"
    );
}

#[test]
fn test_connect_through_generic() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    let module_e = ModDef::from_verilog("ModuleE", module_e_verilog, true, false);

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

    a_inst.get_port("a").connect_through_generic(
        &e_inst.get_port("e"),
        &[(&b_inst, cfg(0xab)), (&c_inst, None), (&d_inst, cfg(0xef))],
        "ft",
    );

    b_inst.get_port("clk").tieoff(0);
    d_inst.get_port("clk").tieoff(0);

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_flipped[7:0]),
    .out(ft_original[7:0]),
    .out_stages()
  );
endmodule
module ModuleC(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original
);
  assign ft_original[7:0] = ft_flipped[7:0];
endmodule
module ModuleD(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ef)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_flipped[7:0]),
    .out(ft_original[7:0]),
    .out_stages()
  );
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a;
  wire [7:0] ModuleB_i_ft_flipped;
  wire [7:0] ModuleB_i_ft_original;
  wire [7:0] ModuleC_i_ft_flipped;
  wire [7:0] ModuleC_i_ft_original;
  wire [7:0] ModuleD_i_ft_flipped;
  wire [7:0] ModuleD_i_ft_original;
  wire [7:0] ModuleE_i_e;
  ModuleA ModuleA_i (
    .a(ModuleA_i_a)
  );
  ModuleB ModuleB_i (
    .ft_flipped(ModuleB_i_ft_flipped),
    .ft_original(ModuleB_i_ft_original),
    .clk(1'h0)
  );
  ModuleC ModuleC_i (
    .ft_flipped(ModuleC_i_ft_flipped),
    .ft_original(ModuleC_i_ft_original)
  );
  ModuleD ModuleD_i (
    .ft_flipped(ModuleD_i_ft_flipped),
    .ft_original(ModuleD_i_ft_original),
    .clk(1'h0)
  );
  ModuleE ModuleE_i (
    .e(ModuleE_i_e)
  );
  assign ModuleB_i_ft_flipped[7:0] = ModuleA_i_a[7:0];
  assign ModuleC_i_ft_flipped[7:0] = ModuleB_i_ft_original[7:0];
  assign ModuleD_i_ft_flipped[7:0] = ModuleC_i_ft_original[7:0];
  assign ModuleE_i_e[7:0] = ModuleD_i_ft_original[7:0];
endmodule
"
    );
}
