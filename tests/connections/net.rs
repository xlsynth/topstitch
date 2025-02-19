// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

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
#[should_panic(expected = "Net width mismatch for TopModule.custom: existing width 4, new width 8")]
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
