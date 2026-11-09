// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use topstitch::*;

#[test]
fn test_basic_grid_check() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);
    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 400), Orientation::R0);
    top.emit_def(&LefDefOptions {
        check_grid_placement: Some((100, 200)),
        check_grid_size: Some((100, 200)),
        ..LefDefOptions::default()
    });
}

#[test]
#[should_panic(expected = "not sized to a multiple of the X grid")]
fn test_basic_width_off_grid() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(101, 200);
    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 400), Orientation::R0);
    top.emit_lef_def(&LefDefOptions {
        check_grid_size: Some((100, 200)),
        ..LefDefOptions::default()
    });
}

#[test]
#[should_panic(expected = "not sized to a multiple of the Y grid")]
fn test_basic_height_off_grid() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 201);
    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 400), Orientation::R0);
    top.emit_def(&LefDefOptions {
        check_grid_size: Some((100, 200)),
        ..LefDefOptions::default()
    });
}

#[test]
#[should_panic(expected = "not placed on the X grid")]
fn test_basic_x_coord_off_grid() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);
    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((301, 400), Orientation::R0);
    top.emit_lef_def(&LefDefOptions {
        check_grid_placement: Some((100, 200)),
        ..LefDefOptions::default()
    });
}

#[test]
#[should_panic(expected = "not placed on the Y grid")]
fn test_basic_y_coord_off_grid() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);
    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 401), Orientation::R0);
    top.emit_def(&LefDefOptions {
        check_grid_placement: Some((100, 200)),
        ..LefDefOptions::default()
    });
}

#[test]
fn test_grid_check_exempt_macro() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(101, 200);
    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 400), Orientation::R0);
    top.emit_lef_def(&LefDefOptions {
        check_grid_size: Some((100, 200)),
        macros_exempt_from_grid_check: HashSet::from(["Block".to_string()]),
        ..LefDefOptions::default()
    });
}

#[test]
fn test_grid_check_exempt_inst() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(101, 200);
    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 400), Orientation::R0);
    top.emit_lef_def(&LefDefOptions {
        check_grid_size: Some((100, 200)),
        instances_exempt_from_grid_check: HashSet::from(["Block_i".to_string()]),
        ..LefDefOptions::default()
    });
}

#[test]
#[should_panic(expected = "unplaced pins")]
fn test_missing_pins_for_placed_macro() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);
    block.add_port("a", IO::Input(1));

    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 400), Orientation::R0);

    top.emit_def(&LefDefOptions::default());
}

#[test]
fn test_missing_pins_check_disabled() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);
    block.add_port("a", IO::Input(1));

    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 400), Orientation::R0);

    top.emit_def(&LefDefOptions {
        check_fully_pinned: false,
        ..LefDefOptions::default()
    });
}

#[test]
fn test_missing_pins_exempt_port() {
    let top = ModDef::new("Top");
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);
    block.add_port("a", IO::Input(1));

    let block_inst = top.instantiate(&block, None, None);
    block_inst.place((300, 400), Orientation::R0);

    top.emit_def(&LefDefOptions {
        pins_that_may_be_unplaced: HashSet::from(["a".to_string()]),
        ..LefDefOptions::default()
    });
}

/// A two-bit bus whose pins share an edge on M1.
fn block_with_touching_pins() -> ModDef {
    let block = ModDef::new("Block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(10, 10);
    let bus = block.add_port("bus", IO::Input(2));
    let pin = PhysicalPin::new("M1", Polygon::from_width_height(2, 2));
    bus.bit(0).place(pin.clone());
    bus.bit(1).place(pin.with_translation((2, 0).into()));
    block
}

#[rstest::rstest]
#[case::lef(ModDef::emit_lef)]
#[case::def(ModDef::emit_def)]
#[should_panic(expected = "Module 'Block': bus[0] and bus[1] touch or overlap on M1")]
fn pin_overlap_is_checked_by_default(#[case] emit: fn(&ModDef, &LefDefOptions) -> String) {
    emit(&block_with_touching_pins(), &LefDefOptions::default());
}

#[test]
#[should_panic(expected = "Module 'Block': bus[0] and bus[1] touch or overlap on M1")]
fn combined_output_checks_child_macro_pins() {
    let top = ModDef::new("Top");
    top.instantiate(&block_with_touching_pins(), None, None);
    top.emit_lef_def(&LefDefOptions::default());
}

#[test]
fn pin_overlap_check_can_be_disabled_without_omitting_pins() {
    let block = block_with_touching_pins();
    let opts = LefDefOptions {
        check_for_pin_overlaps: false,
        ..Default::default()
    };
    assert!(block.emit_lef(&opts).contains("PIN bus[1]"));
    assert!(block.emit_def(&opts).contains("PINS 2 ;"));
}

#[test]
fn pin_overlap_is_not_checked_for_omitted_pins() {
    let block = block_with_touching_pins();
    let top = ModDef::new("Top");
    top.instantiate(&block, None, None);
    // Standalone DEF references the child for placement but does not write its pins.
    assert!(
        top.emit_def(&LefDefOptions::default())
            .contains("COMPONENTS 1 ;")
    );

    let opts = LefDefOptions {
        include_pins: false,
        ..Default::default()
    };
    assert!(!block.emit_lef(&opts).contains("PIN bus"));
    assert!(!block.emit_def(&opts).contains("PINS 2 ;"));
}
