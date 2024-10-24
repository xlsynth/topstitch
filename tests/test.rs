// SPDX-License-Identifier: Apache-2.0

mod tests {

    use indexmap::IndexMap;
    use slang_rs::str2tmpfile;
    use std::time::Instant;
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
  wire A_i_a_axi_m_wready;
  wire B_i_b_axi_s_wvalid;
  wire [7:0] B_i_b_axi_s_wdata;
  wire B_i_b_axi_s_wready;
  A A_i (
    .a_axi_m_wvalid(A_i_a_axi_m_wvalid),
    .a_axi_m_wdata(A_i_a_axi_m_wdata),
    .a_axi_m_wready(A_i_a_axi_m_wready)
  );
  B B_i (
    .b_axi_s_wvalid(B_i_b_axi_s_wvalid),
    .b_axi_s_wdata(B_i_b_axi_s_wdata),
    .b_axi_s_wready(B_i_b_axi_s_wready)
  );
  assign B_i_b_axi_s_wvalid = A_i_a_axi_m_wvalid;
  assign A_i_a_axi_m_wready = B_i_b_axi_s_wready;
  assign B_i_b_axi_s_wdata[7:0] = A_i_a_axi_m_wdata[7:0];
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
  assign inst_a_a_axi_m_wready = inst_b_b_axi_s_wready;
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
        a_mod_def.get_port("constant").tieoff(0x42);

        assert_eq!(
            a_mod_def.emit(true),
            "\
module A(
  output wire [7:0] constant
);
  assign constant[7:0] = 8'h42;
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

        let b0 = a_mod_def.instantiate(&b_mod_def, Some("b0"), None);
        let b1 = a_mod_def.instantiate(&b_mod_def, Some("b1"), None);

        let a_bus = a_mod_def.get_port("bus");
        b0.get_port("half_bus").connect(&a_bus.slice(3, 0));
        a_bus.slice(7, 4).connect(&b1.get_port("half_bus"));

        b_mod_def.set_usage(Usage::EmitStubAndStop);

        assert_eq!(
            a_mod_def.emit(true),
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

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        module_a.def_intf_from_prefix("a_intf", "a_");

        let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
        module_b.def_intf_from_prefix("b_intf", "b_");

        let top_module = ModDef::new("TopModule");

        let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);
        let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

        let a_intf = a_inst.get_intf("a_intf");
        let b_intf = b_inst.get_intf("b_intf");

        a_intf.connect(&b_intf, false);

        assert_eq!(
            top_module.emit(true),
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

        let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
        module_b.def_intf_from_prefix("b", "b_");

        let module_a = ModDef::new("ModuleA");
        module_a.add_port("a_data", IO::Output(32));
        module_a.add_port("a_valid", IO::Output(1));
        module_a.add_port("a_ready", IO::Input(1));
        module_a.def_intf_from_prefix("a", "a_");

        let b_inst = module_a.instantiate(&module_b, Some("inst_b"), None);

        let mod_a_intf = module_a.get_intf("a");
        let b_intf = b_inst.get_intf("b");
        mod_a_intf.connect(&b_intf, false);

        assert_eq!(
            module_a.emit(true),
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

        a_intf.connect(&b_intf, false);

        assert_eq!(
            module.emit(true),
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
        // Define ModuleA
        let module_a_verilog = "
        module ModuleA (
            output [31:0] a_data,
            output a_valid,
            input a_ready
        );
        endmodule
        ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        module_a.def_intf_from_prefix("a", "a_");

        let module_b = ModDef::new("ModuleB");
        module_b
            .instantiate(&module_a, None, None)
            .get_intf("a")
            .export_with_prefix("b", "b_");

        let module_c = ModDef::new("ModuleC");
        module_c
            .instantiate(&module_b, None, None)
            .get_intf("b")
            .export_with_name_underscore("c");

        assert_eq!(
            module_c.emit(true),
            "\
module ModuleB(
  output wire [31:0] b_data,
  output wire b_valid,
  input wire b_ready
);
  wire [31:0] ModuleA_i_a_data;
  wire ModuleA_i_a_valid;
  wire ModuleA_i_a_ready;
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid),
    .a_ready(ModuleA_i_a_ready)
  );
  assign b_data[31:0] = ModuleA_i_a_data[31:0];
  assign b_valid = ModuleA_i_a_valid;
  assign ModuleA_i_a_ready = b_ready;
endmodule
module ModuleC(
  output wire [31:0] c_data,
  output wire c_valid,
  input wire c_ready
);
  wire [31:0] ModuleB_i_b_data;
  wire ModuleB_i_b_valid;
  wire ModuleB_i_b_ready;
  ModuleB ModuleB_i (
    .b_data(ModuleB_i_b_data),
    .b_valid(ModuleB_i_b_valid),
    .b_ready(ModuleB_i_b_ready)
  );
  assign c_data[31:0] = ModuleB_i_b_data[31:0];
  assign c_valid = ModuleB_i_b_valid;
  assign ModuleB_i_b_ready = c_ready;
endmodule
"
        );
    }

    #[test]
    fn test_export_as_single_port() {
        // Define ModuleB with a single output port
        let module_b_verilog = "
        module ModuleB (
            input [7:0] data_in,
            output [7:0] data_out
        );
        endmodule
        ";

        let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
        let module_a = ModDef::new("ModuleA");

        let b_inst = module_a.instantiate(&module_b, Some("inst_b"), None);
        b_inst.get_port("data_in").export_as("b_data_in");
        b_inst.get_port("data_out").export();

        assert_eq!(
            module_a.emit(true),
            "\
module ModuleA(
  input wire [7:0] b_data_in,
  output wire [7:0] data_out
);
  wire [7:0] inst_b_data_in;
  wire [7:0] inst_b_data_out;
  ModuleB inst_b (
    .data_in(inst_b_data_in),
    .data_out(inst_b_data_out)
  );
  assign inst_b_data_in[7:0] = b_data_in[7:0];
  assign data_out[7:0] = inst_b_data_out[7:0];
endmodule
"
        );
    }

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

        original_mod.def_intf_from_prefix("data_intf", "data_");

        let wrapped_mod = original_mod.wrap(None, None);

        let top_mod = ModDef::new("TopModule");
        let wrapped_inst = top_mod.instantiate(&wrapped_mod, Some("wrapped_inst"), None);

        wrapped_inst
            .get_intf("data_intf")
            .export_with_prefix("top", "top_");

        original_mod.set_usage(Usage::EmitNothingAndStop);

        assert_eq!(
            top_mod.emit(true),
            "\
module OriginalModule_wrapper(
  input wire [15:0] data_in,
  output wire [15:0] data_out
);
  wire [15:0] OriginalModule_i_data_in;
  wire [15:0] OriginalModule_i_data_out;
  OriginalModule OriginalModule_i (
    .data_in(OriginalModule_i_data_in),
    .data_out(OriginalModule_i_data_out)
  );
  assign OriginalModule_i_data_in[15:0] = data_in[15:0];
  assign data_out[15:0] = OriginalModule_i_data_out[15:0];
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
        parent_mod.add_port("unused", IO::Input(1)).unused();

        let child_mod = ModDef::new("ChildModule");
        child_mod.add_port("clk", IO::Input(1));
        child_mod.add_port("rst", IO::Input(1));
        child_mod.add_port("data", IO::Output(8));

        let autoconnect_ports = ["clk", "rst", "nonexistent"];
        let child_inst =
            parent_mod.instantiate(&child_mod, Some("child_inst"), Some(&autoconnect_ports));
        child_inst.get_port("data").unused();

        child_mod.set_usage(Usage::EmitStubAndStop);

        assert_eq!(
            parent_mod.emit(true),
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

    #[test]
    #[should_panic(expected = "TestMod.out is not fully driven")]
    fn test_moddef_output_undriven() {
        let mod_def = ModDef::new("TestMod");
        mod_def.add_port("out", IO::Output(1));
        mod_def.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "TestMod.out[0:0] is multiply driven")]
    fn test_moddef_output_multiple_drivers() {
        let mod_def = ModDef::new("TestMod");
        let out_port = mod_def.add_port("out", IO::Output(1));
        let in_port1 = mod_def.add_port("in1", IO::Input(1));
        let in_port2 = mod_def.add_port("in2", IO::Input(1));

        out_port.connect(&in_port1);
        out_port.connect(&in_port2);

        mod_def.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "ParentMod.leaf_inst.in is not fully driven")]
    fn test_modinst_input_undriven() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("in", IO::Input(1));

        let parent = ModDef::new("ParentMod");
        parent.instantiate(&leaf, Some("leaf_inst"), None);
        parent.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "ParentMod.leaf_inst.in[0:0] is multiply driven")]
    fn test_modinst_input_multiple_drivers() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("in", IO::Input(1));

        let parent = ModDef::new("ParentMod");
        let in_port1 = parent.add_port("in1", IO::Input(1));
        let in_port2 = parent.add_port("in2", IO::Input(1));

        let inst = parent.instantiate(&leaf, Some("leaf_inst"), None);

        inst.get_port("in").connect(&in_port1);
        inst.get_port("in").connect(&in_port2);

        parent.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "TestMod.in is not fully used")]
    fn test_moddef_input_not_driving_anything() {
        let mod_def = ModDef::new("TestMod");
        mod_def.add_port("in", IO::Input(1));
        mod_def.validate(); // Should panic
    }

    #[test]
    fn test_moddef_input_unused() {
        let mod_def = ModDef::new("TestMod");
        let in_port = mod_def.add_port("in", IO::Input(1));
        in_port.unused();
        mod_def.validate(); // Should pass
    }

    #[test]
    #[should_panic(expected = "ParentMod.leaf_inst.out is not fully used")]
    fn test_modinst_output_not_driving_anything() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("out", IO::Output(1));

        let parent = ModDef::new("ParentMod");
        parent.instantiate(&leaf, Some("leaf_inst"), None);
        parent.validate(); // Should panic
    }

    #[test]
    fn test_modinst_output_unused() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("out", IO::Output(1));

        let parent = ModDef::new("ParentMod");
        let inst = parent.instantiate(&leaf, Some("leaf_inst"), None);
        inst.get_port("out").unused();
        parent.validate(); // Should pass
    }

    #[test]
    #[should_panic(expected = "Invalid connection")]
    fn test_moddef_input_driven_within_moddef() {
        let mod_def = ModDef::new("TestMod");
        let in_port_0 = mod_def.add_port("in0", IO::Input(1));
        let in_port_1 = mod_def.add_port("in1", IO::Input(1));
        in_port_0.connect(&in_port_1);
        mod_def.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "Invalid connection")]
    fn test_modinst_output_driven_within_moddef() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("out", IO::Output(1));

        let parent = ModDef::new("ParentMod");
        let inst = parent.instantiate(&leaf, Some("leaf_inst"), None);

        let in_port = parent.add_port("in", IO::Input(1));
        inst.get_port("out").connect(&in_port);

        parent.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "Slice ModDef2.in[0:0] is not in module ModDef1")]
    fn test_moddef_port_connected_outside_moddef() {
        let mod_def_1 = ModDef::new("ModDef1");
        let port_1 = mod_def_1.add_port("out", IO::Output(1));

        let mod_def_2 = ModDef::new("ModDef2");
        let port_2 = mod_def_2.add_port("in", IO::Input(1));

        port_1.connect(&port_2);

        mod_def_1.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "Slice ParentMod2.leaf_inst2.in[0:0] is not in module ParentMod1")]
    fn test_modinst_port_connected_outside_instantiating_moddef() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("in", IO::Input(1));
        leaf.add_port("out", IO::Output(1));

        let parent1 = ModDef::new("ParentMod1");
        let inst1 = parent1.instantiate(&leaf, Some("leaf_inst1"), None);

        let parent2 = ModDef::new("ParentMod2");
        let inst2 = parent2.instantiate(&leaf, Some("leaf_inst2"), None);

        inst1.get_port("out").connect(&inst2.get_port("in"));

        parent1.validate(); // Should panic
    }

    #[test]
    fn test_valid_connection_within_moddef() {
        let mod_def = ModDef::new("TestMod");
        let in_port = mod_def.add_port("in", IO::Input(1));
        let out_port = mod_def.add_port("out", IO::Output(1));

        out_port.connect(&in_port);

        mod_def.validate(); // Should pass
    }

    #[test]
    fn test_valid_connection_moddef_to_modinst() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("in", IO::Input(1));
        leaf.add_port("out", IO::Output(1));

        let parent = ModDef::new("ParentMod");
        let inst = parent.instantiate(&leaf, Some("leaf_inst"), None);

        let parent_in = parent.add_port("in", IO::Input(1));
        let parent_out = parent.add_port("out", IO::Output(1));

        inst.get_port("in").connect(&parent_in);
        parent_out.connect(&inst.get_port("out"));

        parent.validate(); // Should pass
    }

    #[test]
    fn test_tieoff_modinst_input() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("in", IO::Input(1));

        let parent = ModDef::new("ParentMod");
        let inst = parent.instantiate(&leaf, Some("leaf_inst"), None);

        inst.get_port("in").tieoff(0);

        parent.validate(); // Should pass
    }

    #[test]
    fn test_tieoff_moddef_output() {
        let mod_def = ModDef::new("TestMod");
        let out_port = mod_def.add_port("out", IO::Output(1));

        out_port.tieoff(1);

        mod_def.validate(); // Should pass
    }

    #[test]
    #[should_panic(expected = "Cannot tie off TestMod.in")]
    fn test_invalid_tieoff_moddef_input() {
        let mod_def = ModDef::new("TestMod");
        let in_port = mod_def.add_port("in", IO::Input(1));

        in_port.tieoff(0);

        mod_def.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "Cannot tie off ParentMod.leaf_inst.out[0:0]")]
    fn test_invalid_tieoff_modinst_output() {
        let leaf = ModDef::new("LeafMod");
        leaf.set_usage(Usage::EmitStubAndStop);
        leaf.add_port("out", IO::Output(1));

        let parent = ModDef::new("ParentMod");
        let inst = parent.instantiate(&leaf, Some("leaf_inst"), None);

        inst.get_port("out").tieoff(0);

        parent.validate(); // Should panic
    }

    // Test 19: Multiple drivers due to overlapping tieoffs
    #[test]
    #[should_panic(expected = "TestMod.out[6:1] is multiply driven")]
    fn test_multiple_drivers_overlapping_tieoffs() {
        let mod_def = ModDef::new("TestMod");
        let out_port = mod_def.add_port("out", IO::Output(8));

        out_port.slice(7, 0).tieoff(0);
        out_port.slice(6, 1).tieoff(1);

        mod_def.validate(); // Should panic
    }

    #[test]
    #[should_panic(expected = "TestMod.out[6:1] is multiply driven")]
    fn test_multiple_drivers_overlapping_connections() {
        let mod_def = ModDef::new("TestMod");
        let out_port = mod_def.add_port("out", IO::Output(8));

        let bus_a = mod_def.add_port("bus_a", IO::Input(8));
        let bus_b = mod_def.add_port("bus_b", IO::Input(8));
        bus_b.slice(0, 0).unused();
        bus_b.slice(7, 7).unused();

        out_port.connect(&bus_a);
        out_port.slice(6, 1).connect(&bus_b.slice(6, 1));

        mod_def.validate(); // Should panic
    }

    #[test]
    fn test_unused_bits_marked_correctly() {
        let mod_def = ModDef::new("TestMod");
        let in_port = mod_def.add_port("in", IO::Input(8));
        let out_port = mod_def.add_port("out", IO::Output(8));

        out_port.slice(0, 0).connect(&in_port.slice(0, 0));
        out_port.slice(7, 7).connect(&in_port.slice(7, 7));
        out_port.slice(6, 1).tieoff(0);

        in_port.slice(6, 1).unused();

        mod_def.validate(); // Should pass
    }

    #[test]
    #[should_panic(expected = "TestMod.in is not fully used")]
    fn test_unused_bits_not_marked() {
        let mod_def = ModDef::new("TestMod");
        let in_port = mod_def.add_port("in", IO::Input(8));
        let out_port = mod_def.add_port("out", IO::Output(8));

        out_port.slice(0, 0).connect(&in_port.slice(0, 0));
        out_port.slice(7, 7).connect(&in_port.slice(7, 7));
        out_port.slice(6, 1).tieoff(0);

        mod_def.validate(); // Should panic
    }

    #[test]
    fn test_params() {
        let verilog = str2tmpfile(
            "\
module Orig #(
  parameter W = 8
) (
  output [W-1:0] data
);
endmodule
",
        )
        .unwrap();

        let base = ModDef::from_verilog_file("Orig", verilog.path(), true, false);

        let w16 = base.parameterize(&[("W", 16)], None, None);
        let w32 = base.parameterize(&[("W", 32)], None, None);

        let top = ModDef::new("Top");

        top.instantiate(&w16, Some("inst0"), None)
            .get_port("data")
            .unused();
        top.instantiate(&w16, Some("inst1"), None)
            .get_port("data")
            .unused();

        top.instantiate(&w32, Some("inst2"), None)
            .get_port("data")
            .unused();
        top.instantiate(&w32, Some("inst3"), None)
            .get_port("data")
            .unused();

        assert_eq!(
            top.emit(true),
            "\
module Orig_W_16(
  output wire [15:0] data
);
  Orig #(
    .W(32'h0000_0010)
  ) Orig_i (
    .data(data)
  );
endmodule

module Orig_W_32(
  output wire [31:0] data
);
  Orig #(
    .W(32'h0000_0020)
  ) Orig_i (
    .data(data)
  );
endmodule

module Top;
  wire [15:0] inst0_data;
  wire [15:0] inst1_data;
  wire [31:0] inst2_data;
  wire [31:0] inst3_data;
  Orig_W_16 inst0 (
    .data(inst0_data)
  );
  Orig_W_16 inst1 (
    .data(inst1_data)
  );
  Orig_W_32 inst2 (
    .data(inst2_data)
  );
  Orig_W_32 inst3 (
    .data(inst3_data)
  );
endmodule
"
        );
    }

    #[test]
    fn test_interface_tieoff_and_unused() {
        let module_a_verilog = "
    module ModuleA (
        input [31:0] a_data,
        input a_valid,
        output a_ready
    );
    endmodule
    ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        module_a.def_intf_from_prefix("a_intf", "a_");

        let top_module = ModDef::new("TopModule");
        top_module.add_port("top_data", IO::Output(32));
        top_module.add_port("top_valid", IO::Output(1));
        top_module.add_port("top_ready", IO::Input(1));
        let top_intf = top_module.def_intf_from_prefix("top_intf", "top_");

        let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);

        let a_intf = a_inst.get_intf("a_intf");

        a_intf.tieoff(0);
        a_intf.unused();

        top_intf.tieoff(0);
        top_intf.unused();

        assert_eq!(
            top_module.emit(true),
            "\
module TopModule(
  output wire [31:0] top_data,
  output wire top_valid,
  input wire top_ready
);
  wire [31:0] inst_a_a_data;
  wire inst_a_a_valid;
  wire inst_a_a_ready;
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid(inst_a_a_valid),
    .a_ready(inst_a_a_ready)
  );
  assign inst_a_a_data[31:0] = 32'h0000_0000;
  assign inst_a_a_valid = 1'h0;
  assign top_data[31:0] = 32'h0000_0000;
  assign top_valid = 1'h0;
endmodule
"
        );
    }

    #[test]
    fn test_instantiate_array() {
        let child_moddef = ModDef::new("child");
        let child_data_out = child_moddef.add_port("data_out", IO::Output(1));
        child_data_out.tieoff(0);

        let parent_moddef = ModDef::new("parent");
        let parent_data_out = parent_moddef.add_port("parent_data_out", IO::Output(6));

        let instances = parent_moddef.instantiate_array(&child_moddef, &[2, 3], None, None);

        // Connect the data_out port of each child instance to a bit in the
        // parent_data_out port
        for (idx, inst) in instances.iter().enumerate() {
            inst.get_port("data_out")
                .connect(&parent_data_out.slice(idx, idx));
        }

        let expected_verilog = "\
module child(
  output wire data_out
);
  assign data_out = 1'h0;
endmodule
module parent(
  output wire [5:0] parent_data_out
);
  wire child_i_0_0_data_out;
  wire child_i_0_1_data_out;
  wire child_i_0_2_data_out;
  wire child_i_1_0_data_out;
  wire child_i_1_1_data_out;
  wire child_i_1_2_data_out;
  child child_i_0_0 (
    .data_out(child_i_0_0_data_out)
  );
  child child_i_0_1 (
    .data_out(child_i_0_1_data_out)
  );
  child child_i_0_2 (
    .data_out(child_i_0_2_data_out)
  );
  child child_i_1_0 (
    .data_out(child_i_1_0_data_out)
  );
  child child_i_1_1 (
    .data_out(child_i_1_1_data_out)
  );
  child child_i_1_2 (
    .data_out(child_i_1_2_data_out)
  );
  assign parent_data_out[0:0] = child_i_0_0_data_out;
  assign parent_data_out[1:1] = child_i_0_1_data_out;
  assign parent_data_out[2:2] = child_i_0_2_data_out;
  assign parent_data_out[3:3] = child_i_1_0_data_out;
  assign parent_data_out[4:4] = child_i_1_1_data_out;
  assign parent_data_out[5:5] = child_i_1_2_data_out;
endmodule
    ";

        // Emit the Verilog code from the parent module
        let emitted_verilog = parent_moddef.emit(true);

        // Assert that the emitted Verilog matches the expected Verilog
        assert_eq!(emitted_verilog.trim(), expected_verilog.trim());
    }

    #[test]
    fn test_crossover() {
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

        a_intf.crossover(&b_intf, "tx", "rx");

        assert_eq!(
            top_module.emit(true),
            "\
module TopModule;
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
  assign inst_b_b_rx = inst_a_a_tx;
  assign inst_a_a_rx = inst_b_b_tx;
endmodule
"
        );
    }

    #[test]
    fn test_large_validation() {
        let a = ModDef::new("A");
        a.set_usage(Usage::EmitStubAndStop);

        let b = ModDef::new("B");
        b.set_usage(Usage::EmitStubAndStop);

        for i in 0..10000 {
            a.add_port(format!("a_{}", i), IO::Output(1000));
            b.add_port(format!("b_{}", i), IO::Input(1000));
        }

        let top = ModDef::new("Top");

        let a_inst = top.instantiate(&a, None, None);
        let b_inst = top.instantiate(&b, None, None);

        for i in 0..10000 {
            a_inst
                .get_port(format!("a_{}", i))
                .connect(&b_inst.get_port(format!("b_{}", i)));
        }

        let start = Instant::now();
        top.validate();
        let duration = start.elapsed();

        assert!(
            duration.as_secs() < 5,
            "Validation took too long: {:?}",
            duration
        );
    }

    #[test]
    fn test_subdivide() {
        let a = ModDef::new("A");
        a.add_port("out", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let b = ModDef::new("B");
        b.add_port("in", IO::Input(8));
        b.set_usage(Usage::EmitNothingAndStop);

        let top = ModDef::new("top");
        let a = top.instantiate(&a, None, None).get_port("out");
        let b = top.instantiate(&b, None, None).get_port("in");

        for (asub, bsub) in a.subdivide(2).iter().zip(b.subdivide(2)) {
            for (asubsub, bsubsub) in asub.subdivide(2).iter().zip(bsub.subdivide(2)) {
                asubsub.connect(&bsubsub);
            }
        }

        assert_eq!(
            top.emit(true),
            "\
module top;
  wire [7:0] A_i_out;
  wire [7:0] B_i_in;
  A A_i (
    .out(A_i_out)
  );
  B B_i (
    .in(B_i_in)
  );
  assign B_i_in[1:0] = A_i_out[1:0];
  assign B_i_in[3:2] = A_i_out[3:2];
  assign B_i_in[5:4] = A_i_out[5:4];
  assign B_i_in[7:6] = A_i_out[7:6];
endmodule
"
        );
    }

    #[test]
    fn test_structs() {
        let structs = "
      package my_pack;
        typedef struct packed {
          logic [1:0] a; // width: 2
          logic [2:0] b; // width: 3
        } my_struct_t;
      endpackage
      ";
        let module_a_verilog = "
      module A (
        output my_pack::my_struct_t [3:0] x // width: 20
      );
      endmodule";
        let module_b_verilog = "
      module B (
        input my_pack::my_struct_t [1:0][1:0][1:0] y // width: 40
      );
      endmodule";
        let a = ModDef::from_verilog(
            "A",
            format!("{structs}\n{module_a_verilog}"),
            false,
            false,
        );
        let b = ModDef::from_verilog(
            "B",
            format!("{structs}\n{module_b_verilog}"),
            false,
            false,
        );

        let top = ModDef::new("Top");
        let a0 = top.instantiate(&a, Some("a0"), None);
        let a1 = top.instantiate(&a, Some("a1"), None);
        let b0 = top.instantiate(&b, Some("b0"), None);

        let b0_inputs = b0.get_port("y").subdivide(2);

        a0.get_port("x").connect(&b0_inputs[0]);
        a1.get_port("x").connect(&b0_inputs[1]);

        assert_eq!(
            top.emit(true),
            "\
module Top;
  wire [19:0] a0_x;
  wire [19:0] a1_x;
  wire [39:0] b0_y;
  A a0 (
    .x(a0_x)
  );
  A a1 (
    .x(a1_x)
  );
  B b0 (
    .y(b0_y)
  );
  assign b0_y[19:0] = a0_x[19:0];
  assign b0_y[39:20] = a1_x[19:0];
endmodule
"
        );
    }

    #[test]
    fn test_intf_slice() {
        let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output [3:0] a_valid,
        input [1:0] a_ready
    );
    endmodule
    ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);

        let mut mapping_lower: IndexMap<String, (String, usize, usize)> = IndexMap::new();
        mapping_lower.insert("data".to_string(), ("a_data".to_string(), 15, 0));
        mapping_lower.insert("valid".to_string(), ("a_valid".to_string(), 1, 0));
        mapping_lower.insert("ready".to_string(), ("a_ready".to_string(), 0, 0));
        module_a.def_intf("lower", mapping_lower);

        let mut mapping_upper: IndexMap<String, (String, usize, usize)> = IndexMap::new();
        mapping_upper.insert("data".to_string(), ("a_data".to_string(), 31, 16));
        mapping_upper.insert("valid".to_string(), ("a_valid".to_string(), 3, 2));
        mapping_upper.insert("ready".to_string(), ("a_ready".to_string(), 1, 1));
        module_a.def_intf("upper", mapping_upper);

        let top_module = ModDef::new("TopModule");
        let a = top_module.instantiate(&module_a, None, None);
        a.get_intf("upper").export_with_prefix("upper", "upper_");
        a.get_intf("lower").export_with_prefix("lower", "lower_");

        assert_eq!(
            top_module.emit(true),
            "\
module TopModule(
  output wire [15:0] upper_data,
  output wire [1:0] upper_valid,
  input wire upper_ready,
  output wire [15:0] lower_data,
  output wire [1:0] lower_valid,
  input wire lower_ready
);
  wire [31:0] ModuleA_i_a_data;
  wire [3:0] ModuleA_i_a_valid;
  wire [1:0] ModuleA_i_a_ready;
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid),
    .a_ready(ModuleA_i_a_ready)
  );
  assign upper_data[15:0] = ModuleA_i_a_data[31:16];
  assign upper_valid[1:0] = ModuleA_i_a_valid[3:2];
  assign ModuleA_i_a_ready[1:1] = upper_ready;
  assign lower_data[15:0] = ModuleA_i_a_data[15:0];
  assign lower_valid[1:0] = ModuleA_i_a_valid[1:0];
  assign ModuleA_i_a_ready[0:0] = lower_ready;
endmodule
"
        );
    }

    #[test]
    fn test_intf_subdivide() {
        let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output [3:0] a_valid,
        input [1:0] a_ready
    );
    endmodule
    ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        let a_intf = module_a.def_intf_from_prefix("a_intf", "a_");
        a_intf.subdivide(2);

        let top_module = ModDef::new("TopModule");
        let a = top_module.instantiate(&module_a, None, None);
        a.get_intf("a_intf_0").export_with_prefix("lower", "lower_");
        a.get_intf("a_intf_1").export_with_prefix("upper", "upper_");

        assert_eq!(
            top_module.emit(true),
            "\
module TopModule(
  output wire [15:0] lower_data,
  output wire [1:0] lower_valid,
  input wire lower_ready,
  output wire [15:0] upper_data,
  output wire [1:0] upper_valid,
  input wire upper_ready
);
  wire [31:0] ModuleA_i_a_data;
  wire [3:0] ModuleA_i_a_valid;
  wire [1:0] ModuleA_i_a_ready;
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid),
    .a_ready(ModuleA_i_a_ready)
  );
  assign lower_data[15:0] = ModuleA_i_a_data[15:0];
  assign lower_valid[1:0] = ModuleA_i_a_valid[1:0];
  assign ModuleA_i_a_ready[0:0] = lower_ready;
  assign upper_data[15:0] = ModuleA_i_a_data[31:16];
  assign upper_valid[1:0] = ModuleA_i_a_valid[3:2];
  assign ModuleA_i_a_ready[1:1] = upper_ready;
endmodule
"
        );
    }

    #[test]
    fn test_complex_intf() {
        let module_a_verilog = "
    module ModuleA (
        output [7:0] a_data_out,
        output a_valid_out,
        input [7:0] a_data_in,
        input a_valid_in
    );
    endmodule
    ";
        let module_b_verilog = "
    module ModuleB (
        output [15:0] b_data_out,
        output [1:0] b_valid_out,
        input [15:0] b_data_in,
        input [1:0] b_valid_in
    );
    endmodule
    ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        module_a.def_intf_from_prefix("a_intf", "a_");

        let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
        let b_intf = module_b.def_intf_from_prefix("b_intf", "b_");
        b_intf.subdivide(2);

        let top_module = ModDef::new("TopModule");
        let a0 = top_module.instantiate(&module_a, Some("a0"), None);
        let a1 = top_module.instantiate(&module_a, Some("a1"), None);
        let b = top_module.instantiate(&module_b, None, None);
        b.get_intf("b_intf_0")
            .crossover(&a0.get_intf("a_intf"), "(.*)_in", "(.*)_out");
        b.get_intf("b_intf_1")
            .crossover(&a1.get_intf("a_intf"), "(.*)_in", "(.*)_out");

        assert_eq!(
            top_module.emit(true),
            "\
module TopModule;
  wire [7:0] a0_a_data_out;
  wire a0_a_valid_out;
  wire [7:0] a0_a_data_in;
  wire a0_a_valid_in;
  wire [7:0] a1_a_data_out;
  wire a1_a_valid_out;
  wire [7:0] a1_a_data_in;
  wire a1_a_valid_in;
  wire [15:0] ModuleB_i_b_data_out;
  wire [1:0] ModuleB_i_b_valid_out;
  wire [15:0] ModuleB_i_b_data_in;
  wire [1:0] ModuleB_i_b_valid_in;
  ModuleA a0 (
    .a_data_out(a0_a_data_out),
    .a_valid_out(a0_a_valid_out),
    .a_data_in(a0_a_data_in),
    .a_valid_in(a0_a_valid_in)
  );
  ModuleA a1 (
    .a_data_out(a1_a_data_out),
    .a_valid_out(a1_a_valid_out),
    .a_data_in(a1_a_data_in),
    .a_valid_in(a1_a_valid_in)
  );
  ModuleB ModuleB_i (
    .b_data_out(ModuleB_i_b_data_out),
    .b_valid_out(ModuleB_i_b_valid_out),
    .b_data_in(ModuleB_i_b_data_in),
    .b_valid_in(ModuleB_i_b_valid_in)
  );
  assign ModuleB_i_b_data_in[7:0] = a0_a_data_out[7:0];
  assign ModuleB_i_b_valid_in[0:0] = a0_a_valid_out;
  assign a0_a_data_in[7:0] = ModuleB_i_b_data_out[7:0];
  assign a0_a_valid_in = ModuleB_i_b_valid_out[0:0];
  assign ModuleB_i_b_data_in[15:8] = a1_a_data_out[7:0];
  assign ModuleB_i_b_valid_in[1:1] = a1_a_valid_out;
  assign a1_a_data_in[7:0] = ModuleB_i_b_data_out[15:8];
  assign a1_a_valid_in = ModuleB_i_b_valid_out[1:1];
endmodule
"
        );
    }

    #[test]
    fn test_intf_subdivide_export() {
        let module_a_verilog = "
      module ModuleA (
          output [15:0] a_data_tx,
          output [1:0] a_valid_tx,
          input [15:0] a_data_rx,
          input [1:0] a_valid_rx
      );
      endmodule
      ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        let a_intf = module_a.def_intf_from_prefix("a", "a_");
        a_intf.subdivide(2);

        let wrapper = ModDef::new("Wrapper");
        let a = wrapper.instantiate(&module_a, None, None);
        a.get_intf("a_0").export_with_prefix("lower", "lower_");
        a.get_intf("a_1").export_with_prefix("upper", "upper_");

        let top_module = ModDef::new("TopModule");
        let w0 = top_module.instantiate(&wrapper, Some("w0"), None);
        let w1 = top_module.instantiate(&wrapper, Some("w1"), None);

        w0.get_intf("lower")
            .crossover(&w1.get_intf("lower"), "(.*)_rx$", "(.*)_tx$");
        w0.get_intf("upper")
            .crossover(&w1.get_intf("upper"), "(.*)_rx$", "(.*)_tx$");

        assert_eq!(
            top_module.emit(true),
            "\
module Wrapper(
  output wire [7:0] lower_data_tx,
  output wire lower_valid_tx,
  input wire [7:0] lower_data_rx,
  input wire lower_valid_rx,
  output wire [7:0] upper_data_tx,
  output wire upper_valid_tx,
  input wire [7:0] upper_data_rx,
  input wire upper_valid_rx
);
  wire [15:0] ModuleA_i_a_data_tx;
  wire [1:0] ModuleA_i_a_valid_tx;
  wire [15:0] ModuleA_i_a_data_rx;
  wire [1:0] ModuleA_i_a_valid_rx;
  ModuleA ModuleA_i (
    .a_data_tx(ModuleA_i_a_data_tx),
    .a_valid_tx(ModuleA_i_a_valid_tx),
    .a_data_rx(ModuleA_i_a_data_rx),
    .a_valid_rx(ModuleA_i_a_valid_rx)
  );
  assign lower_data_tx[7:0] = ModuleA_i_a_data_tx[7:0];
  assign lower_valid_tx = ModuleA_i_a_valid_tx[0:0];
  assign ModuleA_i_a_data_rx[7:0] = lower_data_rx[7:0];
  assign ModuleA_i_a_valid_rx[0:0] = lower_valid_rx;
  assign upper_data_tx[7:0] = ModuleA_i_a_data_tx[15:8];
  assign upper_valid_tx = ModuleA_i_a_valid_tx[1:1];
  assign ModuleA_i_a_data_rx[15:8] = upper_data_rx[7:0];
  assign ModuleA_i_a_valid_rx[1:1] = upper_valid_rx;
endmodule
module TopModule;
  wire [7:0] w0_lower_data_tx;
  wire w0_lower_valid_tx;
  wire [7:0] w0_lower_data_rx;
  wire w0_lower_valid_rx;
  wire [7:0] w0_upper_data_tx;
  wire w0_upper_valid_tx;
  wire [7:0] w0_upper_data_rx;
  wire w0_upper_valid_rx;
  wire [7:0] w1_lower_data_tx;
  wire w1_lower_valid_tx;
  wire [7:0] w1_lower_data_rx;
  wire w1_lower_valid_rx;
  wire [7:0] w1_upper_data_tx;
  wire w1_upper_valid_tx;
  wire [7:0] w1_upper_data_rx;
  wire w1_upper_valid_rx;
  Wrapper w0 (
    .lower_data_tx(w0_lower_data_tx),
    .lower_valid_tx(w0_lower_valid_tx),
    .lower_data_rx(w0_lower_data_rx),
    .lower_valid_rx(w0_lower_valid_rx),
    .upper_data_tx(w0_upper_data_tx),
    .upper_valid_tx(w0_upper_valid_tx),
    .upper_data_rx(w0_upper_data_rx),
    .upper_valid_rx(w0_upper_valid_rx)
  );
  Wrapper w1 (
    .lower_data_tx(w1_lower_data_tx),
    .lower_valid_tx(w1_lower_valid_tx),
    .lower_data_rx(w1_lower_data_rx),
    .lower_valid_rx(w1_lower_valid_rx),
    .upper_data_tx(w1_upper_data_tx),
    .upper_valid_tx(w1_upper_valid_tx),
    .upper_data_rx(w1_upper_data_rx),
    .upper_valid_rx(w1_upper_valid_rx)
  );
  assign w0_lower_data_rx[7:0] = w1_lower_data_tx[7:0];
  assign w0_lower_valid_rx = w1_lower_valid_tx;
  assign w1_lower_data_rx[7:0] = w0_lower_data_tx[7:0];
  assign w1_lower_valid_rx = w0_lower_valid_tx;
  assign w0_upper_data_rx[7:0] = w1_upper_data_tx[7:0];
  assign w0_upper_valid_rx = w1_upper_valid_tx;
  assign w1_upper_data_rx[7:0] = w0_upper_data_tx[7:0];
  assign w1_upper_valid_rx = w0_upper_valid_tx;
endmodule
"
        );
    }
}
