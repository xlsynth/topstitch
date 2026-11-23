// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use topstitch::{
    IO, LefDefOptions, ModDef, Orientation, PackedLayout, PackedPinNaming, PhysicalPin, Polygon,
    Usage,
};

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

#[test]
fn packed_layout_with_a_flat_declaration_keeps_flat_physical_pins() {
    let block = ModDef::new("Grid");
    let layout = PackedLayout::array_of(&PackedLayout::bits(2), 2);
    block.add_port("lanes", layout.input());
    place_test_pins(&block);

    let options = LefDefOptions {
        packed_pin_naming: PackedPinNaming::Split {
            index_prefix: "_".into(),
            index_suffix: String::new(),
        },
        ..Default::default()
    };
    let pins = ["lanes[0]", "lanes[1]", "lanes[2]", "lanes[3]"];
    assert_eq!(entry_names(&block.emit_lef(&options), "PIN "), pins);
    assert_eq!(entry_names(&block.emit_def(&options), "- "), pins);
}

#[test]
fn split_physical_pins_follow_the_actual_port_declaration() {
    let source = ModDef::from_verilog(
        "Imported",
        "module Imported(input logic [1:0][3:0] lanes); endmodule",
    )
    .unwrap();
    let native = ModDef::new("Native");
    native.add_port_array("lanes", IO::Input(4), [2]);
    let copy = ModDef::new("Copy");
    source.get_port("lanes").copy_to(&copy);
    let wrapper = source.wrap(Some("Wrapper"), Some("u_imported"));

    let options = LefDefOptions {
        packed_pin_naming: PackedPinNaming::Split {
            index_prefix: "_".into(),
            index_suffix: String::new(),
        },
        ..Default::default()
    };
    let split = [
        "lanes_0[0]",
        "lanes_0[1]",
        "lanes_0[2]",
        "lanes_0[3]",
        "lanes_1[0]",
        "lanes_1[1]",
        "lanes_1[2]",
        "lanes_1[3]",
    ];
    let flat = [
        "lanes[0]", "lanes[1]", "lanes[2]", "lanes[3]", "lanes[4]", "lanes[5]", "lanes[6]",
        "lanes[7]",
    ];
    for block in [&source, &native, &copy, &wrapper] {
        place_test_pins(block);
        let expected = if block.get_name() == "Imported" || block.get_name() == "Native" {
            &split
        } else {
            &flat
        };
        assert_eq!(entry_names(&block.emit_lef(&options), "PIN "), expected);
        if block.get_name() != "Wrapper" {
            assert_eq!(entry_names(&block.emit_def(&options), "- "), expected);
        }
    }

    source.set_usage(Usage::EmitStubAndStop);
    assert_eq!(entry_names(&source.emit_lef(&options), "PIN "), split);
}

#[test]
fn imported_split_pins_use_declared_outer_indices() {
    let source = ModDef::from_verilog(
        "Imported",
        "module Imported(input logic [1:3][1:0] lanes); endmodule",
    )
    .unwrap();
    place_test_pins(&source);
    let options = LefDefOptions {
        packed_pin_naming: PackedPinNaming::Split {
            index_prefix: "_".into(),
            index_suffix: String::new(),
        },
        ..Default::default()
    };
    let expected = [
        "lanes_3[0]",
        "lanes_3[1]",
        "lanes_2[0]",
        "lanes_2[1]",
        "lanes_1[0]",
        "lanes_1[1]",
    ];
    assert_eq!(entry_names(&source.emit_lef(&options), "PIN "), expected);
    assert_eq!(entry_names(&source.emit_def(&options), "- "), expected);
}

#[test]
#[should_panic(expected = "outer range [-1:1] contains negative indices")]
fn split_physical_pins_reject_negative_outer_indices() {
    let source = ModDef::from_verilog(
        "Imported",
        "module Imported(input logic [-1:1][1:0] lanes); endmodule",
    )
    .unwrap();
    place_test_pins(&source);
    source.emit_def(&split_options());
}

#[test]
#[should_panic(expected = "innermost vector range [5:2] must descend to zero")]
fn split_physical_pins_reject_nonzero_vector_bounds() {
    let source = ModDef::from_verilog(
        "Imported",
        "module Imported(input logic [1:0][5:2] lanes); endmodule",
    )
    .unwrap();
    place_test_pins(&source);
    source.emit_lef(&split_options());
}

#[test]
#[should_panic(expected = "innermost vector range [2:5] must descend to zero")]
fn split_physical_pins_reject_ascending_vector_bounds() {
    let source = ModDef::from_verilog(
        "Imported",
        "module Imported(input logic [1:0][2:5] lanes); endmodule",
    )
    .unwrap();
    place_test_pins(&source);
    source.emit_def(&split_options());
}

#[test]
fn flat_physical_pins_still_use_normalized_offsets_for_unusual_source_bounds() {
    let source = ModDef::from_verilog(
        "Imported",
        "module Imported(input logic [-1:1][5:2] lanes); endmodule",
    )
    .unwrap();
    place_test_pins(&source);
    let pins: Vec<_> = (0..12).map(|bit| format!("lanes[{bit}]")).collect();
    assert_eq!(
        entry_names(&source.emit_lef(&LefDefOptions::default()), "PIN "),
        pins
    );
}

/// Selects the synthesis-style underscore convention for packed word ports.
fn split_options() -> LefDefOptions {
    LefDefOptions {
        packed_pin_naming: PackedPinNaming::Split {
            index_prefix: "_".into(),
            index_suffix: "_".into(),
        },
        ..Default::default()
    }
}

#[test]
fn imported_split_pins_keep_an_opaque_word_together() {
    let source = ModDef::from_verilog(
        "Imported",
        "typedef byte byte_t; module Imported(input byte_t [1:0] lanes); endmodule",
    )
    .unwrap();
    place_test_pins(&source);
    let options = LefDefOptions {
        packed_pin_naming: PackedPinNaming::Split {
            index_prefix: "_".into(),
            index_suffix: String::new(),
        },
        ..Default::default()
    };
    let expected: Vec<_> = (0..2)
        .flat_map(|word| (0..8).map(move |bit| format!("lanes_{word}[{bit}]")))
        .collect();
    assert_eq!(entry_names(&source.emit_lef(&options), "PIN "), expected);
    assert_eq!(entry_names(&source.emit_def(&options), "- "), expected);
}

#[test]
fn split_physical_port_names_round_trip_as_literals() {
    let block = ModDef::new("Grid");
    block.add_port_array("flags", IO::Input(1), [2, 1]);
    block.add_port_array("lanes", IO::Input(2), [2]);
    place_test_pins(&block);

    // Literal brackets, hierarchy separators, and backslashes belong to the word name.
    for (prefix, suffix) in [("[", "]"), ("/", "\\")] {
        let split = LefDefOptions {
            packed_pin_naming: PackedPinNaming::Split {
                index_prefix: prefix.into(),
                index_suffix: suffix.into(),
            },
            ..Default::default()
        };
        for (imported, is_def) in [
            (ModDef::from_lef(block.emit_lef(&split), &split), false),
            (ModDef::from_def(block.emit_def(&split), &split), true),
        ] {
            // Physical import records literal port names, so both export policies must work.
            for options in [&split, &LefDefOptions::default()] {
                let again = if is_def {
                    ModDef::from_def(imported.emit_def(options), options)
                } else {
                    ModDef::from_lef(imported.emit_lef(options), options)
                };
                assert_eq!(again.get_ports(None).len(), 4);
                for (name, width) in [("flags", 1), ("lanes", 2)] {
                    for word in 0..2 {
                        let port = again.get_port(format!("{name}{prefix}{word}{suffix}"));
                        assert!(matches!(port.io(), IO::Input(w) if w == width));
                        for bit in 0..width {
                            assert_eq!(
                                port.bit(bit).get_physical_pin().translation(),
                                block
                                    .get_physical_pin(name, word * width + bit)
                                    .translation()
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn split_physical_port_names_report_both_owners_of_a_collision() {
    for (name, width, prefix, suffix) in [
        ("lanes_0", 1, "_", ""),
        ("lanes_0", 2, "_", ""),
        ("lanes[0]", 2, "[", "]"),
    ] {
        let block = ModDef::new("Grid");
        block.add_port_array("lanes", IO::Input(2), [2]);
        block.add_port(name, IO::Input(width));
        place_test_pins(&block);
        let options = LefDefOptions {
            packed_pin_naming: PackedPinNaming::Split {
                index_prefix: prefix.into(),
                index_suffix: suffix.into(),
            },
            ..Default::default()
        };
        for emit in [ModDef::emit_lef, ModDef::emit_def] {
            let err =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| emit(&block, &options)))
                    .expect_err("colliding word and ordinary port");
            assert_eq!(
                err.downcast_ref::<String>().unwrap(),
                &format!(
                    "physical port name '{name}' is shared by 'lanes' word 0 and '{name}' word 0"
                )
            );
        }
    }
}

#[test]
#[should_panic(
    expected = "physical port name 'lanes111' is shared by 'lanes' word 23 and 'lanes' word 133"
)]
fn split_physical_port_names_reject_ambiguous_indices() {
    let block = ModDef::new("Grid");
    block.set_width_height(10, 10);
    let lanes = block.add_port_array("lanes", IO::Input(2), [12, 12]);
    for (indices, x) in [([1, 11, 0], 0), ([11, 1, 1], 2)] {
        lanes
            .element_at(indices)
            .place(PhysicalPin::from_translation(
                "M1",
                Polygon::from_width_height(1, 1),
                (x, 0).into(),
            ));
    }
    block.emit_lef(&LefDefOptions {
        packed_pin_naming: PackedPinNaming::Split {
            index_prefix: String::new(),
            index_suffix: String::new(),
        },
        check_fully_pinned: false,
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "packed pin index separators cannot contain whitespace")]
fn split_physical_port_names_reject_whitespace_in_separators() {
    let block = ModDef::new("Grid");
    block.add_port_array("lanes", IO::Input(2), [2]);
    place_test_pins(&block);
    block.emit_def(&LefDefOptions {
        packed_pin_naming: PackedPinNaming::Split {
            index_prefix: " ".into(),
            index_suffix: "".into(),
        },
        ..Default::default()
    });
}

#[test]
#[should_panic(
    expected = "packed pin index separators cannot contain whitespace or control characters"
)]
fn split_physical_port_names_reject_control_characters_in_separators() {
    let block = ModDef::new("Grid");
    block.add_port_array("lanes", IO::Input(2), [2]);
    place_test_pins(&block);
    block.emit_def(&LefDefOptions {
        packed_pin_naming: PackedPinNaming::Split {
            index_prefix: "_".into(),
            index_suffix: "\0".into(),
        },
        ..Default::default()
    });
}

/// Places one pin at a distinct location for every logical bit in the block.
fn place_test_pins(block: &ModDef) {
    block.set_width_height(32, 32);
    let mut x = 0;
    for port in block.get_ports(None) {
        for bit in 0..port.io().width() {
            let pin = PhysicalPin::from_translation(
                "M1",
                Polygon::from_width_height(1, 1),
                (x, 0).into(),
            );
            port.bit(bit).place(pin);
            x += 2;
        }
    }
}
