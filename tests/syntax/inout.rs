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
