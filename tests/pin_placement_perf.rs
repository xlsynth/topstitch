// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};
use topstitch::{
    BoundingBox, IO, ModDef, Orientation, PhysicalPin, Polygon, Port, Range, SpreadPinsOptions,
    TrackDefinition, TrackDefinitions, TrackOrientation, Usage,
};

// Pin placement is faster in release mode, so use a larger workload there.
const PIN_COUNT_SCALE: usize = if cfg!(debug_assertions) { 1 } else { 30 };
const PIN_COUNT: usize = 10_000 * PIN_COUNT_SCALE;
const WIDTH: i64 = 120;
const PITCH: i64 = 30;
const LAYER: &str = "M1";

fn block(name: &str, pin_count: usize) -> ModDef {
    let block = ModDef::new(name);
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(WIDTH, (2 * pin_count as i64 - 1) * PITCH);
    let pin = Polygon::from_bbox(&BoundingBox {
        min_x: -10,
        min_y: 0,
        max_x: 10,
        max_y: 20,
    });
    let mut tracks = TrackDefinitions::new();
    tracks.add_track(TrackDefinition::new(
        LAYER,
        PITCH / 2,
        PITCH,
        TrackOrientation::Horizontal,
        Some(pin),
        None,
    ));
    block.set_track_definitions(tracks);
    block
}

fn place_source(block: &ModDef, pin_count: usize) -> Port {
    let source = block.add_port("out", IO::Output(pin_count));
    block
        .place_pins_on_right_edge(&source.to_bits(), [LAYER], Range::default(), None)
        .unwrap();
    source
}

fn placed_pair(target_x: i64) -> (ModDef, Port, Port) {
    let source = block("Source", PIN_COUNT);
    let output = place_source(&source, PIN_COUNT);
    let target = block("Target", PIN_COUNT);
    let input = target.add_port("in", IO::Input(PIN_COUNT));

    let top = ModDef::new("Top");
    let source_inst = top.instantiate(&source, None, None);
    let target_inst = top.instantiate(&target, None, None);
    source_inst.place((0, 0), Orientation::R0);
    target_inst.place((target_x, 0), Orientation::R0);
    let output = source_inst.get_port(output.name());
    let input = target_inst.get_port(input.name());
    (top, output, input)
}

fn assert_runtime(operation: &str, pin_count: usize, elapsed: Duration, limit: Duration) {
    eprintln!("{operation} placement took {elapsed:?} for {pin_count} pins");
    assert!(
        elapsed <= limit,
        "{operation} placement took {elapsed:?} for {pin_count} pins, exceeding the {limit:?} limit"
    );
}

#[test]
fn spread() {
    let pin_count = 2_000 * PIN_COUNT_SCALE;
    let block = block("Block", pin_count);
    place_source(&block, pin_count);
    let target = block.add_port("in", IO::Input(pin_count));
    let bits = target.to_bits();

    let start = Instant::now();
    block
        .spread_pins_on_left_edge(&bits, [LAYER], SpreadPinsOptions::default())
        .unwrap();
    assert_runtime("Spread", pin_count, start.elapsed(), Duration::from_secs(6));
}

#[test]
fn abutted() {
    let (_top, source, target) = placed_pair(WIDTH + 40);
    source.connect(&target);

    let start = Instant::now();
    target.place_abutted();
    assert_runtime(
        "Abutted",
        PIN_COUNT,
        start.elapsed(),
        Duration::from_secs(2),
    );
}

#[test]
fn across() {
    let block = block("Block", PIN_COUNT);
    let source = place_source(&block, PIN_COUNT);
    let target = block.add_port("in", IO::Input(PIN_COUNT));
    source.connect(&target);

    let start = Instant::now();
    target.place_across();
    assert_runtime("Across", PIN_COUNT, start.elapsed(), Duration::from_secs(2));
}

#[test]
fn overlapped() {
    let (_top, source, target) = placed_pair(WIDTH - 20);
    source.connect(&target);
    let pin = PhysicalPin::new(LAYER, Polygon::from_width_height(10, 10));

    let start = Instant::now();
    target.place_overlapped(&pin);
    assert_runtime(
        "Overlapped",
        PIN_COUNT,
        start.elapsed(),
        Duration::from_secs(1),
    );
}
