// SPDX-License-Identifier: Apache-2.0

use num_bigint::ToBigInt;
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
    let a_mod_def = ModDef::from_verilog("A", a_verilog, true, EmitConfig::Leaf);
    let b_mod_def = ModDef::from_verilog("B", b_verilog, true, EmitConfig::Stub);

    // Define module C
    let c_mod_def: ModDef = ModDef::new("C");

    // Instantiate A and B in C
    let a_inst = c_mod_def.instantiate(&a_mod_def, "inst_a");
    let b_inst = c_mod_def.instantiate(&b_mod_def, "inst_b");

    // Connect a_axi_s_wvalid of A to b_axi_s_wvalid of B
    let a_wvalid = a_inst.get_port("a_axi_m_wvalid");
    let b_wvalid = b_inst.get_port("b_axi_s_wvalid");

    a_wvalid.connect(&b_wvalid, 0);

    // Similarly connect a_axi_s_wdata to b_axi_s_wdata
    let a_wdata = a_inst.get_port("a_axi_m_wdata");
    let b_wdata = b_inst.get_port("b_axi_s_wdata");

    a_wdata.connect(&b_wdata, 0);

    assert_eq!(
        c_mod_def.emit(),
        "\
module A(
  output wire a_axi_m_wvalid,
  output wire [7:0] a_axi_m_wdata,
  input wire a_axi_m_wready
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
  wire inst_a_a_axi_m_wvalid;
  wire [7:0] inst_a_a_axi_m_wdata;
  wire inst_a_a_axi_m_wready;
  wire inst_b_b_axi_s_wvalid;
  wire [7:0] inst_b_b_axi_s_wdata;
  wire inst_b_b_axi_s_wready;
  A inst_a (
    .a_axi_m_wvalid(inst_a_a_axi_m_wvalid),
    .a_axi_m_wdata(inst_a_a_axi_m_wdata),
    .a_axi_m_wready(inst_a_a_axi_m_wready)
  );
  B inst_b (
    .b_axi_s_wvalid(inst_b_b_axi_s_wvalid),
    .b_axi_s_wdata(inst_b_b_axi_s_wdata),
    .b_axi_s_wready(inst_b_b_axi_s_wready)
  );
  assign inst_b_b_axi_s_wvalid = inst_a_a_axi_m_wvalid;
  assign inst_b_b_axi_s_wdata[7:0] = inst_a_a_axi_m_wdata[7:0];
endmodule
"
    );
}

#[test]
fn test_tieoff() {
    // Define module A
    let a_mod_def = ModDef::new("A");
    a_mod_def.add_port("constant", IO::Output(8));
    a_mod_def
        .get_port("constant")
        .tieoff(42.to_bigint().unwrap());

    assert_eq!(
        a_mod_def.emit(),
        "\
module A(
  output wire [7:0] constant
);
  assign constant[7:0] = 8'd42;
endmodule
"
    );
}

#[test]
fn test_port_slices() {
    // Define module A
    let a_mod_def = ModDef::new("A");
    a_mod_def.add_port("bus", IO::Input(8));

    // Define module B
    let b_mod_def = ModDef::new("B");
    b_mod_def.add_port("half_bus", IO::Input(4));

    let b0 = a_mod_def.instantiate(&b_mod_def, "b0");
    let b1 = a_mod_def.instantiate(&b_mod_def, "b1");

    let a_bus = a_mod_def.get_port("bus");
    b0.get_port("half_bus").connect(&a_bus.slice(3, 0), 0);
    a_bus.slice(7, 4).connect(&b1.get_port("half_bus"), 0);

    assert_eq!(
        a_mod_def.emit(),
        "\
module B(
  input wire [3:0] half_bus
);

endmodule
module A(
  input wire [7:0] bus
);
  wire [3:0] b0_half_bus;
  wire [3:0] b1_half_bus;
  B b0 (
    .half_bus(b0_half_bus)
  );
  B b1 (
    .half_bus(b1_half_bus)
  );
  assign b0_half_bus[3:0] = bus[3:0];
  assign b1_half_bus[3:0] = bus[7:4];
endmodule
"
    );
}
