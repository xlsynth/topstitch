// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_basic() {
    // Define module A
    let a_mod_def = ModDef::new("A");
    a_mod_def.add_port("a_axi_m_wvalid", IO::Output(1));
    a_mod_def.add_port("a_axi_m_wdata", IO::Output(8));
    a_mod_def.add_port("a_axi_m_wready", IO::Input(1));

    // Validate we observe the port name we used for definition.
    assert_eq!(
        a_mod_def.get_port("a_axi_m_wvalid").name(),
        "a_axi_m_wvalid"
    );

    // Define module B
    let b_mod_def = ModDef::new("B");
    b_mod_def.add_port("b_axi_s_wvalid", IO::Input(1));
    b_mod_def.add_port("b_axi_s_wdata", IO::Input(8));
    b_mod_def.add_port("b_axi_s_wready", IO::Output(1));

    assert_eq!(
        b_mod_def.get_port("b_axi_s_wvalid").name(),
        "b_axi_s_wvalid"
    );

    // Define module C
    let c_mod_def: ModDef = ModDef::new("C");

    // Instantiate A and B in C
    let a_inst = c_mod_def.instantiate(&a_mod_def, None, None);
    let b_inst = c_mod_def.instantiate(&b_mod_def, None, None);

    a_inst
        .get_port("a_axi_m_wvalid")
        .connect(&b_inst.get_port("b_axi_s_wvalid"));
    a_inst
        .get_port("a_axi_m_wready")
        .connect(&b_inst.get_port("b_axi_s_wready"));
    a_inst
        .get_port("a_axi_m_wdata")
        .connect(&b_inst.get_port("b_axi_s_wdata"));

    a_mod_def.set_usage(Usage::EmitStubAndStop);
    b_mod_def.set_usage(Usage::EmitStubAndStop);

    assert_eq!(
        c_mod_def.emit(true),
        "\
module A(
  output wire a_axi_m_wvalid,
  output wire [7:0] a_axi_m_wdata,
  input wire a_axi_m_wready
);

endmodule
module B(
  input wire b_axi_s_wvalid,
  input wire [7:0] b_axi_s_wdata,
  output wire b_axi_s_wready
);

endmodule
module C;
  wire A_i_a_axi_m_wvalid;
  wire [7:0] A_i_a_axi_m_wdata;
  wire B_i_b_axi_s_wready;
  A A_i (
    .a_axi_m_wvalid(A_i_a_axi_m_wvalid),
    .a_axi_m_wdata(A_i_a_axi_m_wdata),
    .a_axi_m_wready(B_i_b_axi_s_wready)
  );
  B B_i (
    .b_axi_s_wvalid(A_i_a_axi_m_wvalid),
    .b_axi_s_wdata(A_i_a_axi_m_wdata),
    .b_axi_s_wready(B_i_b_axi_s_wready)
  );
endmodule
"
    );
}

#[test]
fn test_from_verilog() {
    let a_verilog = "\
module A(
  output wire a_axi_m_wvalid,
  output wire [7:0] a_axi_m_wdata,
  input wire a_axi_m_wready
);
  wire foo;
endmodule";
    let b_verilog = "\
module B(
  input wire b_axi_s_wvalid,
  input wire [7:0] b_axi_s_wdata,
  output wire b_axi_s_wready
);
  wire bar;
endmodule";
    let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
    let b_mod_def = ModDef::from_verilog("B", b_verilog, true, false);

    // Define module C
    let c_mod_def: ModDef = ModDef::new("C");

    // Instantiate A and B in C
    let a_inst = c_mod_def.instantiate(&a_mod_def, Some("inst_a"), None);
    let b_inst = c_mod_def.instantiate(&b_mod_def, Some("inst_b"), None);

    a_inst
        .get_port("a_axi_m_wvalid")
        .connect(&b_inst.get_port("b_axi_s_wvalid"));
    a_inst
        .get_port("a_axi_m_wready")
        .connect(&b_inst.get_port("b_axi_s_wready"));
    a_inst
        .get_port("a_axi_m_wdata")
        .connect(&b_inst.get_port("b_axi_s_wdata"));

    b_mod_def.set_usage(Usage::EmitStubAndStop);

    assert_eq!(
        c_mod_def.emit(true),
        "\
module B(
  input wire b_axi_s_wvalid,
  input wire [7:0] b_axi_s_wdata,
  output wire b_axi_s_wready
);

endmodule
module C;
  wire inst_a_a_axi_m_wvalid;
  wire [7:0] inst_a_a_axi_m_wdata;
  wire inst_b_b_axi_s_wready;
  A inst_a (
    .a_axi_m_wvalid(inst_a_a_axi_m_wvalid),
    .a_axi_m_wdata(inst_a_a_axi_m_wdata),
    .a_axi_m_wready(inst_b_b_axi_s_wready)
  );
  B inst_b (
    .b_axi_s_wvalid(inst_a_a_axi_m_wvalid),
    .b_axi_s_wdata(inst_a_a_axi_m_wdata),
    .b_axi_s_wready(inst_b_b_axi_s_wready)
  );
endmodule
"
    );
}
