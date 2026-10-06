// SPDX-License-Identifier: Apache-2.0

use topstitch::{
    BoundingBox, Coordinate, IO, LefDefOptions, ModDef, MultiplePinShapesPolicy, Polygon, Range,
    TrackDefinition, TrackDefinitions, TrackOrientation,
};

// Synthetic input with the same ports and pin geometry as REFERENCE_LEF.
const REFERENCE_DEF: &str = r#"# SPDX-License-Identifier: Apache-2.0
# A small block with ordinary DEF pins, placement, and routing.
VERSION 5.8 ;
DIVIDERCHAR "/" ;
BUSBITCHARS "[]" ;
DESIGN reference ;
UNITS DISTANCE MICRONS 1000 ;
DIEAREA ( 0 0 ) ( 2000 1600 ) ;
ROW row0 core 0 0 N DO 100 BY 1 STEP 20 0 ;
TRACKS Y 50 DO 16 STEP 100 LAYER M2 ;
COMPONENTS 1 ;
- u_buf BUF2 + PLACED ( 1000 800 ) N ;
END COMPONENTS
PINS 4 ;
- data[0] + NET data[0] + DIRECTION INPUT + USE SIGNAL
  + LAYER M2 ( -10 -20 ) ( 10 20 ) + FIXED ( 1980 350 ) N ;
- data[1] + NET data[1] + DIRECTION INPUT + USE SIGNAL
  + LAYER M2 ( -10 -20 ) ( 20 40 ) + FIXED ( 1980 750 ) FN ;
- debug + NET debug + DIRECTION OUTPUT + USE SIGNAL ;
- VDD + NET VDD + DIRECTION INOUT + USE POWER
  + LAYER M1 ( -20 -20 ) ( 20 20 ) + FIXED ( 1000 1500 ) N ;
END PINS
SPECIALNETS 1 ;
- VDD ( PIN VDD ) + USE POWER
  + ROUTED M1 40 ( 0 1500 ) ( 2000 1500 ) ;
END SPECIALNETS
NETS 3 ;
- data[0] ( PIN data[0] ) ( u_buf A )
  + ROUTED M2 ( 1980 350 ) ( 1000 350 ) ( 1000 800 ) ;
- data[1] ( PIN data[1] ) ( u_buf B ) ;
- debug ( PIN debug ) ( u_buf Y ) ;
END NETS
END DESIGN
"#;

fn options() -> LefDefOptions {
    LefDefOptions {
        units_microns: 1000,
        ..Default::default()
    }
}

fn bounds(bbox: BoundingBox) -> (i64, i64, i64, i64) {
    (bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y)
}

#[test]
fn reads_reference_def_into_moddef() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("reference.def");
    std::fs::write(&path, REFERENCE_DEF).unwrap();
    let block = ModDef::from_def_file(
        &path,
        &LefDefOptions {
            units_microns: 2000,
            ..options()
        },
    );
    assert_eq!(block.get_name(), "reference");
    assert_eq!(bounds(block.bbox().unwrap()), (0, 0, 4000, 3200));
    assert_eq!(block.get_ports(None).len(), 2); // POWER is excluded by default.
    assert!(matches!(block.get_port("data").io(), IO::Input(2)));
    assert!(matches!(block.get_port("debug").io(), IO::Output(1)));
    assert!(!block.get_port("debug").has_physical_pin());
    assert!(block.get_track_definitions().is_none());
    // File units are doubled, and FN reflects the asymmetric second pin.
    for (bit, expected) in [(0, (3940, 660, 3980, 740)), (1, (3920, 1460, 3980, 1580))] {
        let pin = block.get_physical_pin("data", bit);
        assert_eq!(pin.layer, "M2");
        assert_eq!(bounds(pin.transformed_polygon().bbox()), expected);
    }
}

#[test]
fn reference_import_respects_name_use_and_layer_filters() {
    let block = ModDef::from_def(
        REFERENCE_DEF,
        &LefDefOptions {
            ignore_pin_names: ["debug".to_string()].into(),
            skip_pin_uses: Default::default(),
            valid_pin_layers: Some(["M1".to_string()].into()),
            ..options()
        },
    );
    assert_eq!(block.get_ports(None).len(), 2);
    assert!(!block.has_port("debug"));
    assert!(block.get_port("VDD").has_physical_pin());
    for bit in 0..2 {
        assert!(!block.get_port("data").bit(bit).has_physical_pin());
    }
}

#[test]
fn multiple_shapes_respect_policy_and_layer_selection() {
    let def = r#"VERSION 5.8 ;
DESIGN reference ;
UNITS DISTANCE MICRONS 1000 ;
PINS 2 ;
- data[0] + NET data[0] + DIRECTION INPUT
  + PORT + LAYER M1 ( 0 0 ) ( 500 500 ) + FIXED ( 0 0 ) N
  + PORT + LAYER M2 ( 0 0 ) ( 10 20 ) + FIXED ( 100 200 ) N
  + PORT + LAYER M2 ( 0 0 ) ( 20 30 ) + FIXED ( 150 250 ) FN ;
- data[1] + NET data[1] + DIRECTION INPUT
  + PORT + LAYER M2 ( 0 0 ) ( 500 500 ) + FIXED ( 0 0 ) N
  + PORT + LAYER M1 ( 0 0 ) ( 10 20 ) + FIXED ( 300 400 ) N
  + PORT + LAYER M1 ( 0 0 ) ( 20 30 ) + FIXED ( 350 450 ) FN ;
END PINS
END DESIGN
"#;
    let first = ModDef::from_def(def, &options());
    assert_eq!(first.get_physical_pin("data", 0).layer, "M1");
    assert_eq!(first.get_physical_pin("data", 1).layer, "M2");
    let bad_later_shape = def.replacen("( 10 20 )", "( invalid )", 1);
    assert_eq!(
        ModDef::from_def(&bad_later_shape, &options())
            .get_physical_pin("data", 0)
            .layer,
        "M1"
    );
    let strict = LefDefOptions {
        multiple_pin_shapes_policy: MultiplePinShapesPolicy::Error,
        ..options()
    };
    assert!(std::panic::catch_unwind(|| ModDef::from_def(def, &strict)).is_err());

    let mut opts = LefDefOptions {
        multiple_pin_shapes_policy: MultiplePinShapesPolicy::BoundingBox,
        pin_layer_selections: [
            (("data".to_string(), 0), "M2".to_string()),
            (("data".to_string(), 1), "M1".to_string()),
        ]
        .into(),
        ..options()
    };
    let block = ModDef::from_def(def, &opts);
    for (bit, layer, expected) in [
        (0, "M2", (100, 200, 150, 280)),
        (1, "M1", (300, 400, 350, 480)),
    ] {
        let pin = block.get_physical_pin("data", bit);
        assert_eq!(pin.layer, layer);
        assert_eq!(bounds(pin.transformed_polygon().bbox()), expected);
        assert_eq!(pin.polygon.0.len(), 4);
    }
    opts.valid_pin_layers = Some(["M2".to_string()].into());
    assert!(std::panic::catch_unwind(|| ModDef::from_def(def, &opts)).is_err());
    opts.valid_pin_layers = None;
    opts.pin_layer_selections.clear();
    assert!(std::panic::catch_unwind(|| ModDef::from_def(def, &opts)).is_err());
    let ignored = LefDefOptions {
        ignore_pin_names: ["data".to_string()].into(),
        ..strict
    };
    assert!(!ModDef::from_def(&bad_later_shape, &ignored).has_port("data"));
}

#[test]
fn imported_pins_block_tracks_after_being_moved_to_an_edge() {
    let reference = ModDef::from_def(REFERENCE_DEF, &options());
    let target = ModDef::new("resized");
    target.set_width_height(2600, 1600);
    target.add_port("data", IO::Input(2));
    let mut tracks = TrackDefinitions::new();
    tracks.add_track(TrackDefinition::new(
        "M2",
        50,
        100,
        TrackOrientation::Horizontal,
        Some(Polygon::from_width_height(20, 40)),
        None,
    ));
    target.set_track_definitions(tracks);
    let track = target.get_track("M2").unwrap();
    assert!(matches!(
        track.get_orientation(),
        TrackOrientation::Horizontal
    ));
    let original = reference.get_port("data").bit(0).get_physical_pin();
    let delta = Coordinate {
        x: target.bbox().unwrap().max_x - original.transformed_polygon().bbox().max_x,
        y: 2 * track.get_period(),
    };
    let pin = &original + delta;
    target.get_port("data").bit(0).place(pin.clone());
    target.block_tracks_for_pin(&pin).unwrap();
    target
        .place_pins_on_right_edge(&[("data", 1)], ["M2"], Range::new(550, 1050), None)
        .unwrap();
    assert_eq!(
        target.get_physical_pin("data", 0).translation(),
        original.translation() + delta
    );
    assert_eq!(target.get_physical_pin("data", 1).translation().y, 650);
}
