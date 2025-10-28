// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

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
  assign constant = 8'h42;
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
  A a_inst (
    .a0(8'h23),
    .a1(8'h43),
    .a2({4'h5, b1})
  );
  assign b0 = 8'h12;
endmodule
"
    );
}

#[test]
fn test_tieoff_modinst_input() {
    let leaf = ModDef::new("LeafMod");
    leaf.set_usage(Usage::EmitStubAndStop);
    leaf.add_port("in", IO::Input(1));

    let parent = ModDef::new("ParentMod");
    let inst = parent.instantiate(&leaf, Some("leaf_inst"), None);

    inst.get_port("in").tieoff(0);

    parent.emit(true); // Should pass
}

#[test]
fn test_tieoff_moddef_output() {
    let mod_def = ModDef::new("TestMod");
    let out_port = mod_def.add_port("out", IO::Output(1));

    out_port.tieoff(1);

    mod_def.emit(true); // Should pass
}

#[test]
#[should_panic(expected = "TestMod.in[0:0] has the wrong directionality to be tied off")]
fn test_invalid_tieoff_moddef_input() {
    let mod_def = ModDef::new("TestMod");
    let in_port = mod_def.add_port("in", IO::Input(1));

    in_port.tieoff(0);

    mod_def.emit(true); // Should panic
}

#[test]
#[should_panic(
    expected = "ParentMod.leaf_inst.out[0:0] has the wrong directionality to be tied off"
)]
fn test_invalid_tieoff_modinst_output() {
    let leaf = ModDef::new("LeafMod");
    leaf.set_usage(Usage::EmitStubAndStop);
    leaf.add_port("out", IO::Output(1));

    let parent = ModDef::new("ParentMod");
    let inst = parent.instantiate(&leaf, Some("leaf_inst"), None);

    inst.get_port("out").tieoff(0);

    parent.emit(true); // Should panic
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
  assign top_data = 32'h0000_0000;
  assign top_valid = 1'h0;
endmodule
"
    );
}
