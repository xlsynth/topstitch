// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use topstitch::{LefDefOptions, ModDef, Orientation, Usage};

fn target_out(sub: &str) -> PathBuf {
    let base = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let mut p = PathBuf::from(base);
    p.push("test-out");
    p.push(sub);
    let _ = fs::create_dir_all(p.parent().unwrap());
    p
}

#[test]
fn emit_lef_for_single_module() {
    let block = ModDef::new("block");
    block.set_width_height(100, 200);
    block.set_layer("OUTLINE");

    let opts = LefDefOptions::default();
    let lef = block.emit_lef(&opts);
    assert!(lef.contains("MACRO block"));
    assert!(lef.contains("SIZE 100 BY 200 ;"));

    let lef_path = target_out("single_block.lef");
    block
        .emit_lef_to_file(&lef_path, &opts)
        .expect("emit single block LEF");
    let lef_disk = fs::read_to_string(&lef_path).unwrap();
    assert_eq!(lef, lef_disk);
}

#[test]
fn emit_lef_and_def_strings_and_files() {
    let top = ModDef::new("top");
    let block = ModDef::new("block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);

    let b = top.instantiate(&block, Some("b0"), None);
    b.place((10, 20), Orientation::R0);

    let opts = LefDefOptions {
        omit_top_module_in_hierarchy: false,
        divider_char: ".".to_string(),
        ..LefDefOptions::default()
    };
    let (lef, def) = top.emit_lef_def(&opts);
    assert!(lef.contains("MACRO block"));
    assert!(lef.contains("SIZE 100 BY 200 ;"));
    assert!(def.contains("DESIGN top ;"));
    assert!(def.contains("- top.b0 block + PLACED ( 10 20 ) N ;"));

    // Write files and verify content round-trip
    let lef_path = target_out("out.lef");
    let def_path = target_out("out.def");
    let _ = top.emit_lef_def_to_files(&lef_path, &def_path, &opts);
    let lef2 = fs::read_to_string(&lef_path).unwrap();
    let def2 = fs::read_to_string(&def_path).unwrap();
    assert_eq!(lef, lef2);
    assert_eq!(def, def2);
}

#[test]
fn emit_def_with_orientation_e() {
    let top = ModDef::new("top");
    let block = ModDef::new("block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(12, 34);

    let b = top.instantiate(&block, Some("b1"), None);
    b.place((56, 78), Orientation::R270);

    let (_, def) = top.emit_lef_def(&LefDefOptions::default());
    assert!(def.contains("- b1 block + PLACED ( 56 66 ) E ;"));
}
