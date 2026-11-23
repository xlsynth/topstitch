// SPDX-License-Identifier: Apache-2.0

use std::panic::{AssertUnwindSafe, catch_unwind};
use topstitch::{
    EmitOptions, FlopFlavor, FlopPins, GateMap, IO, LogicPort, ModDef, Mux2Pins, PipelineConfig,
    Primitive, PrimitivePolicy, Reset, ResetKind, Usage, concat,
};

/// Makes a black-box library cell for testing connectivity and physical hierarchy.
fn cell(name: &str, inputs: &[&str], out: &str) -> ModDef {
    let cell = ModDef::new(name);
    for p in inputs {
        cell.add_port(p, IO::Input(1));
    }
    cell.add_port(out, IO::Output(1));
    cell.set_usage(Usage::EmitNothingAndStop);
    cell
}

/// Common clock and data pin names used by the test cells.
fn flop_pins(reset: Option<&str>) -> FlopPins<'_> {
    FlopPins {
        d: "D",
        clk: "CLK",
        q: "Q",
        reset,
    }
}

/// Scalar cells with an active-low clear and preset for falling-edge registers.
fn library() -> GateMap {
    let mut map = GateMap::new();
    map.and2(cell("And2", &["A", "B"], "Y"), ["A", "B"], "Y")
        .unwrap();
    map.or2(cell("Or2", &["A", "B"], "Y"), ["A", "B"], "Y")
        .unwrap();
    map.xor2(cell("Xor2", &["A", "B"], "Y"), ["A", "B"], "Y")
        .unwrap();
    map.inv(cell("Inv", &["A"], "Y"), "A", "Y").unwrap();
    map.mux2(
        cell("Mux2", &["S", "A", "B"], "Y"),
        Mux2Pins {
            select: "S",
            when_false: "A",
            when_true: "B",
            out: "Y",
        },
    )
    .unwrap();
    map.flop(
        FlopFlavor::posedge(),
        cell("Dff", &["D", "CLK"], "Q"),
        flop_pins(None),
    )
    .unwrap();
    for (value, name) in [(false, "Clear"), (true, "Preset")] {
        map.flop(
            FlopFlavor::negedge().reset(ResetKind::AsyncLow, value),
            cell(name, &["D", "CLK", "RN"], "Q"),
            flop_pins(Some("RN")),
        )
        .unwrap();
    }
    map
}

#[test]
fn policies_apply_to_the_definition_even_for_explicit_foreign_primitive_defs() {
    let parent = ModDef::new("Package");
    let a = parent.add_port("a", IO::Input(4));
    let bypass = concat([a.bit(0), a.slice(3, 1)]);
    parent
        .set_primitive_policy(PrimitivePolicy::Forbid)
        .unwrap();
    let local = a.slice(2, 1).delay(0);
    parent
        .add_port("word", IO::Output(6))
        .connect(&concat([bypass, local]));
    parent
        .add_port("direct", IO::Output(4))
        .connect_pipeline(&a, PipelineConfig::default());
    assert!(!parent.has_port("clk"));
    let leaf = ModDef::new("Extern");
    let foreign = leaf.primitive_def(Primitive::And, 4);
    let count = parent.get_instances().len();
    assert!(
        catch_unwind(AssertUnwindSafe(|| parent.instantiate(
            &foreign,
            Some("manual"),
            None
        )))
        .is_err()
    );
    assert!(catch_unwind(AssertUnwindSafe(|| &a & 1)).is_err());
    parent.add_port("y", IO::Output(4)).connect(&a);
    assert_eq!(parent.get_instances().len(), count);
    let mapped = ModDef::new("Die");
    mapped
        .set_primitive_policy(PrimitivePolicy::RequireMapping(library()))
        .unwrap();
    let inst = mapped.instantiate(&foreign, Some("manual"), None);
    inst.get_port("a").tieoff(0);
    inst.get_port("b").tieoff(0);
    let output = mapped.add_port("result", IO::Output(4));
    output.connect(&inst.get_port("y"));
    assert_eq!(inst.get_instances().len(), 4);
    assert!(mapped.set_primitive_policy(PrimitivePolicy::Allow).is_err());
    parent
        .instantiate(&mapped, Some("die"), None)
        .get_port("result")
        .unused();
    let sv = parent.emit(Default::default());
    assert!(sv.contains("And2 bit_3"));
    assert!(!sv.contains("always_ff"));
}

#[test]
fn checked_bindings_and_missing_flavors_fail_without_partial_logic() {
    let mut map = GateMap::new();
    let physical = cell("And2", &["i0", "i1"], "o");
    assert!(map.and2(physical.clone(), ["i0", "i0"], "o").is_err());
    assert!(map.and2(physical.clone(), ["o", "i1"], "i0").is_err());
    map.and2(physical.clone(), ["i0", "i1"], "o").unwrap();
    assert!(map.and2(physical, ["i0", "i1"], "o").is_err());
    let m = ModDef::new("M");
    m.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let a = m.add_port("a", IO::Input(2));
    let clk = m.add_port("clk", IO::Input(1));
    let rst = m.add_port("rst", IO::Input(1));
    let invalid: [&dyn Fn(); 2] = [
        &|| {
            let _ = &a | 1;
        },
        &|| {
            a.delay_with(3, (&clk, Reset::async_low(&rst, 2)));
        },
    ];
    for f in invalid {
        assert!(catch_unwind(AssertUnwindSafe(f)).is_err());
        assert!(m.get_instances().is_empty());
        assert!(a.get_connections().is_empty());
    }
}

#[test]
fn scalar_cells_assemble_words_balanced_trees_and_reset_flavors() {
    let m = ModDef::new("Physical");
    m.set_primitive_policy(PrimitivePolicy::RequireMapping(library()))
        .unwrap();
    let a = m.add_port("a", IO::Input(6));
    let s = m.add_port("s", IO::Input(3));
    let clk = m.add_port("clk", IO::Input(1));
    let rst = m.add_port("rst", IO::Input(1));
    let cases = [
        ("masked", &a & 1, 6),
        ("reduce", a.or(), 5),
        ("sel", s.select_from_or((0..6).map(|i| a.bit(i)), 0), 6),
        (
            "registered",
            a.flop_with((clk.negedge(), Reset::async_low(&rst, 0x15))),
            6,
        ),
        ("delayed", a.delay_with(3, &clk), 18),
        ("one_bit", a.bit(0).xor(), 1),
    ];
    for (name, out, count) in cases {
        let inst = out.get_mod_inst().unwrap();
        assert_eq!(inst.get_instances().len(), count, "{name}");
        if name == "registered" {
            for bit in 0..6 {
                let target = inst
                    .get_instance(format!("stage_0_bit_{bit}"))
                    .get_mod_def()
                    .get_name();
                assert_eq!(target, if bit % 2 == 0 { "Preset" } else { "Clear" });
            }
        }
        m.add_port(name, IO::Output(out.io().width())).connect(&out);
    }
    let sv = m.emit(EmitOptions::default());
    assert!(sv.contains("Mux2 bit_0_mux_5"));
    assert!(!sv.contains("always_ff"));
    assert_eq!(m.emit(Default::default()), sv);
}

#[test]
fn whole_delay_precedes_flops_and_pipeline_uses_its_owning_module_policy() {
    let mut map = library();
    map.delay(FlopFlavor::posedge(), flop_pins(None), |width, depth| {
        let delay = ModDef::new(format!("Delay{width}x{depth}"));
        delay.add_port("CLK", IO::Input(1));
        delay.add_port("D", IO::Input(width));
        delay.add_port("Q", IO::Output(width));
        delay.add_port("taps", IO::Output(width * depth));
        delay.set_usage(Usage::EmitNothingAndStop);
        Ok(delay)
    })
    .unwrap();
    let m = ModDef::new("Pipeline");
    m.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let a = m.add_port("a", IO::Input(8));
    let y = m.add_port("y", IO::Output(8));
    y.connect_pipeline(
        &a,
        PipelineConfig {
            clk: "clk".into(),
            depth: 3,
            inst_name: Some("path".into()),
        },
    );
    let path = m.get_instance("path");
    assert_eq!(
        path.get_instance("delay").get_mod_def().get_name(),
        "Delay8x3"
    );
    let clk = m.get_port("clk");
    let q = a.delay_with(3, &clk);
    assert_eq!(q.get_mod_inst().unwrap().get_instances().len(), 1);
    m.add_port("q", IO::Output(8)).connect(&q);
    let sv = m.emit(Default::default());
    assert!(sv.contains("Delay8x3 delay") && sv.contains(".taps()"));
}

#[test]
fn rtl_delay_is_one_shift_assignment_and_muxes_stay_word_wide() {
    let m = ModDef::new("Delays");
    let clk = m.add_port("clk", IO::Input(1));
    m.set_default_clock(&clk.declare_clock().unwrap()).unwrap();
    let d = m.add_port("d", IO::Input(8));
    let rst = m.add_port("rst_n", IO::Input(1));
    let q = d.delay_with(3, Reset::async_low(&rst, 0xa5));
    m.add_port("q", IO::Output(8)).connect(&q);
    let wide_mask = &d & 0xff;
    m.add_port("mask", IO::Output(8)).connect(&wide_mask);
    let sv = m.emit(Default::default());
    assert_eq!(sv.matches("always_ff").count(), 1);
    assert!(sv.contains("logic [2:0][7:0] delay_stages;"));
    assert!(sv.contains("delay_stages <= {delay_stages[1:0], d};"));
    assert!(sv.contains("d & 8'hff"));
}

#[test]
fn zero_depth_explicit_defs_are_wiring_and_external_domains_cannot_clock_delays() {
    let m = ModDef::new("Bypass");
    m.set_primitive_policy(PrimitivePolicy::Forbid).unwrap();
    let i = m.add_port("i", IO::Input(8));
    let fifo = PipelineConfig::default();
    let a = m.instantiate(&fifo.to_mod_def(8), Some("a"), None);
    let b = m.instantiate(&fifo.to_mod_def(8), Some("b"), None);
    a.get_port("in").connect(&i);
    b.get_port("in").connect(&a.get_port("out"));
    m.add_port("o", IO::Output(8)).connect(&b.get_port("out"));
    m.set_primitive_policy(PrimitivePolicy::Allow).unwrap();
    m.set_default_clock(&m.declare_external_clock("domain").unwrap())
        .unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| i.delay(2))).is_err());
    assert_eq!(m.get_instances().len(), 2);
    let sv = m.emit(Default::default());
    assert!(sv.contains("assign o =") && !sv.contains("module topstitch_delay"));
    assert!(!sv.contains("always_ff") && !sv.contains("clk"));
}

#[test]
fn parameterized_delay_can_serve_multiple_depths() {
    let delay = ModDef::from_verilog(
        "DelayLine",
        "module DelayLine #(parameter WIDTH=1, DEPTH=1)(input CLK, input [WIDTH-1:0] D, output [WIDTH-1:0] Q); endmodule",
    ).unwrap();
    let mut map = GateMap::new();
    map.delay(
        FlopFlavor::posedge(),
        flop_pins(None),
        move |width, cycles| Ok(delay.parameterize(&[("WIDTH", width), ("DEPTH", cycles)])),
    )
    .unwrap();
    let m = ModDef::new("MultiDepth");
    m.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let d = m.add_port("data", IO::Input(12));
    let clk = m.add_port("clk", IO::Input(1));
    for n in [1, 2, 9] {
        let q = d.delay_with(n, &clk);
        m.add_port(format!("y{n}"), IO::Output(12)).connect(&q);
    }
    assert!(catch_unwind(AssertUnwindSafe(|| d.flop_with(&clk))).is_err());
    let sv = m.emit(Default::default());
    assert_eq!(sv.matches("DelayLine #(").count(), 3);
    assert_eq!(sv.matches(".WIDTH(32'h0000_000c)").count(), 3);
}

#[test]
fn bedrock_delay_maps_expressions_and_pipelined_connections_with_stage_taps() {
    let source = include_str!("fixtures/br_delay_nr.sv");
    let cell = ModDef::from_verilog("br_delay_nr", source).unwrap();
    let mut map = GateMap::new();
    map.delay_with_stages(
        FlopFlavor::posedge(),
        FlopPins {
            clk: "clk",
            d: "in",
            q: "out",
            reset: None,
        },
        "out_stages",
        move |width, depth| Ok(cell.parameterize(&[("Width", width), ("NumStages", depth)])),
    )
    .unwrap();

    let top = ModDef::new("Pipeline");
    top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let clk = top.add_port("clk", IO::Input(1));
    top.set_default_clock(&clk.declare_clock().unwrap())
        .unwrap();
    let d = top.add_port("d", IO::Input(5));
    let q = d.delay(3);
    top.add_port("q", IO::Output(5)).connect(&q);
    let a = top.add_port("a", IO::Input(4));
    top.add_port("b", IO::Output(4)).connect_pipeline(
        &a,
        PipelineConfig {
            clk: "clk".into(),
            depth: 2,
            inst_name: Some("pipe".into()),
        },
    );
    let delays = [
        (q.get_mod_inst().unwrap(), "d", 5, 3),
        (top.get_instance("pipe"), "p", 4, 2),
    ];
    for (delay, prefix, width, depth) in &delays {
        for stage in 0..=*depth {
            top.add_port(format!("{prefix}{stage}"), IO::Output(*width))
                .connect(&delay.delay_stage(stage));
        }
    }

    let sv = top.emit(Default::default());
    assert_eq!(sv.matches("br_delay_nr #(").count(), 2);
    for (delay, _, width, depth) in delays {
        let helper_name = delay.get_mod_def().get_name();
        let (_, body) = sv.split_once(&format!("module {helper_name}(")).unwrap();
        let (body, _) = body.split_once("endmodule").unwrap();
        assert!(body.contains("br_delay_nr #("));
        assert!(body.contains(&format!(".Width(32'h0000_{width:04x})")));
        assert!(body.contains(&format!(".NumStages(32'h0000_{depth:04x})")));
        for pin in ["clk", "in", "out", "out_stages"] {
            assert!(body.contains(&format!(".{pin}({pin})")));
        }
    }
    assert!(sv.contains(".out_stages({d3, d2, d1, d0})"));
    assert!(sv.contains(".out_stages({p2, p1, p0})"));
    ModDef::from_verilog("Pipeline", format!("{source}\n{sv}")).unwrap();
}

#[test]
fn bank_preserves_a_500_bit_reset_and_uses_one_instance_per_stage() {
    let bank = ModDef::from_verilog(
        "ResetBank",
        include_str!("../examples/input/logic_cells.sv"),
    )
    .unwrap();
    let mut map = library();
    let pins = flop_pins(Some("RN"));
    let flavor = FlopFlavor::negedge().reset_by_value(ResetKind::AsyncLow);
    map.flop_by_width(flavor, pins, move |width, reset| {
        Ok(bank.parameterize::<topstitch::ParameterValue>(&[
            ("WIDTH", width.into()),
            ("RESET_VALUE", reset.clone().into()),
        ]))
    })
    .unwrap();
    let m = ModDef::new("WideReset");
    m.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let clk = m.add_port("clk", IO::Input(1));
    let rn = m.add_port("rn", IO::Input(1));
    let data = m.add_port("data", IO::Input(500));
    let reset_hex = "a5".repeat(62) + "3";
    let other_hex = "b5".repeat(62) + "7";
    let options = (
        clk.negedge(),
        Reset::async_low(&rn, topstitch::hex(&reset_hex)),
    );
    let q = data.flop_with(&options);
    let delayed = data.delay_with(3, &options);
    for (name, output, depth) in [("q", q, 1), ("delayed", delayed, 3)] {
        let cells = output.get_mod_inst().unwrap().get_instances();
        assert_eq!(cells.len(), depth);
        for cell in cells {
            assert_eq!(cell.get_mod_def().get_name(), "ResetBank");
            assert_eq!(cell.get_port("D").io(), IO::Input(500));
        }
        m.add_port(name, IO::Output(500)).connect(&output);
    }
    let other = data.flop_with((
        clk.negedge(),
        Reset::async_low(&rn, topstitch::hex(&other_hex)),
    ));
    m.add_port("other", IO::Output(500)).connect(&other);
    let sv = m.emit(Default::default()).replace('_', "");
    assert_eq!(
        sv.matches(&format!(".RESETVALUE(500'h{reset_hex})"))
            .count(),
        4
    );
    assert_eq!(
        sv.matches(&format!(".RESETVALUE(500'h{other_hex})"))
            .count(),
        1
    );
}

#[test]
fn pipeline_uses_plain_banks_and_an_applicable_whole_delay_takes_precedence() {
    let mut map = library();
    let pins = flop_pins(None);
    let bank = ModDef::from_verilog(
        "Bank",
        "module Bank #(parameter WIDTH=1)(input CLK, input [WIDTH-1:0] D, output [WIDTH-1:0] Q); endmodule",
    ).unwrap();
    map.flop_by_width(FlopFlavor::posedge(), pins, move |width| {
        Ok(bank.parameterize(&[("WIDTH", width)]))
    })
    .unwrap();
    let delay = ModDef::from_verilog(
        "Line",
        "module Line #(parameter W=1,N=1)(input CLK, input [W-1:0] D, output [W-1:0] Q); endmodule",
    )
    .unwrap();
    for whole in [false, true] {
        let mut map = map.clone();
        if whole {
            let delay = delay.clone();
            map.delay(FlopFlavor::posedge(), pins, move |w, n| {
                Ok(delay.parameterize(&[("W", w), ("N", n)]))
            })
            .unwrap();
        }
        let m = ModDef::new("Top");
        m.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
            .unwrap();
        let d = m.add_port("d", IO::Input(8));
        let y = m.add_port("y", IO::Output(8));
        y.connect_pipeline(
            &d,
            PipelineConfig {
                depth: 3,
                inst_name: Some("path".into()),
                ..Default::default()
            },
        );
        let stages = m.get_instance("path").get_instances();
        assert_eq!(stages.len(), if whole { 1 } else { 3 });
        for stage in stages {
            assert_eq!(
                stage.get_mod_def().get_name(),
                if whole { "Line" } else { "Bank" }
            );
        }
        let q = d.flop_with(m.get_port("clk"));
        assert_eq!(
            q.get_mod_inst().unwrap().get_instances()[0]
                .get_mod_def()
                .get_name(),
            "Bank"
        );
        m.add_port("q", IO::Output(8)).connect(&q);
        m.emit(Default::default());
    }
}

#[test]
fn fixed_reset_bank_precedes_the_general_bank_only_for_its_uniform_value() {
    let pins = flop_pins(Some("RN"));
    let general = FlopFlavor::negedge().reset_by_value(ResetKind::AsyncLow);
    let clear = FlopFlavor::negedge().reset(ResetKind::AsyncLow, false);
    let mut map = library();
    assert!(
        map.flop_by_width(general, flop_pins(None), |_, _| unreachable!())
            .is_err()
    );
    let make = |name, width| {
        let bank = ModDef::new(name);
        bank.add_port("D", IO::Input(width));
        bank.add_port("Q", IO::Output(width));
        bank.add_port("CLK", IO::Input(1));
        bank.add_port("RN", IO::Input(1));
        bank.set_usage(Usage::EmitNothingAndStop);
        Ok(bank)
    };
    map.flop_by_width(general, pins, move |width, _| make("General", width))
        .unwrap();
    map.flop_by_width(clear, pins, move |width| make("ClearBank", width))
        .unwrap();
    assert!(
        map.flop_by_width(general, pins, |_, _| unreachable!())
            .is_err()
    );
    assert!(map.flop_by_width(clear, pins, |_| unreachable!()).is_err());
    let m = ModDef::new("ResetVariants");
    m.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let d = m.add_port("d", IO::Input(8));
    let clk = m.add_port("clk", IO::Input(1));
    let rn = m.add_port("rn", IO::Input(1));
    for (value, name) in [(0, "ClearBank"), (0xff, "General"), (0x42, "General")] {
        let q = d.flop_with((clk.negedge(), Reset::async_low(&rn, value)));
        assert_eq!(
            q.get_mod_inst()
                .unwrap()
                .get_instance("stage_0")
                .get_mod_def()
                .get_name(),
            name
        );
    }
}

#[test]
fn selected_bank_errors_do_not_silently_fall_back_to_scalar_flops() {
    for bad_pins in [false, true] {
        let mut map = library();
        map.flop_by_width(FlopFlavor::posedge(), flop_pins(None), move |_| {
            if bad_pins {
                Ok(cell("TooNarrow", &["D", "CLK"], "Q"))
            } else {
                Err("unsupported bank width".into())
            }
        })
        .unwrap();
        let m = ModDef::new("Filter");
        m.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
            .unwrap();
        let clk = m.add_port("clk", IO::Input(1));
        let d = m.add_port("d", IO::Input(8));
        assert!(catch_unwind(AssertUnwindSafe(|| d.flop_with(&clk))).is_err());
        assert!(m.get_instances().is_empty());
        assert!(d.get_connections().is_empty() && clk.get_connections().is_empty());
    }
}

#[test]
fn wrapping_a_mapped_operation_preserves_its_cells_and_explicit_requests_obey_policy() {
    let source = ModDef::new("Source");
    source
        .set_primitive_policy(PrimitivePolicy::RequireMapping(library()))
        .unwrap();
    let a = source.add_port("a", IO::Input(4));
    let gate = (&a & 15).get_mod_inst().unwrap().get_mod_def();
    let wrapper = gate.wrap(Some("Wrapper"), Some("placed"));
    assert_eq!(wrapper.get_instance("placed").get_instances().len(), 4);
    let sv = wrapper.emit(Default::default());
    assert_eq!(sv.matches("And2 bit_").count(), 4);
    assert!(sv.contains("Source__topstitch_and_4 placed"));
    let top = ModDef::new("Package");
    top.set_primitive_policy(PrimitivePolicy::Forbid).unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| top.instantiate(&gate, None, None))).is_err());
    assert!(top.get_instances().is_empty());
    top.instantiate(&wrapper, Some("wrapped"), None);
}

#[test]
fn generated_bank_is_shared_across_parents_flops_and_delay_stages() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let mut map = GateMap::new();
    let counter = calls.clone();
    map.flop_by_width(FlopFlavor::posedge(), flop_pins(None), move |width| {
        counter.fetch_add(1, Ordering::Relaxed);
        let bank = ModDef::new(format!("Bank{width}"));
        let d = bank.add_port("D", IO::Input(width));
        let clk = bank.add_port("CLK", IO::Input(1));
        bank.add_port("Q", IO::Output(width))
            .connect(&d.flop_with(&clk));
        Ok(bank)
    })
    .unwrap();
    let top = ModDef::new("Top");
    for (name, width) in [("First", 4), ("Second", 4), ("Third", 8)] {
        let m = ModDef::new(name);
        m.set_primitive_policy(PrimitivePolicy::RequireMapping(map.clone()))
            .unwrap();
        let d = m.add_port("d", IO::Input(width));
        let clk = m.add_port("clk", IO::Input(1));
        m.add_port("q", IO::Output(width))
            .connect(&d.flop_with(&clk));
        m.add_port("delayed", IO::Output(width))
            .connect(&d.delay_with(2, &clk));
        let inst = top.instantiate(&m, Some(name), None);
        inst.get_port("d").tieoff(0);
        inst.get_port("clk").tieoff(0);
        inst.get_port("q").unused();
        inst.get_port("delayed").unused();
    }
    let sv = top.emit(Default::default());
    assert_eq!(sv.matches("module Bank4(").count(), 1);
    assert_eq!(sv.matches("Bank4 stage_").count(), 6);
    assert_eq!(sv.matches("module Bank8(").count(), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
fn late_port_and_net_names_leave_graph_handles_stable_and_match_placement_paths() {
    for collision in ["port", "net", "pin_wire"] {
        let top = ModDef::new("Top");
        let inv = cell("Inv", &["A"], "Y");
        inv.set_usage(Usage::EmitStubAndStop);
        let mut map = GateMap::new();
        map.inv(inv, "A", "Y").unwrap();
        top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
            .unwrap();
        let a = top.add_port("a", IO::Input(1));
        let y = !&a;
        let key = y.get_mod_inst().unwrap().name().to_owned();
        match collision {
            "port" => top.add_port("not_0", IO::Output(1)).connect(&y),
            "net" => {
                y.specify_net_name("not_0");
                top.add_port("q", IO::Output(1)).connect(&(!&y));
            }
            _ => {
                top.add_port("not_0_y", IO::Output(1)).connect(&a);
                top.add_port("q", IO::Output(1)).connect(&(!&y));
            }
        }
        let sv = top.emit(Default::default());
        assert!(sv.contains("Top__topstitch_not_1 not_0_1 ("));
        assert!(!sv.contains('@'));
        if collision == "net" {
            assert!(sv.contains(".y(not_0)") && sv.contains(".a(not_0)"));
        } else if collision == "pin_wire" {
            assert!(sv.contains(".y(not_0_1_y)") && sv.contains(".a(not_0_1_y)"));
        }
        assert_eq!(top.get_instance(&key).get_port("y"), y);
        let (placements, _) = top.collect_placements_and_mod_defs(&Default::default());
        assert!(placements.contains_key("not_0_1/bit_0"));
    }
}

#[test]
fn a_factory_changing_the_parent_policy_cannot_publish_its_old_implementation() {
    let top = ModDef::new("Parent");
    let parent = top.clone();
    let cell =
        ModDef::from_verilog("Dff", "module Dff(input d, clk, output q); endmodule").unwrap();
    let mut map = GateMap::new();
    map.flop_by_width(
        FlopFlavor::posedge(),
        FlopPins {
            d: "d",
            q: "q",
            clk: "clk",
            reset: None,
        },
        move |_| {
            parent.set_primitive_policy(PrimitivePolicy::Allow)?;
            Ok(cell.clone())
        },
    )
    .unwrap();
    top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let d = top.add_port("d", IO::Input(1));
    let clk = top.add_port("clk", IO::Input(1));
    top.add_port("q", IO::Output(1)).connect(&d.flop_with(&clk));
    let sv = top.emit(Default::default());
    assert!(sv.contains("always_ff") && !sv.contains("Dff"));
    ModDef::from_verilog("Parent", sv).unwrap();
}

#[test]
fn recursive_cell_mappings_are_rejected_without_publishing_an_occurrence() {
    let self_cell = cell("Recursive", &["A", "B"], "Y");
    for candidate in [
        self_cell.clone(),
        self_cell.wrap(Some("Indirect"), Some("nested")),
    ] {
        let mut map = GateMap::new();
        map.and2(candidate, ["A", "B"], "Y").unwrap();
        assert!(
            self_cell
                .set_primitive_policy(PrimitivePolicy::RequireMapping(map))
                .unwrap_err()
                .contains("recursive")
        );
        assert!(self_cell.get_instances().is_empty());
    }

    let owner = ModDef::new("RecursiveBank");
    let d = owner.add_port("D", IO::Input(1));
    let clk = owner.add_port("CLK", IO::Input(1));
    owner.add_port("Q", IO::Output(1));
    let target = std::sync::Mutex::new(Some(owner.clone()));
    let mut map = GateMap::new();
    map.flop_by_width(FlopFlavor::posedge(), flop_pins(None), move |_| {
        Ok(target
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| cell("Dff", &["D", "CLK"], "Q")))
    })
    .unwrap();
    owner
        .set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| d.flop_with(&clk))).is_err());
    assert!(owner.get_instances().is_empty());
    let retried = d.flop_with(&clk);
    assert_eq!(
        retried
            .get_mod_inst()
            .unwrap()
            .get_instance("stage_0")
            .get_mod_def()
            .get_name(),
        "Dff"
    );
}

#[test]
fn bank_factory_rejects_its_own_key_and_can_retry_through_a_smaller_bank() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let top = Arc::new(ModDef::new("Top"));
    let policy_owner = Arc::downgrade(&top);
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = calls.clone();
    let recurse_once = AtomicBool::new(true);
    let scalar = cell("Dff", &["D", "CLK"], "Q");
    let mut map = GateMap::new();
    map.flop(FlopFlavor::posedge(), scalar.clone(), flop_pins(None))
        .unwrap();
    map.flop_by_width(FlopFlavor::posedge(), flop_pins(None), move |width| {
        factory_calls.fetch_add(1, Ordering::Relaxed);
        // The scalar base case bounds recursion even if the mapping guard is missing.
        if width == 1 && !recurse_once.swap(false, Ordering::Relaxed) {
            return Ok(scalar.clone());
        }
        let bank = ModDef::new(format!("Bank{width}"));
        bank.set_primitive_policy(policy_owner.upgrade().unwrap().primitive_policy())?;
        let data = bank.add_port("D", IO::Input(width));
        let clk = bank.add_port("CLK", IO::Input(1));
        let q = bank.add_port("Q", IO::Output(width));
        for bit in 0..width {
            q.bit(bit).connect(&data.bit(bit).flop_with(&clk));
        }
        Ok(bank)
    })
    .unwrap();
    top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let data = top.add_port("d", IO::Input(4));
    let clk = top.add_port("clk", IO::Input(1));

    let error = catch_unwind(AssertUnwindSafe(|| data.flop_with(&clk))).unwrap_err();
    let message = error.downcast_ref::<String>().unwrap();
    assert!(
        message.contains("recursive mapping factory") && message.contains("width 1"),
        "{message}"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(top.get_instances().is_empty());

    let q = data.flop_with(&clk);
    let bank = q.get_mod_inst().unwrap().get_instance("stage_0");
    assert_eq!(bank.get_instances().len(), 4);
    assert_eq!(calls.load(Ordering::Relaxed), 4);
}

#[test]
fn invalid_delay_tap_width_is_retried_before_sharing_the_valid_binding() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = calls.clone();
    let mut map = GateMap::new();
    map.delay_with_stages(
        FlopFlavor::posedge(),
        flop_pins(None),
        "TAPS",
        move |width, depth| {
            let first = factory_calls.fetch_add(1, Ordering::Relaxed) == 0;
            let words = if first { depth } else { depth + 1 };
            let line = ModDef::new("DelayLine");
            line.add_port("D", IO::Input(width));
            line.add_port("CLK", IO::Input(1));
            line.add_port("Q", IO::Output(width));
            line.add_port("TAPS", IO::Output(width * words));
            line.set_usage(Usage::EmitNothingAndStop);
            Ok(line)
        },
    )
    .unwrap();

    for name in ["First", "Second"] {
        let top = ModDef::new(name);
        top.set_primitive_policy(PrimitivePolicy::RequireMapping(map.clone()))
            .unwrap();
        let d = top.add_port("d", IO::Input(4));
        let clk = top.add_port("clk", IO::Input(1));
        if name == "First" {
            let error = catch_unwind(AssertUnwindSafe(|| d.delay_with(3, &clk))).unwrap_err();
            let message = error.downcast_ref::<String>().unwrap();
            assert!(
                message.contains("TAPS") && message.contains("16"),
                "{message}"
            );
            assert!(top.get_instances().is_empty());
            assert_eq!(calls.load(Ordering::Relaxed), 1);
        }
        let q = d.delay_with(3, &clk);
        let delay = q.get_mod_inst().unwrap();
        assert_eq!(
            delay.get_instance("delay").get_port("TAPS").io(),
            IO::Output(16)
        );
        assert_eq!(delay.delay_stage(3).width(), 4);
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }
}

#[test]
fn rejected_feedthroughs_leave_all_widths_and_hops_available_to_retry() {
    let blocked = ModDef::new("Blocked");
    blocked
        .set_primitive_policy(PrimitivePolicy::Forbid)
        .unwrap();
    let pipe = PipelineConfig {
        depth: 2,
        ..Default::default()
    };
    assert!(
        catch_unwind(AssertUnwindSafe(|| blocked.feedthrough_pipeline(
            "rx",
            "tx",
            4,
            pipe.clone()
        )))
        .is_err()
    );
    assert!(blocked.get_ports(None).is_empty());
    blocked.feedthrough("rx", "tx", 4);

    let top = ModDef::new("Top");
    top.add_port("bus_a", IO::Input(1));
    top.add_port("bus_b", IO::Input(4));
    let intf = top.def_intf_from_prefix("bus", "bus_");
    let limited = ModDef::new("Limited");
    let mut map = GateMap::new();
    map.flop_by_width(FlopFlavor::posedge(), flop_pins(None), |width| {
        if width != 1 {
            return Err("only scalar banks are available".into());
        }
        Ok(cell("Scalar", &["D", "CLK"], "Q"))
    })
    .unwrap();
    limited
        .set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| intf.feedthrough_pipeline(
            &limited,
            "i",
            "o",
            pipe.clone()
        )))
        .is_err()
    );
    assert!(limited.get_ports(None).is_empty() && limited.get_instances().is_empty());
    assert!(!limited.has_intf("i") && !limited.has_intf("o"));

    let driver = cell("Driver", &[], "Y");
    let a = top.instantiate(&driver, Some("driver"), None).get_port("Y");
    let first = ModDef::new("First");
    let first_inst = top.instantiate(&first, Some("first"), None);
    let last_inst = top.instantiate(&blocked, Some("last"), None);
    let q = top.add_port("q", IO::Output(1));
    assert!(
        catch_unwind(AssertUnwindSafe(|| a.connect_through_generic(
            &q,
            &[(&first_inst, None), (&last_inst, Some(pipe.clone()))],
            "hop"
        )))
        .is_err()
    );
    assert!(first.get_ports(None).is_empty() && a.get_connections().is_empty());
    a.connect_through(&q, &[&first_inst, &last_inst], "hop");
    assert!(first.has_port("hop_flipped") && blocked.has_port("hop_original"));
}

#[test]
fn stopping_at_a_wrapper_keeps_its_mapped_body_and_stops_at_library_instances() {
    let owner = ModDef::new("Owner");
    let inverter = cell("Inv", &["A"], "Y");
    inverter.set_usage(Usage::EmitStubAndStop);
    let mut map = GateMap::new();
    map.inv(inverter, "A", "Y").unwrap();
    owner
        .set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let pin = owner.add_port("d", IO::Input(2));
    let mapped = (!pin).get_mod_inst().unwrap().get_mod_def();
    let helper_header = format!("module {}(", mapped.get_name());
    let stopped = mapped.wrap(Some("Stopped"), None);
    stopped.set_usage(Usage::EmitDefinitionAndStop);
    let sv = stopped.emit(Default::default());
    assert!(sv.contains(&helper_header) && sv.contains("Inv bit_0 ("));
    assert!(!sv.contains("module Inv("));
    let open = mapped.wrap(Some("Open"), None);
    let root = ModDef::new("Root");
    let d = root.add_port("d", IO::Input(2));
    for def in [stopped, open] {
        let inst = root.instantiate(&def, None, None);
        inst.get_port("a").connect(&d);
        root.add_port(def.get_name(), IO::Output(2))
            .connect(&inst.get_port("y"));
    }
    let sv = root.emit(Default::default());
    assert_eq!(sv.matches(&helper_header).count(), 1);
    assert_eq!(sv.matches("module Inv(").count(), 1);
    let external = ModDef::from_verilog("Ext", "module Ext(input a); endmodule").unwrap();
    external.set_usage(Usage::EmitDefinitionAndStop);
    assert!(external.emit(Default::default()).is_empty());
}

#[test]
fn mapped_delays_expose_stage_taps_through_a_library_pin_or_flops() {
    let word = 4;
    for stage_pin in [true, false] {
        let mut map = library();
        let make = move |width, cycles| {
            let def = ModDef::new(format!("Line_{width}_{cycles}"));
            def.add_port("D", IO::Input(width));
            def.add_port("CLK", IO::Input(1));
            def.add_port("Q", IO::Output(width));
            if stage_pin {
                def.add_port("TAPS", IO::Output(width * (cycles + 1)));
            }
            def.set_usage(Usage::EmitNothingAndStop);
            Ok(def)
        };
        if stage_pin {
            map.delay_with_stages(FlopFlavor::posedge(), flop_pins(None), "TAPS", make)
                .unwrap();
        }
        let top = ModDef::new("Top");
        top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
            .unwrap();
        let clk = top.add_port("clk", IO::Input(1));
        top.set_default_clock(&clk.declare_clock().unwrap())
            .unwrap();
        let d = top.add_port("d", IO::Input(word));
        let q = d.delay(2);
        let delay = q.get_mod_inst().unwrap();
        let taps = delay.get_port("out_stages");
        assert_eq!(taps.io().width(), 12);
        top.add_port("tap1", IO::Output(word))
            .connect(&delay.delay_stage(1));
        top.add_port("q", IO::Output(word)).connect(&q);
        let sv = top.emit(Default::default());
        if stage_pin {
            assert!(sv.contains(".TAPS("));
        } else {
            assert_eq!(sv.matches("Dff").count(), 8);
            assert!(!sv.contains("Line_4_2 "));
        }
        let def = delay.get_mod_def();
        let clock = def.clocks().pop().unwrap();
        assert_eq!(
            def.clock_associations()
                .unwrap()
                .for_port(&def.get_port("out_stages"))[0]
                .role,
            topstitch::ClockRole::TimedBy(clock)
        );
    }
}

#[test]
#[should_panic(expected = "This mapped delay supplies only its final output")]
fn a_final_only_library_delay_cannot_supply_stage_taps() {
    let mut map = GateMap::new();
    map.delay(FlopFlavor::posedge(), flop_pins(None), |width, _| {
        let cell = ModDef::new("FinalOnly");
        cell.add_port("D", IO::Input(width));
        cell.add_port("CLK", IO::Input(1));
        cell.add_port("Q", IO::Output(width));
        cell.set_usage(Usage::EmitNothingAndStop);
        Ok(cell)
    })
    .unwrap();
    let top = ModDef::new("Top");
    top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let clk = top.add_port("clk", IO::Input(1));
    top.set_default_clock(&clk.declare_clock().unwrap())
        .unwrap();
    let d = top.add_port("d", IO::Input(4));
    let q = d.delay(2);
    let inst = q.get_mod_inst().unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| inst.get_port("out_stages"))).is_err());
    top.add_port("q", IO::Output(4)).connect(&q);
    assert!(top.emit(Default::default()).contains("FinalOnly delay"));
    inst.delay_stage(1);
}

#[test]
fn named_interface_pipelines_fail_before_creating_hardware_and_can_be_retried() {
    let source = ModDef::new("Source");
    source.add_port("bus_valid", IO::Input(1));
    source.add_port("bus_data", IO::Input(8));
    let bus = source.def_intf_from_prefix("bus", "bus_");
    let route = ModDef::new("Route");
    let named = PipelineConfig {
        depth: 1,
        inst_name: Some("stage".into()),
        ..Default::default()
    };
    assert!(
        catch_unwind(AssertUnwindSafe(
            || bus.feedthrough_pipeline(&route, "rx", "tx", named)
        ))
        .is_err()
    );
    assert!(route.get_ports(None).is_empty());
    assert!(route.get_instances().is_empty());
    assert!(!route.has_intf("rx") && !route.has_intf("tx"));
    bus.feedthrough_pipeline(
        &route,
        "rx",
        "tx",
        PipelineConfig {
            depth: 1,
            ..Default::default()
        },
    );
    assert_eq!(route.get_instances().len(), 2);
}

#[test]
fn inline_and_mapped_sv_elaborate_with_every_slice_format() {
    let cells = include_str!("../examples/input/logic_cells.sv");
    let cell = |name| ModDef::from_verilog(name, cells).unwrap();
    for mapped in [false, true] {
        let top = ModDef::new("Datapath");
        let and2 = cell("And2");
        if mapped {
            let mut map = GateMap::new();
            map.and2(and2.clone(), ["A", "B"], "Y").unwrap();
            map.or2(cell("Or2"), ["A", "B"], "Y").unwrap();
            map.mux2(
                cell("Mux2"),
                Mux2Pins {
                    select: "S",
                    when_true: "B",
                    when_false: "A",
                    out: "Y",
                },
            )
            .unwrap();
            map.flop(FlopFlavor::posedge(), cell("Dff"), flop_pins(None))
                .unwrap();
            top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
                .unwrap();
        }
        let clk = top.add_port("clk", IO::Input(1));
        let sel = top.add_port("sel", IO::Input(2));
        for w in [1, 4] {
            let a = top.add_port(format!("a{w}"), IO::Input(w));
            let b = top.add_port(format!("b{w}"), IO::Input(w));
            // This consumer is created before its driver.
            let early = top.instantiate(
                &top.primitive_def(Primitive::Or, w),
                Some(&format!("early{w}")),
                None,
            );
            let word = sel.select_from_or([&a, &b], 0);
            early.get_port("a").connect(&(&word & &a));
            early.get_port("b").connect(&b);

            let shared = early.get_port("y");
            let enable = &a | &b;
            enable.specify_net_name(&format!("en{w}"));
            let q = (&shared & &enable).delay_with(3, &clk);
            // Inline the final word into a consumer while independently reading a tap.
            top.add_port(format!("q{w}"), IO::Output(w))
                .connect(&(&q | &b));
            top.add_port(format!("tap{w}"), IO::Output(w))
                .connect(&q.get_mod_inst().unwrap().delay_stage(2));

            // A reduction whose sole use is an ordinary instance input can inline there.
            let flag = top.instantiate(&and2, Some(&format!("flag{w}")), None);
            flag.get_port("A").connect(&shared.or());
            flag.get_port("B").connect(&sel.bit(0));
            top.add_port(format!("flag_out{w}"), IO::Output(1))
                .connect(&flag.get_port("Y"));
        }

        for bitblast_assignments in [false, true] {
            for preserve_full_width_slices in [false, true] {
                for preserve_single_bit_slices in [false, true] {
                    let options = EmitOptions {
                        bitblast_assignments,
                        preserve_full_width_slices,
                        preserve_single_bit_slices,
                        ..Default::default()
                    };
                    let sv = format!("`default_nettype none\n{cells}\n{}", top.emit(options));
                    ModDef::from_verilog("Datapath", sv)
                        .unwrap_or_else(|err| panic!("mapped={mapped}, {options:?}: {err}"));
                }
            }
        }
    }
}

#[test]
fn mapped_helper_names_reserve_external_cells_even_at_a_stop_boundary() {
    let sources = "module Top__topstitch_and_2(input logic A, input logic B, output logic Y); assign Y = A & B; endmodule
                   module Top__topstitch_and_2_1(output logic spare); assign spare = 0; endmodule";
    let mut map = GateMap::new();
    map.and2(
        ModDef::from_verilog("Top__topstitch_and_2", sources).unwrap(),
        ["A", "B"],
        "Y",
    )
    .unwrap();
    let top = ModDef::new("Top");
    top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let a = top.add_port("a", IO::Input(2));
    let b = top.add_port("b", IO::Input(2));
    top.add_port("y", IO::Output(2)).connect(&(&a & &b));
    let extra = ModDef::from_verilog("Top__topstitch_and_2_1", sources).unwrap();
    top.instantiate(&extra, Some("extra"), None)
        .get_port("spare")
        .unused();
    top.set_usage(Usage::EmitDefinitionAndStop);
    let sv = top.emit(Default::default());
    assert!(sv.contains("module Top__topstitch_and_2_2("));
    assert!(sv.contains("Top__topstitch_and_2_2 and_0 ("));
    assert!(
        sv.contains("Top__topstitch_and_2 bit_0 (")
            && sv.contains("Top__topstitch_and_2_1 extra (")
    );
    ModDef::from_verilog("Top", format!("{sources}\n{sv}")).unwrap();
}

#[test]
fn independent_wrappers_keep_distinct_mapped_helpers_with_the_same_preferred_name() {
    let top = ModDef::new("Top");
    let d = top.add_port("d", IO::Input(1));
    let sources = "module InvA(input logic D, output logic Q); assign Q = ~D; endmodule
                   module InvB(input logic D, output logic Q); assign Q = ~D; endmodule";
    for (wrapper_name, cell_name) in [("Alpha", "InvA"), ("Beta", "InvB")] {
        let mut map = GateMap::new();
        map.inv(ModDef::from_verilog(cell_name, sources).unwrap(), "D", "Q")
            .unwrap();
        let builder = ModDef::new("Builder");
        builder
            .set_primitive_policy(PrimitivePolicy::RequireMapping(map))
            .unwrap();
        let pin = builder.add_port("d", IO::Input(1));
        let wrapper = (!pin)
            .get_mod_inst()
            .unwrap()
            .get_mod_def()
            .wrap(Some(wrapper_name), None);
        let inst = top.instantiate(&wrapper, None, None);
        inst.get_port("a").connect(&d);
        top.add_port(wrapper_name, IO::Output(1))
            .connect(&inst.get_port("y"));
    }
    let sv = top.emit(Default::default());
    assert_eq!(sv, top.emit(Default::default()));
    for (wrapper, helper, cell) in [
        ("Alpha", "Builder__topstitch_not_1", "InvA"),
        ("Beta", "Builder__topstitch_not_1_1", "InvB"),
    ] {
        let body = |name: &str| {
            sv.split_once(&format!("module {name}("))
                .unwrap()
                .1
                .split_once("endmodule")
                .unwrap()
                .0
        };
        assert!(body(wrapper).contains(&format!("{helper} not_0 (")));
        assert!(body(helper).contains(&format!("{cell} bit_0 (")));
    }
    ModDef::from_verilog("Top", format!("{sources}\n{sv}")).unwrap();

    top.stub_recursive("^Inv[AB]$");
    let stubbed = top.emit(Default::default());
    for cell in ["InvA", "InvB"] {
        assert_eq!(stubbed.matches(&format!("module {cell}(")).count(), 1);
    }
    ModDef::from_verilog("Top", stubbed).unwrap();
}

/// Provides pin locations so mapped interstage connections have a measurable distance.
fn placed_flop_map() -> GateMap {
    let flop = cell("Dff", &["D", "CLK"], "Q");
    for (name, x) in [("D", 0), ("Q", 8)] {
        flop.place_pin(
            name,
            0,
            topstitch::PhysicalPin::from_translation(
                "M1",
                topstitch::Polygon::from_width_height(2, 2),
                (x, 0).into(),
            ),
        );
    }
    let mut map = GateMap::new();
    map.flop(FlopFlavor::posedge(), flop, flop_pins(None))
        .unwrap();
    map
}

#[test]
fn mapped_delay_traces_only_taps_connected_on_the_occurrence() {
    let top = ModDef::new("Top");
    top.set_primitive_policy(PrimitivePolicy::RequireMapping(placed_flop_map()))
        .unwrap();
    let clk = top.add_port("clk", IO::Input(1));
    for (name, tap_used) in [("observed", true), ("unobserved", false)] {
        let d = top.add_port(format!("{name}_in"), IO::Input(1));
        let y = top.add_port(format!("{name}_out"), IO::Output(1));
        let q = d.delay_with(2, &clk);
        y.connect(&q);
        let delay = q.get_mod_inst().unwrap();
        let first = delay.get_instance("stage_0_bit_0");
        let last = delay.get_instance("stage_1_bit_0");
        let middle = first.get_port("Q").bit(0);
        let next = last.get_port("D").bit(0);
        if tap_used {
            let tap = top.add_port("tap", IO::Output(1));
            tap.connect(&delay.delay_stage(1));
            assert_eq!(tap.bit(0).trace_through_hierarchy(), Some(middle.clone()));
            let err =
                catch_unwind(AssertUnwindSafe(|| middle.trace_through_hierarchy())).unwrap_err();
            assert!(
                err.downcast_ref::<String>()
                    .unwrap()
                    .contains("due to fanout")
            );
            assert_eq!(middle.get_connection_distance(), None);
        } else {
            assert_eq!(middle.trace_through_hierarchy(), Some(next.clone()));
            assert_eq!(middle.get_connection_distance(), Some(6));
        }
        assert_eq!(next.trace_through_hierarchy(), Some(middle));
        for (left, right) in [
            (d.bit(0), first.get_port("D").bit(0)),
            (last.get_port("Q").bit(0), y.bit(0)),
        ] {
            assert_eq!(left.trace_through_hierarchy().as_ref(), Some(&right));
            assert_eq!(right.trace_through_hierarchy(), Some(left));
        }
    }
}

/// Wraps a two-stage delay whose individual mapped flops can be traced and measured.
fn wrapped_mapped_delay(width: usize) -> ModDef {
    let builder = ModDef::new("Builder");
    builder
        .set_primitive_policy(PrimitivePolicy::RequireMapping(placed_flop_map()))
        .unwrap();
    let d = builder.add_port("d", IO::Input(width));
    let clk = builder.add_port("clk", IO::Input(1));
    let delay = d.delay_with(2, &clk).get_mod_inst().unwrap();
    delay.get_mod_def().wrap(Some("DelayBlock"), Some("delay"))
}

#[test]
fn wrapped_delay_traces_a_tap_only_when_its_forwarded_bits_are_used() {
    for packed in [false, true] {
        check_forwarded_delay_taps(packed);
    }
}

/// Exports the same debug bus using direct slices or a concatenation.
fn check_forwarded_delay_taps(packed: bool) {
    let block = wrapped_mapped_delay(2);
    let adapter = ModDef::new("Adapter");
    let block = adapter.instantiate(&block, Some("block"), Some(&["in", "clk", "out"]));
    let taps = block.get_port("out_stages");
    let view = adapter.add_port("view", IO::Output(5));
    let status = adapter.add_port("status", IO::Input(3));
    if packed {
        let relay = ModDef::new("DebugFeedthrough");
        relay.feedthrough("input_bus", "output_bus", 5);
        let relay = adapter.instantiate(&relay, Some("debug_relay"), None);
        relay
            .get_port("input_bus")
            .connect(&concat([taps.slice(3, 2), status.slice(2, 0)]));
        view.connect(&relay.get_port("output_bus"));
    } else {
        view.slice(4, 3).connect(&taps.slice(3, 2));
        view.slice(2, 0).connect(&status);
    }
    taps.slice(1, 0).unused();
    taps.slice(5, 4).unused();

    let top = ModDef::new("Top");
    for observed in [false, true] {
        let name = if observed { "observed" } else { "open" };
        let inst = top.instantiate(&adapter, Some(name), Some(&["clk", "status"]));
        inst.get_port("in")
            .connect(&top.add_port(format!("{name}_d"), IO::Input(2)));
        inst.get_port("out")
            .connect(&top.add_port(format!("{name}_q"), IO::Output(2)));
        let view = inst.get_port("view");
        view.slice(2, 0).export_as(format!("{name}_status"));
        if observed {
            top.add_port("tap", IO::Output(1)).connect(&view.bit(4));
            view.bit(3).unused();
        } else {
            view.slice(4, 3).unused();
        }

        let delay = inst.get_instance("block").get_instance("delay");
        for bit in 0..2 {
            let q = delay
                .get_instance(format!("stage_0_bit_{bit}"))
                .get_port("Q")
                .bit(0);
            let d = delay
                .get_instance(format!("stage_1_bit_{bit}"))
                .get_port("D")
                .bit(0);
            assert_eq!(d.trace_through_hierarchy().as_ref(), Some(&q));
            if observed && bit == 1 {
                assert_eq!(
                    top.get_port("tap")
                        .bit(0)
                        .trace_through_hierarchy()
                        .as_ref(),
                    Some(&q)
                );
                let err =
                    catch_unwind(AssertUnwindSafe(|| q.trace_through_hierarchy())).unwrap_err();
                assert!(
                    err.downcast_ref::<String>()
                        .unwrap()
                        .contains("due to fanout")
                );
                assert_eq!(q.get_connection_distance(), None);
            } else {
                assert_eq!(q.trace_through_hierarchy().as_ref(), Some(&d));
                assert_eq!(q.get_connection_distance(), Some(6));
                assert_eq!(d.get_connection_distance(), Some(6));
            }
        }
    }
}

#[test]
fn final_delay_tap_traces_only_aliases_with_receivers() {
    let block = wrapped_mapped_delay(1);
    let delay = block.get_instance("delay");
    block
        .add_port("final_alias", IO::Output(1))
        .connect(&delay.delay_stage(2));
    let flop = delay.get_instance("stage_1_bit_0").get_mod_def();

    let top = ModDef::new("Top");
    let inst = top.instantiate(&block, Some("block"), Some(&["in", "clk"]));
    inst.get_port("out").unused();
    let taps = inst.get_port("out_stages");
    taps.slice(1, 0).unused();
    taps.bit(2).specify_net_name("final_tap");
    let receiver = top.instantiate(&flop, Some("receiver"), None);
    receiver.get_port("CLK").connect(&top.get_port("clk"));
    receiver.get_port("Q").unused();
    let d = receiver.get_port("D").bit(0);
    d.connect(&inst.get_port("final_alias"));

    let q = inst
        .get_instance("delay")
        .get_instance("stage_1_bit_0")
        .get_port("Q")
        .bit(0);
    assert_eq!(q.trace_through_hierarchy().as_ref(), Some(&d));
    assert_eq!(d.trace_through_hierarchy().as_ref(), Some(&q));
    assert_eq!(q.get_connection_distance(), Some(6));
    assert_eq!(d.get_connection_distance(), Some(6));

    let second = top.add_port("second", IO::Output(1));
    second.connect(&taps.bit(2));
    assert_eq!(second.trace_through_hierarchy().as_ref(), Some(&q));
    assert_eq!(d.trace_through_hierarchy().as_ref(), Some(&q));
    let err = catch_unwind(AssertUnwindSafe(|| q.trace_through_hierarchy())).unwrap_err();
    assert!(
        err.downcast_ref::<String>()
            .unwrap()
            .contains("due to fanout")
    );
    assert_eq!(q.get_connection_distance(), None);
}

#[test]
fn zero_cycle_delay_traces_only_the_connected_output_for_each_bit() {
    let top = ModDef::new("Top");
    let d = top.add_port("d", IO::Input(3));
    let y = top.add_port("y", IO::Output(3));
    let bypass = top.instantiate(
        &PipelineConfig::default().to_mod_def(3),
        Some("bypass"),
        None,
    );
    bypass.get_port("in").connect(&d);
    let q = bypass.get_port("out");
    let taps = bypass.get_port("out_stages");
    y.bit(0).connect(&q.bit(0));
    q.bit(1).unused();
    y.bit(1).connect(&taps.bit(1));
    y.bit(2).connect(&q.bit(2));
    top.add_port("also_used", IO::Output(1))
        .connect(&taps.bit(2));

    for bit in [0, 1] {
        assert_eq!(d.bit(bit).trace_through_hierarchy(), Some(y.bit(bit)));
        assert_eq!(y.bit(bit).trace_through_hierarchy(), Some(d.bit(bit)));
    }
    let err = catch_unwind(AssertUnwindSafe(|| d.bit(2).trace_through_hierarchy())).unwrap_err();
    assert!(
        err.downcast_ref::<String>()
            .unwrap()
            .contains("due to fanout")
    );
}

#[test]
fn late_signal_names_conflict_only_with_instances_emitted_in_sv() {
    for mapped in [false, true] {
        for name_port in [false, true] {
            let top = ModDef::new("Top");
            if mapped {
                top.set_primitive_policy(PrimitivePolicy::RequireMapping(library()))
                    .unwrap();
            }
            let d = top.add_port("d", IO::Input(1));
            let inv = top.instantiate(&top.primitive_def(Primitive::Not, 1), Some("invert"), None);
            inv.get_port("a").connect(&d);
            let y = inv.get_port("y");
            let port_name = if name_port {
                "invert"
            } else {
                y.specify_net_name("invert");
                "out"
            };
            top.add_port(port_name, IO::Output(1)).connect(&y);

            if mapped {
                let err =
                    catch_unwind(AssertUnwindSafe(|| top.emit(Default::default()))).unwrap_err();
                assert!(
                    err.downcast_ref::<String>()
                        .unwrap()
                        .contains("instance invert")
                );
            } else {
                ModDef::from_verilog("Top", top.emit(Default::default())).unwrap();
            }
        }
    }
}
