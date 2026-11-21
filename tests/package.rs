// SPDX-License-Identifier: Apache-2.0

use num_bigint::{BigInt, BigUint};
use rstest::rstest;
use std::collections::HashMap;
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

    let pkgs = packages_from_text(verilog);

    let a: &ConstantValue = pkgs["pkg_a"].get("a").unwrap();
    assert_eq!(i32::try_from(a).unwrap(), 22);
    assert_eq!(a, &pkgs["pkg_a"].parameters["a"]);
    assert_eq!(a, &pkgs["pkg_a"]["a"]);
    assert!(pkgs["pkg_b"].get("missing").is_none());
    assert!(pkgs["pkg_b"].get("my_t").is_none());
    let b: i32 = (&pkgs["pkg_b"]["b"]).try_into().unwrap();
    assert_eq!(b, 123);
    assert_eq!(i32::try_from(&pkgs["pkg_b"]["c"]).unwrap(), 145);
    assert_eq!(
        pkgs["pkg_b"].types["my_t"]
            .as_ref()
            .unwrap()
            .width()
            .unwrap(),
        12
    );
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

    let pkgs = packages_from_text(verilog);

    for (name, width) in [("my_struct_t", 6), ("my_enum_t", 2), ("my_array_t", 24)] {
        assert_eq!(
            pkgs["my_pkg"].types[name]
                .as_ref()
                .unwrap()
                .width()
                .unwrap(),
            width
        );
    }

    let pkg = &pkgs["my_pkg"];
    let ty = pkg.ty("my_enum_t").unwrap();
    assert_eq!(u8::try_from(ty.enum_member("Green").unwrap()).unwrap(), 0);
    let dtype = DataType::from_sv_type(ty.into_owned());
    let members = dtype.enum_members().unwrap();
    let ordered: Vec<_> = members
        .iter()
        .map(|(name, value)| (name.as_str(), u8::try_from(value).unwrap()))
        .collect();
    assert_eq!(ordered, [("Red", 2), ("Green", 0), ("Blue", 1)]);
}

/// A palette with nested pixel formats in two channel orders.
fn palette() -> Package {
    packages_from_text(
        "package palette;
             typedef logic flag_t;
             typedef enum { Off, On } mode;
             typedef struct packed { logic [4:0] red, green, blue; } rgb;
             typedef struct packed { rgb ink; logic [7:0] opacity; } swatch;
             typedef struct packed { logic [7:0] opacity; rgb ink; } reversed_swatch;
             typedef struct packed { swatch front, back; logic [3:0] flags; } packet;
             typedef rgb [3:0] quartet;
             typedef rgb strip [4];
             typedef struct { int x, y; } point;
             typedef union packed { rgb channels; logic [14:0] value; } pixel;
         endpackage",
    )
    .remove("palette")
    .unwrap()
}

#[test]
fn native_and_imported_layouts_select_the_same_wires_after_import() {
    let imported = layout(&palette(), "rgb");
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
fn enum_values_preserve_width_signedness_and_four_state_bits() {
    let packages = packages_from_text(
        "package p;
           typedef enum logic signed [139:0] {
             Negative = -140'sd2, Wide = 140'sd1 << 130, Special = 140'b10xz
           } code_t;
         endpackage",
    );
    let package = &packages["p"];
    let ty = package.ty("code_t").unwrap();
    let dtype = DataType::from_sv_type(ty.clone().into_owned());
    let members = dtype.enum_members().unwrap();

    let negative = &members["Negative"];
    assert!(negative.is_signed());
    assert_eq!(i8::try_from(negative).unwrap(), -2);
    assert!(u8::try_from(negative).is_err());

    let wide = &members["Wide"];
    assert_eq!(wide.width(), 140);
    assert!(!wide.has_unknown());
    assert_eq!(BigInt::try_from(wide).unwrap(), BigInt::from(1) << 130);
    assert!(u128::try_from(wide).is_err());
    let raw: &IntegerValue = ty.enum_member("Wide").unwrap();
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
    let packages = packages_from_text(
        "package p;
           typedef enum logic [1:0] { Idle = 0, Busy = 2 } mode_t;
           typedef struct packed { logic valid; mode_t [1:0] modes; } header_t;
         endpackage",
    );
    let imported = layout(&packages["p"], "header_t");
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
    let packages = packages_from_text(
        "package lanes;
           typedef enum logic [2:0] { A = 1, B = 2 } [1:0] packed_pair;
           typedef enum logic [2:0] { C = 3, D = 4 } unpacked_pair [2];
           typedef enum logic signed [2:0] { E = -2, F } [1:0] mixed [3];
           typedef mixed alias_t [4];
         endpackage",
    );
    let p = &packages["lanes"];
    let unpacked = DataType::from_sv_type(p.ty("unpacked_pair").unwrap().into_owned());
    assert!(unpacked.packed_layout().is_none());
    assert_eq!(u8::try_from(&unpacked.enum_members().unwrap()["D"]), Ok(4));
    let modes = layout(p, "packed_pair");
    assert_eq!(modes.element(0).width(), 3);
    assert_eq!(modes.element(1).enum_members(), modes.enum_members());
    for (name, member, expected) in [
        ("packed_pair", "B", 2),
        ("unpacked_pair", "D", 4),
        ("mixed", "F", -1),
        ("alias_t", "E", -2),
    ] {
        let ty = p.ty(name).unwrap();
        assert_eq!(
            i8::try_from(ty.enum_member(member).unwrap()).unwrap(),
            expected
        );
    }
}

#[test]
fn enum_members_follow_the_type_alias_not_its_package_scope() {
    let packages = packages_from_text(
        "package origin; typedef enum { B = 9 } mode_t; endpackage
         package local_colors;
           typedef origin::mode_t alias_t;
           typedef enum logic signed [2:0] { B = -1, C = 0 } color_t;
         endpackage",
    );
    let local = &packages["local_colors"];
    let alias = local.ty("alias_t").unwrap();
    assert_eq!(
        i8::try_from(local.enumerator("B").unwrap().as_ref()).unwrap(),
        -1
    );
    assert_eq!(i8::try_from(alias.enum_member("B").unwrap()).unwrap(), 9);
    assert!(alias.enum_member("C").is_err());
}

#[test]
fn enum_lookup_does_not_search_struct_fields() {
    let packages = packages_from_text(
        "package p;
           typedef struct packed {
             enum logic { Idle, Run } state;
             enum logic [1:0] { Low = 1, High = 2 } level;
           } status_t;
         endpackage",
    );
    let ty = packages["p"].ty("status_t").unwrap();
    assert!(ty.enum_member("Run").is_err());
}

#[test]
fn palette_fields_on_a_bus() {
    let p = palette();
    let swatch = layout(&p, "swatch");
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

    let reversed = layout(&p, "reversed_swatch").on(&pixels);
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
    let dtype = DataType::from_sv_type(p.ty("packet").unwrap().into_owned());
    let packet = dtype.packed_layout().unwrap();
    let front = packet.field("front");
    let green = front.field("ink").field("green");
    let parent = green.range_in_parent();
    assert_eq!((parent.msb(), parent.lsb(), parent.width()), (9, 5, 5));
    assert_eq!(green.range_in(&front), BitRange::new(17, 13));
    assert_eq!(green.range_in_root(), BitRange::new(44, 40));
    assert_eq!(green.range_in(&packet), green.range_in_root());
    assert_eq!(green.range_in(&green), BitRange::new(4, 0));
    assert_eq!(packet.range_in_root(), BitRange::new(49, 0));

    let fields = dtype.packed_layout().unwrap().fields();
    drop(dtype);
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
    let packet = layout(&p, "packet");
    packet
        .field("front")
        .field("ink")
        .field("green")
        .range_in(&packet.field("back"));
}

#[test]
#[should_panic(expected = "does not contain green")]
fn an_identical_layout_from_another_package_is_not_the_root() {
    let a = palette();
    let b = palette();
    layout(&a, "packet")
        .field("front")
        .field("ink")
        .field("green")
        .range_in(&layout(&b, "packet"));
}

#[test]
#[should_panic(expected = "is the root and has no parent")]
fn the_root_has_no_parent_range() {
    layout(&palette(), "swatch").range_in_parent();
}

#[rstest]
#[case("strip")]
#[case("point")]
#[case("pixel")]
#[case("flag_t")]
fn rejects_types_without_a_selectable_packed_layout(#[case] name: &str) {
    let p = palette();
    let ty = p.ty(name).unwrap();
    assert!(PackedLayout::from_sv_type(&ty).is_err());
}

#[test]
#[should_panic(expected = "No field cyan in")]
fn rejects_unknown_field() {
    layout(&palette(), "rgb").field("cyan");
}

#[test]
#[should_panic(expected = "green must be a single packed struct")]
fn cannot_descend_into_a_scalar_field() {
    layout(&palette(), "rgb").field("green").field("x");
}

#[rstest]
#[case(56)]
#[case(58)]
#[should_panic(expected = "Width mismatch binding")]
fn rejects_binding_a_struct_to_the_wrong_slice_width(#[case] msb: usize) {
    let p = palette();
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Output(64));
    layout(&p, "swatch").on(&bus.slice(msb, 35));
}

#[rstest]
#[case(5, 1)] // The parent bus has this bit, but the field does not.
#[case(1, 2)] // Both bits exist, but the range is reversed.
#[should_panic(expected = "Invalid slice")]
fn rejects_invalid_field_slices(#[case] msb: usize, #[case] lsb: usize) {
    let p = palette();
    let display = ModDef::new("display");
    let bus = display.add_port("pixels", IO::Output(64));
    layout(&p, "rgb")
        .on(&bus.slice(24, 10))
        .field("green")
        .slice(msb, lsb);
}

/// Imports an owned layout from a fixture typedef.
fn layout(package: &Package, name: &str) -> PackedLayout {
    PackedLayout::from_sv_type(package.types[name].as_ref().unwrap()).unwrap()
}

/// Compile an inline Verilog fixture and return its packages.
fn packages_from_text(verilog: &str) -> HashMap<String, Package> {
    VerilogLibrary::from_config(&ParserConfig {
        sources: &[Source::text(verilog)],
        ..Default::default()
    })
    .expect("package fixture should compile")
    .packages()
    .expect("fixture packages should be readable")
}
