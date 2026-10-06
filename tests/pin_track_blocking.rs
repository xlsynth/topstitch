// SPDX-License-Identifier: Apache-2.0

use topstitch::{
    BoundingBox, Coordinate, IO, ModDef, Orientation, PhysicalPin, PinPlacementError, Polygon,
    RIGHT_EDGE_INDEX, Range, TOP_EDGE_INDEX, TrackDefinition, TrackDefinitions, TrackOrientation,
};

fn rectangle(min_x: i64, min_y: i64, max_x: i64, max_y: i64) -> Polygon {
    Polygon::from_bbox(&BoundingBox {
        min_x,
        min_y,
        max_x,
        max_y,
    })
}

fn tracks(offset: i64) -> TrackDefinitions {
    let mut tracks = TrackDefinitions::new();
    for (layer, orientation) in [
        ("M1", TrackOrientation::Vertical),
        ("M2", TrackOrientation::Horizontal),
        ("M3", TrackOrientation::Horizontal),
    ] {
        tracks.add_track(TrackDefinition::new(
            layer,
            offset,
            10,
            orientation,
            Some(rectangle(-1, 0, 1, 2)),
            None,
        ));
    }
    tracks
}

fn block() -> ModDef {
    let block = ModDef::new("block");
    block.set_width_height(100, 100);
    block.set_track_definitions(tracks(0));
    block
}

#[test]
fn placing_geometry_is_independent_of_track_setup_order() {
    for setup_stage in 0..3 {
        let block = ModDef::new("block");
        block.add_port("data", IO::Input(1));
        if setup_stage >= 1 {
            block.set_width_height(100, 100);
        }
        if setup_stage >= 2 {
            block.set_track_definitions(tracks(0));
        }

        let pin = PhysicalPin::new("M2", rectangle(98, 18, 100, 22));
        block.get_port("data").bit(0).place(pin.clone());

        if setup_stage == 0 {
            block.set_width_height(100, 100);
        }
        if setup_stage < 2 {
            block.set_track_definitions(tracks(0));
        }
        assert_eq!(
            block.get_physical_pin("data", 0).transformed_polygon(),
            pin.transformed_polygon()
        );
        assert!(block.can_place_pin_on_right_edge("M2", 2));

        block.block_tracks_for_pin(&pin).unwrap();
        assert!(!block.can_place_pin_on_right_edge("M2", 2));
    }
}

#[test]
fn moving_and_replacing_geometry_do_not_reserve_old_or_new_tracks() {
    let block = block();
    block.add_port("data", IO::Input(1));
    let original = PhysicalPin::new("M2", rectangle(98, 18, 100, 22));
    block.place_pin("data", 0, original.clone());
    block
        .get_port("data")
        .bit(0)
        .place(&original + Coordinate { x: 0, y: 20 });
    let replacement = PhysicalPin::new("M2", rectangle(98, 58, 100, 62));
    block.place_pin("data", 0, replacement.clone());

    for index in [2, 4, 6] {
        assert!(block.can_place_pin_on_right_edge("M2", index));
    }
    assert_eq!(
        block.get_physical_pin("data", 0).transformed_polygon(),
        replacement.transformed_polygon()
    );
    block.block_tracks_for_pin(&replacement).unwrap();
    assert!(block.can_place_pin_on_right_edge("M2", 2));
    assert!(block.can_place_pin_on_right_edge("M2", 4));
    assert!(!block.can_place_pin_on_right_edge("M2", 6));
}

#[test]
fn explicit_blocking_uses_transformed_boundaries_and_only_the_pin_layer() {
    let block = ModDef::new("block");
    block.set_shape(rectangle(-100, -100, 0, 0));
    block.set_track_definitions(tracks(5));
    let pin_shape = rectangle(-2, -16, 0, 16);
    let right_pin = PhysicalPin::from_translation("M2", pin_shape.clone(), (0, -50).into());
    let top_pin = PhysicalPin::from_orientation_then_translation(
        "M1",
        pin_shape,
        Orientation::R90,
        (-50, 0).into(),
    );
    block.block_tracks_for_pin(&right_pin).unwrap();
    block.block_tracks_for_pin(&top_pin).unwrap();

    // Tracks are at -95, -85, ..., -5; the shared segments span -66 through -34.
    for index in 0..10 {
        let available = !(3..=6).contains(&index);
        assert_eq!(block.can_place_pin_on_right_edge("M2", index), available);
        assert_eq!(block.can_place_pin_on_top_edge("M1", index), available);
        assert!(block.can_place_pin_on_left_edge("M2", index));
        assert!(block.can_place_pin_on_bottom_edge("M1", index));
        assert!(block.can_place_pin_on_right_edge("M3", index));
    }
}

#[test]
fn explicit_blocking_is_additive_idempotent_and_tolerates_occupied_spans() {
    let block = block();
    block.mark_keepout_range(RIGHT_EDGE_INDEX, "M2", 2, 4);
    block.mark_pin_range(RIGHT_EDGE_INDEX, "M2", 5, 5);
    let pin = PhysicalPin::new("M2", rectangle(98, 30, 100, 60));
    block.block_tracks_for_pin(&pin).unwrap();
    block.block_tracks_for_pin(&pin).unwrap();

    assert!(matches!(
        block.check_pin_placement_on_edge_index(RIGHT_EDGE_INDEX, "M2", 2),
        Err(PinPlacementError::OverlapsKeepout { .. })
    ));
    for index in 3..=6 {
        assert!(!block.can_place_pin_on_right_edge("M2", index));
    }
    for index in [1, 7] {
        assert!(block.can_place_pin_on_right_edge("M2", index));
    }
}

#[test]
fn interior_and_point_contacts_do_not_block_tracks() {
    let block = block();
    for polygon in [
        rectangle(40, 40, 60, 60),
        rectangle(100, 100, 110, 110),
        Polygon::new(vec![(90, 40).into(), (100, 50).into(), (90, 60).into()]),
        // Crossing the outline does not share a boundary segment with it.
        rectangle(95, 40, 105, 60),
    ] {
        block
            .block_tracks_for_pin(&PhysicalPin::new("M2", polygon))
            .unwrap();
    }
    for index in 0..=10 {
        assert!(block.can_place_pin_on_right_edge("M2", index));
        assert!(block.can_place_pin_on_left_edge("M2", index));
    }
}

#[test]
fn explicit_blocking_reports_missing_setup_and_layer() {
    let block = ModDef::new("block");
    let pin = PhysicalPin::new("M2", rectangle(98, 18, 100, 22));
    assert!(matches!(
        block.block_tracks_for_pin(&pin),
        Err(PinPlacementError::NotInitialized(_))
    ));
    block.set_width_height(100, 100);
    assert!(matches!(
        block.block_tracks_for_pin(&pin),
        Err(PinPlacementError::NotInitialized(_))
    ));
    block.set_track_definitions(tracks(0));
    let unknown_layer = PhysicalPin::new("unknown", pin.polygon.clone());
    assert!(matches!(
        block.block_tracks_for_pin(&unknown_layer),
        Err(PinPlacementError::LayerUnavailable { layer }) if layer == "unknown"
    ));
    assert!(block.can_place_pin_on_right_edge("M2", 2));

    // Changing the outline invalidates the per-edge occupancy maps.
    block.set_width_height(100, 100);
    assert!(matches!(
        block.block_tracks_for_pin(&pin),
        Err(PinPlacementError::NotInitialized(_))
    ));
}

#[test]
fn track_based_placement_still_reserves_tracks() {
    let block = block();
    block.add_port("data", IO::Input(2));
    block.get_port("data").bit(0).place_on_right_edge("M2", 4);
    assert!(!block.can_place_pin_on_right_edge("M2", 4));
    block
        .place_pins_on_right_edge(&[("data", 1)], ["M2"], Range::new(40, 60), None)
        .unwrap();
    let second_pin = block.get_physical_pin("data", 1);
    assert_ne!(second_pin.translation().y, 40);
    assert!(!block.can_place_pin_on_right_edge("M2", (second_pin.translation().y / 10) as usize));
    assert!(block.can_place_pin_on_edge_index(TOP_EDGE_INDEX, "M1", 4));
}

#[test]
fn slice_and_port_block_their_selected_bits_without_changing_geometry() {
    let block = block();
    let port = block.add_port("data", IO::Input(4));
    let pins: Vec<_> = (0..4)
        .map(|bit| {
            PhysicalPin::from_translation(
                "M2",
                rectangle(-2, -2, 0, 2),
                (100, (bit + 1) * 20).into(),
            )
        })
        .collect();
    for bit in [1, 2] {
        port.bit(bit).place(pins[bit].clone());
    }

    // Unpinned bits outside the slice must not prevent blocking its two bits.
    port.slice(2, 1).block_tracks().unwrap();
    for (index, available) in [(2, true), (4, false), (6, false), (8, true)] {
        assert_eq!(block.can_place_pin_on_right_edge("M2", index), available);
    }
    assert!(!port.bit(0).has_physical_pin());
    assert!(!port.bit(3).has_physical_pin());

    for bit in [0, 3] {
        port.bit(bit).place(pins[bit].clone());
    }
    port.block_tracks().unwrap();
    port.block_tracks().unwrap();
    for (bit, original) in pins.iter().enumerate() {
        assert!(!block.can_place_pin_on_right_edge("M2", (bit + 1) * 2));
        let actual = port.bit(bit).get_physical_pin();
        assert_eq!(actual.layer, original.layer);
        assert_eq!(actual.polygon, original.polygon);
        assert_eq!(actual.transform, original.transform);
    }
}

#[test]
fn interface_blocks_only_member_slices_and_accepts_overlapping_members() {
    let block = block();
    let data = block.add_port("data", IO::Input(5));
    let ready = block.add_port("ready", IO::Output(1));
    for bit in 1..=3 {
        let y = (bit as i64) * 20 + 10;
        data.bit(bit)
            .place(PhysicalPin::new("M2", rectangle(98, y - 2, 100, y + 2)));
    }
    ready
        .bit(0)
        .place(PhysicalPin::new("M1", rectangle(48, 98, 52, 100)));
    let intf = block.def_intf(
        "selected",
        [
            ("lower".to_string(), ("data".to_string(), 2, 1)),
            ("upper".to_string(), ("data".to_string(), 3, 2)),
            ("ready".to_string(), ("ready".to_string(), 0, 0)),
        ]
        .into(),
    );
    intf.block_tracks().unwrap();
    intf.block_tracks().unwrap();

    for index in 0..=10 {
        assert_eq!(
            block.can_place_pin_on_right_edge("M2", index),
            ![3, 5, 7].contains(&index)
        );
        assert_eq!(block.can_place_pin_on_top_edge("M1", index), index != 5);
    }
    assert!(!data.bit(0).has_physical_pin());
    assert!(!data.bit(4).has_physical_pin());
}

#[test]
fn missing_geometry_leaves_slice_and_port_reservations_unchanged() {
    let block = block();
    let port = block.add_port("data", IO::Input(3));
    port.bit(0)
        .place(PhysicalPin::new("M2", rectangle(98, 18, 100, 22)));
    port.bit(2)
        .place(PhysicalPin::new("M2", rectangle(98, 58, 100, 62)));
    block.mark_pin_range(RIGHT_EDGE_INDEX, "M2", 8, 8);
    block.mark_keepout_range(RIGHT_EDGE_INDEX, "M2", 9, 9);

    for result in [port.slice(2, 0).block_tracks(), port.block_tracks()] {
        assert!(matches!(
            result,
            Err(PinPlacementError::MissingPhysicalPin { port, bit: 1 }) if port == "data"
        ));
        assert!(block.can_place_pin_on_right_edge("M2", 2));
        assert!(block.can_place_pin_on_right_edge("M2", 6));
        assert!(matches!(
            block.check_pin_placement_on_edge_index(RIGHT_EDGE_INDEX, "M2", 8),
            Err(PinPlacementError::OverlapsExistingPin { .. })
        ));
        assert!(matches!(
            block.check_pin_placement_on_edge_index(RIGHT_EDGE_INDEX, "M2", 9),
            Err(PinPlacementError::OverlapsKeepout { .. })
        ));
    }
}

#[test]
fn interface_blocking_is_atomic_when_a_later_member_has_missing_geometry_or_layer() {
    for missing_layer in [false, true] {
        let block = block();
        let early = block.add_port("early", IO::Input(2));
        let late = block.add_port("late", IO::Input(2));
        early
            .bit(0)
            .place(PhysicalPin::new("M2", rectangle(98, 18, 100, 22)));
        early
            .bit(1)
            .place(PhysicalPin::new("M2", rectangle(98, 28, 100, 32)));
        late.bit(0)
            .place(PhysicalPin::new("M2", rectangle(98, 58, 100, 62)));
        if missing_layer {
            late.bit(1)
                .place(PhysicalPin::new("unknown", rectangle(98, 78, 100, 82)));
        }
        block.mark_keepout_range(RIGHT_EDGE_INDEX, "M2", 9, 9);
        let intf = block.def_intf(
            "selected",
            [
                ("first".to_string(), ("early".to_string(), 1, 0)),
                ("second".to_string(), ("late".to_string(), 1, 0)),
            ]
            .into(),
        );

        let result = intf.block_tracks();
        if missing_layer {
            assert!(matches!(
                result,
                Err(PinPlacementError::LayerUnavailable { layer }) if layer == "unknown"
            ));
        } else {
            assert!(matches!(
                result,
                Err(PinPlacementError::MissingPhysicalPin { port, bit: 1 }) if port == "late"
            ));
        }
        for index in [2, 3, 6, 8] {
            assert!(block.can_place_pin_on_right_edge("M2", index));
        }
        assert!(matches!(
            block.check_pin_placement_on_edge_index(RIGHT_EDGE_INDEX, "M2", 9),
            Err(PinPlacementError::OverlapsKeepout { .. })
        ));
    }
}

#[test]
fn wrappers_reject_instances_including_empty_interfaces_before_accessing_geometry() {
    let leaf = ModDef::new("leaf");
    leaf.add_port("data", IO::Input(2));
    leaf.def_intf(
        "bus",
        [("data".to_string(), ("data".to_string(), 1, 0))].into(),
    );
    let empty = leaf.def_intf("empty", Default::default());
    // Empty ModDef interfaces require neither geometry nor track setup.
    empty.block_tracks().unwrap();

    let top = ModDef::new("top");
    let instance = top.instantiate(&leaf, Some("instance"), None);
    // No instance placement or pin geometry exists to transform.
    for result in [
        instance.get_port("data").slice(1, 0).block_tracks(),
        instance.get_port("data").block_tracks(),
        instance.get_intf("bus").block_tracks(),
        instance.get_intf("empty").block_tracks(),
    ] {
        assert_eq!(result, Err(PinPlacementError::RequiresModDef));
    }
}
