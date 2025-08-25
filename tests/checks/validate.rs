// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;
use topstitch::*;

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
fn test_large_validation() {
    let a = ModDef::new("A");
    a.set_usage(Usage::EmitStubAndStop);

    let b = ModDef::new("B");
    b.set_usage(Usage::EmitStubAndStop);

    for i in 0..10000 {
        a.add_port(format!("a_{i}"), IO::Output(1000));
        b.add_port(format!("b_{i}"), IO::Input(1000));
    }

    let top = ModDef::new("Top");

    let a_inst = top.instantiate(&a, None, None);
    let b_inst = top.instantiate(&b, None, None);

    for i in 0..10000 {
        a_inst
            .get_port(format!("a_{i}"))
            .connect(&b_inst.get_port(format!("b_{i}")));
    }

    let start = Instant::now();
    top.validate();
    let duration = start.elapsed();

    assert!(
        duration.as_secs() < 5,
        "Validation took too long: {duration:?}"
    );
}
