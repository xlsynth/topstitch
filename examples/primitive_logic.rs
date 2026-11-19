// SPDX-License-Identifier: Apache-2.0

//! Run with `cargo run --example primitive_logic` to print the generated SystemVerilog.
//! Add `-- --instances` to inspect the same design before emission.
use topstitch::{EmitOptions, IO, LogicPort, ModDef, Port, PortSlice, Primitive, Reset};

fn main() {
    let top = ModDef::new("LanePicker");
    let clk = top.add_port("clk", IO::Input(1));
    let io_clk = top.add_port("io_clk", IO::Input(1));
    let rst_n = top.add_port("rst_n", IO::Input(1));
    top.set_default_clock(&clk.declare_clock().unwrap())
        .unwrap();

    let enabled = top.add_port("enabled", IO::Input(1));
    let hold = top.add_port("hold", IO::Input(1));
    let sel = top.add_port("sel", IO::Input(3));
    let idle = top.add_port("idle", IO::Input(8));
    let lanes = top.add_port("lanes", IO::Input(64)).subdivide(8);

    let auto_select = top.add_port("auto_select", IO::Input(1));
    let requests = top.add_port("requests", IO::Input(lanes.len()));
    top.add_port("any_request", IO::Output(1))
        .connect(&requests.or());
    let manual = sel.select_from(&lanes);
    let automatic = first_requesting_lane(&requests, &lanes, &idle);
    let selected = auto_select.select(automatic, manual);
    let q = (&enabled & !&hold).select(selected, &idle).flop();
    top.add_port("data", IO::Output(8)).connect(&q);
    top.add_port("monitor", IO::Output(8)).connect(&q);

    let partial = sel.select_from_or(lanes.iter().take(6), &idle);
    let io_ff = (io_clk.negedge(), Reset::async_low(&rst_n, 0xa5));
    top.add_port("io_data", IO::Output(8))
        .connect(&partial.flop_with(&io_ff));

    // The same AND definition can also be instantiated and wired by hand.
    let gate = top.instantiate(
        &top.primitive_def(Primitive::And, 1),
        Some("explicit_and"),
        None,
    );
    gate.get_port("a").connect(&enabled);
    gate.get_port("b").connect(&hold);
    top.add_port("both", IO::Output(1))
        .connect(&gate.get_port("y"));

    if std::env::args().nth(1).as_deref() == Some("--instances") {
        for inst in top.get_instances() {
            let def = inst.get_mod_def();
            println!(
                "{}  {}  {:?}",
                inst.name(),
                def.get_name(),
                def.primitive_kind()
            );
        }
    } else {
        print!("{}", top.emit(EmitOptions::default()));
    }
}

/// Selects the lowest-numbered requesting lane, or `idle` when no lane requests.
fn first_requesting_lane(requests: &Port, lanes: &[PortSlice], idle: &Port) -> Port {
    let mut selected = idle.clone();
    for (index, lane) in lanes.iter().enumerate().rev() {
        selected = requests.bit(index).select(lane, selected);
    }
    selected
}
