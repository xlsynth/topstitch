// SPDX-License-Identifier: Apache-2.0

use topstitch::{BoundingBox, LefDefOptions, Mat3, ModDef, Orientation, Polygon, IO};

#[test]
fn generate_lef_with_pins_and_positions() {
    // Define a leaf block with shape and ports
    let block = ModDef::new("block");
    block.set_shape(Polygon::from_width_height(100, 50));
    block.set_layer("OUTLINE");
    block.add_port("a", IO::Input(2));
    block.add_port("b", IO::Output(1));

    // Define physical pins with explicit layer and position offsets
    let pin = Polygon::from_bbox(&BoundingBox {
        min_x: 0,
        min_y: -5,
        max_x: 20,
        max_y: 5,
    });
    block
        .get_port("a")
        .bit(0)
        .place("M1", (0, 15).into(), pin.clone());
    block
        .get_port("a")
        .bit(1)
        .place("M1", (0, 35).into(), pin.clone());
    block.get_port("b").bit(0).place(
        "M2",
        (100, 25).into(),
        pin.apply_transform(&Mat3::from_orientation(Orientation::MY)),
    );

    block.set_usage(topstitch::Usage::EmitStubAndStop);

    // Create a top that instantiates and places the block
    let top = ModDef::new("top");
    let inst = top.instantiate(&block, Some("u0"), None);
    inst.place((0, 0), Orientation::R0);

    // Emit LEF and check contents
    let (lef, _def) = top.emit_lef_def(&LefDefOptions::default());

    // Macro and outline
    assert!(lef.contains("MACRO block"));
    assert!(lef.contains("LAYER OUTLINE"));
    assert!(lef.contains("POLYGON ( 0 0 0 50 100 50 100 0 ) ;"));

    // a[0]
    assert!(lef.contains("PIN a[0]"));
    assert!(lef.contains("DIRECTION INPUT"));
    assert!(lef.contains("LAYER M1"));
    assert!(lef.contains("POLYGON ( 0 10 0 20 20 20 20 10 ) ;"));

    // a[1]
    assert!(lef.contains("PIN a[1]"));
    assert!(lef.contains("DIRECTION INPUT"));
    assert!(lef.contains("LAYER M1"));
    assert!(lef.contains("POLYGON ( 0 30 0 40 20 40 20 30 ) ;"));

    // b[0]
    assert!(lef.contains("PIN b[0]"));
    assert!(lef.contains("DIRECTION OUTPUT"));
    assert!(lef.contains("LAYER M2"));
    assert!(lef.contains("POLYGON ( 100 20 100 30 80 30 80 20 ) ;"));
}
