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
    let source_inst = mod_def.instantiate(&source, Some("producer"), None);
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
  WireSource producer (
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
    let source_inst = mod_def.instantiate(&source, Some("producer"), None);
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
  wire [3:0] z;
  wire [3:0] producer_x;
  SlicedWireSource producer (
    .x({z[3:2], producer_x[1:0]})
  );
  assign y[3] = z[3];
  assign y[2] = z[2];
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
#[case::module("module", "top")]
#[case::port("port", "output")]
#[case::instance("instance", "stage")]
#[case::imported_port("imported port", "clock")]
#[case::parameter("parameter", "source")]
#[case::qualified_type("enum type", "bits")]
fn both_emitters_reject_reserved_names(#[case] role: &str, #[case] name: &str) {
    let top = ModDef::new(if role == "module" { name } else { "Top" });
    match role {
        "port" => {
            top.add_port(name, IO::Output(1)).tieoff(0);
        }
        "instance" => {
            top.instantiate(&ModDef::new("Leaf"), Some(name), None);
        }
        "imported port" => {
            let leaf =
                ModDef::from_verilog("Leaf", format!("module Leaf(output {name}); endmodule"))
                    .unwrap();
            top.instantiate(&leaf, None, None).get_port(name).unused();
        }
        "parameter" => {
            let leaf = ModDef::from_verilog(
                "Leaf",
                format!("module Leaf #(parameter {name}=0) (); endmodule"),
            )
            .unwrap();
            top.instantiate(&leaf.parameterize(&[(name, 1)]), None, None);
        }
        "enum type" => {
            let leaf = ModDef::from_verilog("Leaf", format!("package {name}; typedef enum logic {{A,B}} t; endpackage module Leaf(input {name}::t d); endmodule")).unwrap();
            top.instantiate(&leaf, None, None).get_port("d").tieoff(0);
        }
        _ => {}
    }
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        top.emit(EmitOptions::default())
    }))
    .unwrap_err();
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(
        message.contains("reserved") && message.contains(name),
        "{message}"
    );
    #[cfg(feature = "block-ir")]
    assert_eq!(top.to_block_ir().unwrap_err().to_string(), *message);
}

#[test]
fn explicit_keyword_net_names_fail_at_the_request() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("q", IO::Output(1));
    let top = ModDef::new("Top");
    let q = top.instantiate(&leaf, None, None).get_port("q");
    for name in ["always", "clock", r"\wire"] {
        let error =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| q.specify_net_name(name)))
                .unwrap_err();
        assert!(error.downcast_ref::<String>().unwrap().contains("reserved"));
    }
    q.specify_net_name("Clock");
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

/// Invalid identifiers must fail before either backend can produce malformed RTL.
#[test]
fn both_emitters_reject_invalid_identifier_syntax() {
    for (role, name) in [
        ("module", "9bad"),
        ("port", "hello-world"),
        ("instance", "a.b"),
        ("port", "with$dollar"),
        ("port", r"\escaped"),
        ("imported port", "lane[0]"),
    ] {
        let top = ModDef::new(if role == "module" { name } else { "Top" });
        match role {
            "port" => {
                top.add_port(name, IO::Output(1)).tieoff(0);
            }
            "instance" => {
                top.instantiate(&ModDef::new("Leaf"), Some(name), None);
            }
            "imported port" => {
                let rtl = format!(r"module Leaf(output \{name} ); endmodule");
                let leaf = ModDef::from_verilog("Leaf", rtl).unwrap();
                top.instantiate(&leaf, None, None).get_port(name).unused();
            }
            _ => {}
        }
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            top.emit(EmitOptions::default())
        }))
        .unwrap_err();
        let message = panic.downcast_ref::<String>().unwrap();
        assert!(
            message.contains("not a valid") && message.contains(name),
            "{message}"
        );
        #[cfg(feature = "block-ir")]
        assert_eq!(top.to_block_ir().unwrap_err().to_string(), *message);
    }
}

#[test]
fn invalid_net_names_fail_before_modifying_the_design() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("q", IO::Output(1));
    let top = ModDef::new("Top");
    let q = top.instantiate(&leaf, None, None).get_port("q");
    for name in ["bad-net", "9lane", "", "with$dollar"] {
        let error =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| q.specify_net_name(name)))
                .unwrap_err();
        assert!(
            error
                .downcast_ref::<String>()
                .unwrap()
                .contains("not a valid")
        );
    }
    q.specify_net_name("valid_lane_0");
}
