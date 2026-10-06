// SPDX-License-Identifier: Apache-2.0

use topstitch::{Coordinate, IO, ModDef, PhysicalPin, Polygon};

#[test]
fn copy_edge_two_pins_from_square_to_longer_rectangle() {
    let square = ModDef::new("square");
    square.set_width_height(100, 100);
    let square_data = square.add_port("data", IO::Input(2));

    let rectangle = ModDef::new("rectangle");
    rectangle.set_width_height(200, 100);
    let rectangle_data = rectangle.add_port("data", IO::Input(2));

    // The square's only two pins touch edge 2, its right edge at x = 100.
    for (bit, y) in [(0, 22), (1, 72)] {
        square_data.bit(bit).place(PhysicalPin::from_translation(
            "M2",
            Polygon::from_width_height(4, 6),
            Coordinate { x: 96, y },
        ));
    }

    // Edges 1 and 3 are longer; copy the pins to the new position of edge 2.
    let delta = rectangle.get_edge(2).unwrap().a - square.get_edge(2).unwrap().a;
    for bit in 0..2 {
        let pin = square_data.bit(bit).get_physical_pin();
        rectangle_data.bit(bit).place(pin + delta);
    }

    assert_eq!(delta, Coordinate { x: 100, y: 0 });
    for (bit, expected) in [
        (0, [(196, 22), (196, 28), (200, 28), (200, 22)]),
        (1, [(196, 72), (196, 78), (200, 78), (200, 72)]),
    ] {
        let pin = rectangle_data.bit(bit).get_physical_pin();
        assert_eq!(pin.layer, "M2");
        assert_eq!(pin.transformed_polygon().0, expected.map(Coordinate::from));
    }
}
