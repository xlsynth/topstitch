// SPDX-License-Identifier: Apache-2.0

mod tests {

    use indexmap::IndexMap;
    use slang_rs::str2tmpfile;
    use slang_rs::SlangConfig;
    use std::time::Instant;
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
    fn test_tieoff_mod_inst() {
        // Define module A
        let a_mod_def = ModDef::new("A");
        a_mod_def.add_port("a0", IO::Input(8)).unused();
        a_mod_def.add_port("a1", IO::Input(8)).unused();
        a_mod_def.add_port("a2", IO::Input(8)).unused();
        let b_mod_def = ModDef::new("B");
        b_mod_def.add_port("b0", IO::Output(8)).tieoff(0x12);
        let a_inst = b_mod_def.instantiate(&a_mod_def, Some("a_inst"), None);
        a_inst.get_port("a0").tieoff(0x23);
        a_inst.get_port("a1").slice(3, 0).tieoff(0x3);
        a_inst.get_port("a1").slice(7, 4).tieoff(0x4);
        a_inst.get_port("a2").slice(7, 4).tieoff(0x5);
        a_inst.get_port("a2").slice(3, 0).export_as("b1");

        assert_eq!(
            b_mod_def.emit(true),
            "\
module A(
  input wire [7:0] a0,
  input wire [7:0] a1,
  input wire [7:0] a2
);

endmodule
module B(
  output wire [7:0] b0,
  input wire [3:0] b1
);
  wire [7:0] a_inst_a1;
  wire [7:0] a_inst_a2;
  A a_inst (
    .a0(8'h23),
    .a1(a_inst_a1),
    .a2(a_inst_a2)
  );
  assign a_inst_a2[3:0] = b1[3:0];
  assign b0[7:0] = 8'h12;
  assign a_inst_a1[3:0] = 4'h3;
  assign a_inst_a1[7:4] = 4'h4;
  assign a_inst_a2[7:4] = 4'h5;
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
  ChildModule child_inst (
    .clk(child_inst_clk),
    .rst(child_inst_rst),
    .data()
  );
  assign child_inst_clk = clk;
  assign child_inst_rst = rst;
endmodule
"
        );
    }

    #[test]
    #[should_panic(expected = "TestMod.out (ModDef Output) is undriven")]
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
    #[should_panic(expected = "ParentMod.leaf_inst.in (ModInst Input) is undriven")]
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
    #[should_panic(expected = "TestMod.in (ModDef Input) is unused")]
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
    #[should_panic(expected = "ParentMod.leaf_inst.out (ModInst Output) is unused")]
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
        bus_b.bit(0).unused();
        bus_b.bit(7).unused();

        out_port.connect(&bus_a);
        out_port.slice(6, 1).connect(&bus_b.slice(6, 1));

        mod_def.validate(); // Should panic
    }

    #[test]
    fn test_unused_bits_marked_correctly() {
        let mod_def = ModDef::new("TestMod");
        let in_port = mod_def.add_port("in", IO::Input(8));
        let out_port = mod_def.add_port("out", IO::Output(8));

        out_port.bit(0).connect(&in_port.bit(0));
        out_port.bit(7).connect(&in_port.bit(7));
        out_port.slice(6, 1).tieoff(0);

        in_port.slice(6, 1).unused();

        mod_def.validate(); // Should pass
    }

    #[test]
    #[should_panic(expected = "TestMod.in[6:1] (ModDef Input) is unused")]
    fn test_unused_bits_not_marked() {
        let mod_def = ModDef::new("TestMod");
        let in_port = mod_def.add_port("in", IO::Input(8));
        let out_port = mod_def.add_port("out", IO::Output(8));

        out_port.bit(0).connect(&in_port.bit(0));
        out_port.bit(7).connect(&in_port.bit(7));
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
  Orig_W_16 inst0 (
    .data()
  );
  Orig_W_16 inst1 (
    .data()
  );
  Orig_W_32 inst2 (
    .data()
  );
  Orig_W_32 inst3 (
    .data()
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
  ModuleA inst_a (
    .a_data(32'h0000_0000),
    .a_valid(1'h0),
    .a_ready()
  );
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
        let a = ModDef::from_verilog("A", format!("{structs}\n{module_a_verilog}"), false, false);
        let b = ModDef::from_verilog("B", format!("{structs}\n{module_b_verilog}"), false, false);

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
    fn test_unions() {
        let unions = "
      package my_pack;
        typedef union {
          logic [1:0] a; // width: 2
          logic [2:0] b; // width: 3
        } my_union_t;
      endpackage
      ";
        let module_a_verilog = "
      module A (
        output my_pack::my_union_t x // width: 3
      );
      endmodule";
        let module_b_verilog = "
      module B (
        input my_pack::my_union_t y // width: 3
      );
      endmodule";
        let a = ModDef::from_verilog("A", format!("{unions}\n{module_a_verilog}"), false, false);
        let b = ModDef::from_verilog("B", format!("{unions}\n{module_b_verilog}"), false, false);

        let top = ModDef::new("Top");
        let a0 = top.instantiate(&a, Some("a0"), None);
        let b0 = top.instantiate(&b, Some("b0"), None);

        a0.get_port("x").connect(&b0.get_port("y"));

        assert_eq!(
            top.emit(true),
            "\
module Top;
  wire [2:0] a0_x;
  wire [2:0] b0_y;
  A a0 (
    .x(a0_x)
  );
  B b0 (
    .y(b0_y)
  );
  assign b0_y[2:0] = a0_x[2:0];
endmodule
"
        );
    }

    #[test]
    fn test_unions_complex() {
        let unions = "
      package my_pack;
        typedef struct packed {
          logic [1:0] a; // width: 2
          logic [1:0] b; // width: 2
        } my_struct_t; // width: 4
        typedef union packed {
          my_struct_t [1:0] c; // width: 8
          logic [7:0] d; // width: 8
        } my_union_t; // width: 8
      endpackage
      ";
        let module_a_verilog = "
      module A (
        output my_pack::my_union_t [1:0] x // width: 16
      );
      endmodule";
        let module_b_verilog = "
      module B (
        input my_pack::my_union_t [1:0] y // width: 16
      );
      endmodule";
        let a = ModDef::from_verilog("A", format!("{unions}\n{module_a_verilog}"), false, false);
        let b = ModDef::from_verilog("B", format!("{unions}\n{module_b_verilog}"), false, false);

        let top = ModDef::new("Top");
        let a0 = top.instantiate(&a, Some("a0"), None);
        let b0 = top.instantiate(&b, Some("b0"), None);

        a0.get_port("x").connect(&b0.get_port("y"));

        assert_eq!(
            top.emit(true),
            "\
module Top;
  wire [15:0] a0_x;
  wire [15:0] b0_y;
  A a0 (
    .x(a0_x)
  );
  B b0 (
    .y(b0_y)
  );
  assign b0_y[15:0] = a0_x[15:0];
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

    #[test]
    fn test_intf_regex() {
        let module_a_verilog = "
      module ModuleA (
          input [7:0] a_data_in,
          input a_valid_in,
          output [7:0] a_data_out,
          output a_valid_out,
          input [7:0] b_data_in,
          input b_valid_in,
          output [7:0] b_data_out,
          output b_valid_out
      );
      endmodule
      ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        module_a.def_intf_from_regexes("left", &[("a_(.*)_in", "a_$1"), ("b_(.*)_out", "b_$1")]);
        module_a.def_intf_from_regexes("right", &[("a_(.*)_out", "a_$1"), ("b_(.*)_in", "b_$1")]);

        let top_module = ModDef::new("TopModule");
        let left = top_module.instantiate(&module_a, Some("left"), None);
        let right = top_module.instantiate(&module_a, Some("right"), None);

        left.get_intf("left").unused_and_tieoff(0);
        left.get_intf("right")
            .connect(&right.get_intf("left"), false);
        right.get_intf("right").unused_and_tieoff(0);

        assert_eq!(
            top_module.emit(true),
            "\
module TopModule;
  wire [7:0] left_a_data_out;
  wire left_a_valid_out;
  wire [7:0] left_b_data_in;
  wire left_b_valid_in;
  wire [7:0] right_a_data_in;
  wire right_a_valid_in;
  wire [7:0] right_b_data_out;
  wire right_b_valid_out;
  ModuleA left (
    .a_data_in(8'h00),
    .a_valid_in(1'h0),
    .a_data_out(left_a_data_out),
    .a_valid_out(left_a_valid_out),
    .b_data_in(left_b_data_in),
    .b_valid_in(left_b_valid_in),
    .b_data_out(),
    .b_valid_out()
  );
  ModuleA right (
    .a_data_in(right_a_data_in),
    .a_valid_in(right_a_valid_in),
    .a_data_out(),
    .a_valid_out(),
    .b_data_in(8'h00),
    .b_valid_in(1'h0),
    .b_data_out(right_b_data_out),
    .b_valid_out(right_b_valid_out)
  );
  assign right_a_data_in[7:0] = left_a_data_out[7:0];
  assign right_a_valid_in = left_a_valid_out;
  assign left_b_data_in[7:0] = right_b_data_out[7:0];
  assign left_b_valid_in = right_b_valid_out;
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
  assign ft_right_data_out[7:0] = ft_left_data_out[7:0];
  assign ft_right_valid_out = ft_left_valid_out;
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
    fn test_intf_crossover_through() {
        let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data_out,
          output a_valid_out,
          input a_ready_out
      );
      endmodule
      ";

        let module_e_verilog = "
      module ModuleE (
          input [7:0] e_data_in,
          input e_valid_in,
          output e_ready_in
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

        a_inst.get_intf("a").crossover_through(
            &e_inst.get_intf("e"),
            &[&b_inst, &c_inst, &d_inst],
            "(.*)_out",
            "(.*)_in",
            "ft_x",
            "ft_y",
        );

        assert_eq!(
            top_module.emit(true),
            "\
module ModuleB(
  input wire [7:0] ft_x_data_in,
  output wire [7:0] ft_y_data_out,
  input wire ft_x_valid_in,
  output wire ft_y_valid_out,
  output wire ft_x_ready_in,
  input wire ft_y_ready_out
);
  assign ft_y_data_out[7:0] = ft_x_data_in[7:0];
  assign ft_y_valid_out = ft_x_valid_in;
  assign ft_x_ready_in = ft_y_ready_out;
endmodule
module ModuleC(
  input wire [7:0] ft_x_data_in,
  output wire [7:0] ft_y_data_out,
  input wire ft_x_valid_in,
  output wire ft_y_valid_out,
  output wire ft_x_ready_in,
  input wire ft_y_ready_out
);
  assign ft_y_data_out[7:0] = ft_x_data_in[7:0];
  assign ft_y_valid_out = ft_x_valid_in;
  assign ft_x_ready_in = ft_y_ready_out;
endmodule
module ModuleD(
  input wire [7:0] ft_x_data_in,
  output wire [7:0] ft_y_data_out,
  input wire ft_x_valid_in,
  output wire ft_y_valid_out,
  output wire ft_x_ready_in,
  input wire ft_y_ready_out
);
  assign ft_y_data_out[7:0] = ft_x_data_in[7:0];
  assign ft_y_valid_out = ft_x_valid_in;
  assign ft_x_ready_in = ft_y_ready_out;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  wire ModuleA_i_a_ready_out;
  wire [7:0] ModuleB_i_ft_x_data_in;
  wire [7:0] ModuleB_i_ft_y_data_out;
  wire ModuleB_i_ft_x_valid_in;
  wire ModuleB_i_ft_y_valid_out;
  wire ModuleB_i_ft_x_ready_in;
  wire ModuleB_i_ft_y_ready_out;
  wire [7:0] ModuleC_i_ft_x_data_in;
  wire [7:0] ModuleC_i_ft_y_data_out;
  wire ModuleC_i_ft_x_valid_in;
  wire ModuleC_i_ft_y_valid_out;
  wire ModuleC_i_ft_x_ready_in;
  wire ModuleC_i_ft_y_ready_out;
  wire [7:0] ModuleD_i_ft_x_data_in;
  wire [7:0] ModuleD_i_ft_y_data_out;
  wire ModuleD_i_ft_x_valid_in;
  wire ModuleD_i_ft_y_valid_out;
  wire ModuleD_i_ft_x_ready_in;
  wire ModuleD_i_ft_y_ready_out;
  wire [7:0] ModuleE_i_e_data_in;
  wire ModuleE_i_e_valid_in;
  wire ModuleE_i_e_ready_in;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out),
    .a_ready_out(ModuleA_i_a_ready_out)
  );
  ModuleB ModuleB_i (
    .ft_x_data_in(ModuleB_i_ft_x_data_in),
    .ft_y_data_out(ModuleB_i_ft_y_data_out),
    .ft_x_valid_in(ModuleB_i_ft_x_valid_in),
    .ft_y_valid_out(ModuleB_i_ft_y_valid_out),
    .ft_x_ready_in(ModuleB_i_ft_x_ready_in),
    .ft_y_ready_out(ModuleB_i_ft_y_ready_out)
  );
  ModuleC ModuleC_i (
    .ft_x_data_in(ModuleC_i_ft_x_data_in),
    .ft_y_data_out(ModuleC_i_ft_y_data_out),
    .ft_x_valid_in(ModuleC_i_ft_x_valid_in),
    .ft_y_valid_out(ModuleC_i_ft_y_valid_out),
    .ft_x_ready_in(ModuleC_i_ft_x_ready_in),
    .ft_y_ready_out(ModuleC_i_ft_y_ready_out)
  );
  ModuleD ModuleD_i (
    .ft_x_data_in(ModuleD_i_ft_x_data_in),
    .ft_y_data_out(ModuleD_i_ft_y_data_out),
    .ft_x_valid_in(ModuleD_i_ft_x_valid_in),
    .ft_y_valid_out(ModuleD_i_ft_y_valid_out),
    .ft_x_ready_in(ModuleD_i_ft_x_ready_in),
    .ft_y_ready_out(ModuleD_i_ft_y_ready_out)
  );
  ModuleE ModuleE_i (
    .e_data_in(ModuleE_i_e_data_in),
    .e_valid_in(ModuleE_i_e_valid_in),
    .e_ready_in(ModuleE_i_e_ready_in)
  );
  assign ModuleB_i_ft_x_data_in[7:0] = ModuleA_i_a_data_out[7:0];
  assign ModuleC_i_ft_x_data_in[7:0] = ModuleB_i_ft_y_data_out[7:0];
  assign ModuleD_i_ft_x_data_in[7:0] = ModuleC_i_ft_y_data_out[7:0];
  assign ModuleE_i_e_data_in[7:0] = ModuleD_i_ft_y_data_out[7:0];
  assign ModuleB_i_ft_x_valid_in = ModuleA_i_a_valid_out;
  assign ModuleC_i_ft_x_valid_in = ModuleB_i_ft_y_valid_out;
  assign ModuleD_i_ft_x_valid_in = ModuleC_i_ft_y_valid_out;
  assign ModuleE_i_e_valid_in = ModuleD_i_ft_y_valid_out;
  assign ModuleA_i_a_ready_out = ModuleB_i_ft_x_ready_in;
  assign ModuleB_i_ft_y_ready_out = ModuleC_i_ft_x_ready_in;
  assign ModuleC_i_ft_y_ready_out = ModuleD_i_ft_x_ready_in;
  assign ModuleD_i_ft_y_ready_out = ModuleE_i_e_ready_in;
endmodule
"
        );
    }

    #[test]
    fn test_funnel() {
        let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data_out,
          output a_valid_out,
          input a_ready_in
      );
      endmodule
      ";

        let module_c_verilog = "
      module ModuleC (
          input [7:0] c_data_in,
          input c_valid_in,
          output c_ready_out
      );
      endmodule
      ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);

        let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);

        let module_b = ModDef::new("ModuleB");
        module_b.feedthrough("ft_left_i", "ft_right_o", 10);
        module_b.feedthrough("ft_right_i", "ft_left_o", 10);

        let top_module = ModDef::new("TopModule");
        let a_inst = top_module.instantiate(&module_a, None, None);
        let b_inst = top_module.instantiate(&module_b, None, None);
        let c_inst = top_module.instantiate(&module_c, None, None);

        let mut funnel = Funnel::new(
            (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
            (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
        );

        funnel.connect(
            &a_inst.get_port("a_data_out"),
            &c_inst.get_port("c_data_in"),
        );
        funnel.connect(
            &a_inst.get_port("a_valid_out"),
            &c_inst.get_port("c_valid_in"),
        );
        funnel.connect(
            &a_inst.get_port("a_ready_in"),
            &c_inst.get_port("c_ready_out"),
        );
        funnel.done();

        assert_eq!(
            top_module.emit(true),
            "\
module ModuleB(
  input wire [9:0] ft_left_i,
  output wire [9:0] ft_right_o,
  input wire [9:0] ft_right_i,
  output wire [9:0] ft_left_o
);
  assign ft_right_o[9:0] = ft_left_i[9:0];
  assign ft_left_o[9:0] = ft_right_i[9:0];
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  wire ModuleA_i_a_ready_in;
  wire [9:0] ModuleB_i_ft_left_i;
  wire [9:0] ModuleB_i_ft_right_o;
  wire [9:0] ModuleB_i_ft_right_i;
  wire [9:0] ModuleB_i_ft_left_o;
  wire [7:0] ModuleC_i_c_data_in;
  wire ModuleC_i_c_valid_in;
  wire ModuleC_i_c_ready_out;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out),
    .a_ready_in(ModuleA_i_a_ready_in)
  );
  ModuleB ModuleB_i (
    .ft_left_i(ModuleB_i_ft_left_i),
    .ft_right_o(ModuleB_i_ft_right_o),
    .ft_right_i(ModuleB_i_ft_right_i),
    .ft_left_o(ModuleB_i_ft_left_o)
  );
  ModuleC ModuleC_i (
    .c_data_in(ModuleC_i_c_data_in),
    .c_valid_in(ModuleC_i_c_valid_in),
    .c_ready_out(ModuleC_i_c_ready_out)
  );
  assign ModuleB_i_ft_left_i[7:0] = ModuleA_i_a_data_out[7:0];
  assign ModuleC_i_c_data_in[7:0] = ModuleB_i_ft_right_o[7:0];
  assign ModuleB_i_ft_left_i[8:8] = ModuleA_i_a_valid_out;
  assign ModuleC_i_c_valid_in = ModuleB_i_ft_right_o[8:8];
  assign ModuleA_i_a_ready_in = ModuleB_i_ft_left_o[0:0];
  assign ModuleB_i_ft_right_i[0:0] = ModuleC_i_c_ready_out;
  assign ModuleB_i_ft_left_i[9:9] = 1'h0;
  assign ModuleB_i_ft_right_i[9:1] = 9'h000;
endmodule
"
        );
    }

    #[test]
    #[should_panic(expected = "Funnel error: out of capacity")]
    fn test_funnel_capacity() {
        let module_a_verilog = "
      module ModuleA (
          output a_out_0,
          output a_out_1,
          input a_in_0,
          input a_in_1
      );
      endmodule
      ";

        let module_c_verilog = "
      module ModuleC (
          input c_in_0,
          input c_in_1,
          output c_out_0,
          output c_out_1
      );
      endmodule
      ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);

        let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);

        let module_b = ModDef::new("ModuleB");
        module_b.feedthrough("ft_left_i", "ft_right_o", 1);
        module_b.feedthrough("ft_right_i", "ft_left_o", 1);

        let top_module = ModDef::new("TopModule");
        let a_inst = top_module.instantiate(&module_a, None, None);
        let b_inst = top_module.instantiate(&module_b, None, None);
        let c_inst = top_module.instantiate(&module_c, None, None);

        let mut funnel = Funnel::new(
            (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
            (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
        );

        funnel.connect(&a_inst.get_port("a_in_0"), &c_inst.get_port("c_out_0"));
        funnel.connect(&a_inst.get_port("a_in_1"), &c_inst.get_port("c_out_1"));
    }

    #[test]
    fn test_funnel_connect_intf() {
        let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data,
          output a_valid,
          input a_ready
      );
      endmodule
      ";

        let module_c_verilog = "
      module ModuleC (
          input [7:0] c_data,
          input c_valid,
          output c_ready
      );
      endmodule
      ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        module_a.def_intf_from_name_underscore("a");

        let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);
        module_c.def_intf_from_name_underscore("c");

        let module_b = ModDef::new("ModuleB");
        module_b.feedthrough("ft_left_i", "ft_right_o", 10);
        module_b.feedthrough("ft_right_i", "ft_left_o", 10);

        let top_module = ModDef::new("TopModule");
        let a_inst = top_module.instantiate(&module_a, None, None);
        let b_inst = top_module.instantiate(&module_b, None, None);
        let c_inst = top_module.instantiate(&module_c, None, None);

        let mut funnel = Funnel::new(
            (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
            (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
        );

        funnel.connect_intf(&a_inst.get_intf("a"), &c_inst.get_intf("c"), false);
        funnel.done();

        assert_eq!(
            top_module.emit(true),
            "\
module ModuleB(
  input wire [9:0] ft_left_i,
  output wire [9:0] ft_right_o,
  input wire [9:0] ft_right_i,
  output wire [9:0] ft_left_o
);
  assign ft_right_o[9:0] = ft_left_i[9:0];
  assign ft_left_o[9:0] = ft_right_i[9:0];
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data;
  wire ModuleA_i_a_valid;
  wire ModuleA_i_a_ready;
  wire [9:0] ModuleB_i_ft_left_i;
  wire [9:0] ModuleB_i_ft_right_o;
  wire [9:0] ModuleB_i_ft_right_i;
  wire [9:0] ModuleB_i_ft_left_o;
  wire [7:0] ModuleC_i_c_data;
  wire ModuleC_i_c_valid;
  wire ModuleC_i_c_ready;
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid),
    .a_ready(ModuleA_i_a_ready)
  );
  ModuleB ModuleB_i (
    .ft_left_i(ModuleB_i_ft_left_i),
    .ft_right_o(ModuleB_i_ft_right_o),
    .ft_right_i(ModuleB_i_ft_right_i),
    .ft_left_o(ModuleB_i_ft_left_o)
  );
  ModuleC ModuleC_i (
    .c_data(ModuleC_i_c_data),
    .c_valid(ModuleC_i_c_valid),
    .c_ready(ModuleC_i_c_ready)
  );
  assign ModuleB_i_ft_left_i[7:0] = ModuleA_i_a_data[7:0];
  assign ModuleC_i_c_data[7:0] = ModuleB_i_ft_right_o[7:0];
  assign ModuleB_i_ft_left_i[8:8] = ModuleA_i_a_valid;
  assign ModuleC_i_c_valid = ModuleB_i_ft_right_o[8:8];
  assign ModuleA_i_a_ready = ModuleB_i_ft_left_o[0:0];
  assign ModuleB_i_ft_right_i[0:0] = ModuleC_i_c_ready;
  assign ModuleB_i_ft_left_i[9:9] = 1'h0;
  assign ModuleB_i_ft_right_i[9:1] = 9'h000;
endmodule
"
        );
    }

    #[test]
    fn test_funnel_crossover_intf() {
        let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data_out,
          output a_valid_out,
          input a_ready_in
      );
      endmodule
      ";

        let module_c_verilog = "
      module ModuleC (
          input [7:0] c_data_in,
          input c_valid_in,
          output c_ready_out
      );
      endmodule
      ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        module_a.def_intf_from_name_underscore("a");

        let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);
        module_c.def_intf_from_name_underscore("c");

        let module_b = ModDef::new("ModuleB");
        module_b.feedthrough("ft_left_i", "ft_right_o", 10);
        module_b.feedthrough("ft_right_i", "ft_left_o", 10);

        let top_module = ModDef::new("TopModule");
        let a_inst = top_module.instantiate(&module_a, None, None);
        let b_inst = top_module.instantiate(&module_b, None, None);
        let c_inst = top_module.instantiate(&module_c, None, None);

        let mut funnel = Funnel::new(
            (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
            (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
        );

        funnel.crossover_intf(
            &a_inst.get_intf("a"),
            &c_inst.get_intf("c"),
            "(.*)_out",
            "(.*)_in",
        );
        funnel.done();

        assert_eq!(
            top_module.emit(true),
            "\
module ModuleB(
  input wire [9:0] ft_left_i,
  output wire [9:0] ft_right_o,
  input wire [9:0] ft_right_i,
  output wire [9:0] ft_left_o
);
  assign ft_right_o[9:0] = ft_left_i[9:0];
  assign ft_left_o[9:0] = ft_right_i[9:0];
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  wire ModuleA_i_a_ready_in;
  wire [9:0] ModuleB_i_ft_left_i;
  wire [9:0] ModuleB_i_ft_right_o;
  wire [9:0] ModuleB_i_ft_right_i;
  wire [9:0] ModuleB_i_ft_left_o;
  wire [7:0] ModuleC_i_c_data_in;
  wire ModuleC_i_c_valid_in;
  wire ModuleC_i_c_ready_out;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out),
    .a_ready_in(ModuleA_i_a_ready_in)
  );
  ModuleB ModuleB_i (
    .ft_left_i(ModuleB_i_ft_left_i),
    .ft_right_o(ModuleB_i_ft_right_o),
    .ft_right_i(ModuleB_i_ft_right_i),
    .ft_left_o(ModuleB_i_ft_left_o)
  );
  ModuleC ModuleC_i (
    .c_data_in(ModuleC_i_c_data_in),
    .c_valid_in(ModuleC_i_c_valid_in),
    .c_ready_out(ModuleC_i_c_ready_out)
  );
  assign ModuleB_i_ft_left_i[7:0] = ModuleA_i_a_data_out[7:0];
  assign ModuleC_i_c_data_in[7:0] = ModuleB_i_ft_right_o[7:0];
  assign ModuleB_i_ft_left_i[8:8] = ModuleA_i_a_valid_out;
  assign ModuleC_i_c_valid_in = ModuleB_i_ft_right_o[8:8];
  assign ModuleA_i_a_ready_in = ModuleB_i_ft_left_o[0:0];
  assign ModuleB_i_ft_right_i[0:0] = ModuleC_i_c_ready_out;
  assign ModuleB_i_ft_left_i[9:9] = 1'h0;
  assign ModuleB_i_ft_right_i[9:1] = 9'h000;
endmodule
"
        );
    }

    #[test]
    fn test_flip_and_copy() {
        let module_a_verilog = "
      module ModuleA (
          output [7:0] bus_data_out,
          output bus_valid_out,
          input bus_ready_in
      );
      endmodule
      ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        let a_intf = module_a.def_intf_from_name_underscore("bus");

        let module_b = ModDef::new("ModuleB");
        let b_intf = a_intf.copy_to(&module_b);
        b_intf.unused_and_tieoff(0);

        let top_module = ModDef::new("TopModule");
        let a_inst = top_module.instantiate(&module_a, None, None);
        a_inst.get_intf("bus").unused_and_tieoff(0);

        let b_inst = top_module.instantiate(&module_b, None, None);
        b_inst.get_intf("bus").unused_and_tieoff(0);

        let module_c = ModDef::new("ModuleC");
        let c_intf = a_inst.get_intf("bus").flip_to(&module_c);
        c_intf.unused_and_tieoff(0);
        let c_inst = top_module.instantiate(&module_c, None, None);
        c_inst.get_intf("bus").unused_and_tieoff(0);

        assert_eq!(
            top_module.emit(true),
            "\
module ModuleB(
  output wire [7:0] bus_data_out,
  output wire bus_valid_out,
  input wire bus_ready_in
);
  assign bus_data_out[7:0] = 8'h00;
  assign bus_valid_out = 1'h0;
endmodule
module ModuleC(
  input wire [7:0] bus_data_out,
  input wire bus_valid_out,
  output wire bus_ready_in
);
  assign bus_ready_in = 1'h0;
endmodule
module TopModule;
  ModuleA ModuleA_i (
    .bus_data_out(),
    .bus_valid_out(),
    .bus_ready_in(1'h0)
  );
  ModuleB ModuleB_i (
    .bus_data_out(),
    .bus_valid_out(),
    .bus_ready_in(1'h0)
  );
  ModuleC ModuleC_i (
    .bus_data_out(8'h00),
    .bus_valid_out(1'h0),
    .bus_ready_in()
  );
endmodule
"
        );
    }

    #[test]
    fn test_stub_recursive() {
        let a_def = ModDef::new("a");
        let b_def = ModDef::new("b");
        let c_def = ModDef::new("skip_c");
        let d_def = ModDef::new("skip_d");
        let e_def = ModDef::new("e");
        let f_def = ModDef::new("f");
        let g_def = ModDef::new("g");

        a_def.instantiate(&b_def, None, None);
        a_def.instantiate(&c_def, None, None);
        b_def.instantiate(&d_def, None, None);
        c_def.instantiate(&e_def, None, None);
        d_def.instantiate(&f_def, None, None);
        e_def.instantiate(&g_def, None, None);

        a_def.stub_recursive("^skip_(.*)$");

        assert_eq!(
            a_def.emit(true),
            "\
module skip_d;

endmodule
module b;
  skip_d skip_d_i (
    
  );
endmodule
module skip_c;

endmodule
module a;
  b b_i (
    
  );
  skip_c skip_c_i (
    
  );
endmodule
"
        );
    }

    #[test]
    fn test_inout_rename() {
        let module_a_verilog = "
      module ModuleA (
          inout [15:0] a,
          inout [7:0] b,
          inout [7:0] c,
          inout [7:0] d,
          inout [15:0] e,
          inout [15:0] f,
          inout [15:0] g
      );
      endmodule
      ";
        let a_def = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        let top = ModDef::new("Top");
        top.add_port("a", IO::InOut(8));
        top.add_port("b", IO::InOut(8));
        top.add_port("c", IO::InOut(16));
        let a_inst = top.instantiate(&a_def, None, None);
        a_inst.get_port("a").slice(7, 0).connect(&top.get_port("a"));
        a_inst
            .get_port("a")
            .slice(15, 8)
            .connect(&top.get_port("b"));
        a_inst.get_port("b").connect(&top.get_port("c").slice(7, 0));
        a_inst
            .get_port("c")
            .connect(&top.get_port("c").slice(15, 8));
        a_inst.get_port("d").export();
        a_inst.get_port("e").slice(15, 8).export_as("e");
        a_inst.get_port("f").slice(7, 0).export_as("f");
        a_inst.get_port("g").slice(11, 8).export_as("g");

        a_inst.get_port("e").slice(7, 0).unused();
        a_inst.get_port("f").slice(15, 8).unused();
        a_inst.get_port("g").slice(15, 12).unused();
        a_inst.get_port("g").slice(7, 0).unused();

        assert_eq!(
            top.emit(true),
            "\
module Top(
  inout wire [7:0] a,
  inout wire [7:0] b,
  inout wire [15:0] c,
  inout wire [7:0] d,
  inout wire [7:0] e,
  inout wire [7:0] f,
  inout wire [3:0] g
);
  wire [7:0] UNUSED_ModuleA_i_e_7_0;
  wire [7:0] UNUSED_ModuleA_i_f_15_8;
  wire [3:0] UNUSED_ModuleA_i_g_15_12;
  wire [7:0] UNUSED_ModuleA_i_g_7_0;
  ModuleA ModuleA_i (
    .a({b[7:0], a[7:0]}),
    .b(c[7:0]),
    .c(c[15:8]),
    .d(d[7:0]),
    .e({e[7:0], UNUSED_ModuleA_i_e_7_0}),
    .f({UNUSED_ModuleA_i_f_15_8, f[7:0]}),
    .g({UNUSED_ModuleA_i_g_15_12, g[3:0], UNUSED_ModuleA_i_g_7_0})
  );
endmodule
"
        );
    }

    #[test]
    fn test_parameterize_with_header() {
        let header = str2tmpfile("`define MY_PARAM_A 12").unwrap();
        let header_name = header.path().file_name().unwrap().to_str().unwrap();

        let source = str2tmpfile(&format!(
            "
      `include \"{header_name}\"
      module MyModule #(
          parameter MY_PARAM_B = 23
      ) (
          input [`MY_PARAM_A-1:0] a,
          output [MY_PARAM_B-1:0] b
      );
      endmodule
      "
        ))
        .unwrap();

        let cfg = SlangConfig {
            sources: &[source.path().to_str().unwrap()],
            incdirs: &[header.path().parent().unwrap().to_str().unwrap()],
            parameters: &[],
            ..Default::default()
        };
        let orig = ModDef::from_verilog_using_slang("MyModule", &cfg, false);
        let modified = orig.parameterize(&[("MY_PARAM_B", 34)], Some("MyModifiedModule"), None);

        assert_eq!(orig.get_port("a").io().width(), 12);
        assert_eq!(orig.get_port("b").io().width(), 23);

        assert_eq!(modified.get_port("a").io().width(), 12);
        assert_eq!(modified.get_port("b").io().width(), 34);
    }

    #[test]
    fn test_enum_type_remap() {
        let input_verilog = "
        package color_pkg;
            typedef enum bit[1:0] {RED, GREEN, BLUE} rgb_t;
        endpackage
        module ModA import color_pkg::*; (
            input rgb_t portA,
            output rgb_t portB,
            input rgb_t [3:0] portC,
            output rgb_t [3:0] portD
        );
        endmodule
        ";

        let mod_a = ModDef::from_verilog("ModA", input_verilog, true, false);
        let wrapped = mod_a.wrap(None, None);

        assert_eq!(
            wrapped.emit(true),
            "\
module ModA_wrapper(
  input wire [1:0] portA,
  output wire [1:0] portB,
  input wire [7:0] portC,
  output wire [7:0] portD
);
  wire [1:0] ModA_i_portA;
  wire [1:0] ModA_i_portB;
  wire [7:0] ModA_i_portC;
  wire [7:0] ModA_i_portD;
  ModA ModA_i (
    .portA(color_pkg::rgb_t'(ModA_i_portA)),
    .portB(ModA_i_portB),
    .portC(ModA_i_portC),
    .portD(ModA_i_portD)
  );
  assign ModA_i_portA[1:0] = portA[1:0];
  assign portB[1:0] = ModA_i_portB[1:0];
  assign ModA_i_portC[7:0] = portC[7:0];
  assign portD[7:0] = ModA_i_portD[7:0];
endmodule
"
        );
    }

    #[test]
    fn test_enum_type_remap_parameterized() {
        let input_verilog = str2tmpfile(
            "
        package color_pkg;
            typedef enum bit[1:0] {RED, GREEN, BLUE} rgb_t;
        endpackage
        module ModA import color_pkg::*; #(
            parameter MY_PARAM = 8
        ) (
            input rgb_t portA,
            output rgb_t portB,
            input rgb_t [3:0] portC,
            output rgb_t [3:0] portD,
            input [MY_PARAM-1:0] portE
        );
        endmodule
        ",
        )
        .unwrap();

        let mod_a = ModDef::from_verilog_file("ModA", input_verilog.path(), true, false);
        let mod_a_parameterized = mod_a.parameterize(&[("MY_PARAM", 16)], None, None);
        let wrapped = mod_a_parameterized.wrap(None, None);

        assert_eq!(
            wrapped.emit(true),
            "\
module ModA_MY_PARAM_16(
  input wire [1:0] portA,
  output wire [1:0] portB,
  input wire [7:0] portC,
  output wire [7:0] portD,
  input wire [15:0] portE
);
  ModA #(
    .MY_PARAM(32'h0000_0010)
  ) ModA_i (
    .portA(color_pkg::rgb_t'(portA)),
    .portB(portB),
    .portC(portC),
    .portD(portD),
    .portE(portE)
  );
endmodule

module ModA_MY_PARAM_16_wrapper(
  input wire [1:0] portA,
  output wire [1:0] portB,
  input wire [7:0] portC,
  output wire [7:0] portD,
  input wire [15:0] portE
);
  wire [1:0] ModA_MY_PARAM_16_i_portA;
  wire [1:0] ModA_MY_PARAM_16_i_portB;
  wire [7:0] ModA_MY_PARAM_16_i_portC;
  wire [7:0] ModA_MY_PARAM_16_i_portD;
  wire [15:0] ModA_MY_PARAM_16_i_portE;
  ModA_MY_PARAM_16 ModA_MY_PARAM_16_i (
    .portA(ModA_MY_PARAM_16_i_portA),
    .portB(ModA_MY_PARAM_16_i_portB),
    .portC(ModA_MY_PARAM_16_i_portC),
    .portD(ModA_MY_PARAM_16_i_portD),
    .portE(ModA_MY_PARAM_16_i_portE)
  );
  assign ModA_MY_PARAM_16_i_portA[1:0] = portA[1:0];
  assign portB[1:0] = ModA_MY_PARAM_16_i_portB[1:0];
  assign ModA_MY_PARAM_16_i_portC[7:0] = portC[7:0];
  assign portD[7:0] = ModA_MY_PARAM_16_i_portD[7:0];
  assign ModA_MY_PARAM_16_i_portE[15:0] = portE[15:0];
endmodule
"
        );
    }

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
            },
        );

        a_inst.get_port("in").connect_pipeline(
            &b_inst.get_port("out"),
            PipelineConfig {
                clk: "clk_new".to_string(),
                depth: 0xff,
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
  ) pipeline_conn_1 (
    .clk(clk_existing),
    .in(a_i_out[170:0]),
    .out(b_i_in[170:0]),
    .out_stages()
  );
  br_delay_nr #(
    .Width(32'h0000_00ef),
    .NumStages(32'h0000_00ff)
  ) pipeline_conn_3 (
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

    #[test]
    fn test_inout_modinst() {
        let a_verilog = "\
module A(
  inout a0,
  inout a1,
  inout b,
  inout [1:0] c,
  inout d,
  input e
);
endmodule";
        let b_verilog = "\
module B(
  inout [1:0] a,
  inout b,
  inout [1:0] c,
  output d,
  inout e
);
endmodule";
        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::from_verilog("B", b_verilog, true, false);

        // Define module C
        let c_mod_def: ModDef = ModDef::new("C");

        // Instantiate A and B in C
        let a_inst = c_mod_def.instantiate(&a_mod_def, Some("inst_a"), None);
        let b_inst = c_mod_def.instantiate(&b_mod_def, Some("inst_b"), None);

        b_inst.get_port("a").bit(0).connect(&a_inst.get_port("a0"));
        a_inst.get_port("a1").connect(&b_inst.get_port("a").bit(1));
        a_inst.get_port("b").connect(&b_inst.get_port("b"));
        b_inst.get_port("c").connect(&a_inst.get_port("c"));
        b_inst.get_port("d").connect(&a_inst.get_port("d"));
        a_inst.get_port("e").connect(&b_inst.get_port("e"));

        assert_eq!(
            c_mod_def.emit(true),
            "\
module C;
  wire inst_b_a_0_0_inst_a_a0_0_0;
  wire inst_a_a1_0_0_inst_b_a_1_1;
  wire inst_a_b_0_0_inst_b_b_0_0;
  wire [1:0] inst_b_c_1_0_inst_a_c_1_0;
  wire inst_b_d_0_0_inst_a_d_0_0;
  wire inst_a_e_0_0_inst_b_e_0_0;
  A inst_a (
    .a0(inst_b_a_0_0_inst_a_a0_0_0),
    .a1(inst_a_a1_0_0_inst_b_a_1_1),
    .b(inst_a_b_0_0_inst_b_b_0_0),
    .c(inst_b_c_1_0_inst_a_c_1_0),
    .d(inst_b_d_0_0_inst_a_d_0_0),
    .e(inst_a_e_0_0_inst_b_e_0_0)
  );
  B inst_b (
    .a({inst_a_a1_0_0_inst_b_a_1_1, inst_b_a_0_0_inst_a_a0_0_0}),
    .b(inst_a_b_0_0_inst_b_b_0_0),
    .c(inst_b_c_1_0_inst_a_c_1_0),
    .d(inst_b_d_0_0_inst_a_d_0_0),
    .e(inst_a_e_0_0_inst_b_e_0_0)
  );
endmodule
"
        );
    }

    #[test]
    #[should_panic(expected = "B.inst_a.a (ModInst InOut) is unused")]
    fn test_inout_unused_0() {
        let a_verilog = "\
module A(
  inout a
);
endmodule";

        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def: ModDef = ModDef::new("B");

        b_mod_def.instantiate(&a_mod_def, Some("inst_a"), None);

        b_mod_def.validate();
    }

    #[test]
    #[should_panic(expected = "B.inst_a.a[1] (ModInst InOut) is unused")]
    fn test_inout_unused_1() {
        let a_verilog = "\
module A(
  inout [1:0] a
);
endmodule";

        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);

        let b_mod_def: ModDef = ModDef::new("B");
        b_mod_def.add_port("b", IO::InOut(1));

        let a_inst = b_mod_def.instantiate(&a_mod_def, Some("inst_a"), None);
        a_inst
            .get_port("a")
            .bit(0)
            .connect(&b_mod_def.get_port("b"));

        b_mod_def.validate();
    }

    #[test]
    #[should_panic(expected = "A.a (ModDef InOut) is unused")]
    fn test_inout_unused_2() {
        let a_mod_def: ModDef = ModDef::new("A");
        a_mod_def.add_port("a", IO::InOut(1));
        a_mod_def.validate();
    }

    #[test]
    #[should_panic(expected = "B.b[1] (ModDef InOut) is unused")]
    fn test_inout_unused_3() {
        let a_verilog = "\
module A(
  inout a
);
endmodule";

        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);

        let b_mod_def: ModDef = ModDef::new("B");
        b_mod_def.add_port("b", IO::InOut(2));

        let a_inst = b_mod_def.instantiate(&a_mod_def, Some("inst_a"), None);
        a_inst
            .get_port("a")
            .bit(0)
            .connect(&b_mod_def.get_port("b").bit(0));

        b_mod_def.validate();
    }

    #[test]
    fn test_multiple_modules_1() {
        let source = str2tmpfile(
            "\
module A;
endmodule
module B;
A A_i();
C C_i();
endmodule
module C;
endmodule
      ",
        )
        .unwrap();

        let cfg = SlangConfig {
            sources: &[source.path().to_str().unwrap()],
            ..Default::default()
        };
        let results = ModDef::all_from_verilog_using_slang(&cfg, false);

        let module_names: Vec<String> = results.iter().map(|mod_def| mod_def.get_name()).collect();
        let mut sorted_module_names = module_names.clone();
        sorted_module_names.sort();
        assert_eq!(sorted_module_names, vec!["B"]);
    }

    #[test]
    fn test_multiple_modules_2() {
        let source = str2tmpfile(
            "\
module A;
endmodule
module B;
endmodule
module C;
endmodule
  ",
        )
        .unwrap();

        let cfg = SlangConfig {
            sources: &[source.path().to_str().unwrap()],
            ..Default::default()
        };
        let results = ModDef::all_from_verilog_using_slang(&cfg, false);

        let module_names: Vec<String> = results.iter().map(|mod_def| mod_def.get_name()).collect();
        let mut sorted_module_names = module_names.clone();
        sorted_module_names.sort();
        assert_eq!(sorted_module_names, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_protected() {
        let module_a_verilog = "
      module ModuleA (
          input a
      );
      `protected
      asdf
      `endprotected
      endmodule
      ";

        let a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        let top = ModDef::new("TopModule");
        let a_inst = top.instantiate(&a, None, None);
        a_inst.get_port("a").tieoff(0);

        assert_eq!(
            top.emit(false),
            "\
module TopModule;
  ModuleA ModuleA_i (
    .a(1'h0)
  );
endmodule
"
        );
    }

    #[test]
    fn test_connect_to_net() {
        let a_verilog = "\
module A(
  output [7:0] ao
);
endmodule";
        let b_verilog = "\
module B(
  input [7:0] bi
);
endmodule";
        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::from_verilog("B", b_verilog, true, false);
        let top = ModDef::new("TopModule");
        let a_inst = top.instantiate(&a_mod_def, None, None);
        let b_inst = top.instantiate(&b_mod_def, None, None);
        a_inst.get_port("ao").connect_to_net("custom");
        b_inst.get_port("bi").connect_to_net("custom");
        assert_eq!(
            top.emit(true),
            "\
module TopModule;
  wire [7:0] custom;
  A A_i (
    .ao(custom)
  );
  B B_i (
    .bi(custom)
  );
endmodule
"
        );
    }

    #[test]
    fn test_connect_to_net_multiple_receivers() {
        let a_verilog = "\
module A(
  output [7:0] ao
);
endmodule";
        let b_verilog = "\
module B(
  input [7:0] bi
);
endmodule";
        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::from_verilog("B", b_verilog, true, false);
        let top = ModDef::new("TopModule");
        let a_inst = top.instantiate(&a_mod_def, None, None);
        let b_inst_0 = top.instantiate(&b_mod_def, Some("B_i_0"), None);
        let b_inst_1 = top.instantiate(&b_mod_def, Some("B_i_1"), None);
        a_inst.get_port("ao").connect_to_net("custom");
        b_inst_0.get_port("bi").connect_to_net("custom");
        b_inst_1.get_port("bi").connect_to_net("custom");
        assert_eq!(
            top.emit(true),
            "\
module TopModule;
  wire [7:0] custom;
  A A_i (
    .ao(custom)
  );
  B B_i_0 (
    .bi(custom)
  );
  B B_i_1 (
    .bi(custom)
  );
endmodule
"
        );
    }

    #[test]
    fn test_connect_to_net_with_slice() {
        let a_verilog = "\
module A(
  output [7:0] a
);
endmodule";
        let b_verilog = "\
module B(
  input [3:0] b0,
  input [3:0] b1
);
endmodule";
        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::from_verilog("B", b_verilog, true, false);
        let top = ModDef::new("TopModule");
        let a_inst = top.instantiate(&a_mod_def, None, None);
        let b_inst = top.instantiate(&b_mod_def, Some("B_i_0"), None);
        a_inst.get_port("a").slice(3, 0).connect_to_net("custom0");
        a_inst.get_port("a").slice(7, 4).connect_to_net("custom1");
        b_inst.get_port("b0").connect_to_net("custom0");
        b_inst.get_port("b1").connect_to_net("custom1");
        assert_eq!(
            top.emit(true),
            "\
module TopModule;
  wire [3:0] custom0;
  wire [3:0] custom1;
  A A_i (
    .a({custom1, custom0})
  );
  B B_i_0 (
    .b0(custom0),
    .b1(custom1)
  );
endmodule
"
        );
    }

    #[test]
    #[should_panic(expected = "TopModule.B_i.bi (ModInst Input) is undriven")]
    fn test_connect_to_net_undriven_input() {
        let a_verilog = "\
module A(
  output ao
);
endmodule";
        let b_verilog = "\
module B(
  input bi
);
endmodule";
        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::from_verilog("B", b_verilog, true, false);
        let top = ModDef::new("TopModule");
        let a_inst = top.instantiate(&a_mod_def, None, None);
        top.instantiate(&b_mod_def, None, None);
        a_inst.get_port("ao").connect_to_net("custom");
        top.validate();
    }

    #[test]
    #[should_panic(expected = "TopModule.A_i.ao (ModInst Output) is unused")]
    fn test_connect_to_net_unused_output() {
        let a_verilog = "\
module A(
  output ao
);
endmodule";
        let b_verilog = "\
module B(
  input bi
);
endmodule";
        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::from_verilog("B", b_verilog, true, false);
        let top = ModDef::new("TopModule");
        top.instantiate(&a_mod_def, None, None);
        let b_inst = top.instantiate(&b_mod_def, None, None);
        b_inst.get_port("bi").connect_to_net("custom");
        top.validate();
    }

    #[test]
    #[should_panic(
        expected = "Net width mismatch for TopModule.custom: existing width 4, new width 8"
    )]
    fn test_connect_to_net_width_mismatch() {
        let a_verilog = "\
module A(
  output [3:0] ao
);
endmodule";
        let b_verilog = "\
module B(
  input [7:0] bi
);
endmodule";
        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::from_verilog("B", b_verilog, true, false);
        let top = ModDef::new("TopModule");
        let a_inst = top.instantiate(&a_mod_def, None, None);
        let b_inst = top.instantiate(&b_mod_def, None, None);
        a_inst.get_port("ao").connect_to_net("custom");
        b_inst.get_port("bi").connect_to_net("custom");
        top.validate();
    }

    #[test]
    fn test_has_port() {
        let a_verilog = "\
        module A(
          output a
        );
        endmodule";

        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::new("B");
        let a_inst = b_mod_def.instantiate(&a_mod_def, None, None);
        a_inst.get_port("a").export_as("b");

        assert!(a_mod_def.has_port("a"));
        assert!(!a_mod_def.has_port("b"));
        assert!(a_inst.has_port("a"));
        assert!(!a_inst.has_port("b"));
        assert!(b_mod_def.has_port("b"));
        assert!(!b_mod_def.has_port("a"));
    }

    #[test]
    fn test_get_inst_ports() {
        let a_verilog = "\
        module A(
          output a0,
          input a1
        );
        endmodule";

        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        let b_mod_def = ModDef::new("B");
        let a_inst = b_mod_def.instantiate(&a_mod_def, None, None);

        for (i, port) in a_inst.get_ports(None).iter().enumerate() {
            port.export_as(format!("b{}", i));
        }

        let ports = a_mod_def.get_ports(None);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].name(), "a0");
        assert_eq!(ports[1].name(), "a1");

        let ports = a_inst.get_ports(None);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].name(), "a0");
        assert_eq!(ports[1].name(), "a1");

        let ports = b_mod_def.get_ports(None);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].name(), "b0");
        assert_eq!(ports[1].name(), "b1");
    }

    #[test]
    #[should_panic(expected = "Empty interface definition for A.b")]
    fn test_empty_prefix_interface() {
        let a_verilog = "\
        module A(
          output a_data,
          output a_valid
        );
        endmodule";

        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        a_mod_def.def_intf_from_name_underscore("b");
    }

    #[test]
    #[should_panic(expected = "Empty interface definition for A.b")]
    fn test_empty_regex_interface() {
        let a_verilog = "\
        module A(
          output a_data,
          output a_valid
        );
        endmodule";

        let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
        a_mod_def.def_intf_from_regex("b", "^b_(.*)$", "${1}");
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

        a_inst.get_port("a").connect_through(
            &e_inst.get_port("e"),
            &[&b_inst, &c_inst, &d_inst],
            "ft",
        );

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
  assign flipped[7:0] = original[7:0];
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
  assign flipped[3:0] = original[3:0];
endmodule
"
        );
    }

    #[test]
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

    #[test]
    fn test_has_intf() {
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

        assert!(module_a.has_intf("a_intf"));
        assert!(!module_a.has_intf("b_intf"));

        let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
        module_b.def_intf_from_prefix("b_intf", "b_");

        let top_module = ModDef::new("TopModule");

        let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

        assert!(b_inst.has_intf("b_intf"));
        assert!(!b_inst.has_intf("a_intf"));
    }

    #[test]
    fn test_intf_copy_to_with_prefix() {
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
        let b_intf = module_a
            .get_intf("a")
            .copy_to_with_prefix(&module_b, "b", "");
        b_intf.unused_and_tieoff(0);

        assert_eq!(
            module_b.emit(true),
            "\
module ModuleB(
  output wire [31:0] data,
  output wire valid,
  input wire ready
);
  assign data[31:0] = 32'h0000_0000;
  assign valid = 1'h0;
endmodule
"
        );
    }

    #[test]
    fn test_intf_copy_to_with_name_underscore() {
        let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid,
        input a_ready
    );
    endmodule
    ";

        let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
        module_a.def_intf_from_name_underscore("a");

        let module_b = ModDef::new("ModuleB");
        let b_intf = module_a
            .get_intf("a")
            .copy_to_with_name_underscore(&module_b, "b");
        b_intf.unused_and_tieoff(0);

        assert_eq!(
            module_b.emit(true),
            "\
module ModuleB(
  output wire [31:0] b_data,
  output wire b_valid,
  input wire b_ready
);
  assign b_data[31:0] = 32'h0000_0000;
  assign b_valid = 1'h0;
endmodule
"
        );
    }

    #[test]
    fn test_define_with_parameterize() {
        let source = str2tmpfile(
            "
          module foo #(
            parameter N=1
          ) (
            `ifdef BAR
            input [N-1:0] a
            `else
            output [N-1:0] b
            `endif
        );
        endmodule",
        )
        .unwrap();

        let cfg_no_define = SlangConfig {
            sources: &[source.path().to_str().unwrap()],
            ..Default::default()
        };
        let orig_no_define = ModDef::from_verilog_using_slang("foo", &cfg_no_define, false);
        let parameterized_no_define = orig_no_define.parameterize(&[("N", 8)], None, None);

        assert_eq!(
            parameterized_no_define.emit(true),
            "\
module foo_N_8(
  output wire [7:0] b
);
  foo #(
    .N(32'h0000_0008)
  ) foo_i (
    .b(b)
  );
endmodule
"
        );

        let cfg_with_define = SlangConfig {
            sources: &[source.path().to_str().unwrap()],
            defines: &[("BAR", "1")],
            ..Default::default()
        };
        let orig_with_define = ModDef::from_verilog_using_slang("foo", &cfg_with_define, false);
        let parameterized_with_define = orig_with_define.parameterize(&[("N", 8)], None, None);

        assert_eq!(
            parameterized_with_define.emit(true),
            "\
module foo_N_8(
  input wire [7:0] a
);
  foo #(
    .N(32'h0000_0008)
  ) foo_i (
    .a(a)
  );
endmodule
"
        );
    }

    #[test]
    fn test_negative_indices() {
        let verilog = str2tmpfile(
            "\
        module foo (
          output [-4:-2] a
        );
        endmodule",
        )
        .unwrap();

        let foo = ModDef::from_verilog_file("foo", verilog.path(), true, false);
        let bar = foo.stub("bar");
        bar.get_port("a").tieoff(0);

        assert_eq!(
            bar.emit(true),
            "\
module bar(
  output wire [2:0] a
);
  assign a[2:0] = 3'h0;
endmodule
"
        );
    }

    #[test]
    fn test_negative_indices_parameterized() {
        let verilog = str2tmpfile(
            "\
        module foo #(
            parameter N=1
        ) (
            input [N-1:0] a
        );
        endmodule",
        )
        .unwrap();

        let foo = ModDef::from_verilog_file("foo", verilog.path(), true, false);

        let parameterized = foo.parameterize(&[("N", 0)], None, None);

        assert_eq!(
            parameterized.emit(true),
            "\
module foo_N_0(
  input wire [1:0] a
);
  foo #(
    .N(32'h0000_0000)
  ) foo_i (
    .a(a)
  );
endmodule
"
        );
    }
}
