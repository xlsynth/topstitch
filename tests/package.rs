// SPDX-License-Identifier: Apache-2.0

use num_bigint::{BigInt, BigUint};
use rstest::rstest;
use topstitch::*;

#[test]
fn test_extract_packages() {
    let verilog = "
      package pkg_a;
        localparam int a=22;
      endpackage
      package pkg_b;
        localparam int b=123;
        localparam int c=b+pkg_a::a;
        typedef logic [33:22] my_t;
      endpackage
    ";

    let pkgs = extract_packages_from_verilog(verilog, false).unwrap();

    assert_eq!(pkgs["pkg_a"].parameter::<i32>("a"), 22);
    assert_eq!(pkgs["pkg_a"]["a"], pkgs["pkg_a"].parameters["a"]);
    assert!(pkgs["pkg_b"].parameters.get("missing").is_none());
    assert!(pkgs["pkg_b"].parameters.get("my_t").is_none());
    let b: i32 = (&pkgs["pkg_b"]["b"]).try_into().unwrap();
    assert_eq!(b, 123);
    assert_eq!(pkgs["pkg_b"].parameter::<u16>("c"), 145);
    assert_eq!(i32::try_from(&pkgs["pkg_b"]["c"]).unwrap(), 145);
    assert_eq!(pkgs["pkg_b"].type_width("my_t"), 12);
}

#[test]
fn test_extract_packages_with_typedefs() {
    let verilog = "
      package my_pkg;
        typedef struct packed {
          logic [2:0] a;
          logic [1:0] b;
          logic c;
        } my_struct_t;
        typedef enum logic [1:0] {
          Red = 2,
          Green = 0,
          Blue = 1
        } my_enum_t;
        typedef my_struct_t [3:0] my_array_t;
      endpackage
    ";

    let pkgs = extract_packages_from_verilog(verilog, false).unwrap();

    for (name, width) in [("my_struct_t", 6), ("my_enum_t", 2), ("my_array_t", 24)] {
        assert_eq!(pkgs["my_pkg"].type_width(name), width);
        assert_eq!(
            pkgs["my_pkg"].types[name].as_ref().unwrap().width(),
            Ok(width)
        );
    }

    let pkg = &pkgs["my_pkg"];
    let members = pkg.typedef("my_enum_t").enum_members().unwrap();
    let ordered: Vec<_> = members
        .iter()
        .map(|(name, value)| (name.as_str(), u8::try_from(value).unwrap()))
        .collect();
    assert_eq!(ordered, [("Red", 2), ("Green", 0), ("Blue", 1)]);
    assert_eq!(pkg.enum_member::<u8>("my_enum_t", "Green"), 0);
}

/// A palette with signed tones and pixel formats in two channel orders.
fn palette() -> Package {
    extract_packages_from_verilog(
        "package palette;
             typedef enum logic signed [7:0] { Dark = -3, Dim, Bright = 42 } tone;
             typedef tone [1:0] tone_pair;
             localparam tone default_tone = Dim;
             localparam logic [3:0] mask = 4'b1zx0;
             typedef logic [159:0] pattern;
             localparam pattern stipple = (pattern'(1) << 139) | 9;
             typedef struct packed { logic [4:0] red, green, blue; } rgb;
             typedef struct packed { rgb ink; logic [7:0] opacity; } swatch;
             typedef struct packed { logic [7:0] opacity; rgb ink; } reversed_swatch;
             typedef struct packed { swatch front, back; logic [3:0] flags; } packet;
             typedef rgb [3:0] quartet;
             typedef rgb strip [4];
             typedef struct { int x, y; } point;
             typedef union packed { rgb channels; logic [14:0] value; } pixel;
         endpackage",
        false,
    )
    .unwrap()
    .remove("palette")
    .unwrap()
}

#[test]
fn native_and_imported_layouts_select_the_same_wires_after_import() {
    let imported = palette().typedef("rgb").packed_layout().unwrap();
    let channel = PackedLayout::array_of(&PackedLayout::bits(1), 5);
    let native = PackedLayout::structure(
        "display_pixel",
        [
            ("red", channel.clone()),
            ("green", channel.clone()),
            ("blue", channel),
        ],
    );
    let module = ModDef::new("display");
    let color_in = module.add_port("color_in", native.input());
    let color_out = module.add_port("color_out", imported.output());

    for (name, field) in native.fields() {
        assert_eq!(
            field.range_in(&native),
            imported.field(&name).range_in(&imported)
        );
        let source = color_in.field(&name);
        let sink = color_out.field(&name);
        source.connect(&sink);
        let connections = source.get_connections();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].other, sink.to_port_slice());
    }
    assert_eq!(
        color_in.field("green").to_port_slice(),
        color_in.slice(9, 5)
    );
    assert_eq!(
        color_out.field("green").to_port_slice(),
        color_out.slice(9, 5)
    );
    assert_eq!(
        color_in.field("green").element(1).to_port_slice(),
        color_in.bit(6)
    );
    assert_eq!(
        color_out.field("green").element(1).to_port_slice(),
        color_out.bit(6)
    );
}

#[test]
#[should_panic(expected = "duplicate packed field red")]
fn native_struct_rejects_duplicate_field_names() {
    PackedLayout::structure(
        "color",
        [
            ("red", PackedLayout::bits(2)),
            ("red", PackedLayout::bits(3)),
        ],
    );
}

#[test]
fn evaluated_palette_constants() {
    let p = palette();
    assert_eq!(p.enum_member::<i16>("tone", "Dim"), -2);
    assert_eq!(p.parameter::<i16>("default_tone"), -2);
    assert_eq!(
        p.parameter::<BigInt>("stipple"),
        (BigInt::from(1) << 139) + 9
    );
    assert!(std::panic::catch_unwind(|| p.parameter::<u128>("stipple")).is_err());
    assert!(std::panic::catch_unwind(|| p.parameter::<u8>("mask")).is_err());
    assert!(std::panic::catch_unwind(|| p.enum_member::<u8>("tone", "Dark")).is_err());
    assert_eq!(p.enum_member::<i16>("tone_pair", "Dim"), -2);
}

#[test]
fn enum_values_preserve_width_signedness_and_four_state_bits() {
    let packages = extract_packages_from_verilog(
        "package p;
           typedef enum logic signed [139:0] {
             Negative = -140'sd2, Wide = 140'sd1 << 130, Special = 140'b10xz
           } code_t;
         endpackage",
        false,
    )
    .unwrap();
    let package = &packages["p"];
    let members = package.typedef("code_t").enum_members().unwrap();

    let negative = &members["Negative"];
    assert!(negative.is_signed());
    assert_eq!(i8::try_from(negative).unwrap(), -2);
    assert!(u8::try_from(negative).is_err());

    let wide = &members["Wide"];
    assert_eq!(wide.width(), 140);
    assert!(!wide.has_unknown());
    assert_eq!(BigInt::try_from(wide).unwrap(), BigInt::from(1) << 130);
    assert!(u128::try_from(wide).is_err());
    let raw: &IntegerValue = package.enum_member("code_t", "Wide");
    assert_eq!(wide.bits(), &raw.bits);

    let special = &members["Special"];
    assert!(special.has_unknown());
    assert_eq!(special.bits(), &BigUint::from(0b1000u8));
    assert_eq!(special.x_mask(), &BigUint::from(0b0010u8));
    assert_eq!(special.z_mask(), &BigUint::from(0b0001u8));
    assert!(BigInt::try_from(special).is_err());
}

#[test]
fn native_and_imported_enum_fields_keep_their_encodings_on_wires() {
    let packages = extract_packages_from_verilog(
        "package p;
           typedef enum logic [1:0] { Idle = 0, Busy = 2 } mode_t;
           typedef struct packed { logic valid; mode_t [1:0] modes; } header_t;
         endpackage",
        false,
    )
    .unwrap();
    let imported = packages["p"].packed_layout("header_t");
    drop(packages);
    let mode = PackedLayout::enumeration("Mode", 2, [("Idle", 0), ("Busy", 2)]);
    let modes = PackedLayout::array_of(&mode, 2);
    let native = PackedLayout::structure(
        "Header",
        [("valid", PackedLayout::bits(1)), ("modes", modes)],
    );
    let expected = mode.enum_members().unwrap();
    let top = ModDef::new("Top");
    let input = top.add_port("input_word", native.input());
    let output = top.add_port("output_word", imported.output());
    input.connect(&output);
    for (layout, port) in [(&native, &input), (&imported, &output)] {
        let field = layout.field("modes").element(1);
        assert_eq!(field.width(), 2);
        assert_eq!(field.enum_members(), Some(expected));
        let view = port.field("modes").element(1);
        assert_eq!(view.enum_members(), Some(expected));
        assert_eq!(view.to_port_slice(), port.slice(3, 2));
        assert!(port.field("valid").enum_members().is_none());
    }
    let selected = input.field("modes").element(1).export_as("mode");
    assert_eq!(
        selected.packed_view().unwrap().enum_members(),
        Some(expected)
    );
    let rtl = top.emit(EmitOptions::default());
    assert!(rtl.contains("output wire [1:0] mode"), "{rtl}");
    assert!(rtl.contains("assign mode = input_word[3:2];"), "{rtl}");
}

#[test]
fn native_enum_accepts_exact_signed_four_state_encodings() {
    let value =
        |bits: u8, x: u8, z: u8| EnumValue::from_bits(140, true, bits.into(), x.into(), z.into());
    let minus_one = EnumValue::from_bits(
        140,
        true,
        (BigUint::from(1u8) << 140) - 1u8,
        0u8.into(),
        0u8.into(),
    );
    let layout = PackedLayout::from_enum_values(
        "Status",
        [
            ("MinusOne", minus_one),
            ("Unknown", value(0b1000, 0b0010, 0b0001)),
        ],
    );
    let members = layout.enum_members().unwrap();
    assert_eq!(layout.width(), 140);
    assert_eq!(i8::try_from(&members["MinusOne"]), Ok(-1));
    assert_eq!(members["Unknown"].x_mask(), &BigUint::from(2u8));
    assert_eq!(members["Unknown"].z_mask(), &BigUint::from(1u8));
    assert!(u8::try_from(&members["Unknown"]).is_err());
}

#[test]
fn native_enum_rejects_ambiguous_or_invalid_encodings() {
    for entries in [
        vec![],
        vec![("", 0)],
        vec![("A", 0), ("A", 1)],
        vec![("A", 0), ("B", 0)],
        vec![("A", -1)],
        vec![("A", 4)],
    ] {
        assert!(
            std::panic::catch_unwind(|| PackedLayout::enumeration("Mode", 2, entries)).is_err()
        );
    }
    let known =
        |width, signed| EnumValue::from_bits(width, signed, 1u8.into(), 0u8.into(), 0u8.into());
    for other in [known(3, false), known(2, true)] {
        assert!(
            std::panic::catch_unwind(|| PackedLayout::from_enum_values(
                "Mode",
                [("A", known(2, false)), ("B", other)]
            ))
            .is_err()
        );
    }
    for (width, bits, x, z) in [(0, 0u8, 0u8, 0u8), (2, 0, 4, 0), (2, 0, 1, 1), (2, 1, 0, 1)] {
        assert!(
            std::panic::catch_unwind(|| EnumValue::from_bits(
                width,
                false,
                bits.into(),
                x.into(),
                z.into()
            ))
            .is_err()
        );
    }
}

#[test]
fn enum_arrays_share_their_element_constants() {
    let packages = extract_packages_from_verilog(
        "package lanes;
           typedef enum logic [2:0] { A = 1, B = 2 } [1:0] packed_pair;
           typedef enum logic [2:0] { C = 3, D = 4 } unpacked_pair [2];
           typedef enum logic signed [2:0] { E = -2, F } [1:0] mixed [3];
           typedef mixed alias_t [4];
         endpackage",
        false,
    )
    .unwrap();
    let p = &packages["lanes"];
    assert_eq!(p.enumerator::<u8>("B"), 2);
    assert_eq!(p.enumerator::<u8>("D"), 4);
    assert_eq!(p.enumerator::<i8>("F"), -1);
    assert_eq!(p.type_width("packed_pair"), 6);
    assert!(p.typedef("unpacked_pair").packed_layout().is_none());
    let modes = p.packed_layout("packed_pair");
    assert_eq!(modes.element(0).width(), 3);
    assert_eq!(
        modes.element(1).enum_members(),
        p.typedef("packed_pair").enum_members()
    );
    assert_eq!(p.enum_member::<u8>("packed_pair", "B"), 2);
    assert_eq!(p.enum_member::<u8>("unpacked_pair", "D"), 4);
    assert_eq!(p.enum_member::<i8>("mixed", "F"), -1);
    assert_eq!(p.enum_member::<i8>("alias_t", "E"), -2);
}

#[test]
fn package_enumerator_lookup_uses_the_declaring_scope() {
    let packages = extract_packages_from_verilog(
        "package origin; typedef enum { B = 9 } mode_t; endpackage
         package local_colors;
           typedef origin::mode_t alias_t;
           enum logic signed [2:0] { B = -1 } color;
         endpackage",
        false,
    )
    .unwrap();
    let local = &packages["local_colors"];
    assert_eq!(local.enumerator::<i8>("B"), -1);
    assert_eq!(local.enum_member::<i8>("alias_t", "B"), 9);
    assert_eq!(packages["origin"].enumerator::<u8>("B"), 9);
    assert!(std::panic::catch_unwind(|| local.enumerator::<u8>("B")).is_err());
}

#[test]
#[should_panic(expected = "Missing enum constant palette::default_tone")]
fn a_localparam_is_not_an_enumerator() {
    palette().enumerator::<i8>("default_tone");
}

#[test]
#[should_panic(expected = "p::status_t is not an enum typedef")]
fn enum_lookup_does_not_search_struct_fields() {
    let pkgs = extract_packages_from_verilog(
        "package p;
           typedef struct packed {
             enum logic { Idle, Run } state;
             enum logic [1:0] { Low = 1, High = 2 } level;
           } status_t;
         endpackage",
        false,
    )
    .unwrap();
    pkgs["p"].enum_member::<u8>("status_t", "Run");
}

#[test]
#[should_panic(expected = "palette::background")]
fn missing_parameter_names_the_symbol() {
    palette().parameter::<u8>("background");
}

#[test]
#[should_panic(expected = "palette::tone::Pale")]
fn missing_enum_member_names_the_symbol() {
    palette().enum_member::<i8>("tone", "Pale");
}

#[test]
#[should_panic(expected = "Missing typedef palette::background")]
fn missing_typedef_names_the_symbol() {
    palette().type_width("background");
}

#[test]
#[should_panic(expected = "Cannot size palette::strip")]
fn an_unpacked_array_has_no_packed_width() {
    palette().type_width("strip");
}

#[test]
fn palette_fields_on_a_bus() {
    let p = palette();
    assert_eq!(p.type_width("quartet"), 60);
    let swatch = p.packed_layout("swatch");
    let ink = swatch.field("ink");
    assert_eq!(ink.range_in_root(), BitRange::new(22, 8));
    let fields = ink.fields();
    let names: Vec<_> = fields.iter().map(|(name, _)| name.as_ref()).collect();
    assert_eq!(names, ["red", "green", "blue"]);
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Output(64));
    let pixels = bus.slice(57, 35);
    let channels = swatch.on(&pixels).field("ink");
    assert_eq!(channels.field("red").to_port_slice(), bus.slice(57, 53));
    assert_eq!(channels.field("blue").to_port_slice(), bus.slice(47, 43));
    let color = channels.field("green");
    assert_eq!(color.to_port_slice(), bus.slice(52, 48));
    assert_eq!(color.slice(3, 1), bus.slice(51, 49));
    assert_eq!(color.slice(4, 0), color.to_port_slice());
    assert_eq!(color.bit(0), bus.bit(48));
    assert_eq!(color.bit(4), bus.bit(52));

    let reversed = p.packed_layout("reversed_swatch").on(&pixels);
    assert_eq!(
        reversed.field("ink").field("green").to_port_slice(),
        bus.slice(44, 40)
    );
    let rebound = ink.on(&bus.slice(24, 10)).field("green");
    assert_eq!(rebound.to_port_slice(), bus.slice(19, 15));
}

#[rstest]
#[case(0, 1)]
#[case(usize::MAX, 0)]
#[should_panic(expected = "bit range")]
fn rejects_invalid_bit_ranges(#[case] msb: usize, #[case] lsb: usize) {
    BitRange::new(msb, lsb);
}

#[test]
fn nested_ranges_use_the_requested_occurrence() {
    let p = palette();
    let packet = p.packed_layout("packet");
    let front = packet.field("front");
    let green = front.field("ink").field("green");
    let parent = green.range_in_parent();
    assert_eq!((parent.msb(), parent.lsb(), parent.width()), (9, 5, 5));
    assert_eq!(green.range_in(&front), BitRange::new(17, 13));
    assert_eq!(green.range_in_root(), BitRange::new(44, 40));
    assert_eq!(green.range_in(&packet), green.range_in_root());
    assert_eq!(green.range_in(&green), BitRange::new(4, 0));
    assert_eq!(packet.range_in_root(), BitRange::new(49, 0));

    // The same occurrence can be reached through a fresh lookup or iteration.
    let fields = p.packed_layout("packet").fields();
    drop(p);
    let again = fields
        .into_iter()
        .find(|(name, _)| name.as_ref() == "front")
        .unwrap()
        .1;
    assert_eq!(green.range_in(&again), green.range_in(&front));
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Input(64));
    let bound = packet
        .on(&bus.slice(58, 9))
        .field("front")
        .field("ink")
        .field("green");
    assert_eq!(bound.to_port_slice(), bus.slice(53, 49));
}

#[test]
#[should_panic(expected = "back does not contain green")]
fn a_sibling_of_the_same_type_is_not_an_ancestor() {
    let p = palette();
    let packet = p.packed_layout("packet");
    packet
        .field("front")
        .field("ink")
        .field("green")
        .range_in(&packet.field("back"));
}

#[test]
#[should_panic(expected = "packet does not contain green")]
fn an_identical_layout_from_another_package_is_not_the_root() {
    let a = palette();
    let b = palette();
    a.packed_layout("packet")
        .field("front")
        .field("ink")
        .field("green")
        .range_in(&b.packed_layout("packet"));
}

#[test]
#[should_panic(expected = "swatch is the root and has no parent")]
fn the_root_has_no_parent_range() {
    palette().packed_layout("swatch").range_in_parent();
}

#[rstest]
#[case("strip")]
#[case("point")]
#[case("pixel")]
#[should_panic(expected = "must be a packed struct, array, or enum")]
fn rejects_types_without_a_packed_layout(#[case] name: &str) {
    palette().packed_layout(name);
}

#[test]
#[should_panic(expected = "No field cyan in rgb")]
fn rejects_unknown_field() {
    palette().packed_layout("rgb").field("cyan");
}

#[test]
#[should_panic(expected = "green must be a single packed struct")]
fn cannot_descend_into_a_scalar_field() {
    palette().packed_layout("rgb").field("green").field("x");
}

#[rstest]
#[case(56)]
#[case(58)]
#[should_panic(expected = "Width mismatch binding swatch")]
fn rejects_binding_a_struct_to_the_wrong_slice_width(#[case] msb: usize) {
    let p = palette();
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Output(64));
    p.packed_layout("swatch").on(&bus.slice(msb, 35));
}

#[rstest]
#[case(5, 1)] // The parent bus has this bit, but the field does not.
#[case(1, 2)] // Both bits exist, but the range is reversed.
#[should_panic(expected = "Invalid slice")]
fn rejects_invalid_field_slices(#[case] msb: usize, #[case] lsb: usize) {
    let p = palette();
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Output(64));
    p.packed_layout("rgb")
        .on(&bus.slice(24, 10))
        .field("green")
        .slice(msb, lsb);
}
