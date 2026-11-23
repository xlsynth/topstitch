// SPDX-License-Identifier: Apache-2.0

use topstitch::{ConvertibleToPortSlice, EmitOptions, IO, ModDef};

#[test]
fn bit_dimensions_are_indexable_across_native_imported_and_flat_layouts() {
    use topstitch::PackedLayout;

    let native = ModDef::new("Native");
    let native_port = native.add_port_array("lanes", IO::Input(8), [3]);
    let layout_port = native.add_port(
        "layout",
        PackedLayout::array_of(&PackedLayout::bits(8), 3).input(),
    );
    let imported = ModDef::from_verilog(
        "Imported",
        "typedef byte byte_t; module Imported(input logic [2:0][7:0] lanes, input byte_t [2:0] bytes); endmodule",
    )
    .unwrap();
    for port in [
        native_port,
        layout_port,
        imported.get_port("lanes"),
        imported.get_port("bytes"),
    ] {
        assert_eq!(port.packed_dimensions(), [3, 8]);
        assert_eq!(port.element_at([2, 7]).to_port_slice(), port.bit(23));
        assert_eq!(port.element(2).elements().len(), 8);
    }

    let flat = native.add_port("flat", IO::Input(8));
    assert_eq!(flat.packed_dimensions(), [8]);
    assert_eq!(flat.element(7).to_port_slice(), flat.bit(7));
    assert_eq!(flat.elements().len(), 8);

    let bits = PackedLayout::bits(8);
    assert_eq!(bits.element(7).range_in(&bits).lsb(), 7);
}

#[test]
fn imported_array_stubs_preserve_the_original_packed_declaration() {
    use topstitch::Usage;

    let leaf = ModDef::from_verilog(
        "Leaf",
        "module Leaf(input logic [2:0][7:0] lanes); endmodule",
    )
    .unwrap();
    let clone = leaf.stub("Clone");
    clone.set_usage(Usage::EmitStubAndStop);
    let copy = ModDef::new("Copy");
    leaf.get_port("lanes").copy_to(&copy);
    copy.set_usage(Usage::EmitStubAndStop);
    let wrapper = leaf.wrap(Some("Wrapper"), None);
    assert!(
        wrapper
            .emit(Default::default())
            .contains("input wire [23:0] lanes")
    );
    assert!(
        copy.emit(Default::default())
            .contains("input wire [23:0] lanes")
    );

    leaf.set_usage(Usage::EmitStubAndStop);
    for stub in [&leaf, &clone] {
        let rtl = stub.emit(Default::default());
        assert!(rtl.contains("input wire [2:0][7:0] lanes"), "{rtl}");
        let parent = format!(
            "{rtl} module Parent(input logic [2:0][7:0] lanes, output logic q); \
             {} u(.lanes(lanes)); assign q = u.lanes[1][2]; endmodule",
            stub.get_name()
        );
        ModDef::from_verilog("Parent", parent).unwrap();
    }
}

#[test]
#[should_panic(expected = "range [5:2] must descend to zero")]
fn imported_array_stubs_reject_unrepresentable_vector_bounds() {
    let leaf = ModDef::from_verilog(
        "Leaf",
        "module Leaf(input logic [1:0][5:2] lanes); endmodule",
    )
    .unwrap();
    leaf.set_usage(topstitch::Usage::EmitStubAndStop);
    leaf.emit(Default::default());
}

#[test]
#[should_panic(expected = "range [-1:1] must descend to zero")]
fn imported_array_stubs_reject_negative_indices() {
    let leaf = ModDef::from_verilog(
        "Leaf",
        "module Leaf(input logic [-1:1][3:0] lanes); endmodule",
    )
    .unwrap();
    leaf.set_usage(topstitch::Usage::EmitStubAndStop);
    leaf.emit(Default::default());
}

#[test]
fn packed_array_words_and_cross_word_slices_preserve_sv_index_order() {
    let top = ModDef::new("PackedWords");
    let words = top.add_port_array("words", IO::Input(4), [3]).elements();
    assert_eq!(words.len(), 3);
    for (i, word) in words.iter().enumerate() {
        assert_eq!(
            word.to_port_slice(),
            top.get_port("words").slice(4 * i + 3, 4 * i)
        );
    }
    let outputs = top
        .add_port_array("reversed", IO::Output(4), [3])
        .elements();
    for (output, input) in outputs.iter().zip(words.iter().rev()) {
        output.connect(input);
    }
    top.add_port("middle", IO::Output(6))
        .connect(&top.get_port("words").slice(8, 3));

    top.add_port("upper", IO::Output(8))
        .connect(&top.get_port("words").slice(11, 4));

    let flags = top.add_port_array("flags", IO::Input(1), [3]).elements();
    let mirrored = top.add_port("mirrored", IO::Output(3));
    for (i, flag) in flags.iter().rev().enumerate() {
        mirrored.bit(i).connect(flag);
    }
    let wrapper = top.wrap(Some("Wrapper"), None);
    for bitblast_assignments in [false, true] {
        for preserve_full_width_slices in [false, true] {
            for preserve_single_bit_slices in [false, true] {
                let rtl = wrapper.emit(EmitOptions {
                    bitblast_assignments,
                    preserve_full_width_slices,
                    preserve_single_bit_slices,
                    ..Default::default()
                });
                if preserve_full_width_slices && !preserve_single_bit_slices {
                    assert!(rtl.contains("mirrored[0] = flags[2];"), "{rtl}");
                }
                ModDef::from_verilog("Wrapper", rtl).unwrap();
            }
        }
    }
    let sv = wrapper.emit(Default::default());
    assert_eq!(sv.matches("input wire [2:0][3:0] words").count(), 2);
    assert_eq!(sv.matches("output wire [2:0][3:0] reversed").count(), 2);
    assert!(sv.contains("assign reversed[0] = words[2];"), "{sv}");
    assert!(sv.contains("assign upper = words[2:1];"), "{sv}");
    assert!(sv.contains("{words[2][0], words[1], words[0][3]}"), "{sv}");
}

#[test]
fn misaligned_slices_group_consecutive_whole_words_on_both_sides() {
    let top = ModDef::new("SwapEnds");
    let input = top.add_port_array("din", IO::Input(8), [64]);
    let output = top.add_port_array("dout", IO::Output(8), [64]);
    output.slice(510, 1).connect(&input.slice(510, 1));
    output.bit(511).connect(&input.bit(0));
    output.bit(0).connect(&input.bit(511));

    let sv = top.emit(Default::default());
    assert!(sv.contains(
        "assign {dout[63][6:0], dout[62:1], dout[0][7:1]} = {din[63][6:0], din[62:1], din[0][7:1]};"
    ), "{sv}");
}

#[test]
fn whole_port_exports_preserve_packed_dimensions_and_slices_are_flat() {
    let child = ModDef::new("Child");
    child.add_port_array("lanes", IO::Input(8), [5]);
    child.add_port_array("more", IO::Output(8), [5]);
    child.set_usage(topstitch::Usage::EmitStubAndStop);
    let top = ModDef::new("Top");
    let inst = top.instantiate(&child, Some("child"), None);
    inst.get_port("lanes").export();
    inst.get_port("more").slice(12, 5).export_as("part");
    inst.get_port("more").slice(4, 0).unused();
    inst.get_port("more").slice(39, 13).unused();
    let sv = top.emit(Default::default());
    assert_eq!(sv.matches("input wire [4:0][7:0] lanes").count(), 2);
    assert!(sv.contains("output wire [7:0] part"));
}

#[test]
fn whole_port_selected_row_and_raw_slice_keep_distinct_declarations() {
    let source = ModDef::new("Source");
    source.add_port_array("words", IO::Output(8), [1, 3]);
    source.set_usage(topstitch::Usage::EmitStubAndStop);
    let top = ModDef::new("Top");
    let words = top.instantiate(&source, None, None).get_port("words");
    let row = words.element(0);
    let bits = row.to_port_slice();
    assert_eq!(bits, words.to_port_slice());

    words.export_as("whole");
    row.export_as("row");
    bits.export_as("raw");
    let sv = top.emit(Default::default());
    for declaration in [
        "output wire [0:0][2:0][7:0] whole",
        "output wire [2:0][7:0] row",
        "output wire [23:0] raw",
    ] {
        assert!(sv.contains(declaration), "{sv}");
    }
    ModDef::from_verilog("Top", sv).unwrap();

    let pass = ModDef::new("Pass");
    row.feedthrough(&pass, "in_row", "out_row");
    let sv = pass.emit(Default::default());
    assert!(sv.contains("input wire [2:0][7:0] in_row"), "{sv}");
    assert!(sv.contains("output wire [2:0][7:0] out_row"), "{sv}");
}

#[test]
fn feedthroughs_preserve_whole_array_shapes_and_flatten_partial_slices() {
    let source = ModDef::new("Source");
    source.add_port_array("words", IO::Input(8), [3]);
    let words = source.get_port("words");
    let pass = ModDef::new("Pass");
    words.feedthrough(&pass, "all_out", "all_in");
    words.slice(12, 5).feedthrough(&pass, "part_out", "part_in");
    let sv = pass.emit(Default::default());
    for pin in ["all_out", "all_in"] {
        assert!(sv.contains(&format!("[2:0][7:0] {pin}")), "{sv}");
    }
    for pin in ["part_out", "part_in"] {
        assert!(sv.contains(&format!("[7:0] {pin}")), "{sv}");
    }
    ModDef::from_verilog("Pass", sv).unwrap();
}

#[test]
fn imported_packed_ranges_keep_logical_indices_on_a_flat_wrapper() {
    let source = r"package grid_types;
        typedef enum logic [1:0] {IDLE, RUN} mode_t;
        endpackage
        module Grid #(parameter ROWS=2, COLS=3)
        (input logic [ROWS-1:0][COLS-1:0][7:0] lanes, input logic [0:1][7:0] ascending,
         input logic [3:2][7:0] shifted, input logic enable, input grid_types::mode_t mode);
        endmodule";
    let leaf = ModDef::from_verilog("Grid", source).unwrap();
    for (rows, cols) in [(2, 3), (1, 4)] {
        let variant = leaf.parameterize(&[("ROWS", rows), ("COLS", cols)]);
        let wrapper = variant.wrap(Some("Wrapped"), None);
        let lanes = wrapper.get_port("lanes");
        let (rows, cols) = (rows as usize, cols as usize);
        assert_eq!(lanes.packed_dimensions(), [rows, cols, 8]);
        assert!(wrapper.get_port("enable").packed_dimensions().is_empty());
        assert_eq!(lanes.element(rows - 1).width(), cols * 8);
        assert_eq!(lanes.element(0).element(1).bit(2), lanes.bit(10));
        let ascending = wrapper.get_port("ascending");
        assert_eq!(ascending.element(1).to_port_slice(), ascending.slice(7, 0));
        let shifted = wrapper.get_port("shifted");
        assert_eq!(shifted.element(3).to_port_slice(), shifted.slice(15, 8));
        let sv = wrapper.emit(Default::default());
        assert!(
            sv.contains(&format!("input wire [{}:0] lanes", rows * cols * 8 - 1)),
            "{sv}"
        );
        assert!(sv.contains("input wire [15:0] ascending"), "{sv}");
        assert!(sv.contains("input wire [15:0] shifted"), "{sv}");
        assert!(sv.contains("input wire enable"), "{sv}");
        assert!(sv.contains(".mode(grid_types::mode_t'(mode))"), "{sv}");
        ModDef::from_verilog("Wrapped", format!("{source}\n{sv}")).unwrap();
    }
}

#[test]
fn flat_slices_cross_multiple_packed_dimensions_on_both_sides() {
    let module = ModDef::new("Packed");
    let input = module.add_port_array("din", IO::Input(4), [2, 1, 3]);
    let output = module.add_port_array("dout", IO::Output(4), [2, 3]);
    assert_eq!(input.packed_dimensions(), [2, 1, 3, 4]);
    assert_eq!(
        input.element_at([1, 0, 1]).to_port_slice(),
        input.slice(19, 16)
    );
    output.slice(22, 0).connect(&input.slice(23, 1));
    output.bit(23).connect(&input.bit(0));
    for bitblast_assignments in [false, true] {
        let rtl = module.emit(EmitOptions {
            bitblast_assignments,
            ..Default::default()
        });
        assert!(rtl.contains("[1:0][0:0][2:0][3:0] din"), "{rtl}");
        assert!(rtl.contains("din[0][0][0][0]"), "{rtl}");
        let parsed = ModDef::from_verilog("Packed", rtl).unwrap();
        let din = parsed.get_port("din");
        assert_eq!(din.element_at([1, 0, 1]).to_port_slice(), din.slice(19, 16));
        let dout = parsed.get_port("dout");
        assert_eq!(
            dout.element(1).element(1).to_port_slice(),
            dout.slice(19, 16)
        );
        assert_eq!(dout.element(1).element(1).bit(2), dout.bit(18));
    }
}

#[test]
fn physical_exports_name_the_same_bit_as_flat_or_split_netlist_ports() {
    use topstitch::{BoundingBox, LefDefOptions, PackedPinNaming, PhysicalPin, Polygon};

    let module = ModDef::new("Grid");
    module.set_width_height(100, 100);
    module.set_layer("OUTLINE");
    let lanes = module.add_port_array("lanes", IO::Input(8), [2, 3]);
    let shape = Polygon::from_bbox(&BoundingBox {
        min_x: 0,
        min_y: 0,
        max_x: 1,
        max_y: 1,
    });
    for bit in 0..48 {
        lanes.bit(bit).place(PhysicalPin::from_translation(
            "M1",
            shape.clone(),
            (2 * bit as i64, 0).into(),
        ));
    }
    for (naming, chars, pin, bus, offset) in [
        (PackedPinNaming::Flat, "[]", "lanes[24]", "lanes", 24),
        (
            PackedPinNaming::Split {
                index_prefix: "_".into(),
                index_suffix: "".into(),
            },
            "<>",
            "lanes_1_0<0>",
            "lanes_1_0",
            0,
        ),
        (
            PackedPinNaming::Split {
                index_prefix: "[".into(),
                index_suffix: "]".into(),
            },
            "[]",
            r"lanes\[1\]\[0\][0]",
            "lanes[1][0]",
            0,
        ),
    ] {
        let opts = LefDefOptions {
            packed_pin_naming: naming,
            bus_bit_chars: chars.into(),
            ..Default::default()
        };
        let lef = module.emit_lef(&opts);
        let def = module.emit_def(&opts);
        assert!(lef.contains(&format!("PIN {pin}\n")), "{lef}");
        assert!(def.contains(&format!("- {pin} ")), "{def}");
        for imported in [ModDef::from_lef(lef, &opts), ModDef::from_def(def, &opts)] {
            assert_eq!(
                imported
                    .get_port(bus)
                    .bit(offset)
                    .get_physical_pin()
                    .translation(),
                lanes
                    .element_at([1, 0])
                    .bit(0)
                    .get_physical_pin()
                    .translation()
            );
        }
    }
}
