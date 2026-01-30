// SPDX-License-Identifier: Apache-2.0

use topstitch::{IO, ModDef, PhysicalPin, Polygon};

fn make_pin(x: i64) -> PhysicalPin {
    PhysicalPin::from_translation("M1", Polygon::from_width_height(1, 1), (x, 0).into())
}

#[test]
fn unpinned_port_slices_merges_runs() {
    let block = ModDef::new("block");
    let a = block.add_port("a", IO::Input(8));
    let b = block.add_port("b", IO::Output(4));

    block.get_port("a").bit(0).place(make_pin(0));
    block.get_port("a").bit(4).place(make_pin(4));
    block.get_port("a").bit(6).place(make_pin(6));

    let missing = block.unpinned_port_slices();

    assert_eq!(
        missing,
        vec![a.slice(3, 1), a.slice(5, 5), a.slice(7, 7), b.slice(3, 0)]
    );
}

#[test]
fn unpinned_port_slices_single_run() {
    let block = ModDef::new("block");
    let bus = block.add_port("bus", IO::InOut(5));

    block.get_port("bus").bit(0).place(make_pin(0));
    block.get_port("bus").bit(1).place(make_pin(1));

    let missing = block.unpinned_port_slices();

    assert_eq!(missing, vec![bus.slice(4, 2)]);
}
