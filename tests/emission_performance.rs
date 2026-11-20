// SPDX-License-Identifier: Apache-2.0

//! Run with `cargo test --release --test emission_performance --features block-ir -- --test-threads=1 --nocapture`.

use std::time::{Duration, Instant};
use topstitch::{EmitOptions, IO, ModDef};

const STAGES: usize = 128;

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_sv_emission() {
    for shared in [true, false] {
        let top = lane_chain(shared);
        let start = Instant::now();
        let sv = top.emit(EmitOptions::default());
        check_elapsed("SV", shared, start.elapsed(), Duration::from_millis(150));
        let definitions = if shared { 2 } else { STAGES + 1 };
        assert_eq!(sv.matches("endmodule").count(), definitions);
        assert_eq!(sv.matches("    .data_in(").count(), STAGES);
    }
}

#[cfg(feature = "block-ir")]
#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_block_ir_emission() {
    for shared in [true, false] {
        let top = lane_chain(shared);
        let start = Instant::now();
        let package = top.to_block_ir().unwrap();
        // Finishing each distinct block also verifies the package built so far.
        let budget = if shared {
            Duration::from_millis(150)
        } else {
            Duration::from_secs(2)
        };
        check_elapsed("Block IR", shared, start.elapsed(), budget);
        let start = Instant::now();
        let text = package.to_string();
        check_elapsed(
            "IR serialization",
            shared,
            start.elapsed(),
            Duration::from_millis(150),
        );
        assert_eq!(text.matches("  instantiation lane_").count(), STAGES);
        let definitions = if shared { 2 } else { STAGES + 1 };
        assert_eq!(package.members.len(), definitions);
        assert_eq!(
            package.get_top_block().unwrap().instantiations.len(),
            STAGES
        );
    }
}

/// Build the same wiring with either one reused definition or a distinct definition per stage.
fn lane_chain(shared: bool) -> ModDef {
    let top = ModDef::new("Chain");
    let mut previous = top.add_port("data_in", IO::Input(32));
    let first = lane(0);
    for i in 0..STAGES {
        let stage = if shared || i == 0 {
            first.clone()
        } else {
            lane(i)
        };
        let inst = top.instantiate(&stage, Some(&format!("lane_{i}")), None);
        inst.get_port("data_in").connect(&previous);
        previous = inst.get_port("data_out");
    }
    top.add_port("data_out", IO::Output(32)).connect(&previous);
    top
}

/// Cross the two halves of a 32-bit bus at each stage.
fn lane(i: usize) -> ModDef {
    let lane = ModDef::new(format!("Lane_{i}"));
    let input = lane.add_port("data_in", IO::Input(32));
    let output = lane.add_port("data_out", IO::Output(32));
    output.slice(15, 0).connect(&input.slice(31, 16));
    output.slice(31, 16).connect(&input.slice(15, 0));
    lane
}

/// Time only emission. Keep compiler startup, generator construction and output checks outside the budget.
fn check_elapsed(backend: &str, shared: bool, elapsed: Duration, budget: Duration) {
    let definitions = if shared { "shared" } else { "distinct" };
    eprintln!("{backend}, {STAGES} instances with {definitions} definitions: {elapsed:?}");
    assert!(
        elapsed < budget,
        "{backend} with {definitions} definitions took {elapsed:?} (budget {budget:?})"
    );
}
