// SPDX-License-Identifier: Apache-2.0

use std::panic::{AssertUnwindSafe, catch_unwind};
use topstitch::{EmitOptions, IO, LogicPort, ModDef, Primitive, Reset};

#[test]
fn anonymous_and_and_explicit_and_have_the_same_pins_and_definition() {
    let top = ModDef::new("Top");
    let a = top.add_port("a", IO::Input(2));
    let b = top.add_port("b", IO::Input(2));
    let y = &a & &b;
    let anonymous = y.get_mod_inst().unwrap();
    let named = top.instantiate(&top.primitive_def(Primitive::And, 2), Some("u_and"), None);
    assert_eq!(
        anonymous.get_mod_def().get_port("a"),
        named.get_mod_def().get_port("a")
    );
    assert_eq!(y.get_connections().len(), 0);
    assert_eq!(anonymous.get_port("a").get_connections().len(), 1);
    assert_eq!(top.get_instances().len(), 2);
    assert_eq!(
        anonymous.get_mod_def().primitive_kind(),
        Some(Primitive::And)
    );
    named.get_port("a").connect(&a);
    named.get_port("b").connect(&b);
    top.add_port("explicit", IO::Output(2))
        .connect(&named.get_port("y"));
    let filtered = (&y | !&b) ^ a.slice(1, 0);
    filtered.specify_net_name("filtered");
    top.add_port("result", IO::Output(2)).connect(&filtered);

    let sv = top.emit(Default::default());
    assert!(sv.contains("assign explicit = a & b;") && sv.contains("wire [1:0] filtered"));
    assert!(!sv.contains('@'));
    assert!(!sv.contains("module topstitch_and_2("));
    assert_eq!(top.emit(Default::default()), sv);
}

#[test]
fn every_clock_and_reset_form_registers_the_selected_data() {
    let top = ModDef::new("Clocks");
    let clk = top.add_port("clk", IO::Input(1));
    let second = top.add_port("io_clk", IO::Input(1));
    let reset = top.add_port("reset", IO::Input(1));
    let data = top.add_port("d", IO::Input(5));
    top.set_default_clock(&clk.declare_clock().unwrap())
        .unwrap();
    let config = (second.negedge(), Reset::async_low(&reset, 21));
    let forms = [
        data.flop(),
        data.flop_with(&second),
        data.flop_with(Reset::sync_high(&reset, 3)),
        data.flop_with((&second, Reset::sync_low(&reset, 4))),
        data.flop_with(second.negedge()),
        data.flop_with(&config),
        data.flop_with((&clk, Reset::async_high(&reset, 2))),
    ];
    for (i, q) in forms.iter().enumerate() {
        top.add_port(format!("q{i}"), IO::Output(5)).connect(q);
    }
    let sv = top.emit(Default::default());
    assert_eq!(sv.matches("always_ff").count(), forms.len());
    assert!(sv.contains("negedge io_clk or negedge reset"));
    assert!(sv.contains("posedge clk or posedge reset"));
}

#[test]
fn asynchronous_reset_events_and_conditions_share_a_signal() {
    let top = ModDef::new("ResetOperands");
    let clk = top.add_port("clk", IO::Input(1));
    let reset = top.add_port("reset", IO::Input(1));
    let data = top.add_port("d", IO::Input(1));
    let q = data.flop_with((&clk, Reset::async_high(!&reset, 0)));
    top.add_port("q", IO::Output(1)).connect(&q);
    let delay = top.primitive_def(
        Primitive::Delay {
            cycles: 2,
            falling: false,
            reset: Some((topstitch::ResetKind::AsyncLow, 1u8.into())),
        },
        1,
    );
    let delay = top.instantiate(&delay, Some("delay_i"), None);
    delay.get_port("in").connect(&data);
    delay.get_port("clk").connect(&clk);
    delay.get_port("rst").tieoff(1);
    top.add_port("delayed", IO::Output(1))
        .connect(&delay.get_port("out"));

    for bitblast_assignments in [false, true] {
        let sv = top.emit(EmitOptions {
            bitblast_assignments,
            ..Default::default()
        });
        // A computed reset can already have a wire when assignments are bit-blasted.
        for (edge, rhs, active_low) in [("posedge", "~reset", false), ("negedge", "1'h1", true)] {
            let net = sv
                .lines()
                .find_map(|line| {
                    let (lhs, value) = line.trim().strip_prefix("assign ")?.split_once(" = ")?;
                    (value.strip_suffix(';') == Some(rhs)).then_some(lhs)
                })
                .unwrap_or_else(|| panic!("Missing reset wire for {rhs}:\n{sv}"));
            assert!(
                sv.contains(&format!("posedge clk or {edge} {net})")),
                "{sv}"
            );
            let sense = if active_low { "!" } else { "" };
            assert!(sv.contains(&format!("if ({sense}{net})")), "{sv}");
        }
    }
}

#[test]
fn muxes_accept_iterators_slices_and_empty_or_partial_tables() {
    let top = ModDef::new("Mux");
    let data = top.add_port("lanes", IO::Input(8));
    let sel = top.add_port("sel", IO::Input(3));
    let fallback = top.add_port("fallback", IO::Input(1));
    let empty = std::iter::empty::<topstitch::Port>();
    let choices = [
        sel.select_from((0..8).map(|i| data.bit(i))),
        sel.select_from_or((0..6).map(|i| data.bit(i)), &fallback),
        sel.select_from_or(empty, &fallback),
        sel.bit(0).select(data.bit(1), data.bit(0)),
    ];
    for (i, result) in choices.iter().enumerate() {
        top.add_port(format!("y{i}"), IO::Output(1)).connect(result);
    }
    let sv = top.emit(Default::default());
    assert!(sv.contains("sel[0] ? lanes[1] : lanes[0]"));
    assert!(sv.contains("assign y2 = fallback"));
    for bitblast_assignments in [false, true] {
        let sv = top.emit(EmitOptions {
            bitblast_assignments,
            ..Default::default()
        });
        assert_eq!(sv.matches("case (sel)").count(), 2, "{sv}");
        assert_eq!(sv.matches("3'd5:").count(), 2, "{sv}");
        assert_eq!(sv.matches("3'd7:").count(), 1, "{sv}");
        assert!(sv.contains("= '0;") && sv.contains("= fallback;"), "{sv}");
    }
}

#[test]
fn invalid_construction_leaves_no_partial_instances() {
    let top = ModDef::new("Top");
    let data = top.add_port("d", IO::Input(2));
    let sink = top.add_port("y", IO::Output(2));
    let other = ModDef::new("Elsewhere");
    let foreign = other.add_port("d", IO::Input(2));
    let reject = |f: &dyn Fn()| {
        assert!(catch_unwind(AssertUnwindSafe(f)).is_err());
        assert!(top.get_instances().is_empty());
        assert!(data.get_connections().is_empty());
    };
    reject(&|| {
        data.flop();
    });
    reject(&|| {
        data.select_from([&data; 3]);
    });
    reject(&|| {
        data.select_from([&data; 5]);
    });
    reject(&|| {
        data.select_from_or([&data; 5], &data);
    });
    reject(&|| {
        data.bit(0).select(&data, data.bit(0));
    });
    reject(&|| {
        let _ = &data & &foreign;
    });
    reject(&|| {
        let _ = &data | &sink;
    });
    reject(&|| {
        data.flop_with((&data, Reset::sync_low(data.bit(0), 0)));
    });
    reject(&|| {
        data.flop_with((data.bit(0), Reset::sync_low(data.bit(0), 4)));
    });
    top.set_default_clock(&top.declare_external_clock("off_chip").unwrap())
        .unwrap();
    reject(&|| {
        data.flop();
    });
}

#[test]
fn split_and_shared_outputs_materialize_without_leaking_graph_names() {
    let top = ModDef::new("Top");
    let a = top.add_port("a", IO::Input(4));
    let b = top.add_port("b", IO::Input(4));
    let gate = &a & &b;
    // Occupy the natural temporary name and connect different bit ranges.
    top.add_port("and_result", IO::Output(2))
        .connect(&gate.slice(1, 0));
    let high = gate.slice(3, 2);
    top.add_port("hi", IO::Output(2)).connect(&high);
    top.add_port("hi_copy", IO::Output(2)).connect(&high);
    let sv = top.emit(EmitOptions {
        bitblast_assignments: true,
        ..Default::default()
    });
    assert!(!sv.contains('@'));
    for bit in 0..4 {
        assert!(sv.contains(&format!("a[{bit}] & b[{bit}]")));
    }
}

#[test]
fn naming_the_consumer_pin_keeps_its_driver_in_emitted_logic() {
    let top = ModDef::new("Top");
    let a = top.add_port("a", IO::Input(1));
    let y = !&a;
    let z = !&y;
    z.get_mod_inst()
        .unwrap()
        .get_port("a")
        .specify_net_name("between");
    top.add_port("result", IO::Output(1)).connect(&z);
    let sv = top.emit(Default::default());
    assert!(sv.contains("assign between = ~a;"));
    assert!(sv.contains("assign result = ~between;"));
}

#[test]
fn nested_handles_preserve_occurrences_and_reject_cross_occurrence_wiring() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("out", IO::Output(1));
    let block = ModDef::new("Block");
    block.instantiate(&leaf, Some("leaf"), None);
    let clk = block.add_port("clk", IO::Input(1));
    block
        .set_default_clock(&clk.declare_clock().unwrap())
        .unwrap();
    let top = ModDef::new("Top");
    let a = top.instantiate(&block, Some("a"), None);
    let b = top.instantiate(&block, Some("b"), None);
    let lhs = a.get_instance("leaf").get_port("out");
    let rhs = b.get_instance("leaf").get_port("out");
    assert!(catch_unwind(AssertUnwindSafe(|| &lhs & &rhs)).is_err());
    let q = (!&lhs).flop();
    assert_eq!(q.get_mod_inst().unwrap().get_parent().unwrap().name(), "a");
    assert_eq!(a.get_instances().len(), 3);
    assert_eq!(b.get_instances().len(), 3);
}

#[test]
fn explicit_flop_supports_feedback_through_anonymous_gates() {
    let top = ModDef::new("Feedback");
    let clk = top.add_port("clk", IO::Input(1));
    let enable = top.add_port("toggle", IO::Input(1));
    let reg = top.primitive_def(
        Primitive::Flop {
            falling: false,
            reset: None,
        },
        1,
    );
    let state = top.instantiate(&reg, None, None);
    let q = state.get_port("q");
    let d = &q ^ &enable;
    state.get_port("d").connect(&d);
    state.get_port("clk").connect(&clk);
    top.add_port("value", IO::Output(1)).connect(&q);
    let sv = top.emit(Default::default());
    assert_eq!(sv.matches("always_ff").count(), 1);
    assert!(sv.contains("^ toggle"));
    assert!(!sv.contains("flop@"));
}

#[test]
fn packed_fields_support_bitwise_operators_at_their_declared_offsets() {
    let rtl = "package p; typedef struct packed { logic flag; logic [2:0] value; } item; endpackage
               module Sink(input p::item [1:0] items); endmodule";
    let top = ModDef::new("Top");
    let sink = ModDef::from_verilog("Sink", rtl).unwrap();
    top.instantiate(&sink, Some("sink"), Some(&["items"]));
    let items = top.get_port("items");
    let upper = items.element(1).field("value");
    let lower = items.element(0).field("value");

    top.add_port("delta", IO::Output(3))
        .connect(&(&upper ^ lower));
    top.add_port("disabled", IO::Output(1))
        .connect(&!items.element(1).field("flag"));
    top.add_port("masked", IO::Output(3))
        .connect(&(3u8 & upper));

    let sv = top.emit(Default::default());
    assert!(
        sv.contains("assign delta = items[6:4] ^ items[2:0];"),
        "{sv}"
    );
    assert!(sv.contains("assign disabled = ~items[7];"), "{sv}");
    assert!(sv.contains("assign masked = 3'h3 & items[6:4];"), "{sv}");
    ModDef::from_verilog("Top", format!("{rtl}\n{sv}")).unwrap();
}

#[test]
fn reductions_have_a_word_input_and_a_single_bit_output() {
    let top = ModDef::new("Reductions");
    let data = top.add_port("data", IO::Input(8));
    let inputs = [
        ("all_set", data.and(), Primitive::ReduceAnd, 8, "&data"),
        (
            "any_set",
            data.slice(5, 2).or(),
            Primitive::ReduceOr,
            4,
            "|data[5:2]",
        ),
        (
            "parity",
            data.bit(0).xor(),
            Primitive::ReduceXor,
            1,
            "^data[0]",
        ),
    ];
    for (name, out, kind, width, _) in &inputs {
        let def = out.get_mod_inst().unwrap().get_mod_def();
        assert_eq!(def.primitive_kind().as_ref(), Some(kind));
        assert_eq!(def.get_port("a").io().width(), *width);
        assert_eq!(out.io().width(), 1);
        top.add_port(name, IO::Output(1)).connect(out);
    }
    let sv = top.emit(Default::default());
    for (name, _, _, _, expr) in inputs {
        assert!(sv.contains(&format!("assign {name} = {expr};")), "{sv}");
    }
}

#[test]
fn gates_keep_literal_pins_and_preserve_operand_order() {
    use num_bigint::{BigInt, BigUint};
    use topstitch::{ConnectedItem, Port, bits, hex};

    let top = ModDef::new("LiteralGates");
    let bus = top.add_port("bus", IO::Input(500));
    let ones16 = BigUint::from(0xffffu32);
    let sparse = (&ones16 << 484usize) | &ones16;
    let alternating = hex("a".repeat(125));
    let low: Port = 0 | &bus;
    let outputs = [
        ("low", low, Primitive::Or, "a", BigUint::from(0u8), 500),
        ("wide", &bus & &sparse, Primitive::And, "b", sparse, 500),
        (
            "pattern",
            hex("a".repeat(125)) ^ bus.clone(),
            Primitive::Xor,
            "a",
            alternating,
            500,
        ),
        (
            "middle",
            bits(16, 1) & bus.slice(400, 385),
            Primitive::And,
            "a",
            1u8.into(),
            16,
        ),
        (
            "bit_out",
            &bus.bit(399) | 1,
            Primitive::Or,
            "b",
            1u8.into(),
            1,
        ),
        (
            "low_byte",
            BigInt::from(0xd6) ^ &bus.slice(7, 0),
            Primitive::Xor,
            "a",
            0xd6u8.into(),
            8,
        ),
    ];
    for (name, out, kind, tied_pin, value, width) in outputs {
        let gate = out.get_mod_inst().unwrap();
        assert_eq!(gate.get_mod_def().primitive_kind(), Some(kind));
        let connections = gate.get_port(tied_pin).get_connections();
        assert_eq!(connections.len(), 1);
        assert_eq!(
            connections[0].other,
            ConnectedItem::Tieoff(bits(width, value))
        );
        top.add_port(name, IO::Output(width)).connect(&out);
    }
    assert_eq!(top.get_instances().len(), 6);
    let sv = top.emit(Default::default()).replace('_', "");
    assert!(
        sv.contains(&format!(
            "bus & 500'h{:0>125}",
            ((&ones16 << 484usize) | &ones16).to_str_radix(16)
        )),
        "{sv}"
    );
    assert!(
        sv.contains(&format!("500'h{} ^ bus", "a".repeat(125))),
        "{sv}"
    );
    assert!(
        sv.lines()
            .any(|line| line.contains("assign low = ") && line.contains("| bus;")),
        "{sv}"
    );
}

#[test]
fn mux_literals_infer_width_from_data_including_the_fallback() {
    use topstitch::{ConnectedItem, LogicInput, bits, hex};

    let top = ModDef::new("LiteralMux");
    let data = top.add_port("data", IO::Input(8));
    let sel = top.add_port("sel", IO::Input(2));
    let mixed: [LogicInput; 4] = [0.into(), (&data).into(), hex("0Xa_A").into(), 1u8.into()];
    let choices = [
        (sel.bit(0).select(&data, 0), "d0", 0),
        (sel.bit(1).select(0x7f, &data), "d1", 0x7f),
        (sel.bit(0).select(bits(8, 3), 0), "d0", 0),
        (sel.select_from(&mixed), "d2", 0xaa),
        (sel.select_from_or([0, 0x80], &data), "d1", 0x80),
        (sel.select_from_or([&data], 0xa5), "fallback", 0xa5),
        (
            sel.select_from_or(std::iter::empty::<u8>(), bits(8, 0xee)),
            "fallback",
            0xee,
        ),
    ];
    for (i, (out, pin, val)) in choices.into_iter().enumerate() {
        let gate = out.get_mod_inst().unwrap();
        assert!(matches!(
            gate.get_mod_def().primitive_kind(),
            Some(Primitive::Select { .. })
        ));
        assert_eq!(
            gate.get_port(pin).get_connections()[0].other,
            ConnectedItem::Tieoff(bits(8, val))
        );
        top.add_port(format!("y{i}"), IO::Output(8)).connect(&out);
    }
    let sv = top.emit(Default::default());
    assert!(sv.contains("sel[0] ? data : 8'h00"), "{sv}");
    let split = top.emit(EmitOptions {
        bitblast_assignments: true,
        ..Default::default()
    });
    assert_eq!(split.matches("case (sel)").count(), 3, "{split}");
}

#[test]
fn invalid_literals_do_not_add_gates_or_connections() {
    use num_bigint::{BigInt, BigUint};
    use topstitch::{bits, hex};

    let top = ModDef::new("RejectedLiterals");
    let data = top.add_port("data", IO::Input(500));
    let sel = top.add_port("sel", IO::Input(1));
    let reject = |f: &dyn Fn()| {
        assert!(catch_unwind(AssertUnwindSafe(f)).is_err());
        assert!(top.get_instances().is_empty());
        assert!(sel.get_connections().is_empty());
        assert!(data.get_connections().is_empty());
    };
    reject(&|| {
        let _ = &data & (BigUint::from(1u8) << 500usize);
    });
    reject(&|| {
        let _ = BigInt::from(-1) | &data;
    });
    reject(&|| {
        let _ = sel.select(&data, -1);
    });
    reject(&|| {
        let _ = sel.select(1, 0);
    });
    reject(&|| {
        let _ = sel.select(bits(7, 1), bits(8, 2));
    });
    reject(&|| {
        let _ = sel.select_from_or([0], 0);
    });
    reject(&|| {
        let _ = &data ^ bits(501, 0);
    });
    reject(&|| {
        let _ = bits(3, 8);
    });
    reject(&|| {
        let _ = bits(0, 0);
    });
    reject(&|| {
        let _ = hex("0xg");
    });
}

#[test]
fn hierarchy_trace_stops_at_logic_and_preserves_concat_and_zero_delay_bit_offsets() {
    let top = ModDef::new("Top");
    let a = top.add_port("a", IO::Input(3));
    let b = top.add_port("b", IO::Input(3));
    let gate = &a & &b;
    let y = top.add_port("y", IO::Output(3));
    y.connect(&gate);
    assert_eq!(y.bit(1).trace_through_hierarchy(), Some(gate.bit(1)));
    let pure = ModDef::new("Wires");
    pure.set_primitive_policy(topstitch::PrimitivePolicy::Forbid)
        .unwrap();
    let hi = pure.add_port("hi", IO::Input(3));
    let lo = pure.add_port("lo", IO::Input(5));
    let bus = topstitch::concat([&hi, &lo]);
    let pass = pure.instantiate(
        &topstitch::PipelineConfig::default().to_mod_def(8),
        Some("bypass"),
        None,
    );
    pass.get_port("in").connect(&bus);
    let out = pure.add_port("out", IO::Output(8));
    out.connect(&pass.get_port("out"));
    assert_eq!(
        out.slice(7, 6).trace_through_hierarchy(),
        Some(hi.slice(2, 1))
    );
    assert_eq!(lo.bit(2).trace_through_hierarchy(), Some(out.bit(2)));
    assert_eq!(out.slice(5, 4).trace_through_hierarchy(), None);
}

#[test]
fn primitive_usage_requires_an_explicit_wrapper() {
    use topstitch::Usage;
    let top = ModDef::new("Top");
    let d = top.add_port("d", IO::Input(4));
    let inv = !&d;
    top.add_port("y", IO::Output(4)).connect(&inv);
    let def = inv.get_mod_inst().unwrap().get_mod_def();
    for usage in [
        Usage::EmitNothingAndStop,
        Usage::EmitStubAndStop,
        Usage::EmitDefinitionAndStop,
    ] {
        assert!(catch_unwind(AssertUnwindSafe(|| def.set_usage(usage))).is_err());
        assert_eq!(def.get_usage(), Usage::EmitDefinitionAndDescend);
    }
    assert!(catch_unwind(AssertUnwindSafe(|| def.emit(Default::default()))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| def.stub("Stub"))).is_err());
    top.stub_recursive("^topstitch_not");
    let sv = top.emit(Default::default());
    assert!(sv.contains("assign y = ~d;") && !sv.contains("module topstitch_not"));
    assert!(
        top.collect_placements_and_mod_defs(&Default::default())
            .0
            .is_empty()
    );

    let wrapper = def.wrap(Some("Inverter"), Some("logic_inst"));
    wrapper.set_usage(Usage::EmitDefinitionAndStop);
    let sv = wrapper.emit(Default::default());
    assert!(sv.contains("module Inverter(") && sv.contains("assign y = ~a;"));
    wrapper.set_usage(Usage::EmitStubAndStop);
    let sv = wrapper.emit(Default::default());
    assert!(sv.contains("module Inverter(") && !sv.contains("assign"));
    wrapper.set_usage(Usage::EmitNothingAndStop);
    assert!(wrapper.emit(Default::default()).is_empty());
}

#[test]
fn trace_through_two_occurrences_of_the_same_concat_in_both_directions() {
    let top = ModDef::new("Top");
    let a = top.add_port("a", IO::Input(1));
    let b = top.add_port("b", IO::Input(1));
    let c = top.add_port("c", IO::Input(1));
    let first = topstitch::concat([&a, &b]);
    let second = topstitch::concat([first.bit(1), c.bit(0)]);
    let out = top.add_port("out", IO::Output(2));
    out.connect(&second);
    top.add_port("low", IO::Output(1)).connect(&first.bit(0));
    assert_eq!(a.trace_through_hierarchy(), Some(out.bit(1)));
    assert_eq!(out.bit(1).trace_through_hierarchy(), Some(a.bit(0)));
}

#[test]
#[allow(
    clippy::erasing_op,
    reason = "The overloaded AND must emit a gate even when an operand is zero."
)]
fn an_explicitly_unused_result_still_emits_the_requested_gate() {
    let top = ModDef::new("Top");
    let d = top.add_port("d", IO::Input(4));
    (&d & 0).unused();
    for (bitblast_assignments, gates) in [(false, 1), (true, 4)] {
        let sv = top.emit(EmitOptions {
            bitblast_assignments,
            ..Default::default()
        });
        assert_eq!(sv.matches(" & ").count(), gates);
        assert!(!sv.contains("module topstitch_and"));
    }
}

#[test]
fn sequential_definitions_expose_their_clock_and_reset_roles() {
    use topstitch::{Clock, ClockRole};
    let top = ModDef::new("Top");
    let clk = top.add_port("clk", IO::Input(1));
    top.set_default_clock(&clk.declare_clock().unwrap())
        .unwrap();
    let d = top.add_port("d", IO::Input(8));
    let rst = top.add_port("rst", IO::Input(1));
    for (result, input, output, reset_role) in [
        (d.flop_with(Reset::async_low(&rst, 0)), "d", "q", true),
        (
            d.delay_with(3, Reset::sync_high(&rst, 3)),
            "in",
            "out",
            false,
        ),
    ] {
        let def = result.get_mod_inst().unwrap().get_mod_def();
        let clock = def.clocks().pop().unwrap();
        assert!(matches!(clock, Clock::Pin(_)));
        let roles = def.clock_associations().unwrap();
        for pin in [input, output] {
            assert_eq!(
                roles.for_port(&def.get_port(pin))[0].role,
                ClockRole::TimedBy(clock.clone())
            );
        }
        assert_eq!(
            roles.for_port(&def.get_port("clk"))[0].role,
            ClockRole::ClockPin
        );
        assert_eq!(
            roles.for_port(&def.get_port("rst"))[0].role,
            if reset_role {
                ClockRole::Async
            } else {
                ClockRole::TimedBy(clock)
            }
        );
    }
}

#[test]
fn pipeline_stage_bus_includes_input_and_all_register_stages() {
    for depth in [0, 1, 3] {
        let cfg = topstitch::PipelineConfig {
            depth,
            ..Default::default()
        };
        assert_eq!(
            cfg.to_mod_def(4).get_port("out_stages").io().width(),
            4 * (depth + 1)
        );
        let top = ModDef::new("Top");
        if depth > 0 {
            let clk = top.add_port("clk", IO::Input(1));
            top.set_default_clock(&clk.declare_clock().unwrap())
                .unwrap();
        }
        let d = top.add_port("d", IO::Input(4));
        let (q, delay) = if depth == 0 {
            let delay = top.instantiate(&cfg.to_mod_def(4), Some("bypass"), None);
            delay.get_port("in").connect(&d);
            (delay.get_port("out"), delay)
        } else {
            let q = d.delay(depth);
            let delay = q.get_mod_inst().unwrap();
            (q, delay)
        };
        top.add_port("q", IO::Output(4)).connect(&q);
        top.add_port("tap", IO::Output(4))
            .connect(&delay.delay_stage(0));
        assert_eq!(delay.delay_stage(depth).width(), 4);
        assert!(catch_unwind(AssertUnwindSafe(|| delay.delay_stage(depth + 1))).is_err());
        for bitblast_assignments in [false, true] {
            let sv = top.emit(EmitOptions {
                bitblast_assignments,
                ..Default::default()
            });
            assert_eq!(sv.matches("always_ff").count(), usize::from(depth > 0));
        }
    }
}

#[test]
fn primitive_inputs_can_read_inout_values() {
    let pad_sv = "module Pad(inout wire [3:0] p, input wire drive); assign p = drive ? 4'h5 : 4'hz; endmodule";
    let cell = ModDef::from_verilog("Pad", pad_sv).unwrap();
    let top = ModDef::new("Top");
    let pad = top.add_port("pad", IO::InOut(4));
    let drive = top.add_port("drive", IO::Input(1));
    let clk = top.add_port("clk", IO::Input(1));
    let reset = top.add_port("reset", IO::InOut(1));
    let inst = top.instantiate(&cell, Some("driver"), None);
    inst.get_port("drive").connect(&drive);
    let child_pad = inst.get_port("p");
    child_pad.connect(&pad);

    assert_eq!(topstitch::concat([&pad]), pad);
    assert_eq!(child_pad.delay(0), child_pad);
    let values = [
        ("inverted", !&pad),
        ("masked", &child_pad & 7),
        ("parity", pad.xor()),
        ("chosen", pad.bit(0).select(&child_pad, 2)),
        (
            "joined",
            topstitch::concat([pad.slice(1, 0), child_pad.slice(3, 2)]),
        ),
        ("q", pad.flop_with((&clk, Reset::async_high(&reset, 9)))),
    ];
    for (name, value) in values {
        top.add_port(name, IO::Output(value.io().width()))
            .connect(&value);
    }
    let before = top.get_instances().len();
    let other = ModDef::new("Other");
    let foreign = other.add_port("foreign", IO::InOut(4));
    assert!(catch_unwind(AssertUnwindSafe(|| &pad ^ &foreign)).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| pad.flop_with(pad.bit(0)))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| !top.get_port("q"))).is_err());
    assert_eq!(top.get_instances().len(), before);

    for bitblast_assignments in [false, true] {
        let sv = top.emit(EmitOptions {
            bitblast_assignments,
            ..Default::default()
        });
        ModDef::from_verilog("Top", format!("{pad_sv}\n{sv}")).unwrap();
    }
}

#[test]
fn concatenated_inout_values_and_bidirectional_slice_connections() {
    let rtl = "module Driver(inout wire [3:0] whole, joined, direct, part);
                 assign whole = 4'h5, joined = 4'h5, direct = 4'h5, part = 4'h5;
               endmodule";
    let top = ModDef::new("Top");
    let driver = top.instantiate(
        &ModDef::from_verilog("Driver", rtl).unwrap(),
        Some("driver"),
        None,
    );
    let pads = |name| top.add_port(name, IO::InOut(4));

    let whole = pads("whole");
    driver
        .get_port("whole")
        .connect(&topstitch::concat([&whole]).delay(0));

    let joined = pads("joined");
    let value = topstitch::concat([joined.slice(1, 0), joined.slice(3, 2)]);
    value.specify_net_name("joined_value");
    driver.get_port("joined").connect(&value);

    let direct = pads("direct");
    let pin = driver.get_port("direct");
    pin.slice(3, 2).connect(&direct.slice(1, 0));
    pin.slice(1, 0).connect(&direct.slice(3, 2));

    let wide = top.add_port("wide", IO::InOut(5));
    let part = wide.slice(3, 0).delay(0);
    part.specify_net_name("part_value");
    driver.get_port("part").connect(&part);
    wide.bit(4).unused();

    let sv = top.emit(Default::default());
    for binding in [
        ".whole(whole)",
        ".joined(joined_value)",
        ".direct({direct[1:0], direct[3:2]})",
        ".part(part_value)",
    ] {
        assert!(sv.contains(binding), "{sv}");
    }
    assert!(
        sv.contains("assign joined_value = {joined[1:0], joined[3:2]};"),
        "{sv}"
    );
    assert!(sv.contains("assign part_value = wide[3:0];"), "{sv}");
    ModDef::from_verilog("Top", format!("{rtl}\n{sv}")).unwrap();
}

#[test]
fn gate_driving_an_input_shared_with_an_inout_keeps_the_resolved_net() {
    let cell_sv = "module Pad(inout wire [3:0] p, input wire drive); assign p = drive ? 4'hf : 4'hz; endmodule";
    let cell = ModDef::from_verilog("Pad", cell_sv).unwrap();
    let top = ModDef::new("Top");
    let a = top.add_port("a", IO::Input(4));
    let b = !&a;
    let c = !&b;
    let pad = top.instantiate(&cell, Some("pad"), None);
    pad.get_port("drive")
        .connect(&top.add_port("drive", IO::Input(1)));
    pad.get_port("p")
        .connect(&c.get_mod_inst().unwrap().get_port("a"));
    top.add_port("y", IO::Output(4)).connect(&c);

    let sv = top.emit(Default::default());
    assert!(
        sv.contains(".p(not_result)") && sv.contains("assign y = ~not_result;"),
        "{sv}"
    );
    ModDef::from_verilog("Top", format!("{cell_sv}\n{sv}")).unwrap();
}

#[test]
fn long_priority_mux_chains_materialize_intermediate_results() {
    const WIDTH: usize = 256;
    let top = ModDef::new("PriorityMux");
    let enables = top.add_port("enables", IO::Input(WIDTH));
    let data = top.add_port("data", IO::Input(WIDTH));
    let mut result = top.add_port("fallback", IO::Input(1));
    for i in 0..WIDTH {
        result = enables.bit(i).select(data.bit(i), result);
    }
    top.add_port("y", IO::Output(1)).connect(&result);

    let sv = top.emit(Default::default());
    assert_eq!(sv.matches(" ? ").count(), WIDTH);
    assert!(sv.matches("assign ").count() > 1);
    ModDef::from_verilog("PriorityMux", sv).unwrap();
}

#[test]
fn primitive_bodies_reject_wiring_edits_without_changing_other_occurrences() {
    use topstitch::{FlopFlavor, FlopPins, GateMap, PipelineConfig, PrimitivePolicy};
    let reject = |edit: &dyn Fn()| {
        let panic = catch_unwind(AssertUnwindSafe(edit)).unwrap_err();
        assert!(
            panic
                .downcast_ref::<String>()
                .unwrap()
                .contains("fixed internal wiring")
        );
    };
    for mapped in [false, true] {
        let top = ModDef::new("Top");
        let cell = ModDef::new("Bank");
        if mapped {
            cell.add_port("D", IO::Input(1));
            cell.add_port("C", IO::Input(1));
            cell.add_port("Q", IO::Output(1));
            cell.set_usage(topstitch::Usage::EmitStubAndStop);
            let mut map = GateMap::new();
            map.flop(
                FlopFlavor::posedge(),
                cell.clone(),
                FlopPins {
                    d: "D",
                    clk: "C",
                    q: "Q",
                    reset: None,
                },
            )
            .unwrap();
            top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
                .unwrap();
        }

        let clk = top.add_port("clk", IO::Input(1));
        let a = top.add_port("a", IO::Input(1));
        let b = top.add_port("b", IO::Input(1));
        let qa = a.flop_with(&clk);
        qa.specify_net_name("qa_result");
        top.add_port("ya", IO::Output(1)).connect(&qa);
        let qb = b.flop_with(&clk);
        let yb = top.add_port("yb", IO::Output(1));
        yb.connect(&qb);
        let before = top.emit(Default::default());
        let endpoint = yb.bit(0).trace_through_hierarchy().unwrap();
        let first = qa.get_mod_inst().unwrap();
        let def = first.get_mod_def();
        let d = def.get_port("d");
        let q = def.get_port("q");
        let links = q.get_connections().len();
        reject(&|| q.connect(&d));
        reject(&|| q.tieoff(0));
        reject(&|| d.unused_or_tieoff(0));
        reject(&|| {
            q.connect_pipeline(
                &d,
                PipelineConfig {
                    depth: 1,
                    ..Default::default()
                },
            )
        });
        assert_eq!(q.get_connections().len(), links);

        if mapped {
            let stage = first.get_instance("stage_0_bit_0");
            let stage_d = stage.get_port("D");
            reject(&|| stage_d.connect(&d));
            reject(&|| stage_d.tieoff(0));
            reject(&|| stage.get_port("Q").unused());
            for _ in 0..2 {
                reject(&|| stage_d.specify_net_name("internal"));
            }
            reject(&|| d.connect_through(&q, &[&stage], "route"));
            assert!(!cell.has_port("route_flipped"));
        }

        assert_eq!(yb.bit(0).trace_through_hierarchy(), Some(endpoint));
        assert_eq!(top.emit(Default::default()), before);
        let spare = top.add_port("spare", IO::Output(1));
        reject(&|| spare.todo_jam_connect(&[&a, &d]));
        assert!(spare.get_connections().is_empty());
    }
}
