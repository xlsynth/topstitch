// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use topstitch::{IO, ModDef, Orientation, PhysicalPin, PinOverlap, Polygon};

#[test]
fn detects_overlap_between_two_rectangular_pins() {
    let module = ModDef::new("block");
    let a = module.add_port("a", IO::Input(1)).bit(0);
    let b = module.add_port("b", IO::Output(1)).bit(0);
    let pin = PhysicalPin::new("M1", Polygon::from_width_height(4, 2));

    a.place(pin.clone());
    b.place(pin.with_translation((5, 0).into()));
    assert_eq!(module.check_pin_overlaps(), Ok(()));

    b.place(pin.with_translation((3, 0).into()));
    assert_eq!(
        module.check_pin_overlaps(),
        Err(PinOverlap::new("M1", [("a".into(), 0), ("b".into(), 0)]))
    );
}

#[test]
fn checks_placed_bits_after_transform_on_the_same_layer() {
    let module = ModDef::new("block");
    let bus = module.add_port("bus", IO::Input(4));
    let square = Polygon::from_width_height(4, 4);
    bus.bit(2).place(PhysicalPin::new("M1", square.clone()));
    bus.bit(0).place(PhysicalPin::new("M2", square.clone()));

    // The reflected square spans x=4..8 and abuts bit 2 along its right edge.
    bus.bit(1)
        .place(PhysicalPin::from_orientation_then_translation(
            "M1",
            square.clone(),
            Orientation::MY,
            (8, 0).into(),
        ));
    module.check_pin_overlaps().unwrap();

    // Move the square into bit 2. The fourth bit is still unplaced.
    bus.bit(1)
        .place(PhysicalPin::from_orientation_then_translation(
            "M1",
            square,
            Orientation::MY,
            (7, -1).into(),
        ));
    assert_eq!(
        module.check_pin_overlaps(),
        Err(PinOverlap::new(
            "M1",
            [("bus".into(), 2), ("bus".into(), 1)]
        ))
    );
}

#[test]
fn uses_polygon_interiors_for_nonrectangular_pins() {
    let module = ModDef::new("block");
    let a = module.add_port("a", IO::Input(1));
    let b = module.add_port("b", IO::Output(1));
    let l_shape = Polygon::new(vec![
        (0, 0).into(),
        (0, 4).into(),
        (2, 4).into(),
        (2, 2).into(),
        (4, 2).into(),
        (4, 0).into(),
    ]);
    a.bit(0).place(PhysicalPin::new("M1", l_shape));
    let square = Polygon::from_width_height(2, 2);
    b.bit(0)
        .place(PhysicalPin::from_orientation_then_translation(
            "M1",
            square.clone(),
            Orientation::R0,
            (2, 2).into(),
        ));
    module.check_pin_overlaps().unwrap();
    b.bit(0)
        .place(PhysicalPin::from_orientation_then_translation(
            "M1",
            square,
            Orientation::R0,
            (1, 2).into(),
        ));
    assert_eq!(
        module.check_pin_overlaps().unwrap_err().to_string(),
        "a[0] and b[0] overlap on M1"
    );
}

#[test]
fn collisions_in_a_set_are_unordered_but_layer_specific() {
    let a = ("a".to_owned(), 3);
    let b = ("b".to_owned(), 1);
    let ab = PinOverlap::new("M1", [a.clone(), b.clone()]);
    let ba = PinOverlap::new("M1", [b.clone(), a.clone()]);
    assert_eq!(ab, ba);
    assert_eq!(ba.pins(), &[a.clone(), b.clone()]);

    let mut seen = HashSet::from([ab]);
    assert!(!seen.insert(ba));
    assert!(seen.insert(PinOverlap::new("M2", [b, a])));
}
