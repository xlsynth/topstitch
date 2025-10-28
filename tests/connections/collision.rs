// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
#[should_panic(expected = "Net \"a_i_x\" is already declared")]
fn test_inst_name_collision() {
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

    top.emit(true);
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

    top.emit(true);
}

#[test]
#[should_panic(expected = "Net \"a_i_x\" is already declared")]
fn test_mod_def_name_collision() {
    let a = ModDef::new("a");
    a.add_port("x", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");
    top.add_port("a_i_x", IO::Input(8)).unused();

    let a_inst = top.instantiate(&a, Some("a_i"), None);
    a_inst.get_port("x").export_as("y");

    top.emit(true);
}

#[test]
#[should_panic(expected = "Net \"custom\" is already declared")]
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

    top.emit(true);
}

#[test]
#[should_panic(expected = "Net \"b_i_oy\" is already declared")]
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

    let a_inst = top.instantiate(&a, Some("a_i"), None);
    let b_inst = top.instantiate(&b, Some("b_i"), None);

    a_inst.get_port("ox").connect(&b_inst.get_port("iy"));
    a_inst.get_port("ox").specify_net_name("b_i_oy");
    b_inst.get_port("oy").connect(&a_inst.get_port("ix"));

    top.emit(true);
}
