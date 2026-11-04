// SPDX-License-Identifier: Apache-2.0

use topstitch::{
    ConvertibleToPortSlice, EmitOptions, IO, ModDef, PackedLayout, extract_packages_from_verilog,
};

const COLORS: &str = "
package paint;
    typedef struct packed { logic [1:0] shade; logic bright; } ink_t;
    typedef struct packed { logic valid; ink_t ink; } pixel_t;
    typedef pixel_t swatch_t;
endpackage
module Palette(input paint::swatch_t pixel, input logic [3:0] raw);
endmodule";

#[test]
fn wires_an_imported_struct_by_name() {
    let palette = ModDef::from_verilog("Palette", COLORS, false, false);
    let top = ModDef::new("Top");
    let inst = top.instantiate(&palette, Some("palette"), None);
    let pixel = inst.get_port("pixel");
    let ink = pixel.field("ink");
    assert_eq!(ink.field("shade").to_port_slice(), pixel.slice(2, 1));
    assert_eq!(
        pixel.slice(3, 0).field("valid").to_port_slice(),
        pixel.bit(3)
    );
    assert_eq!(
        palette
            .stub("Stub")
            .get_port("pixel")
            .field("ink")
            .to_port_slice()
            .lsb(),
        0
    );

    let shade = top.add_port("shade", IO::Input(2));
    ink.field("shade").connect(&shade);
    ink.field("bright").tieoff(1);
    pixel.field("valid").tieoff(0);
    inst.get_port("raw").tieoff(0);
    let rtl = top.emit(EmitOptions::default());
    assert!(rtl.contains("{1'h0, shade, 1'h1}"), "{rtl}");
}

#[test]
fn whole_port_exports_keep_the_layout_through_three_flat_boundaries() {
    let palette = ModDef::from_verilog("Palette", COLORS, false, false);
    let canvas = ModDef::new("Canvas");
    let leaf = canvas.instantiate(&palette, None, None);
    leaf.get_port("raw").tieoff(0);
    leaf.get_port("pixel").slice(3, 0).export_as("color");

    let frame = ModDef::new("Frame");
    frame
        .instantiate(&canvas, None, None)
        .get_port("color")
        .export();
    let display = ModDef::new("Display");
    let color = display
        .instantiate(&frame, None, None)
        .get_port("color")
        .export();
    assert_eq!(
        color.field("ink").field("shade").to_port_slice(),
        color.slice(2, 1)
    );

    // Exporting from the module definition mirrors its input to an output.
    let monitor = color.export_as("monitor");
    assert_eq!(monitor.field("valid").to_port_slice(), monitor.bit(3));

    let rtl = display.emit(EmitOptions::default());
    assert_eq!(rtl.matches("input wire [3:0] color").count(), 3, "{rtl}");
    assert!(rtl.contains("output wire [3:0] monitor"), "{rtl}");
}

#[test]
fn autoconnect_keeps_the_layout_only_on_new_parent_ports() {
    let leaf = ModDef::from_verilog(
        "Leaf",
        "package p; typedef struct packed { logic valid; logic [2:0] tag; } item_t; endpackage
         module Leaf(input p::item_t [1:0] items, input p::item_t raw); endmodule",
        false,
        false,
    );
    let top = ModDef::new("Top");
    top.add_port("raw", IO::Input(4));
    top.instantiate(&leaf, None, Some(&["items", "raw"]));

    let items = top.get_port("items");
    assert_eq!(
        items.element(1).field("valid").to_port_slice(),
        items.bit(7)
    );
    assert!(top.get_port("raw").packed_view().is_none());
}

#[test]
fn exporting_bits_does_not_infer_a_struct_layout() {
    let palette = ModDef::from_verilog("Palette", COLORS, false, false);
    let top = ModDef::new("Top");
    let leaf = top.instantiate(&palette, None, None);
    // This numeric slice exactly covers the nested ink member.
    let bits = leaf.get_port("pixel").slice(2, 0).export_as("ink_bits");
    assert!(bits.packed_view().is_none());
    let raw = leaf.get_port("raw").export();
    assert_eq!(raw.element(3).to_port_slice(), raw.bit(3));
}

#[test]
fn parameterized_instances_use_their_own_layout_even_when_width_is_unchanged() {
    let module = ModDef::from_verilog(
        "Packet",
        "
        module Packet #(parameter int PAD = 2) (
            output struct packed {
                logic [7-PAD:0] upper;
                struct packed { logic valid; logic [1:0] tag; } header;
                logic [PAD-1:0] lower;
            } data
        ); endmodule",
        false,
        false,
    );
    let top = ModDef::new("Top");
    for (name, variant, pad) in [
        ("original", module.clone(), 2),
        ("shifted", module.parameterize(&[("PAD", 5)]), 5),
        ("cached", module.parameterize(&[("PAD", 5)]), 5),
    ] {
        let port = top.instantiate(&variant, Some(name), None).get_port("data");
        assert_eq!(port.io().width(), 11);
        assert_eq!(
            port.field("header").field("tag").to_port_slice(),
            port.slice(pad + 1, pad)
        );
        assert_eq!(
            port.field("header").field("valid").to_port_slice(),
            port.bit(pad + 2)
        );
    }
}

#[test]
fn width_and_partial_slices_do_not_imply_a_type() {
    let module = ModDef::from_verilog("Palette", COLORS, false, false);
    let raw = module.get_port("raw");
    let pixel = module.get_port("pixel");
    assert_eq!(raw.element(0).to_port_slice(), raw.bit(0));
    assert!(
        ModDef::new("Flat")
            .add_port("pixel", IO::Input(4))
            .packed_view()
            .is_none()
    );
    // Even the exact bit range of "ink" has no declared type when selected by number.
    assert!(pixel.slice(2, 0).packed_view().is_none());
    let packages = extract_packages_from_verilog(COLORS, false).unwrap();
    let ink = packages["paint"]
        .packed_layout("ink_t")
        .on(&raw.slice(3, 1));
    assert_eq!(ink.field("shade").to_port_slice(), raw.slice(3, 2));
}

#[test]
#[should_panic(expected = "not a whole port with a packed layout")]
fn partial_slice_rejects_field_selection() {
    let module = ModDef::from_verilog("Palette", COLORS, false, false);
    module.get_port("pixel").slice(2, 0).field("bright");
}

#[test]
fn arrays_of_structs_can_select_fields_after_indexing() {
    let module = ModDef::from_verilog(
        "Pairs",
        "
        package p; typedef struct packed { logic flag; logic data; } item_t; endpackage
        module Pairs(input p::item_t [1:0] pair); endmodule",
        false,
        false,
    );
    let pair = module.get_port("pair");
    assert_eq!(pair.io().width(), 4);
    assert_eq!(pair.element(1).field("flag").to_port_slice(), pair.bit(3));
    assert_eq!(pair.element(0).field("data").to_port_slice(), pair.bit(0));
}

const ARRAYS: &str = "
package bank;
    typedef struct packed { logic flag; logic [1:0] tag; } item_t;
    typedef item_t [1:-1] row_t;
    typedef row_t [2:3] grid_t;
    typedef struct packed { logic valid; grid_t cells; logic [1:0] tail; } frame_t;
endpackage
module Bank #(parameter int N = 1) (
    output bank::item_t [N-1:0] items,
    input bank::frame_t frame,
    input logic [4:5][-2:0] mask
); endmodule";

#[test]
fn declared_indices_survive_aliases_and_apply_one_dimension_at_a_time() {
    let m = ModDef::from_verilog("Bank", ARRAYS, false, false);
    let port = m.get_port("frame");
    let cells = port.field("cells");
    assert_eq!(
        cells.element(2).element(-1).field("flag").to_port_slice(),
        port.bit(13)
    );
    assert_eq!(
        cells.element(3).element(1).field("tag").to_port_slice(),
        port.slice(9, 8)
    );

    // On both ascending dimensions the rightmost index is nearest the low bit.
    let mask = m.get_port("mask");
    assert_eq!(mask.element(5).element(0).to_port_slice(), mask.bit(0));
    assert_eq!(mask.element(4).element(-2).to_port_slice(), mask.bit(5));

    let pkgs = extract_packages_from_verilog(ARRAYS, false).unwrap();
    let grid = pkgs["bank"].packed_layout("grid_t");
    let row = grid.element(2);
    let flag = row.element(-1).field("flag");
    assert_eq!(flag.range_in_parent(), topstitch::BitRange::new(2, 2));
    assert_eq!(flag.range_in(&row), topstitch::BitRange::new(2, 2));
    assert_eq!(flag.range_in_root(), topstitch::BitRange::new(11, 11));
    assert_eq!(
        grid.on(&port.slice(19, 2))
            .element(2)
            .element(-1)
            .field("flag")
            .to_port_slice(),
        port.bit(13)
    );
}

#[test]
#[should_panic(expected = "does not contain")]
fn different_occurrences_of_an_array_element_are_not_ancestors() {
    let pkgs = extract_packages_from_verilog(ARRAYS, false).unwrap();
    let grid = pkgs["bank"].packed_layout("grid_t");
    grid.element(2)
        .element(0)
        .field("tag")
        .range_in(&grid.element(3));
}

#[test]
fn generated_port_keeps_its_layout_after_the_package_is_dropped() {
    let top = ModDef::new("Generated");
    let (grid, entry) = {
        let pkgs = extract_packages_from_verilog(ARRAYS, false).unwrap();
        let layout = pkgs["bank"].packed_layout("grid_t");
        (
            top.add_port("grid", layout.input()),
            top.add_port("entry", layout.element(2).element(0).output()),
        )
    };
    assert_eq!(grid.io().width(), 18);
    assert_eq!(entry.field("flag").to_port_slice(), entry.bit(2));
    entry.connect(&grid.element(3).element(1));
    grid.slice(17, 9).unused();
    grid.slice(5, 0).unused();
    let rtl = top.emit(EmitOptions::default());
    assert!(rtl.contains("input wire [17:0] grid"), "{rtl}");
    assert!(rtl.contains("output wire [2:0] entry"), "{rtl}");
    assert!(rtl.contains("assign entry = grid[8:6]"), "{rtl}");
}

#[test]
fn exporting_an_element_preserves_its_shape_including_singleton_arrays() {
    let module = ModDef::from_verilog("Bank", ARRAYS, false, false);
    let top = ModDef::new("Top");
    for n in [1, 3] {
        let variant = module.parameterize(&[("N", n)]);
        let inst = top.instantiate(&variant, Some(&format!("bank{n}")), None);
        let items = inst.get_port("items");
        let i = n - 1;
        assert_eq!(
            items.element(i).to_port_slice(),
            items.slice(i * 3 + 2, i * 3)
        );
        let exported = items.element(i).export_as(format!("last{n}"));
        assert_eq!(exported.field("tag").to_port_slice(), exported.slice(1, 0));
    }
}

#[test]
#[should_panic(expected = "outside packed dimension [2:3]")]
fn an_out_of_range_index_is_not_a_flat_offset() {
    let pkgs = extract_packages_from_verilog(ARRAYS, false).unwrap();
    pkgs["bank"].packed_layout("grid_t").element(0);
}

#[test]
#[should_panic(expected = "must be a single packed struct")]
fn arrays_do_not_implicitly_select_an_element_for_named_fields() {
    let module = ModDef::from_verilog("Bank", ARRAYS, false, false);
    module
        .get_port("frame")
        .field("cells")
        .element(2)
        .field("flag");
}

#[test]
#[should_panic(expected = "is not a packed array")]
fn a_single_struct_does_not_accept_an_array_index() {
    let module = ModDef::from_verilog("Bank", ARRAYS, false, false);
    module.get_port("frame").element(0);
}

#[test]
fn skipping_an_unpacked_struct_array_omits_the_port_instead_of_flattening_it() {
    let module = ModDef::from_verilog("Mixed", "
        package p; typedef struct packed { logic tag; } word_t; endpackage
        module Mixed(input p::word_t [1:0] packed_words, input p::word_t [1:0] memory [0:2]); endmodule",
        false, true);
    assert!(module.has_port("packed_words"));
    assert!(!module.has_port("memory"));
}

#[test]
fn make_an_array_port_from_a_struct_without_an_array_typedef() {
    let top = ModDef::new("Lanes");
    let pkgs = extract_packages_from_verilog(COLORS, false).unwrap();
    let ink = pkgs["paint"].packed_layout("ink_t");
    let lanes = PackedLayout::array_of(&ink, 4);
    let port = top.add_port("lanes", lanes.inout());
    assert!(matches!(port.io(), IO::InOut(12)));
    let first = lanes.element(3).field("shade");
    assert_eq!(first.range_in(&lanes), topstitch::BitRange::new(11, 10));
    assert_eq!(
        port.element(3).field("shade").to_port_slice(),
        port.slice(11, 10)
    );
    assert_eq!(port.element(0).field("bright").to_port_slice(), port.bit(0));
}

#[test]
#[should_panic(expected = "a packed array must have at least one element")]
fn an_empty_packed_array_has_no_port_layout() {
    let pkgs = extract_packages_from_verilog(COLORS, false).unwrap();
    PackedLayout::array_of(&pkgs["paint"].packed_layout("ink_t"), 0);
}

#[test]
fn declared_vector_indices_are_distinct_from_raw_bit_offsets() {
    let m = ModDef::from_verilog(
        "Offset",
        "module Offset(input logic [8:15] v); endmodule",
        false,
        false,
    );
    let v = m.get_port("v");
    assert_eq!(v.element(15).to_port_slice(), v.bit(0));
    assert_eq!(v.element(8).to_port_slice(), v.bit(7));
}

#[test]
fn a_wrapped_array_and_an_exported_subarray_keep_their_declared_indices() {
    let bank = ModDef::from_verilog("Bank", ARRAYS, false, false);
    let wrapper = bank.wrap(None, None);
    let mask = wrapper.get_port("mask");
    assert_eq!(mask.element(4).element(0).to_port_slice(), mask.bit(3));

    let frame = wrapper.get_port("frame");
    let pkgs = extract_packages_from_verilog(ARRAYS, false).unwrap();
    let grid = pkgs["bank"].packed_layout("grid_t");
    let row = grid.on(&frame.slice(19, 2)).element(2).export_as("row");
    assert_eq!(row.element(-1).field("flag").to_port_slice(), row.bit(2));
}
