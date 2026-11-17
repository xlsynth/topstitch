// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_inst_name_false_collision() {
    let a = ModDef::new("a");
    a.add_port("x", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("i_x", IO::Input(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");

    let a_inst = top.instantiate(&a, Some("a_i"), None);
    let b_inst = top.instantiate(&b, Some("a"), None);

    a_inst.get_port("x").connect(&b_inst.get_port("i_x"));

    assert_eq!(
        top.emit(EmitOptions::default()),
        "\
module Top;
  wire [7:0] a_i_x;
  a a_i (
    .x(a_i_x)
  );
  b a (
    .i_x(a_i_x)
  );
endmodule
"
    );
}

#[test]
#[should_panic(expected = "Net \"custom\" has already been manually specified")]
fn test_specify_net_name_collision() {
    let a = ModDef::new("a");
    a.add_port("ix", IO::Input(8));
    a.add_port("ox", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("iy", IO::Input(8));
    b.add_port("oy", IO::Output(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");

    let a_inst = top.instantiate(&a, None, None);
    let b_inst = top.instantiate(&b, None, None);

    a_inst.get_port("ox").connect(&b_inst.get_port("iy"));
    a_inst.get_port("ox").specify_net_name("custom");
    b_inst.get_port("oy").connect(&a_inst.get_port("ix"));
    b_inst.get_port("oy").specify_net_name("custom");

    top.emit(EmitOptions::default());
}

#[test]
fn test_mod_def_name_false_collision() {
    let a = ModDef::new("a");
    a.add_port("x", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");
    top.add_port("a_i_x", IO::Input(8)).unused();

    let a_inst = top.instantiate(&a, Some("a_i"), None);
    a_inst.get_port("x").export_as("y");

    assert_eq!(
        top.emit(EmitOptions::default()),
        "\
module Top(
  input wire [7:0] a_i_x,
  output wire [7:0] y
);
  a a_i (
    .x(y)
  );
endmodule
"
    );
}

#[test]
#[should_panic(expected = "Net name collision")]
fn test_mod_def_name_collision_with_custom_net_name() {
    let a = ModDef::new("a");
    a.add_port("x", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("y", IO::Input(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");
    top.add_port("custom", IO::Input(8)).unused();

    let a_inst = top.instantiate(&a, None, None);
    let b_inst = top.instantiate(&b, None, None);

    a_inst.get_port("x").connect(&b_inst.get_port("y"));
    a_inst.get_port("x").specify_net_name("custom");

    top.emit(EmitOptions::default());
}

#[test]
#[should_panic(expected = "requested wire name stage_i collides with a port or instance")]
fn requested_wire_cannot_take_an_instance_name() {
    let pass = ModDef::new("Pass");
    pass.feedthrough("d", "q", 1);
    let top = ModDef::new("Top");
    let stage = top.instantiate(&pass, Some("stage_i"), None);
    stage.get_port("d").export_as("d");
    stage.get_port("q").export_as("q");
    stage.get_port("q").specify_net_name("stage_i");
    top.emit(EmitOptions::default());
}

#[test]
fn test_mod_inst_name_collision_with_custom_net_name() {
    let a = ModDef::new("a");
    a.add_port("ix", IO::Input(8));
    a.add_port("ox", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("iy", IO::Input(8));
    b.add_port("oy", IO::Output(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");

    top.add_port("b_i_oy__1", IO::Input(1)).unused();
    let a_inst = top.instantiate(&a, Some("a_i"), None);
    let b_inst = top.instantiate(&b, Some("b_i"), None);

    a_inst.get_port("ox").connect(&b_inst.get_port("iy"));
    a_inst.get_port("ox").specify_net_name("b_i_oy");
    b_inst.get_port("oy").connect(&a_inst.get_port("ix"));

    let sv = top.emit(EmitOptions::default());
    assert!(sv.contains(".ox(b_i_oy)"));
    assert!(sv.contains(".iy(b_i_oy)"));
    assert!(sv.contains(".oy(b_i_oy__2)"));
    assert!(sv.contains(".ix(b_i_oy__2)"));
}
