// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use topstitch::{IO, LefDefOptions, ModDef, Orientation, PhysicalPin, Polygon, Usage};

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

fn entry_names<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    text.lines()
        .filter_map(|line| line.trim_start().strip_prefix(prefix))
        .map(|entry| entry.split_whitespace().next().unwrap())
        .collect()
}

#[test]
fn emit_macro_and_instance_order() {
    let top = ModDef::new("top");
    for (name, instance, x) in [("Z", "u_z", 20), ("A", "u_a", 0)] {
        let block = ModDef::new(name);
        block.set_width_height(10, 10);
        block.set_usage(Usage::EmitStubAndStop);
        top.instantiate(&block, Some(instance), None)
            .place((x, 0), Orientation::R0);
    }

    let mut opts = LefDefOptions::default();
    let (lef, def) = top.emit_lef_def(&opts);
    assert_eq!(entry_names(&lef, "MACRO "), ["A", "Z"]);
    assert_eq!(entry_names(&def, "- "), ["u_a", "u_z"]);

    opts.sort_output = false;
    let (lef, def) = top.emit_lef_def(&opts);
    assert_eq!(entry_names(&lef, "MACRO "), ["Z", "A"]);
    assert_eq!(entry_names(&def, "- "), ["u_z", "u_a"]);
}

#[test]
fn emit_pin_order() {
    let block = ModDef::new("block");
    block.set_width_height(10, 10);
    for (name, x) in [("z", 2), ("a", 0)] {
        let pin =
            PhysicalPin::from_translation("M1", Polygon::from_width_height(1, 1), (x, 0).into());
        block.add_port(name, IO::Input(1)).bit(0).place(pin);
    }

    let mut opts = LefDefOptions::default();
    assert_eq!(entry_names(&block.emit_lef(&opts), "PIN "), ["a", "z"]);
    assert_eq!(entry_names(&block.emit_def(&opts), "- "), ["a", "z"]);

    opts.sort_output = false;
    assert_eq!(entry_names(&block.emit_lef(&opts), "PIN "), ["z", "a"]);
    assert_eq!(entry_names(&block.emit_def(&opts), "- "), ["z", "a"]);
}
