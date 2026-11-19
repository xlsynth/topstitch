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

#[test]
fn top_level_outputs_read_the_resolved_pad() {
    let rtl = "module Driver(inout wire [3:0] pad, input wire en);
                 assign pad = en ? 4'h5 : 4'bz;
               endmodule";
    let driver = ModDef::from_verilog("Driver", rtl).unwrap();
    let top = ModDef::new("Top");
    let pad = top.add_port("pad", IO::InOut(4));
    let inst = top.instantiate(&driver, Some("driver"), None);
    inst.get_port("en")
        .connect(&top.add_port("en", IO::Input(1)));
    inst.get_port("pad").connect(&pad);
    top.add_port("whole", IO::Output(4))
        .connect(&concat([&pad]));
    top.add_port("zero", IO::Output(4)).connect(&pad.delay(0));
    top.add_port("swapped", IO::Output(4))
        .connect(&concat([pad.slice(1, 0), pad.slice(3, 2)]));

    let sv = top.emit(Default::default());
    assert!(sv.contains(".pad(pad)"), "{sv}");
    assert!(sv.contains("assign whole = pad;"), "{sv}");
    assert!(sv.contains("assign zero = pad;"), "{sv}");
    assert!(
        sv.contains("assign swapped = {pad[1:0], pad[3:2]};"),
        "{sv}"
    );
    ModDef::from_verilog("Top", format!("{rtl}\n{sv}")).unwrap();
}

#[test]
fn tied_top_level_inout_is_the_driver_and_readers_sample_the_resolved_net() {
    let rtl = "module Pins(inout wire [3:0] pad, input wire [3:0] sense); endmodule";
    let top = ModDef::new("Top");
    let pad = top.add_port("pad", IO::InOut(4));
    let pins = top.instantiate(
        &ModDef::from_verilog("Pins", rtl).unwrap(),
        Some("pins"),
        None,
    );
    pins.get_port("pad").connect(&pad);
    pins.get_port("sense").connect(&pad);
    pins.get_port("sense").specify_net_name("sampled");
    pad.tieoff(10);
    top.add_port("observed", IO::Output(4)).connect(&pad);
    top.add_port("inverted", IO::Output(4)).connect(&!&pad);

    let sv = top.emit(Default::default());
    assert!(sv.contains(".pad(pad)"), "{sv}");
    assert!(sv.contains(".sense(sampled)"), "{sv}");
    assert!(sv.contains("assign inverted = ~sampled;"), "{sv}");
    assert!(sv.contains("assign observed = sampled;"), "{sv}");
    assert!(sv.contains("assign sampled = pad;"), "{sv}");
    assert_eq!(sv.matches("assign pad = 4'ha;").count(), 1, "{sv}");
    ModDef::from_verilog("Top", format!("{rtl}\n{sv}")).unwrap();
}

#[test]
fn tied_child_inouts_share_named_and_unnamed_slices() {
    let rtl = "module Pins(inout wire [3:0] pad, input wire [3:0] sense); endmodule";
    let cell = ModDef::from_verilog("Pins", rtl).unwrap();
    let top = ModDef::new("Top");
    let first = top.instantiate(&cell, Some("first"), None);
    let second = top.instantiate(&cell, Some("second"), None);
    let a = first.get_port("pad");
    let b = second.get_port("pad");
    first.get_port("sense").connect(&a);
    second.get_port("sense").connect(&b);
    a.slice(3, 2).connect(&b.slice(1, 0));
    a.slice(1, 0).connect(&b.slice(3, 2));
    b.slice(3, 2).specify_net_name("biased");

    a.slice(3, 2).tieoff(2);
    second.get_port("sense").slice(3, 2).tieoff(1);
    for bitblast_assignments in [false, true] {
        let sv = top.emit(EmitOptions {
            bitblast_assignments,
            ..Default::default()
        });
        for pin in ["pad", "sense"] {
            assert!(
                sv.contains(&format!(".{pin}({{first_pad[3:2], biased[3:2]}})")),
                "{sv}"
            );
            assert!(
                sv.contains(&format!(".{pin}({{biased[3:2], first_pad[3:2]}})")),
                "{sv}"
            );
        }
        let drivers: &[&str] = if bitblast_assignments {
            &[
                "first_pad[3] = 1'h1",
                "first_pad[2] = 1'h0",
                "biased[3] = 1'h0",
                "biased[2] = 1'h1",
            ]
        } else {
            &["first_pad[3:2] = 2'h2", "biased[3:2] = 2'h1"]
        };
        assert_eq!(sv.matches("assign ").count(), drivers.len(), "{sv}");
        for driver in drivers {
            assert!(sv.contains(&format!("assign {driver};")), "{sv}");
        }
        ModDef::from_verilog("Top", format!("{rtl}\n{sv}")).unwrap();
    }

    first.get_port("sense").bit(0).tieoff(0);
    let duplicate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        top.emit(Default::default())
    }));
    let diagnostic = duplicate.unwrap_err().downcast::<String>().unwrap();
    assert!(
        diagnostic.contains("tied off multiple times"),
        "{diagnostic}"
    );
}

#[test]
fn named_slices_of_an_inout_keep_drivers_on_the_resolved_port() {
    let model =
        "module Endpoints(output wire [3:0] drive, inout wire [3:0] io, input wire [3:0] sense);
                   assign drive = 4'bzz00;
                   assign io = 4'b01zz;
                 endmodule";
    let cell = ModDef::from_verilog("Endpoints", model).unwrap();
    let top = ModDef::new("Top");
    let pad = top.add_port("pad", IO::InOut(6));
    let inst = top.instantiate(&cell, Some("pins"), None);
    let drive = inst.get_port("drive");
    drive.slice(1, 0).connect(&pad.slice(5, 4));
    drive.slice(3, 2).connect(&pad.slice(1, 0));
    pad.slice(3, 2).unused();
    inst.get_port("io").connect(&drive);
    let sense = inst.get_port("sense");
    sense.connect(&drive);
    sense.slice(2, 1).specify_net_name("sampled");

    for bitblast_assignments in [false, true] {
        let sv = top.emit(EmitOptions {
            bitblast_assignments,
            ..Default::default()
        });
        assert!(sv.contains(".drive({pad[1:0], pad[5:4]})"), "{sv}");
        assert!(sv.contains(".io({pad[1:0], pad[5:4]})"), "{sv}");
        assert!(
            sv.contains(".sense({pad[1], sampled[2:1], pad[4]})"),
            "{sv}"
        );
        assert!(sv.contains("assign sampled[1] = pad[5];"), "{sv}");
        assert!(sv.contains("assign sampled[2] = pad[0];"), "{sv}");
        ModDef::from_verilog("Top", format!("{model}\n{sv}")).unwrap();
    }
}

#[test]
fn overlapping_unused_delay_taps_are_diagnosed_with_or_without_other_observers() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let cells = include_str!("../examples/input/logic_cells.sv");
    let mut map = GateMap::new();
    map.flop(
        FlopFlavor::posedge(),
        ModDef::from_verilog("Dff", cells).unwrap(),
        FlopPins {
            d: "D",
            clk: "CLK",
            q: "Q",
            reset: None,
        },
    )
    .unwrap();

    for policy in [PrimitivePolicy::Allow, PrimitivePolicy::RequireMapping(map)] {
        for observe in [false, true] {
            let top = ModDef::new("Top");
            top.set_primitive_policy(policy.clone()).unwrap();
            let d = top.add_port("d", IO::Input(2));
            let clk = top.add_port("clk", IO::Input(1));
            let q = d.delay_with(3, &clk);
            top.add_port("q", IO::Output(2)).connect(&q);
            let open = top.emit(Default::default());
            let delay = q.get_mod_inst().unwrap();
            let unused = delay.delay_stage(1);
            unused.unused();
            assert_eq!(top.emit(Default::default()), open);

            if observe {
                top.add_port("tap", IO::Output(2))
                    .connect(&delay.delay_stage(2));
                let sv = top.emit(Default::default());
                ModDef::from_verilog("Top", format!("{cells}\n{sv}")).unwrap();
            }
            unused.subdivide(2)[0].unused();
            let error =
                catch_unwind(AssertUnwindSafe(|| top.emit(Default::default()))).unwrap_err();
            let message = error.downcast_ref::<String>().unwrap();
            assert!(
                message.contains("marked as unused multiple times"),
                "{message}"
            );
        }
    }
}
