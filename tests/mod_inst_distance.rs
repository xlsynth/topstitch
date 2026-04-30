// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

fn single_pin_mod(name: &str, port_name: &str, io: IO) -> ModDef {
    let module = ModDef::new(name);
    module.set_usage(Usage::EmitStubAndStop);
    module.set_width_height(4, 4);
    module.add_port(port_name, io);
    module.place_pin(
        port_name,
        0,
        PhysicalPin::from_translation(
            "M1",
            Polygon::from_width_height(2, 2),
            Coordinate { x: 1, y: 1 },
        ),
    );
    module
}

#[test]
fn test_mod_inst_validate_connection_distances_ok() {
    let a = ModDef::new("A");
    a.set_usage(Usage::EmitStubAndStop);
    a.add_port("x", IO::Output(2));
    a.get_port("x").set_max_distance(Some(1));
    a.get_port("x").bit(1).set_max_distance(None);
    a.add_port("z", IO::Input(1));
    a.place_pin(
        "x",
        0,
        PhysicalPin::from_translation(
            "M1",
            Polygon::from_width_height(2, 2),
            Coordinate { x: 3, y: 1 },
        ),
    );
    a.place_pin(
        "x",
        1,
        PhysicalPin::from_translation(
            "M1",
            Polygon::from_width_height(2, 2),
            Coordinate { x: 3, y: 4 },
        ),
    );

    let b = ModDef::new("B");
    b.set_usage(Usage::EmitStubAndStop);
    b.add_port("y", IO::Input(2));
    let b_pin = PhysicalPin::from_translation(
        "M1",
        Polygon::from_width_height(2, 2),
        Coordinate { x: 3, y: 2 },
    );
    b.place_pin("y", 0, b_pin);

    let top = ModDef::new("Top");
    let a_inst = top.instantiate(&a, Some("a_inst"), None);
    let b_inst = top.instantiate(&b, Some("b_inst"), None);
    a_inst.place((0, 0), Orientation::R0);
    b_inst.place((11, 0), Orientation::MY);

    a_inst.get_port("x").connect(&b_inst.get_port("y"));
    a_inst.get_port("z").export();

    a_inst.validate_connection_distances();
}

#[test]
fn test_mod_inst_validate_connection_distances_with_overlap() {
    let a = ModDef::new("A");
    a.set_usage(Usage::EmitStubAndStop);
    a.add_port("x", IO::Output(1));
    a.get_port("x").set_max_distance(Some(0));
    a.place_pin(
        "x",
        0,
        PhysicalPin::from_translation(
            "M1",
            Polygon::from_width_height(2, 2),
            Coordinate { x: 0, y: 0 },
        ),
    );

    let b = ModDef::new("B");
    b.set_usage(Usage::EmitStubAndStop);
    b.add_port("y", IO::Input(1));
    let b_pin = PhysicalPin::from_translation(
        "M1",
        Polygon::from_width_height(1, 1),
        Coordinate { x: 0, y: 1 },
    );
    b.place_pin("y", 0, b_pin);

    let top = ModDef::new("Top");
    let a_inst = top.instantiate(&a, Some("a_inst"), None);
    let b_inst = top.instantiate(&b, Some("b_inst"), None);
    a_inst.place((0, 0), Orientation::R0);
    b_inst.place((1, 0), Orientation::R0);

    a_inst.get_port("x").connect(&b_inst.get_port("y"));

    a_inst.validate_connection_distances();
}

#[test]
fn test_connection_distance_with_gap() {
    let a = single_pin_mod("A", "x", IO::Output(1));
    let b = single_pin_mod("B", "y", IO::Input(1));

    let top = ModDef::new("Top");
    let a_inst = top.instantiate(&a, Some("a_inst"), None);
    let b_inst = top.instantiate(&b, Some("b_inst"), None);
    a_inst.place((0, 0), Orientation::R0);
    b_inst.place((7, 0), Orientation::R0);

    a_inst.get_port("x").connect(&b_inst.get_port("y"));

    assert_eq!(
        a_inst
            .get_port("x")
            .bit(0)
            .get_connected_port_slice_and_distance(),
        Some((b_inst.get_port("y").bit(0), 5))
    );
    assert_eq!(
        a_inst.get_port("x").bit(0).get_connection_distance(),
        Some(5)
    );
    assert_eq!(
        b_inst.get_port("y").bit(0).get_connection_distance(),
        Some(5)
    );
}

#[test]
#[should_panic(expected = "physical pin lookup currently only supports single-bit port slices")]
fn test_connection_distance_multibit_panics() {
    let a = ModDef::new("A");
    a.add_port("x", IO::Output(2));

    let top = ModDef::new("Top");
    let a_inst = top.instantiate(&a, Some("a_inst"), None);

    a_inst.get_port("x").get_connection_distance();
}

#[test]
fn test_connection_distance_tieoff_returns_none() {
    let a = single_pin_mod("A", "x", IO::Input(1));

    let top = ModDef::new("Top");
    let a_inst = top.instantiate(&a, Some("a_inst"), None);
    a_inst.place((0, 0), Orientation::R0);

    a_inst.get_port("x").tieoff(0);

    assert_eq!(a_inst.get_port("x").bit(0).get_connection_distance(), None);
}

#[test]
fn test_connection_distance_multi_fanout_returns_none() {
    let a = single_pin_mod("A", "x", IO::Output(1));
    let b = single_pin_mod("B", "y", IO::Input(1));
    let c = single_pin_mod("C", "z", IO::Input(1));

    let top = ModDef::new("Top");
    let a_inst = top.instantiate(&a, Some("a_inst"), None);
    let b_inst = top.instantiate(&b, Some("b_inst"), None);
    let c_inst = top.instantiate(&c, Some("c_inst"), None);
    a_inst.place((0, 0), Orientation::R0);
    b_inst.place((4, 0), Orientation::R0);
    c_inst.place((8, 0), Orientation::R0);

    a_inst.get_port("x").connect(&b_inst.get_port("y"));
    a_inst.get_port("x").connect(&c_inst.get_port("z"));

    assert_eq!(a_inst.get_port("x").bit(0).get_connection_distance(), None);
}

#[test]
fn test_connection_distance_trace_error_returns_none() {
    let a = ModDef::new("A");
    a.add_port("x", IO::Output(1));

    let b = a.wrap(Some("B"), Some("a_inst"));
    let b_inst_in_a = a.instantiate(&b, Some("b_inst"), None);
    b_inst_in_a.get_port("x").connect(&a.get_port("x"));

    let d = ModDef::new("D");
    d.add_port("p", IO::Input(1));

    let top = ModDef::new("Top");
    let b_inst = top.instantiate(&b, Some("b_inst"), None);
    let d_inst = top.instantiate(&d, Some("d_inst"), None);
    d_inst.get_port("p").connect(&b_inst.get_port("x"));

    assert_eq!(d_inst.get_port("p").bit(0).get_connection_distance(), None);
}

#[test]
#[should_panic(
    expected = "Distance between Top.a_inst.x[0:0] and Top.b_inst.y[0:0] is 2, exceeding the max specified distance of 1"
)]
fn test_mod_inst_validate_connection_distances_panics() {
    let a = ModDef::new("A");
    a.set_usage(Usage::EmitStubAndStop);
    a.add_port("x", IO::Output(1));
    a.get_port("x").set_max_distance(Some(1));
    let a_pin = PhysicalPin::from_translation(
        "M1",
        Polygon::from_width_height(2, 2),
        Coordinate { x: 3, y: 1 },
    );
    a.place_pin("x", 0, a_pin);

    let b = ModDef::new("B");
    b.set_usage(Usage::EmitStubAndStop);
    b.add_port("y", IO::Input(1));
    let b_pin = PhysicalPin::from_translation(
        "M1",
        Polygon::from_width_height(2, 2),
        Coordinate { x: 0, y: 1 },
    );
    b.place_pin("y", 0, b_pin);

    let top = ModDef::new("Top");
    let a_inst = top.instantiate(&a, Some("a_inst"), None);
    let b_inst = top.instantiate(&b, Some("b_inst"), None);
    a_inst.place((0, 0), Orientation::R0);
    b_inst.place((7, 0), Orientation::R0);

    a_inst.get_port("x").connect(&b_inst.get_port("y"));

    a_inst.validate_connection_distances();
}
