// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[rstest::rstest]
#[case::external_first(true)]
#[case::generated_first(false)]
#[should_panic(expected = "at Parent.external_u and Parent.generated_u")]
fn rejects_external_and_generated_modules_with_the_same_name(#[case] external_first: bool) {
    let external =
        ModDef::from_verilog("Leaf", "module Leaf(output q); assign q = 0; endmodule").unwrap();
    let generated = ModDef::new("Leaf");
    generated.add_port("q", IO::Output(1)).tieoff(1);

    let top = ModDef::new("Parent");
    let mut children = [(&external, "external_u"), (&generated, "generated_u")];
    if !external_first {
        children.reverse();
    }
    for (child, name) in children {
        top.instantiate(child, Some(name), None)
            .get_port("q")
            .unused();
    }
    top.emit(EmitOptions::default());
}

/// Places leaves under different parents so binding is checked across the emitted file.
fn emit_leaves(leaves: &[&ModDef]) -> String {
    let top = ModDef::new("Top");
    for (i, leaf) in leaves.iter().enumerate() {
        let wrapper = leaf.wrap(Some(&format!("Wrapper{i}")), Some("u"));
        let inst = top.instantiate(&wrapper, Some(&format!("branch{i}")), None);
        for port in inst.get_ports(None) {
            port.unused_or_tieoff(0);
        }
    }
    top.emit(Default::default())
}

#[test]
#[should_panic(
    expected = "Two distinct modules with the same name: Leaf at Top.branch0.u and Top.branch1.u"
)]
fn distinct_generated_definitions_conflict_across_hierarchy_branches() {
    let first = ModDef::new("Leaf");
    first.add_port("q", IO::Output(1)).tieoff(0);
    let second = ModDef::new("Leaf");
    second.add_port("q", IO::Output(1)).tieoff(1);
    emit_leaves(&[&first, &second]);
}

#[test]
fn shared_generated_definition_is_emitted_once_across_hierarchy_branches() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("q", IO::Output(1)).tieoff(1);
    let sv = emit_leaves(&[&leaf, &leaf]);
    assert_eq!(sv.matches("module Leaf(").count(), 1);
    let sources = [Source::text(sv)];
    let top = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Top"],
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Top")
    .unwrap();
    let leaves: Vec<_> = top
        .report_all_instances()
        .into_iter()
        .filter(|(name, _)| name == "Leaf")
        .collect();
    assert_eq!(
        leaves,
        vec![
            ("Leaf".to_string(), "Top.branch0.u".to_string()),
            ("Leaf".to_string(), "Top.branch1.u".to_string())
        ]
    );
}

#[test]
fn independent_external_imports_can_share_a_binding() {
    // The bodies deliberately disagree. TopStitch uses each import for its port
    // interface and emits both instances by name. The caller supplies the one
    // Leaf definition that determines the behavior of both instances.
    let first = ModDef::from_verilog(
        "Leaf",
        "module Leaf(input d, output q); assign q = d; endmodule",
    )
    .unwrap();
    let second = ModDef::from_verilog(
        "Leaf",
        "module Leaf(output q, input d); assign q = ~d; endmodule",
    )
    .unwrap();
    let sv = emit_leaves(&[&first, &second]);
    assert_eq!(sv.matches("Leaf u (").count(), 2);
    assert!(!sv.contains("module Leaf("));
}

#[rstest::rstest]
#[case::width("output [1:0] q")]
#[case::direction("input q")]
#[case::name("output r")]
#[case::different_case("output Q")]
#[should_panic(
    expected = "Incompatible external references to Leaf with the same parameter overrides at Top.branch0.u and Top.branch1.u"
)]
fn incompatible_external_references_report_both_paths(#[case] port: &str) {
    let first = ModDef::from_verilog("Leaf", "module Leaf(output q); endmodule").unwrap();
    let second = ModDef::from_verilog("Leaf", format!("module Leaf({port}); endmodule")).unwrap();
    emit_leaves(&[&first, &second]);
}

#[rstest::rstest]
#[case::signed_int("int", "32'h0000_0008")]
#[case::narrow_unsigned("logic [15:0]", "16'h0008")]
fn pipeline_and_import_bind_compatible_overrides(
    #[case] width_type: &str,
    #[case] imported_width: &str,
) {
    let rtl = format!(
        "module br_delay_nr #(parameter {width_type} Width=1, parameter int NumStages=1)(
        input clk, input [Width-1:0] in, output [Width-1:0] out,
        output [Width*NumStages-1:0] out_stages
    ); endmodule"
    );
    let pipeline = PipelineConfig {
        depth: 2,
        ..Default::default()
    }
    .to_mod_def(8);
    let imported = ModDef::from_verilog("br_delay_nr", &rtl)
        .unwrap()
        .parameterize(&[("Width", 8), ("NumStages", 2)]);
    let sv = emit_leaves(&[&pipeline, &imported]);
    assert!(sv.contains(".Width(32'h0000_0008)"));
    assert!(sv.contains(&format!(".Width({imported_width})")));
    assert_eq!(sv.matches(".NumStages(32'h0000_0002)").count(), 2);
    ModDef::from_verilog("Top", format!("{rtl}\n{sv}")).unwrap();
}

#[test]
fn untyped_parameter_literal_widths_can_change_port_widths() {
    let leaf = |msb| {
        let rtl = format!(
            "module Leaf #(parameter logic [{msb}:0] MODE=0)(output [$bits(MODE)-1:0] q); endmodule"
        );
        ModDef::from_verilog("Leaf", rtl)
            .unwrap()
            .parameterize(&[("MODE", 1)])
    };
    let sv = emit_leaves(&[&leaf(3), &leaf(7)]);
    assert!(sv.contains(".MODE(4'h1)"));
    assert!(sv.contains(".MODE(8'h01)"));
    let rtl =
        format!("module Leaf #(parameter MODE=0)(output [$bits(MODE)-1:0] q); endmodule\n{sv}");
    let probe = "module Marker; endmodule
        module Probe #(parameter W=0);
            for (genvar i=W; i<=W; i++) begin : width Marker ok(); end
        endmodule
        bind Leaf Probe #(.W($bits(q))) probe();";
    let sources = [Source::text(rtl), Source::text(probe)];
    let top = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Top"],
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Top")
    .unwrap();
    let widths: Vec<_> = top
        .report_all_instances()
        .into_iter()
        .filter_map(|(name, path)| (name == "Marker").then_some(path))
        .collect();
    assert_eq!(
        widths,
        [
            "Top.branch0.u.probe.width[4].ok",
            "Top.branch1.u.probe.width[8].ok"
        ]
    );
}

#[test]
fn external_parameterizations_match_by_named_overrides() {
    let source = "module Leaf #(parameter W=1, parameter MASK=0)(output [W-1:0] q); endmodule";
    let base = ModDef::from_verilog("Leaf", source).unwrap();
    let byte = base.parameterize(&[("W", 8), ("MASK", 1)]);
    let wide = base.parameterize(&[("W", 16)]);
    let separate = ModDef::from_verilog("Leaf", source)
        .unwrap()
        .parameterize(&[("MASK", 1), ("W", 8)]);
    let sv = emit_leaves(&[&byte, &wide, &separate]);
    assert_eq!(sv.matches("Leaf #(").count(), 3);
}

#[test]
#[should_panic(expected = "parameter overrides at Top.branch1.u and Top.branch2.u")]
fn conflicts_are_checked_for_every_external_parameterization() {
    let first = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter W=1, parameter MASK=0)(output [W-1:0] q); endmodule",
    )
    .unwrap();
    let second = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter W=1, parameter MASK=0)(output [W:0] q); endmodule",
    )
    .unwrap();
    emit_leaves(&[
        &first.parameterize(&[("W", 4)]),
        &first.parameterize(&[("W", 8), ("MASK", 1)]),
        &second.parameterize(&[("MASK", 1), ("W", 8)]),
    ]);
}

#[rstest::rstest]
#[case::stub_first(true)]
#[case::stub_last(false)]
fn emitted_stub_binds_compatible_external_references(#[case] stub_first: bool) {
    let rtl = "module Leaf(output [7:0] q); endmodule";
    let external = ModDef::from_verilog("Leaf", rtl).unwrap();
    let stub = ModDef::from_verilog("Leaf", rtl).unwrap();
    stub.set_usage(Usage::EmitStubAndStop);
    let leaves = if stub_first {
        [&stub, &external]
    } else {
        [&external, &stub]
    };
    let sv = emit_leaves(&leaves);
    assert_eq!(sv.matches("module Leaf").count(), 1);
    ModDef::from_verilog("Top", sv).unwrap();
}

#[test]
#[should_panic(
    expected = "External reference to Leaf at Top.branch0.u is incompatible with its emitted stub at Top.branch1.u"
)]
fn fixed_width_stub_rejects_a_mismatched_port() {
    let stub = ModDef::new("Leaf");
    stub.add_port("q", IO::Output(8));
    stub.set_usage(Usage::EmitStubAndStop);
    let external = ModDef::from_verilog("Leaf", "module Leaf(output [3:0] q); endmodule").unwrap();
    emit_leaves(&[&external, &stub]);
}

#[test]
#[should_panic(
    expected = "Instance Top.branch0.u overrides parameters on emitted stub Leaf at Top.branch1.u, which declares none"
)]
fn external_reference_cannot_override_a_parameter_on_a_port_only_stub() {
    let stub = ModDef::new("Leaf");
    stub.add_port("q", IO::Output(1));
    stub.set_usage(Usage::EmitStubAndStop);
    let base = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter MODE=0)(output q); endmodule",
    )
    .unwrap();
    emit_leaves(&[&base.parameterize(&[("MODE", 1)]), &stub]);
}

#[test]
#[should_panic(
    expected = "Instance Top.branch0.u overrides parameters on emitted stub Leaf, which declares none"
)]
fn instance_of_specialized_stub_cannot_override_undeclared_parameters() {
    let base = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter MODE=0)(output q); endmodule",
    )
    .unwrap();
    let stub = base.parameterize(&[("MODE", 1)]);
    stub.set_usage(Usage::EmitStubAndStop);
    emit_leaves(&[&stub]);
}

#[test]
#[should_panic(expected = "at Top.branch.u and Top")]
fn root_definition_conflicts_with_nested_external_reference() {
    let external = ModDef::from_verilog("Top", "module Top(); endmodule").unwrap();
    let inner = external.wrap(Some("Inner"), Some("u"));
    inner
        .wrap(Some("Top"), Some("branch"))
        .emit(Default::default());
}

#[test]
fn external_boundaries_hide_internal_names() {
    let hidden = ModDef::new("Top");
    let leaf = hidden.wrap(Some("Leaf"), Some("hidden"));
    leaf.set_usage(Usage::EmitNothingAndStop);
    let sv = leaf.wrap(Some("Top"), Some("u")).emit(Default::default());
    assert_eq!(sv.matches("module Top;").count(), 1);
    assert!(!sv.contains("hidden"));
}

#[test]
#[should_panic(
    expected = "Incompatible external port names or directions for Leaf at Top.branch0.u and Top.branch1.u"
)]
fn different_overrides_still_require_compatible_ports() {
    let source = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter W=1)(output [W-1:0] q); endmodule",
    )
    .unwrap();
    let sink = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter W=1)(input [W-1:0] q); endmodule",
    )
    .unwrap();
    emit_leaves(&[
        &source.parameterize(&[("W", 4)]),
        &sink.parameterize(&[("W", 8)]),
    ]);
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
