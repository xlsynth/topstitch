// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

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

    println!("{}", top.emit(true));

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
  wire [15:0] ModuleA_i_e;
  wire [15:0] ModuleA_i_f;
  wire [15:0] ModuleA_i_g;
  ModuleA ModuleA_i (
    .a({b, a}),
    .b(c[7:0]),
    .c(c[15:8]),
    .d(d),
    .e({e, ModuleA_i_e[7:0]}),
    .f({ModuleA_i_f[15:8], f}),
    .g({ModuleA_i_g[15:12], g, ModuleA_i_g[7:0]})
  );
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
  wire inst_a_a0;
  wire inst_a_a1;
  wire inst_a_b;
  wire [1:0] inst_a_c;
  wire inst_b_d;
  wire inst_b_e;
  A inst_a (
    .a0(inst_a_a0),
    .a1(inst_a_a1),
    .b(inst_a_b),
    .c(inst_a_c),
    .d(inst_b_d),
    .e(inst_b_e)
  );
  B inst_b (
    .a({inst_a_a1, inst_a_a0}),
    .b(inst_a_b),
    .c(inst_a_c),
    .d(inst_b_d),
    .e(inst_b_e)
  );
endmodule
"
    );
}

#[test]
fn test_connect_modinst_input_output_to_moddef_inouts() {
    let outer_mod_def: ModDef = ModDef::new("Outer");
    outer_mod_def.add_port("a", IO::InOut(1));
    outer_mod_def.add_port("b", IO::InOut(1));

    let inner_mod_def = ModDef::new("Inner");
    inner_mod_def.add_port("i", IO::Input(1));
    inner_mod_def.add_port("o", IO::Output(1));
    inner_mod_def.set_usage(Usage::EmitNothingAndStop);

    let inner_inst = outer_mod_def.instantiate(&inner_mod_def, Some("inst_inner"), None);
    inner_inst
        .get_port("i")
        .connect(&outer_mod_def.get_port("a"));
    inner_inst
        .get_port("o")
        .connect(&outer_mod_def.get_port("b"));

    assert_eq!(
        outer_mod_def.emit(true),
        "\
module Outer(
  inout wire a,
  inout wire b
);
  Inner inst_inner (
    .i(a),
    .o(b)
  );
endmodule
"
    );
}

#[test]
#[should_panic(expected = "B.inst_a.a is unconnected")]
fn test_inout_unused_0() {
    let a_verilog = "\
module A(
  inout a
);
endmodule";

    let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
    let b_mod_def: ModDef = ModDef::new("B");

    b_mod_def.instantiate(&a_mod_def, Some("inst_a"), None);

    b_mod_def.emit(true);
}

#[test]
#[should_panic(expected = "B.inst_a.a[1] is unconnected")]
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

    b_mod_def.emit(true);
}

#[test]
#[should_panic(expected = "A.a is unconnected")]
fn test_inout_unused_2() {
    let a_mod_def: ModDef = ModDef::new("A");
    a_mod_def.add_port("a", IO::InOut(1));
    a_mod_def.emit(true);
}

#[test]
#[should_panic(expected = "B.b[1] is unconnected")]
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

    b_mod_def.emit(true);
}
