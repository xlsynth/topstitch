// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_basic() {
    // Define module A
    let a_mod_def = ModDef::new("A");
    a_mod_def.add_port("a_axi_s_wvalid", IO::Input(1));
    a_mod_def.add_port("a_axi_s_wdata", IO::Input(8));
    a_mod_def.add_port("a_axi_s_wready", IO::Output(1));

    // Define module B
    let b_mod_def = ModDef::new("B");
    b_mod_def.add_port("b_axi_s_wvalid", IO::Input(1));
    b_mod_def.add_port("b_axi_s_wdata", IO::Input(8));
    b_mod_def.add_port("b_axi_s_wready", IO::Output(1));

    // Define module C
    let c_mod_def: ModDef = ModDef::new("C");

    // Instantiate A and B in C
    let a_inst = c_mod_def.instantiate(&a_mod_def, "inst_a");
    let b_inst = c_mod_def.instantiate(&b_mod_def, "inst_b");

    // Connect a_axi_s_wvalid of A to b_axi_s_wvalid of B
    let a_wvalid = a_inst.get_port("a_axi_s_wvalid");
    let b_wvalid = b_inst.get_port("b_axi_s_wvalid");

    a_wvalid.connect(&b_wvalid, 0);

    // Similarly connect a_axi_s_wdata to b_axi_s_wdata
    let a_wdata = a_inst.get_port("a_axi_s_wdata");
    let b_wdata = b_inst.get_port("b_axi_s_wdata");

    a_wdata.connect(&b_wdata, 0);

    assert_eq!(
        c_mod_def.emit(),
        "\
module A(
  input wire a_axi_s_wvalid,
  input wire [7:0] a_axi_s_wdata,
  output wire a_axi_s_wready
);

endmodule
module B(
  input wire b_axi_s_wvalid,
  input wire [7:0] b_axi_s_wdata,
  output wire b_axi_s_wready
);

endmodule
module C;
  wire inst_a_a_axi_s_wvalid;
  wire [7:0] inst_a_a_axi_s_wdata;
  wire inst_a_a_axi_s_wready;
  wire inst_b_b_axi_s_wvalid;
  wire [7:0] inst_b_b_axi_s_wdata;
  wire inst_b_b_axi_s_wready;
  A inst_a (
    .a_axi_s_wvalid(inst_a_a_axi_s_wvalid),
    .a_axi_s_wdata(inst_a_a_axi_s_wdata),
    .a_axi_s_wready(inst_a_a_axi_s_wready)
  );
  B inst_b (
    .b_axi_s_wvalid(inst_b_b_axi_s_wvalid),
    .b_axi_s_wdata(inst_b_b_axi_s_wdata),
    .b_axi_s_wready(inst_b_b_axi_s_wready)
  );
  assign inst_b_b_axi_s_wvalid = inst_a_a_axi_s_wvalid;
  assign inst_b_b_axi_s_wdata[7:0] = inst_a_a_axi_s_wdata[7:0];
endmodule
"
    );
}

#[test]
fn test_from_verilog() {
    let a_verilog = "\
module A(
  input wire a_axi_s_wvalid,
  input wire [7:0] a_axi_s_wdata,
  output wire a_axi_s_wready
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
    let a_mod_def = ModDef::from_verilog("A", a_verilog, true, EmitConfig::Leaf);
    let b_mod_def = ModDef::from_verilog("B", b_verilog, true, EmitConfig::Stub);

    // Define module C
    let c_mod_def: ModDef = ModDef::new("C");

    // Instantiate A and B in C
    let a_inst = c_mod_def.instantiate(&a_mod_def, "inst_a");
    let b_inst = c_mod_def.instantiate(&b_mod_def, "inst_b");

    // Connect a_axi_s_wvalid of A to b_axi_s_wvalid of B
    let a_wvalid = a_inst.get_port("a_axi_s_wvalid");
    let b_wvalid = b_inst.get_port("b_axi_s_wvalid");

    a_wvalid.connect(&b_wvalid, 0);

    // Similarly connect a_axi_s_wdata to b_axi_s_wdata
    let a_wdata = a_inst.get_port("a_axi_s_wdata");
    let b_wdata = b_inst.get_port("b_axi_s_wdata");

    a_wdata.connect(&b_wdata, 0);

    assert_eq!(
        c_mod_def.emit(),
        "\
module A(
  input wire a_axi_s_wvalid,
  input wire [7:0] a_axi_s_wdata,
  output wire a_axi_s_wready
);
  wire foo;
endmodule
module B(
  input wire b_axi_s_wvalid,
  input wire [7:0] b_axi_s_wdata,
  output wire b_axi_s_wready
);

endmodule
module C;
  wire inst_a_a_axi_s_wvalid;
  wire [7:0] inst_a_a_axi_s_wdata;
  wire inst_a_a_axi_s_wready;
  wire inst_b_b_axi_s_wvalid;
  wire [7:0] inst_b_b_axi_s_wdata;
  wire inst_b_b_axi_s_wready;
  A inst_a (
    .a_axi_s_wvalid(inst_a_a_axi_s_wvalid),
    .a_axi_s_wdata(inst_a_a_axi_s_wdata),
    .a_axi_s_wready(inst_a_a_axi_s_wready)
  );
  B inst_b (
    .b_axi_s_wvalid(inst_b_b_axi_s_wvalid),
    .b_axi_s_wdata(inst_b_b_axi_s_wdata),
    .b_axi_s_wready(inst_b_b_axi_s_wready)
  );
  assign inst_b_b_axi_s_wvalid = inst_a_a_axi_s_wvalid;
  assign inst_b_b_axi_s_wdata[7:0] = inst_a_a_axi_s_wdata[7:0];
endmodule
"
    );
}
