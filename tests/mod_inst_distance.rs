// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

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
