// SPDX-License-Identifier: Apache-2.0

use num_bigint::ToBigInt;
use topstitch::*;

#[test]
fn test_basic() {
    // Define module A
    let a_mod_def = ModDef::new("A");
    a_mod_def.add_port("a_axi_m_wvalid", IO::Output(1));
    a_mod_def.add_port("a_axi_m_wdata", IO::Output(8));
    a_mod_def.add_port("a_axi_m_wready", IO::Input(1));

    // Define module B
    let b_mod_def = ModDef::new("B");
    b_mod_def.add_port("b_axi_s_wvalid", IO::Input(1));
    b_mod_def.add_port("b_axi_s_wdata", IO::Input(8));
    b_mod_def.add_port("b_axi_s_wready", IO::Output(1));

    // Define module C
    let c_mod_def: ModDef = ModDef::new("C");

    // Instantiate A and B in C
    let a_inst = c_mod_def.instantiate(&a_mod_def, "inst_a", None);
    let b_inst = c_mod_def.instantiate(&b_mod_def, "inst_b", None);

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
    let a_inst = c_mod_def.instantiate(&a_mod_def, "inst_a", None);
    let b_inst = c_mod_def.instantiate(&b_mod_def, "inst_b", None);

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

    let b0 = a_mod_def.instantiate(&b_mod_def, "b0", None);
    let b1 = a_mod_def.instantiate(&b_mod_def, "b1", None);

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

#[test]
fn test_interfaces() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid,
        input a_ready
    );
    endmodule
    ";

    let module_b_verilog = "
    module ModuleB (
        input [31:0] b_data,
        input b_valid,
        output b_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, EmitConfig::Nothing);
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, EmitConfig::Nothing);
    module_b.def_intf_from_prefix("b_intf", "b_");

    let top_module = ModDef::new("TopModule");

    let a_inst = top_module.instantiate(&module_a, "inst_a", None);
    let b_inst = top_module.instantiate(&module_b, "inst_b", None);

    let a_intf = a_inst.get_intf("a_intf");
    let b_intf = b_inst.get_intf("b_intf");

    a_intf.connect(&b_intf, 0, false);

    assert_eq!(
        top_module.emit(),
        "\
module TopModule;
  wire [31:0] inst_a_a_data;
  wire inst_a_a_valid;
  wire inst_a_a_ready;
  wire [31:0] inst_b_b_data;
  wire inst_b_b_valid;
  wire inst_b_b_ready;
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid(inst_a_a_valid),
    .a_ready(inst_a_a_ready)
  );
  ModuleB inst_b (
    .b_data(inst_b_b_data),
    .b_valid(inst_b_b_valid),
    .b_ready(inst_b_b_ready)
  );
  assign inst_b_b_data[31:0] = inst_a_a_data[31:0];
  assign inst_b_b_valid = inst_a_a_valid;
  assign inst_a_a_ready = inst_b_b_ready;
endmodule
"
    );
}

#[test]
fn test_interface_connection_moddef_to_modinst() {
    let module_b_verilog = "
    module ModuleB (
        output [31:0] b_data,
        output b_valid,
        input b_ready
    );
    endmodule
    ";

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, EmitConfig::Nothing);
    module_b.def_intf_from_prefix("b", "b_");

    let module_a = ModDef::new("ModuleA");
    module_a.add_port("a_data", IO::Output(32));
    module_a.add_port("a_valid", IO::Output(1));
    module_a.add_port("a_ready", IO::Input(1));
    module_a.def_intf_from_prefix("a", "a_");

    let b_inst = module_a.instantiate(&module_b, "inst_b", None);

    let mod_a_intf = module_a.get_intf("a");
    let b_intf = b_inst.get_intf("b");
    mod_a_intf.connect(&b_intf, 0, false);

    assert_eq!(
        module_a.emit(),
        "\
module ModuleA(
  output wire [31:0] a_data,
  output wire a_valid,
  input wire a_ready
);
  wire [31:0] inst_b_b_data;
  wire inst_b_b_valid;
  wire inst_b_b_ready;
  ModuleB inst_b (
    .b_data(inst_b_b_data),
    .b_valid(inst_b_b_valid),
    .b_ready(inst_b_b_ready)
  );
  assign a_data[31:0] = inst_b_b_data[31:0];
  assign a_valid = inst_b_b_valid;
  assign inst_b_b_ready = a_ready;
endmodule
"
    );
}

#[test]
fn test_interface_connection_within_moddef() {
    let module = ModDef::new("MyModule");

    module.add_port("a_data", IO::Input(32));
    module.add_port("a_valid", IO::Input(1));
    module.add_port("a_ready", IO::Output(1));

    module.add_port("b_data", IO::Output(32));
    module.add_port("b_valid", IO::Output(1));
    module.add_port("b_ready", IO::Input(1));

    module.def_intf_from_prefix("a_intf", "a_");
    module.def_intf_from_prefix("b_intf", "b_");

    let a_intf = module.get_intf("a_intf");
    let b_intf = module.get_intf("b_intf");

    a_intf.connect(&b_intf, 0, false);

    assert_eq!(
        module.emit(),
        "\
module MyModule(
  input wire [31:0] a_data,
  input wire a_valid,
  output wire a_ready,
  output wire [31:0] b_data,
  output wire b_valid,
  input wire b_ready
);
  assign b_data[31:0] = a_data[31:0];
  assign b_valid = a_valid;
  assign a_ready = b_ready;
endmodule
"
    );
}

#[test]
fn test_export_interface_with_prefix() {
    // Define ModuleB
    let module_b_verilog = "
    module ModuleB (
        output [31:0] b_data,
        output b_valid,
        input b_ready
    );
    endmodule
    ";

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, EmitConfig::Nothing);
    module_b.def_intf_from_prefix("b", "b_");

    let module_a = ModDef::new("ModuleA");

    let b_inst = module_a.instantiate(&module_b, "inst_b", None);
    let b_intf = b_inst.get_intf("b");
    b_intf.export_with_prefix("a_");

    assert_eq!(
        module_a.emit(),
        "\
module ModuleA(
  output wire [31:0] a_data,
  output wire a_valid,
  input wire a_ready
);
  wire [31:0] inst_b_b_data;
  wire inst_b_b_valid;
  wire inst_b_b_ready;
  ModuleB inst_b (
    .b_data(inst_b_b_data),
    .b_valid(inst_b_b_valid),
    .b_ready(inst_b_b_ready)
  );
  assign a_data[31:0] = inst_b_b_data[31:0];
  assign a_valid = inst_b_b_valid;
  assign inst_b_b_ready = a_ready;
endmodule
"
    );
}

#[test]
fn test_export_as_single_port() {
    // Define ModuleB with a single output port
    let module_b_verilog = "
    module ModuleB (
        output [7:0] data_out
    );
    endmodule
    ";

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, EmitConfig::Nothing);
    let module_a = ModDef::new("ModuleA");

    let b_inst = module_a.instantiate(&module_b, "inst_b", None);
    let data_out_port = b_inst.get_port("data_out");
    data_out_port.export_as("data_out");

    assert_eq!(
        module_a.emit(),
        "\
module ModuleA(
  output wire [7:0] data_out
);
  wire [7:0] inst_b_data_out;
  ModuleB inst_b (
    .data_out(inst_b_data_out)
  );
  assign data_out[7:0] = inst_b_data_out[7:0];
endmodule
"
    );
}

#[test]
fn test_feedthrough() {
    let mod_def = ModDef::new("TestModule");
    mod_def.feedthrough("input_signal", "output_signal", 8, 0);
    assert_eq!(
        mod_def.emit(),
        "\
module TestModule(
  input wire [7:0] input_signal,
  output wire [7:0] output_signal
);
  assign output_signal[7:0] = input_signal[7:0];
endmodule
"
    );
}

#[test]
fn test_wrap() {
    let original_mod = ModDef::new("OriginalModule");
    original_mod.add_port("data_in", IO::Input(16));
    original_mod.add_port("data_out", IO::Output(16));
    original_mod.core.borrow_mut().emit_config = EmitConfig::Nothing;

    original_mod.def_intf_from_prefix("data_intf", "data_");

    let wrapped_mod = original_mod.wrap(None, None, 0);

    let top_mod = ModDef::new("TopModule");
    let wrapped_inst = top_mod.instantiate(&wrapped_mod, "wrapped_inst", None);

    wrapped_inst
        .get_intf("data_intf")
        .export_with_prefix("top_");

    assert_eq!(
        top_mod.emit(),
        "\
module OriginalModule_wrapper(
  input wire [15:0] data_in,
  output wire [15:0] data_out
);
  wire [15:0] OriginalModule_inst_data_in;
  wire [15:0] OriginalModule_inst_data_out;
  OriginalModule OriginalModule_inst (
    .data_in(OriginalModule_inst_data_in),
    .data_out(OriginalModule_inst_data_out)
  );
  assign OriginalModule_inst_data_in[15:0] = data_in[15:0];
  assign data_out[15:0] = OriginalModule_inst_data_out[15:0];
endmodule
module TopModule(
  input wire [15:0] top_in,
  output wire [15:0] top_out
);
  wire [15:0] wrapped_inst_data_in;
  wire [15:0] wrapped_inst_data_out;
  OriginalModule_wrapper wrapped_inst (
    .data_in(wrapped_inst_data_in),
    .data_out(wrapped_inst_data_out)
  );
  assign wrapped_inst_data_in[15:0] = top_in[15:0];
  assign top_out[15:0] = wrapped_inst_data_out[15:0];
endmodule
"
    );
}

#[test]
fn test_autoconnect() {
    let parent_mod = ModDef::new("ParentModule");

    parent_mod.add_port("clk", IO::Input(1));
    parent_mod.add_port("unused", IO::Input(1));

    let child_mod = ModDef::new("ChildModule");
    child_mod.add_port("clk", IO::Input(1));
    child_mod.add_port("rst", IO::Input(1));
    child_mod.add_port("data", IO::Output(8));

    let autoconnect_ports = ["clk", "rst", "nonexistent"];
    parent_mod.instantiate(&child_mod, "child_inst", Some(&autoconnect_ports));

    assert_eq!(
      parent_mod.emit(),
      "\
module ChildModule(
  input wire clk,
  input wire rst,
  output wire [7:0] data
);

endmodule
module ParentModule(
  input wire clk,
  input wire unused,
  input wire rst
);
  wire child_inst_clk;
  wire child_inst_rst;
  wire [7:0] child_inst_data;
  ChildModule child_inst (
    .clk(child_inst_clk),
    .rst(child_inst_rst),
    .data(child_inst_data)
  );
  assign child_inst_clk = clk;
  assign child_inst_rst = rst;
endmodule
"
    );
}
