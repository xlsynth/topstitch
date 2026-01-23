// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_port_slices() {
    // Define module A
    let a_mod_def = ModDef::new("A");
    a_mod_def.add_port("bus", IO::Input(8));

    // Define module B
    let b_mod_def = ModDef::new("B");
    b_mod_def.add_port("half_bus", IO::Input(4));

    let b0 = a_mod_def.instantiate(&b_mod_def, Some("b0"), None);
    let b1 = a_mod_def.instantiate(&b_mod_def, Some("b1"), None);

    let a_bus = a_mod_def.get_port("bus");
    b0.get_port("half_bus").connect(&a_bus.slice(3, 0));
    a_bus.slice(7, 4).connect(&b1.get_port("half_bus"));

    b_mod_def.set_usage(Usage::EmitStubAndStop);

    assert_eq!(
        a_mod_def.emit(true),
        "\
module B(
  input wire [3:0] half_bus
);

endmodule
module A(
  input wire [7:0] bus
);
  B b0 (
    .half_bus(bus[3:0])
  );
  B b1 (
    .half_bus(bus[7:4])
  );
endmodule
"
    );
}

#[test]
fn test_port_slice_get_port_preserves_hierarchy() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("bus", IO::Input(8));

    let mid = leaf.wrap(Some("Mid"), Some("leaf_inst"));
    let top = ModDef::new("Top");
    let mid_inst = top.instantiate(&mid, Some("mid_inst"), None);
    let leaf_from_top = mid_inst.get_instance("leaf_inst");

    let slice = leaf_from_top.get_port("bus").slice(7, 4);
    let port = slice.get_port();
    assert_eq!(port, leaf_from_top.get_port("bus"));
}

#[test]
fn test_port_place_across() {
    let module = ModDef::new("Top");
    module.add_port("a", IO::Input(1));
    module.add_port("b", IO::Output(1));

    module.set_width_height(10, 10);
    let pin_shape = Polygon::from_width_height(1, 1);
    let mut tracks = TrackDefinitions::default();
    tracks.add_track(TrackDefinition::new(
        "M1",
        0,
        1,
        TrackOrientation::Vertical,
        Some(pin_shape.clone()),
        None,
    ));
    module.set_track_definitions(tracks);

    let src_pin = PhysicalPin::from_translation("M1", pin_shape.clone(), Coordinate { x: 3, y: 0 });
    module.place_pin("a", 0, src_pin);

    module.get_port("a").connect(&module.get_port("b"));
    module.get_port("b").place_across();

    let placed = module.get_port("b").bit(0).get_physical_pin();
    assert_eq!(placed.translation(), Coordinate { x: 3, y: 10 });
}
