// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use topstitch::{IO, ModDef, Orientation, PhysicalPin, PinOverlap, Polygon};

#[test]
fn rejects_overlap_and_boundary_contact_between_rectangular_pins() {
    let module = ModDef::new("block");
    let a = module.add_port("a", IO::Input(1)).bit(0);
    let b = module.add_port("b", IO::Output(1)).bit(0);
    let pin = PhysicalPin::new("M1", Polygon::from_width_height(4, 2));

    a.place(pin.clone());
    b.place(pin.with_translation((5, 0).into()));
    assert_eq!(module.check_pin_overlaps(), Ok(()));

    // These offsets give shared area, a shared edge, and a shared corner, respectively.
    for offset in [(3, 0), (4, 0), (4, 2)] {
        b.place(pin.with_translation(offset.into()));
        assert_eq!(
            module.check_pin_overlaps(),
            Err(PinOverlap::new("M1", [("a".into(), 0), ("b".into(), 0)])),
            "offset {offset:?}"
        );
    }
}

#[test]
fn finds_a_collision_in_a_row_of_spaced_pins() {
    let module = ModDef::new("block");
    const WIDTH: usize = 128;
    let bus = module.add_port("bus", IO::Input(WIDTH));
    let pin = PhysicalPin::new("M1", Polygon::from_width_height(4, 2));
    for bit in 0..WIDTH {
        bus.bit(bit)
            .place(pin.with_translation((5 * bit as i64, 0).into()));
    }
    assert_eq!(module.check_pin_overlaps(), Ok(()));

    bus.bit(WIDTH - 1).place(pin);
    assert_eq!(
        module.check_pin_overlaps(),
        Err(PinOverlap::new(
            "M1",
            [("bus".into(), 0), ("bus".into(), WIDTH - 1)]
        ))
    );
}

#[test]
fn checks_placed_bits_after_transform_on_the_same_layer() {
    let module = ModDef::new("block");
    let bus = module.add_port("bus", IO::Input(4));
    let square = Polygon::from_width_height(4, 4);
    bus.bit(2).place(PhysicalPin::new("M1", square.clone()));
    bus.bit(0).place(PhysicalPin::new("M2", square.clone()));

    // The reflected square starts one unit to the right of bit 2.
    bus.bit(1)
        .place(PhysicalPin::from_orientation_then_translation(
            "M1",
            square.clone(),
            Orientation::MY,
            (9, 0).into(),
        ));
    module.check_pin_overlaps().unwrap();

    // Move the square until its left edge touches bit 2. The fourth bit is still unplaced.
    bus.bit(1)
        .place(PhysicalPin::from_orientation_then_translation(
            "M1",
            square,
            Orientation::MY,
            (8, 0).into(),
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
fn checks_the_actual_outline_of_nonrectangular_pins() {
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
    let square = PhysicalPin::new("M1", Polygon::from_width_height(2, 2));
    // Only the bounds intersect: they share area at (3, 3) and just an edge at (4, 3).
    for offset in [(3, 3), (4, 3)] {
        b.bit(0).place(square.with_translation(offset.into()));
        assert_eq!(module.check_pin_overlaps(), Ok(()), "offset {offset:?}");
    }

    // Touch the inner edges of the notch, touch just the outer corner, then overlap an arm.
    for offset in [(2, 2), (4, 2), (1, 2)] {
        b.bit(0).place(square.with_translation(offset.into()));
        assert_eq!(
            module.check_pin_overlaps().unwrap_err().to_string(),
            "a[0] and b[0] touch or overlap on M1",
            "offset {offset:?}"
        );
    }
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
