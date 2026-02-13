// SPDX-License-Identifier: Apache-2.0

use rstest::rstest;

use topstitch::{
    Coordinate, IO, ModDef, Polygon, TrackDefinition, TrackDefinitions, TrackOrientation, Usage,
};

fn hat_shaped_block() -> (ModDef, Polygon, usize) {
    let hat_block = ModDef::new("HatBlock");
    hat_block.set_usage(Usage::EmitStubAndStop);

    hat_block.add_port("x", IO::Input(1));

    hat_block.set_shape(Polygon::new(vec![
        Coordinate { x: 0, y: 0 },
        Coordinate { x: 0, y: 50 },
        Coordinate { x: 10, y: 50 },
        Coordinate { x: 10, y: 40 },
        Coordinate { x: 20, y: 40 },
        Coordinate { x: 20, y: 10 },
        Coordinate { x: 10, y: 10 },
        Coordinate { x: 10, y: 0 },
    ]));

    let mut track_definitions = TrackDefinitions::new();
    let pin_shape = Polygon::new(vec![
        Coordinate { x: -1, y: 0 },
        Coordinate { x: -1, y: 2 },
        Coordinate { x: 1, y: 2 },
        Coordinate { x: 1, y: 0 },
    ]);
    track_definitions.add_track(TrackDefinition::new(
        "M1",
        0,
        1,
        TrackOrientation::Horizontal,
        Some(pin_shape.clone()),
        None,
    ));
    hat_block.set_track_definitions(track_definitions);

    (hat_block, pin_shape, 4)
}

fn hat_shaped_block_pinning_test(local_track: usize) {
    let (hat_block, _, edge_index) = hat_shaped_block();

    hat_block
        .get_port("x")
        .bit(0)
        .place_on_edge_index(edge_index, "M1", local_track);

    let min_pin = hat_block.get_physical_pin("x", 0);
    assert_eq!(
        min_pin.translation(),
        Coordinate {
            x: 20,
            y: 10 + local_track as i64
        }
    );
}

fn hat_shaped_block_keepout_marks_expected_tracks(
    keepout_track: usize,
    blocked_tracks: &[usize],
    clear_tracks: &[usize],
) {
    let (hat_block, pin_shape, edge_index) = hat_shaped_block();

    hat_block
        .get_port("x")
        .bit(0)
        .place_on_edge_index_with_polygon(edge_index, "M1", keepout_track, None, Some(&pin_shape));

    for &track in blocked_tracks {
        assert!(!hat_block.can_place_pin_on_edge_index(edge_index, "M1", track));
    }

    for &track in clear_tracks {
        assert!(hat_block.can_place_pin_on_edge_index(edge_index, "M1", track));
    }
}

#[rstest]
#[case(1)]
#[case(29)]
fn hat_shaped_block_valid_placement(#[case] local_track: usize) {
    hat_shaped_block_pinning_test(local_track);
}

#[rstest]
#[case(0)]
#[case(30)]
#[should_panic(expected = "outside available range")]
fn hat_shaped_block_invalid_placement(#[case] local_track: usize) {
    hat_shaped_block_pinning_test(local_track);
}

#[rstest]
#[case(15, vec![13, 14, 15, 16, 17], vec![12, 18])]
#[case(0, vec![0, 1, 2], vec![3])]
#[case(30, vec![28, 29, 30], vec![27])]
#[case(100, vec![], vec![15])]
fn hat_shaped_block_keepout_track_occupancy(
    #[case] keepout_track: usize,
    #[case] blocked_tracks: Vec<usize>,
    #[case] clear_tracks: Vec<usize>,
) {
    hat_shaped_block_keepout_marks_expected_tracks(keepout_track, &blocked_tracks, &clear_tracks);
}
