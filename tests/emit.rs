// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

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
