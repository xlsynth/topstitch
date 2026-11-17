// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use num_bigint::BigUint;
use std::{collections::HashSet, fs, path::Path, process::Command};
use topstitch::{ConnectedItem, EmitOptions, Funnel, IO, ModDef};
use xlsynth_codegen::{BlockCodegenOptions, emit_system_verilog};
use xlsynth_pir::ir_parser::Parser;

use super::routed_pair;

#[path = "designs.rs"]
mod designs;

#[path = "external.rs"]
mod external;

#[test]
fn hierarchical_fanout_and_sv_formatting_options() -> Result<()> {
    for top in [designs::bitblast_assignments(), routed_pair(true)] {
        let candidate = ir_sv(&top)?;
        for options in [
            EmitOptions::default(),
            EmitOptions {
                bitblast_assignments: true,
                preserve_single_bit_slices: true,
                preserve_full_width_slices: true,
                ..Default::default()
            },
        ] {
            compare(&top, &top.emit(options), &candidate, true)?;
        }
    }
    Ok(())
}

#[test]
fn named_receivers_keep_bit_order_and_fanout() -> Result<()> {
    let top = named_receivers();
    check_design(&top)
}

#[test]
fn partial_names_preserve_driver_and_receiver_bit_indices() -> Result<()> {
    let pass = ModDef::new("Pass");
    pass.feedthrough("d", "q", 8);
    let top = ModDef::new("Top");
    let src = top.instantiate(&pass, Some("src"), None);
    let sink = top.instantiate(&pass, Some("sink"), None);
    // Keep the two buses electrically distinct so a swapped name is observable.
    src.get_port("d").export_as("left");
    let q = src.get_port("q");
    q.export_as("result");
    q.slice(3, 2).specify_net_name("lane");
    q.bit(6).specify_net_name("flag");
    let other = top.instantiate(&pass, Some("other"), None);
    other.get_port("d").export_as("right");
    let right = other.get_port("q");
    let d = sink.get_port("d");
    d.slice(7, 4).connect(&right.slice(3, 0));
    d.slice(3, 0).connect(&right.slice(7, 4));
    d.slice(5, 3).specify_net_name("received");
    sink.get_port("q").export_as("swapped");

    let direct = top.emit(EmitOptions::default());
    let ir = ir_sv(&top)?;
    compare(&top, &direct, &ir, true)?;
    let wrong = ir
        .replace("lane", "temporary_name")
        .replace("received", "lane")
        .replace("temporary_name", "received");
    compare(&top, &direct, &wrong, false)?;
    let error = compare(&top, &direct, &wrong, true).unwrap_err();
    ensure!(error.to_string().contains("does not bind"), "{error}");
    Ok(())
}

#[test]
fn nested_interface_export_and_wide_constant() -> Result<()> {
    let leaf = ModDef::new("Adapter");
    let data = leaf.add_port("bus_in", IO::Input(10));
    let out = leaf.add_port("bus_out", IO::Output(80));
    out.slice(79, 76).connect(&data.slice(3, 0));
    out.slice(75, 5)
        .tieoff((BigUint::from(1u8) << 70usize) | BigUint::from(7u8));
    out.slice(4, 0).connect(&data.slice(9, 5));
    data.bit(4).unused();
    leaf.def_intf_from_prefix("bus", "bus_");
    let wrapper = leaf.wrap(None, None);
    let top = ModDef::new("WrapperTop");
    top.instantiate(&wrapper, Some("wrapper"), None)
        .get_intf("bus")
        .export_with_prefix("bus", "io_");
    check_design(&top)
}

#[test]
fn detects_rewiring_and_functionally_invisible_name_swaps() -> Result<()> {
    let top = routed_pair(true);
    let err = compare(
        &top,
        &top.emit(EmitOptions::default()),
        &ir_sv(&routed_pair(false))?,
        false,
    )
    .unwrap_err();
    ensure!(err.to_string().contains("proof did fail"), "{err}");

    let top = named_receivers();
    let direct = top.emit(EmitOptions::default());
    let candidate = ir_sv(&top)?
        .replace("upper_lane", "temporary_lane")
        .replace("lower_lane", "upper_lane")
        .replace("temporary_lane", "lower_lane");
    // The port functions are identical, and both requested names still exist.
    compare(&top, &direct, &candidate, false)?;
    let err = compare(&top, &direct, &candidate, true).unwrap_err();
    ensure!(err.to_string().contains("does not bind"), "{err}");
    Ok(())
}

#[test]
fn bidirectional_stream_through_a_funnel_matches_its_spec() -> Result<()> {
    let bridge = ModDef::new("Bridge");
    bridge.feedthrough("left_i", "right_o", 10);
    bridge.feedthrough("right_i", "left_o", 10);
    let top = ModDef::new("Stream");
    let data = top.add_port("data", IO::Input(8));
    let valid = top.add_port("valid", IO::Input(1));
    let ready = top.add_port("ready", IO::Output(1));
    let result = top.add_port("result", IO::Output(8));
    let sent = top.add_port("sent", IO::Output(1));
    let accept = top.add_port("accept", IO::Input(1));
    let bridge = top.instantiate(&bridge, Some("bridge"), None);
    let mut funnel = Funnel::new(
        (bridge.get_port("left_i"), bridge.get_port("left_o")),
        (bridge.get_port("right_i"), bridge.get_port("right_o")),
    );
    funnel.connect(&data, &result);
    funnel.connect(&valid, &sent);
    funnel.connect(&ready, &accept);
    funnel.tieoff_remaining();
    let spec = "module Stream(input [7:0] data, input valid, output ready,
                 output [7:0] result, output sent, input accept);
                 assign result = data; assign sent = valid; assign ready = accept; endmodule";
    let direct = top.emit(EmitOptions::default());
    let candidate = ir_sv(&top)?;
    compare(&top, spec, &direct, false)?;
    compare(&top, spec, &candidate, false)?;
    compare(&top, &direct, &candidate, true)
}

#[test]
fn a_requested_name_prevails_over_a_generated_instance_output_name() -> Result<()> {
    let pass = ModDef::new("Pass");
    let input = pass.add_port("d", IO::Input(4));
    pass.add_port("q", IO::Output(4)).connect(&input);
    let top = ModDef::new("Naming");
    let first = top.instantiate(&pass, Some("first"), None);
    let second = top.instantiate(&pass, Some("second"), None);
    first.get_port("d").export_as("a");
    second.get_port("d").export_as("b");
    first.get_port("q").export_as("out_a");
    second.get_port("q").export_as("out_b");
    second.get_port("q").specify_net_name("first_q");
    check_design(&top)
}

#[test]
fn explicit_name_wins_even_when_an_earlier_instance_needs_an_internal_wire() -> Result<()> {
    let pass = ModDef::new("Pass");
    let d = pass.add_port("d", IO::Input(4));
    pass.add_port("q", IO::Output(4)).connect(&d);

    let top = ModDef::new("Chain");
    let first = top.instantiate(&pass, Some("first"), None);
    let second = top.instantiate(&pass, Some("second"), None);
    first.get_port("d").export_as("a");
    first.get_port("q").connect(&second.get_port("d"));
    second.get_port("q").export_as("y");
    second.get_port("q").specify_net_name("first_q");

    // The first instance needs an internal wire. Only the name on the second output is contractual.
    let reference = "module Pass(output [3:0] q, input [3:0] d);
        assign q = d;
    endmodule
    module Chain(output [3:0] y, input [3:0] a);
        wire [3:0] intermediate, first_q;
        Pass first(.d(a), .q(intermediate));
        Pass second(.d(intermediate), .q(first_q));
        assign y = first_q;
    endmodule";
    compare(&top, reference, &top.emit(EmitOptions::default()), true)?;
    compare(&top, reference, &ir_sv(&top)?, true)
}

/// Exercise both a named nonzero-offset source slice and a named bus assembled from two slices.
fn named_receivers() -> ModDef {
    let pass = |name, width| {
        let leaf = ModDef::new(name);
        let input = leaf.add_port("d", IO::Input(width));
        leaf.add_port("q", IO::Output(width)).connect(&input);
        leaf
    };
    let byte = pass("Byte", 8);
    let nibble = pass("Nibble", 4);
    let top = ModDef::new("Wiring");
    let src = top.instantiate(&byte, Some("src"), None);
    let upper = top.instantiate(&nibble, Some("upper"), None);
    let lower = top.instantiate(&nibble, Some("lower"), None);
    src.get_port("d").export_as("data");
    let q = src.get_port("q");
    upper.get_port("d").connect(&q.slice(7, 4));
    upper.get_port("d").specify_net_name("upper_lane");
    lower.get_port("d").slice(1, 0).connect(&q.slice(3, 2));
    lower.get_port("d").slice(3, 2).connect(&q.slice(1, 0));
    lower.get_port("d").specify_net_name("lower_lane");
    top.add_port("tap", IO::Output(2)).connect(&q.slice(2, 1));
    upper.get_port("q").export_as("a");
    lower.get_port("q").export_as("b");
    top
}

/// Use the package text and default codegen settings an external block2sv caller sees.
fn ir_sv(top: &ModDef) -> Result<String> {
    let ir = top.to_block_ir()?.to_string();
    let package = Parser::new(&ir)
        .parse_and_validate_package()
        .map_err(anyhow::Error::msg)?;
    Ok(emit_system_verilog(&package, &BlockCodegenOptions::default())?.system_verilog)
}

/// Run the same generator through both supported emission paths.
fn check_design(top: &ModDef) -> Result<()> {
    compare(top, &top.emit(EmitOptions::default()), &ir_sv(top)?, true)
}

/// Derive the naming contract from the generator, independently of either emitted netlist.
fn requested_nets(top: &ModDef) -> Vec<String> {
    let mut nets = Vec::new();
    let mut pending = vec![top.clone()];
    let mut visited = HashSet::new();
    while let Some(module) = pending.pop() {
        let name = module.get_name();
        if !visited.insert(name.clone()) {
            continue;
        }
        for inst in module.get_instances() {
            pending.push(inst.get_mod_def());
            for port in inst.get_ports(None) {
                for conn in port.get_connections() {
                    if let ConnectedItem::Wire(wire) = conn.other {
                        nets.push(format!(
                            "{name}.{}={}.{}[{}:{}]",
                            wire.name,
                            inst.name(),
                            port.name(),
                            wire.msb,
                            wire.lsb
                        ));
                    }
                }
            }
        }
    }
    nets
}

/// Run the standalone comparison on the actual files downstream users consume.
fn compare(top: &ModDef, reference: &str, candidate: &str, structure: bool) -> Result<()> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("reference.sv"), reference)?;
    fs::write(dir.path().join("candidate.sv"), candidate)?;
    let mut cmd = Command::new("python3");
    cmd.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/check_emission_equivalence.py"))
        .current_dir(dir.path())
        .args(["reference.sv", "candidate.sv", "--top", &top.get_name()]);
    if structure {
        cmd.arg("--structure");
        for net in requested_nets(top) {
            cmd.arg("--net").arg(net);
        }
    }
    let out = cmd.output().context("running cross-emitter comparison")?;
    ensure!(
        out.status.success(),
        "{}: {}",
        top.get_name(),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[test]
fn generated_signal_skips_a_reserved_name_in_both_emitters() -> Result<()> {
    let leaf = ModDef::new("Leaf");
    leaf.feedthrough("d", "number", 1);
    let top = ModDef::new("Top");
    let file = top.instantiate(&leaf, Some("file"), None);
    file.get_port("d").export_as("data");
    let q = file.get_port("number");
    q.export_as("first");
    q.export_as("second");
    for sv in [top.emit(Default::default()), top.emit_sv_via_block_ir()?] {
        ensure!(sv.contains("file_number__1"), "{sv}");
    }
    check_design(&top)
}
