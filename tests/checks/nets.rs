// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
#[should_panic(expected = "Port TestMod.out already exists")]
fn test_duplicate_mod_def_port() {
    let mod_def = ModDef::new("TestMod");
    mod_def.add_port("out", IO::Output(1));
    mod_def.add_port("out", IO::Output(2));
}

#[test]
#[should_panic(expected = "Net name collision")]
fn test_mod_def_name_collision() {
    let top = ModDef::new("top");
    top.add_port("a_b", IO::InOut(1));

    let a = ModDef::new("a");
    a.add_port("b", IO::Output(1)).tieoff(0);

    let b = ModDef::new("b");
    b.add_port("c", IO::Input(1)).unused();

    let a_inst = top.instantiate(&a, Some("a"), None);
    let b_inst = top.instantiate(&b, Some("b"), None);

    a_inst.get_port("b").connect(&b_inst.get_port("c"));
    top.get_port("a_b").unused();

    top.emit(true);
}

#[test]
fn test_mod_def_name_collision_resolution() {
    let top = ModDef::new("top");
    top.add_port("a_b", IO::InOut(1));

    let a = ModDef::new("a");
    a.add_port("b", IO::Output(1));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("c", IO::Input(1));
    b.set_usage(Usage::EmitNothingAndStop);

    let a_inst = top.instantiate(&a, Some("a"), None);
    let b_inst = top.instantiate(&b, Some("b"), None);

    a_inst.get_port("b").connect(&b_inst.get_port("c"));
    a_inst.get_port("b").specify_net_name("xyz");

    top.get_port("a_b").unused();

    assert_eq!(
        top.emit(true),
        "\
module top(
  inout wire a_b
);
  wire xyz;
  a a (
    .b(xyz)
  );
  b b (
    .c(xyz)
  );
endmodule
"
    );
}

#[test]
fn test_mod_def_name_false_collision() {
    let top = ModDef::new("top");
    top.add_port("a_b", IO::InOut(1));

    let a = ModDef::new("a");
    a.add_port("b", IO::InOut(1)).unused();
    a.set_usage(Usage::EmitNothingAndStop);

    let a_inst = top.instantiate(&a, Some("a"), None);

    a_inst.get_port("b").connect(&top.get_port("a_b"));

    assert_eq!(
        top.emit(true),
        "\
module top(
  inout wire a_b
);
  a a (
    .b(a_b)
  );
endmodule
"
    );
}

#[test]
#[should_panic(expected = "Net name collision")]
fn test_mod_inst_name_collision() {
    let top = ModDef::new("top");
    let a = ModDef::new("a");
    a.add_port("b_c_d_e", IO::InOut(1)).unused();
    let a_b = ModDef::new("a_b");
    a_b.add_port("c_d_e", IO::InOut(1)).unused();
    let a_b_c = ModDef::new("a_b_c");
    a_b_c.add_port("d_e", IO::InOut(1)).unused();
    let a_b_c_d = ModDef::new("a_b_c_d");
    a_b_c_d.add_port("e", IO::InOut(1)).unused();

    let a_inst = top.instantiate(&a, Some("a"), None);
    let a_b_inst = top.instantiate(&a_b, Some("a_b"), None);
    let a_b_c_inst = top.instantiate(&a_b_c, Some("a_b_c"), None);
    let a_b_c_d_inst = top.instantiate(&a_b_c_d, Some("a_b_c_d"), None);

    a_inst
        .get_port("b_c_d_e")
        .connect(&a_b_inst.get_port("c_d_e"));
    a_b_c_inst
        .get_port("d_e")
        .connect(&a_b_c_d_inst.get_port("e"));

    top.emit(true);
}

#[test]
fn test_mod_inst_name_collision_resolution() {
    let top = ModDef::new("top");

    let a = ModDef::new("a");
    a.add_port("b_c_d_e", IO::InOut(1)).unused();
    a.set_usage(Usage::EmitNothingAndStop);

    let a_b = ModDef::new("a_b");
    a_b.add_port("c_d_e", IO::InOut(1)).unused();
    a_b.set_usage(Usage::EmitNothingAndStop);

    let a_b_c = ModDef::new("a_b_c");
    a_b_c.add_port("d_e", IO::InOut(1)).unused();
    a_b_c.set_usage(Usage::EmitNothingAndStop);

    let a_b_c_d = ModDef::new("a_b_c_d");
    a_b_c_d.add_port("e", IO::InOut(1)).unused();
    a_b_c_d.set_usage(Usage::EmitNothingAndStop);

    let a_inst = top.instantiate(&a, Some("a"), None);
    let a_b_inst = top.instantiate(&a_b, Some("a_b"), None);
    let a_b_c_inst = top.instantiate(&a_b_c, Some("a_b_c"), None);
    let a_b_c_d_inst = top.instantiate(&a_b_c_d, Some("a_b_c_d"), None);

    a_inst
        .get_port("b_c_d_e")
        .connect(&a_b_inst.get_port("c_d_e"));
    a_inst.get_port("b_c_d_e").specify_net_name("xyz");

    a_b_c_inst
        .get_port("d_e")
        .connect(&a_b_c_d_inst.get_port("e"));

    assert_eq!(
        top.emit(true),
        "\
module top;
  wire xyz;
  a a (
    .b_c_d_e(xyz)
  );
  a_b a_b (
    .c_d_e(xyz)
  );
  wire a_b_c_d_e;
  a_b_c a_b_c (
    .d_e(a_b_c_d_e)
  );
  a_b_c_d a_b_c_d (
    .e(a_b_c_d_e)
  );
endmodule
"
    );
}

#[test]
fn test_mod_inst_name_false_collision() {
    let top = ModDef::new("top");

    let a = ModDef::new("a");
    a.add_port("b_c", IO::InOut(1)).unused();
    a.set_usage(Usage::EmitNothingAndStop);

    let a_b = ModDef::new("a_b");
    a_b.add_port("c", IO::InOut(1)).unused();
    a_b.set_usage(Usage::EmitNothingAndStop);

    let a_inst = top.instantiate(&a, Some("a"), None);
    let a_b_inst = top.instantiate(&a_b, Some("a_b"), None);

    a_inst.get_port("b_c").connect(&a_b_inst.get_port("c"));

    assert_eq!(
        top.emit(true),
        "\
module top;
  wire a_b_c;
  a a (
    .b_c(a_b_c)
  );
  a_b a_b (
    .c(a_b_c)
  );
endmodule
"
    );
}
