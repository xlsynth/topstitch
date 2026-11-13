// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[rstest::rstest]
#[case::external_first(true)]
#[case::generated_first(false)]
#[should_panic(expected = "Leaf has both an external implementation and a generated definition")]
fn rejects_external_and_generated_modules_with_the_same_name(#[case] external_first: bool) {
    let external =
        ModDef::from_verilog("Leaf", "module Leaf(output q); assign q = 0; endmodule").unwrap();
    let generated = ModDef::new("Leaf");
    generated.add_port("q", IO::Output(1)).tieoff(1);

    let top = ModDef::new("Parent");
    let children = if external_first {
        [&external, &generated]
    } else {
        [&generated, &external]
    };
    for (child, name) in children.into_iter().zip(["first", "second"]) {
        top.instantiate(child, Some(name), None)
            .get_port("q")
            .unused();
    }
    top.emit(EmitOptions::default());
}

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
    let mod_def = ModDef::new("BitblastAssignments");
    let y = mod_def.add_port("y", IO::Input(4));
    let x = mod_def.add_port("x", IO::Output(4));

    x.slice(3, 2).connect(&y.slice(1, 0));
    x.slice(1, 0).connect(&y.slice(3, 2));

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
  wire [3:0] z;
  wire [3:0] source_x;
  SlicedWireSource source (
    .x({z[3:2], source_x[1:0]})
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
#[case::module("module name", "module")]
#[case::port("port name", "with$dollar")]
#[case::escaped("port name", r"\plain")]
#[case::instance("instance name", "u.lane")]
#[case::parameter("parameter name", "WIDTH$bits")]
#[case::qualified_type("port type", "types$v1")]
fn emit_rejects_invalid_identifiers(#[case] role: &str, #[case] name: &str) {
    let top = ModDef::new(if role == "module name" { name } else { "Top" });
    match role {
        "module name" => {}
        "port name" => {
            top.add_port(name, IO::Output(1)).tieoff(0);
        }
        "instance name" => {
            top.instantiate(&ModDef::new("Leaf"), Some(name), None);
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
}

#[test]
fn invalid_net_name_requests_leave_the_port_untouched() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("q", IO::Output(1)).tieoff(0);
    let top = ModDef::new("Top");
    let pin = top.instantiate(&leaf, None, None).get_port("q");
    pin.export_as("y");

    for name in ["", "9lane", "lane[0]", "always", r"\plain "] {
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
#[should_panic(expected = "port name \"lane[0]\"")]
fn escaped_external_pin_fails_at_emission() {
    let leaf = ModDef::from_verilog("Leaf", r"module Leaf(output \lane[0] ); endmodule").unwrap();
    let top = ModDef::new("Top");
    top.instantiate(&leaf, None, None)
        .get_port("lane[0]")
        .unused();
    top.emit(Default::default());
}

#[test]
fn emits_ordinary_sv_names_top_and_clock() {
    let top = ModDef::new("top");
    top.add_port("clock", IO::Input(1)).unused();
    let rtl = top.emit(Default::default());
    let imported = ModDef::from_verilog("top", rtl).unwrap();
    assert!(imported.has_port("clock"));
}
