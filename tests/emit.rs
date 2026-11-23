// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[path = "emission/designs.rs"]
mod designs;

#[test]
fn test_emit_options_override_one_setting() {
    let options = EmitOptions {
        validate: false,
        bitblast_assignments: false,
        preserve_single_bit_slices: true,
        preserve_full_width_slices: true,
    };

    let bitblast_options = EmitOptions {
        bitblast_assignments: true,
        ..options
    };

    assert_eq!(
        bitblast_options,
        EmitOptions {
            validate: false,
            bitblast_assignments: true,
            preserve_single_bit_slices: true,
            preserve_full_width_slices: true,
        }
    );
    assert!(!options.bitblast_assignments);
}

#[test]
fn test_bitblast_assignments() {
    let mod_def = designs::bitblast_assignments();

    assert_eq!(
        mod_def.emit(EmitOptions {
            bitblast_assignments: true,
            ..Default::default()
        }),
        "\
module BitblastAssignments(
  input wire [3:0] y,
  output wire [3:0] x
);
  assign x[3] = y[1];
  assign x[2] = y[0];
  assign x[1] = y[3];
  assign x[0] = y[2];
endmodule
"
    );
}

#[test]
fn test_bitblast_assignment_constant_bit_order() {
    let mod_def = ModDef::new("BitblastConstant");
    mod_def.add_port("x", IO::Output(4)).tieoff(0b1001);

    assert_eq!(
        mod_def.emit(EmitOptions {
            bitblast_assignments: true,
            ..Default::default()
        }),
        "\
module BitblastConstant(
  output wire [3:0] x
);
  assign x[3] = 1'h1;
  assign x[2] = 1'h0;
  assign x[1] = 1'h0;
  assign x[0] = 1'h1;
endmodule
"
    );
}

#[test]
fn test_bitblast_assignment_constant_with_nonzero_lsb() {
    let mod_def = ModDef::new("BitblastSlicedConstant");
    let y = mod_def.add_port("y", IO::Input(2));
    let x = mod_def.add_port("x", IO::Output(4));

    x.slice(3, 2).tieoff(0b10);
    x.slice(1, 0).connect(&y);

    assert_eq!(
        mod_def.emit(EmitOptions {
            bitblast_assignments: true,
            ..Default::default()
        }),
        "\
module BitblastSlicedConstant(
  input wire [1:0] y,
  output wire [3:0] x
);
  assign x[3] = 1'h1;
  assign x[2] = 1'h0;
  assign x[1] = y[1];
  assign x[0] = y[0];
endmodule
"
    );
}

#[test]
fn test_bitblast_assignment_skips_unused_with_zero_and_nonzero_lsb() {
    let mod_def = ModDef::new("BitblastUnused");
    let unused_at_zero = mod_def.add_port("unused_at_zero", IO::Input(2));
    let partially_unused = mod_def.add_port("partially_unused", IO::Input(4));
    let x = mod_def.add_port("x", IO::Output(2));

    unused_at_zero.unused();
    partially_unused.slice(3, 2).unused();
    x.connect(&partially_unused.slice(1, 0));

    assert_eq!(
        mod_def.emit(EmitOptions {
            bitblast_assignments: true,
            ..Default::default()
        }),
        "\
module BitblastUnused(
  input wire [1:0] unused_at_zero,
  input wire [3:0] partially_unused,
  output wire [1:0] x
);
  assign x[1] = partially_unused[1];
  assign x[0] = partially_unused[0];
endmodule
"
    );
}

#[test]
fn test_bitblast_assignment_from_named_wire() {
    let source = ModDef::new("WireSource");
    source.set_usage(Usage::EmitStubAndStop);
    source.add_port("x", IO::Output(2));

    let mod_def = ModDef::new("BitblastWire");
    let y = mod_def.add_port("y", IO::Output(2));
    let source_inst = mod_def.instantiate(&source, Some("source"), None);
    let x = source_inst.get_port("x");

    x.connect(&y);
    x.specify_net_name("z");

    assert_eq!(
        mod_def.emit(EmitOptions {
            bitblast_assignments: true,
            ..Default::default()
        }),
        "\
module WireSource(
  output wire [1:0] x
);

endmodule
module BitblastWire(
  output wire [1:0] y
);
  wire [1:0] z;
  WireSource source (
    .x(z)
  );
  assign y[1] = z[1];
  assign y[0] = z[0];
endmodule
"
    );
}

#[test]
fn test_bitblast_assignment_from_named_wire_with_nonzero_lsb() {
    let source = ModDef::new("SlicedWireSource");
    source.set_usage(Usage::EmitStubAndStop);
    source.add_port("x", IO::Output(4));

    let mod_def = ModDef::new("BitblastSlicedWire");
    let y = mod_def.add_port("y", IO::Output(4));
    let source_inst = mod_def.instantiate(&source, Some("source"), None);
    let x = source_inst.get_port("x");

    x.slice(3, 2).connect(&y.slice(3, 2));
    x.slice(3, 2).specify_net_name("z");
    x.slice(1, 0).unused();
    y.slice(1, 0).tieoff(0);

    assert_eq!(
        mod_def.emit(EmitOptions {
            bitblast_assignments: true,
            ..Default::default()
        }),
        "\
module SlicedWireSource(
  output wire [3:0] x
);

endmodule
module BitblastSlicedWire(
  output wire [3:0] y
);
  wire [1:0] z;
  wire [3:0] source_x;
  SlicedWireSource source (
    .x({z, source_x[1:0]})
  );
  assign y[3] = z[1];
  assign y[2] = z[0];
  assign y[1] = 1'h0;
  assign y[0] = 1'h0;
endmodule
"
    );
}

#[test]
fn test_preserve_single_bit_slices() {
    let mod_def = ModDef::new("PreserveSingleBitSlices");
    let y = mod_def.add_port("y", IO::Input(4));
    let x = mod_def.add_port("x", IO::Output(4));

    x.slice(3, 1).connect(&y.slice(2, 0));
    x.bit(0).connect(&y.bit(3));

    assert_eq!(
        mod_def.emit(EmitOptions {
            preserve_single_bit_slices: true,
            ..Default::default()
        }),
        "\
module PreserveSingleBitSlices(
  input wire [3:0] y,
  output wire [3:0] x
);
  assign x[3:1] = y[2:0];
  assign x[0:0] = y[3:3];
endmodule
"
    );
}

#[test]
fn test_preserve_full_width_slices() {
    let mod_def = ModDef::new("PreserveFullWidthSlices");
    let y = mod_def.add_port("y", IO::Input(4));
    let x = mod_def.add_port("x", IO::Output(4));
    x.connect(&y);

    assert_eq!(
        mod_def.emit(EmitOptions {
            preserve_full_width_slices: true,
            ..Default::default()
        }),
        "\
module PreserveFullWidthSlices(
  input wire [3:0] y,
  output wire [3:0] x
);
  assign x[3:0] = y[3:0];
endmodule
"
    );
}

#[rstest::rstest]
#[case::module("module name", "module")]
#[case::leading_digit("module name", "9bad")]
#[case::port("port name", "with$dollar")]
#[case::punctuation("port name", "hello-world")]
#[case::port_keyword("port name", "output")]
#[case::escaped("port name", r"\plain")]
#[case::instance("instance name", "u.lane")]
#[case::parameter("parameter name", "WIDTH$bits")]
#[case::qualified_type("port type", "types$v1")]
fn emit_rejects_invalid_identifiers(#[case] role: &str, #[case] name: &str) {
    let top = design_with_identifier(role, name);
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        top.emit(EmitOptions {
            validate: false,
            ..Default::default()
        })
    }))
    .unwrap_err();
    let message = error.downcast_ref::<String>().unwrap();
    assert!(
        message.contains(role) && message.contains(name),
        "{message}"
    );
    #[cfg(feature = "block-ir")]
    assert_eq!(top.to_block_ir().unwrap_err().to_string(), *message);
}

#[cfg(feature = "block-ir")]
#[rstest::rstest]
#[case::module("module name", "top")]
#[case::port("port name", "token")]
#[case::instance("instance name", "stage")]
#[case::requested_wire("net name", "clock")]
fn block_ir_rejects_xls_keywords_accepted_by_direct_sv(#[case] role: &str, #[case] name: &str) {
    let top = design_with_identifier(role, name);
    let sv = top.emit(Default::default());
    assert!(
        sv.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .any(|id| id == name),
        "{sv}"
    );
    for error in [
        top.to_block_ir().unwrap_err(),
        top.emit_sv_via_block_ir().unwrap_err(),
    ] {
        let message = error.to_string();
        assert!(
            message.contains("Block IR") && message.contains(name),
            "{message}"
        );
    }
}

#[cfg(feature = "block-ir")]
#[test]
fn extern_bindings_preserve_xls_keywords_in_sv_names() {
    let rtl = "package bits;
            typedef enum logic {Idle, Busy} token;
        endpackage
        module stage #(parameter source=1)(input bits::token clock, output [source-1:0] next);
            assign next = {source{clock == bits::Busy}};
        endmodule";
    let leaf = ModDef::from_verilog("stage", rtl)
        .unwrap()
        .parameterize(&[("source", 3)]);
    let top = ModDef::new("Wrapper");
    let inst = top.instantiate(&leaf, Some("u_leaf"), None);
    inst.get_port("clock").export_as("enable");
    inst.get_port("next").export_as("result");

    let ir = top.to_block_ir().unwrap().to_string();
    let parsed = xlsynth_pir::ir_parser::Parser::new(&ir)
        .parse_and_validate_package()
        .unwrap();
    let round_trip_sv = xlsynth_codegen::emit_system_verilog(&parsed, &Default::default())
        .unwrap()
        .system_verilog;
    let via_ir = top.emit_sv_via_block_ir().unwrap();
    assert_eq!(via_ir, round_trip_sv);
    for sv in [top.emit(Default::default()), via_ir] {
        for binding in ["stage #(", ".source(", ".clock(bits::token'(", ".next("] {
            assert!(sv.contains(binding), "{sv}");
        }
        let linked = ModDef::from_verilog("Wrapper", format!("{rtl}\n{sv}")).unwrap();
        assert_eq!(linked.get_port("result").io().width(), 3);
    }
}

#[cfg(feature = "block-ir")]
#[test]
fn root_stub_with_xls_keywords_emits_its_sv_declaration() {
    let stub = ModDef::new("top");
    stub.add_port("clock", IO::Input(1));
    stub.set_usage(Usage::EmitStubAndStop);
    let sv = stub.emit_sv_via_block_ir().unwrap();
    assert_eq!(sv, stub.emit(Default::default()));
    let declaration = ModDef::from_verilog("top", sv).unwrap();
    assert!(declaration.has_port("clock"));
    assert!(
        stub.to_block_ir()
            .unwrap_err()
            .to_string()
            .contains("is external")
    );
}

/// Builds a small hierarchy that emits the supplied name in the selected role.
fn design_with_identifier(role: &str, name: &str) -> ModDef {
    let top = ModDef::new(if role == "module name" { name } else { "Top" });
    match role {
        "module name" => {}
        "port name" => {
            top.add_port(name, IO::Output(1)).tieoff(0);
        }
        "instance name" => {
            top.instantiate(&ModDef::new("Leaf"), Some(name), None);
        }
        "net name" => {
            let leaf = ModDef::new("Leaf");
            leaf.add_port("q", IO::Output(1)).tieoff(1);
            let q = top.instantiate(&leaf, None, None).get_port("q");
            q.specify_net_name(name);
            q.export_as("out");
        }
        "parameter name" => {
            let rtl = format!("module Leaf #(parameter {name}=1) (); endmodule");
            let leaf = ModDef::from_verilog("Leaf", rtl).unwrap();
            top.instantiate(&leaf.parameterize(&[(name, 2)]), None, None);
        }
        "port type" => {
            let rtl = format!(
                "package {name}; typedef enum logic {{Idle, Busy}} mode_t; endpackage
                module Leaf(input {name}::mode_t mode); endmodule"
            );
            let leaf = ModDef::from_verilog("Leaf", rtl).unwrap();
            top.instantiate(&leaf, None, None)
                .get_port("mode")
                .tieoff(0);
        }
        _ => unreachable!(),
    }
    top
}

#[test]
fn invalid_net_name_requests_leave_the_port_untouched() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("q", IO::Output(1)).tieoff(0);
    let top = ModDef::new("Top");
    let pin = top.instantiate(&leaf, None, None).get_port("q");
    pin.export_as("y");

    for name in [
        "",
        "9lane",
        "lane[0]",
        "with$dollar",
        "always",
        r"\wire",
        r"\plain ",
    ] {
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pin.specify_net_name(name)))
                .is_err(),
            "{name:?}"
        );
    }
    pin.specify_net_name("lane");
    assert!(top.emit(Default::default()).contains("wire lane;"));
}

#[test]
fn escaped_names_inside_an_external_module_can_be_linked() {
    let rtl = r"module Leaf(input d, output q);
        wire \lane[0] ;
        buf \buffer[0] (\lane[0] , d);
        assign q = \lane[0] ;
    endmodule";
    let leaf = ModDef::from_verilog("Leaf", rtl).unwrap();
    let top = leaf.wrap(Some("Wrapper"), Some("u_leaf"));
    let wrapper = top.emit(Default::default());
    assert!(wrapper.contains("Leaf u_leaf"), "{wrapper}");
    ModDef::from_verilog("Wrapper", format!("{rtl}\n{wrapper}")).unwrap();
}

#[test]
fn escaped_external_pin_fails_at_emission() {
    let leaf = ModDef::from_verilog("Leaf", r"module Leaf(output \lane[0] ); endmodule").unwrap();
    let top = ModDef::new("Top");
    top.instantiate(&leaf, None, None)
        .get_port("lane[0]")
        .unused();
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        top.emit(Default::default())
    }))
    .unwrap_err();
    let message = error.downcast_ref::<String>().unwrap();
    assert!(message.contains(r#"port name "lane[0]""#), "{message}");
    #[cfg(feature = "block-ir")]
    assert_eq!(top.to_block_ir().unwrap_err().to_string(), *message);
}

#[test]
fn emits_ordinary_sv_names_top_and_clock() {
    let top = ModDef::new("top");
    top.add_port("clock", IO::Input(1)).unused();
    let rtl = top.emit(Default::default());
    let imported = ModDef::from_verilog("top", rtl).unwrap();
    assert!(imported.has_port("clock"));
}

#[test]
fn external_views_check_shared_pins_for_the_same_binding() {
    fn leaf(pins: &[(&str, IO)]) -> ModDef {
        let leaf = ModDef::new("Leaf");
        for (pin, io) in pins {
            leaf.add_port(pin, io.clone());
        }
        leaf.set_usage(Usage::EmitNothingAndStop);
        leaf
    }

    // Views can omit pins or present them in a different order. A conflict can
    // be on a pin that was absent from the first view visited.
    let top = ModDef::new("Wrapper");
    for (name, pins) in [
        ("a", vec![("q", IO::Output(4)), ("en", IO::Input(1))]),
        ("b", vec![("en", IO::Input(1)), ("q", IO::Output(4))]),
        ("c", vec![("extra", IO::Output(1))]),
    ] {
        let inst = top.instantiate(&leaf(&pins), Some(name), None);
        for port in inst.get_ports(None) {
            port.unused_or_tieoff(0);
        }
    }
    let direct = top.emit(Default::default());
    assert!(direct.contains("Leaf a"));
    #[cfg(feature = "block-ir")]
    top.emit_sv_via_block_ir().unwrap();

    for io in [IO::Output(8), IO::Input(4)] {
        let parent = ModDef::new("Outer");
        parent.instantiate(&top, Some("nested"), None);
        let inst = parent.instantiate(&leaf(&[("q", io)]), Some("bad"), None);
        inst.get_port("q").unused_or_tieoff(0);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.emit(Default::default())
        }))
        .unwrap_err();
        assert!(
            panic
                .downcast_ref::<String>()
                .unwrap()
                .contains("conflicting declarations of external port Leaf.q")
        );
        #[cfg(feature = "block-ir")]
        for error in [
            parent.to_block_ir().unwrap_err(),
            parent.emit_sv_via_block_ir().unwrap_err(),
        ] {
            let message = error.to_string();
            assert!(
                message.contains("Leaf.q")
                    && message.contains("Outer.bad")
                    && message.contains("Wrapper."),
                "{message}"
            );
        }
    }
}
