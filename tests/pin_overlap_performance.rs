// SPDX-License-Identifier: Apache-2.0

use rstest::rstest;
use std::time::{Duration, Instant};
use topstitch::{IO, ModDef, PhysicalPin, Polygon};

/// Times only the overlap check. All of these layouts are collision-free so the full search runs.
fn check_within(module: &ModDef, budget: Duration) {
    let start = Instant::now();
    module.check_pin_overlaps().unwrap();
    let elapsed = start.elapsed();
    eprintln!("Pin overlap check took {elapsed:?}");
    assert!(
        elapsed < budget,
        "{} pin overlap check took {elapsed:?} (budget {budget:?})",
        module.get_name()
    );
}

#[rstest]
#[case::row((2, 0))]
#[case::column((0, 2))]
fn separated_pins_in_a_line(#[case] stride: (i64, i64)) {
    const COUNT: usize = 32_768;
    let module = ModDef::new("line");
    let pins = module.add_port("pins", IO::Input(COUNT));
    let pin = PhysicalPin::new("M1", Polygon::from_width_height(1, 1));
    for bit in 0..COUNT {
        let offset = (bit as i64 * stride.0, bit as i64 * stride.1);
        pins.bit(bit).place(pin.with_translation(offset.into()));
    }

    // A scan that prunes along only one axis must still handle the perpendicular line.
    check_within(&module, Duration::from_secs(4));
}

#[test]
fn many_candidates_for_one_complex_pin() {
    const STEPS: i64 = 2048;
    let module = ModDef::new("shared_shape");
    module
        .add_port("stair", IO::Input(1))
        .bit(0)
        .place(PhysicalPin::new("M1", staircase(STEPS)));

    // Every square is inside the stair's bounding box, in the empty corner above its diagonal.
    const SIDE: usize = 32;
    let probes = module.add_port("probes", IO::Input(SIDE * SIDE));
    let square = PhysicalPin::new("M1", Polygon::from_width_height(1, 1));
    let corner = 3 * STEPS / 4;
    for bit in 0..SIDE * SIDE {
        let x = corner + 2 * (bit % SIDE) as i64;
        let y = corner + 2 * (bit / SIDE) as i64;
        probes
            .bit(bit)
            .place(square.with_translation((x, y).into()));
    }
    check_within(&module, Duration::from_secs(6));
}

#[rstest]
#[case::no_candidates(1024, false, Duration::from_secs(2))]
#[case::one_candidate_each(512, true, Duration::from_secs(8))]
fn independent_complex_pins(
    #[case] count: usize,
    #[case] add_probes: bool,
    #[case] budget: Duration,
) {
    const STEPS: i64 = 256;
    let module = ModDef::new("independent_shapes");
    let stairs = module.add_port("stairs", IO::Input(count));
    let probes = add_probes.then(|| module.add_port("probes", IO::Input(count)));
    let stair = PhysicalPin::new("M1", staircase(STEPS));
    let square = PhysicalPin::new("M1", Polygon::from_width_height(1, 1));
    for bit in 0..count {
        let x = bit as i64 * (STEPS + 4);
        stairs.bit(bit).place(stair.with_translation((x, 0).into()));
        if let Some(probes) = &probes {
            // Each stair gets one exact comparison, with no opportunity to reuse its geometry.
            probes
                .bit(bit)
                .place(square.with_translation((x + STEPS - 2, STEPS - 2).into()));
        }
    }
    check_within(&module, budget);
}

/// A right triangle with a stepped diagonal, leaving the top right of its bounding box empty.
fn staircase(steps: i64) -> Polygon {
    let mut vertices = vec![(0, 0).into(), (0, steps).into()];
    for x in 1..=steps {
        vertices.push((x, steps + 1 - x).into());
        vertices.push((x, steps - x).into());
    }
    Polygon::new(vertices)
}
