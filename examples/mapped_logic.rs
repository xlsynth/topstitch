// SPDX-License-Identifier: Apache-2.0

//! Run with and without `-- --mapped` to see the two implementations of the same design.
use topstitch::{
    EmitOptions, FlopFlavor, FlopPins, GateMap, IO, LogicPort, ModDef, Mux2Pins, ParameterValue,
    PrimitivePolicy, Reset, ResetKind, concat,
};

fn main() {
    let top = ModDef::new("SampleAligner");
    if std::env::args().any(|arg| arg == "--mapped") {
        top.set_primitive_policy(PrimitivePolicy::RequireMapping(sample_cells()))
            .unwrap();
    }

    let clk = top.add_port("clk", IO::Input(1));
    top.set_default_clock(&clk.declare_clock().unwrap())
        .unwrap();
    let rst_n = top.add_port("rst_n", IO::Input(1));
    let slot = top.add_port("slot", IO::Input(3));
    let lanes = top.add_port("lanes", IO::Input(40)).subdivide(5);

    // Five slots contain samples. The three spare encodings return the idle word.
    let sample = slot.select_from_or(&lanes, 0x80);
    let payload = &sample & 0x7f;
    let valid = payload.or();
    let tagged = concat([valid.bit(0), payload.slice(6, 0)]);
    top.add_port("sample", IO::Output(8)).connect(&sample);
    top.add_port("aligned", IO::Output(8))
        .connect(&tagged.delay(3));
    top.add_port("falling", IO::Output(8))
        .connect(&sample.delay_with(2, (clk.negedge(), Reset::async_low(&rst_n, 0xa5))));
    print!("{}", top.emit(EmitOptions::default()));
}

/// Registers the functional cell models in examples/input/logic_cells.sv.
fn sample_cells() -> GateMap {
    let cell = |name| ModDef::from_verilog(name, include_str!("input/logic_cells.sv")).unwrap();
    let mut lib = GateMap::new();
    lib.and2(cell("And2"), ["A", "B"], "Y").unwrap();
    lib.or2(cell("Or2"), ["A", "B"], "Y").unwrap();
    lib.mux2(
        cell("Mux2"),
        Mux2Pins {
            select: "S",
            when_true: "B",
            when_false: "A",
            out: "Y",
        },
    )
    .unwrap();
    let pins = FlopPins {
        d: "D",
        clk: "CLK",
        q: "Q",
        reset: None,
    };
    lib.flop(FlopFlavor::posedge(), cell("Dff"), pins).unwrap();
    let bank = cell("ResetBank");
    lib.flop_by_width(
        FlopFlavor::negedge().reset_by_value(ResetKind::AsyncLow),
        FlopPins {
            reset: Some("RN"),
            ..pins
        },
        move |width, reset| {
            Ok(bank.parameterize::<ParameterValue>(&[
                ("WIDTH", width.into()),
                ("RESET_VALUE", reset.clone().into()),
            ]))
        },
    )
    .unwrap();
    lib
}
