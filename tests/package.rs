// SPDX-License-Identifier: Apache-2.0

use num_bigint::BigInt;
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
          Red = 0,
          Green = 1,
          Blue = 2
        } my_enum_t;
        typedef my_struct_t [3:0] my_array_t;
      endpackage
    ";

    let pkgs = extract_packages_from_verilog(verilog, false).unwrap();

    for (name, width) in [("my_struct_t", 6), ("my_enum_t", 2), ("my_array_t", 24)] {
        assert_eq!(pkgs["my_pkg"].type_width(name), width);
    }
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
    let swatch = p.packed_struct("swatch");
    let ink = swatch.field("ink");
    assert_eq!(ink.range_in_root(), BitRange::new(22, 8));
    let names: Vec<_> = ink.fields().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["red", "green", "blue"]);
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Output(64));
    let pixels = bus.slice(57, 35);
    let channels = swatch.on(&pixels).field("ink");
    assert_eq!(channels.field("red").to_port_slice(), bus.slice(57, 53));
    assert_eq!(channels.field("blue").to_port_slice(), bus.slice(47, 43));
    let color = channels.field("green");
    assert_eq!(color.to_port_slice(), bus.slice(52, 48));
    assert_eq!(color.bit(0), bus.bit(48));
    assert_eq!(color.bit(4), bus.bit(52));

    let reversed = p.packed_struct("reversed_swatch").on(&pixels);
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
    let packet = p.packed_struct("packet");
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
    let again = p
        .packed_struct("packet")
        .fields()
        .into_iter()
        .find(|(name, _)| *name == "front")
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
    let packet = p.packed_struct("packet");
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
    a.packed_struct("packet")
        .field("front")
        .field("ink")
        .field("green")
        .range_in(&b.packed_struct("packet"));
}

#[test]
#[should_panic(expected = "swatch is the root and has no parent")]
fn the_root_has_no_parent_range() {
    palette().packed_struct("swatch").range_in_parent();
}

#[rstest]
#[case("quartet")]
#[case("strip")]
#[case("point")]
#[case("pixel")]
#[should_panic(expected = "must be a single packed struct")]
fn rejects_types_without_a_single_packed_struct_layout(#[case] name: &str) {
    palette().packed_struct(name);
}

#[test]
#[should_panic(expected = "No field cyan in rgb")]
fn rejects_unknown_field() {
    palette().packed_struct("rgb").field("cyan");
}

#[test]
#[should_panic(expected = "green must be a single packed struct")]
fn cannot_descend_into_a_scalar_field() {
    palette().packed_struct("rgb").field("green").field("x");
}

#[rstest]
#[case(56)]
#[case(58)]
#[should_panic(expected = "Width mismatch binding swatch")]
fn rejects_binding_a_struct_to_the_wrong_slice_width(#[case] msb: usize) {
    let p = palette();
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Output(64));
    p.packed_struct("swatch").on(&bus.slice(msb, 35));
}

#[test]
#[should_panic(expected = "Bit 5 outside field green")]
fn a_field_bit_cannot_spill_into_its_neighbor() {
    let p = palette();
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Output(64));
    p.packed_struct("rgb")
        .on(&bus.slice(24, 10))
        .field("green")
        .bit(5);
}
