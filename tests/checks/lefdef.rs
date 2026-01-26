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
