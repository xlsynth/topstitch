// SPDX-License-Identifier: Apache-2.0

use topstitch::{BoundingBox, IO, LefDefOptions, ModDef, MultiplePinShapesPolicy};

// Synthetic input with the same ports and pin geometry as REFERENCE_DEF.
const REFERENCE_LEF: &str = r#"# SPDX-License-Identifier: Apache-2.0
# The same logical ports and absolute pin geometry as reference.def.
VERSION 5.8 ;
BUSBITCHARS "<>" ;
UNITS DATABASE MICRONS 1000 ; END UNITS
PROPERTYDEFINITIONS
  MACRO note STRING ;
END PROPERTYDEFINITIONS
LAYER M1
  TYPE ROUTING ; DIRECTION HORIZONTAL ; PITCH 0.2 ; WIDTH 0.1 ;
END M1
NONDEFAULTRULE wide
  LAYER M1 WIDTH 0.2 ; SPACING 0.2 ; END M1
END wide
SITE core
  CLASS CORE ; SIZE 0.2 BY 0.4 ;
END core

MACRO reference
  CLASS BLOCK ;
  ORIGIN ( 0.0 -1.0 ) ;
  SIZE 2.0 BY
    1.6 ; # Statements may span lines.
  PIN data<0> DIRECTION INPUT ; USE SiGnAl ;
    PORT LAYER M2 ;
      RECT ( 1.97 1.33 )
        ( 1.99 1.37 ) ;
    END
  END data<0>
  PIN data<1> DIRECTION INPUT ;
    PORT LAYER M2 ; RECT 1.96 1.73 1.99 1.79 ; END
  END data<1>
  PIN debug DIRECTION OUTPUT ; END debug
  PIN VDD DIRECTION INOUT ; USE pOwEr ;
    PORT LAYER M1 ; RECT 0.98 2.48 1.02 2.52 ; END
  END VDD
  OBS LAYER M1 ; RECT 0.4 1.4 0.8 1.8 ; END
  PROPERTY note "PIN fake ; END reference" ;
END reference
END LIBRARY
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
fn reads_reference_lef_into_moddef() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("reference.lef");
    std::fs::write(&path, REFERENCE_LEF).unwrap();
    let opts = LefDefOptions {
        units_microns: 2000,
        check_fully_pinned: false,
        ..options()
    };
    let block = ModDef::from_lef_file(&path, &opts);
    assert_eq!(block.get_name(), "reference");
    assert_eq!(bounds(block.bbox().unwrap()), (0, 0, 4000, 3200));
    assert_eq!(block.get_ports(None).len(), 2); // POWER is excluded by default.
    assert!(matches!(block.get_port("data").io(), IO::Input(2)));
    assert!(matches!(block.get_port("debug").io(), IO::Output(1)));
    assert!(!block.get_port("debug").has_physical_pin());
    assert!(block.get_track_definitions().is_none());
    let round_trip = ModDef::from_lef(block.emit_lef(&opts), &opts);
    // ORIGIN shifts raw geometry before scaling, but does not shift SIZE.
    for (bit, expected) in [(0, (3940, 660, 3980, 740)), (1, (3920, 1460, 3980, 1580))] {
        let pin = block.get_physical_pin("data", bit);
        assert_eq!(pin.layer, "M2");
        assert_eq!(bounds(pin.transformed_polygon().bbox()), expected);
        assert_eq!(
            round_trip
                .get_physical_pin("data", bit)
                .transformed_polygon()
                .0,
            pin.transformed_polygon().0
        );
    }
}

#[test]
fn reference_import_respects_name_use_and_layer_filters() {
    let block = ModDef::from_lef(
        REFERENCE_LEF,
        &LefDefOptions {
            ignore_pin_names: ["debug".to_string()].into(),
            skip_pin_uses: Default::default(),
            valid_pin_layers: Some(["M1".to_string()].into()),
            ..options()
        },
    );
    assert_eq!(block.get_ports(None).len(), 2);
    assert!(!block.has_port("debug"));
    assert_eq!(
        bounds(
            block
                .get_physical_pin("VDD", 0)
                .transformed_polygon()
                .bbox()
        ),
        (980, 1480, 1020, 1520)
    );
    for bit in 0..2 {
        assert!(!block.get_port("data").bit(bit).has_physical_pin());
    }
}

#[test]
fn multiple_shapes_respect_policy_and_layer_selection() {
    let lef = r#"BUSBITCHARS "<>" ;
MACRO reference
  SIZE 1 BY 1 ;
  PIN data<0> DIRECTION INPUT ;
    PORT LAYER M1 ; RECT 0 0 0.5 0.5 ; END
    PORT LAYER M2 ; RECT 0.1 0.2 0.11 0.22 ; RECT 0.13 0.25 0.15 0.28 ; END
  END data<0>
  PIN data<1> DIRECTION INPUT ;
    PORT LAYER M2 ; RECT 0 0 0.5 0.5 ; END
    PORT LAYER M1 ; RECT 0.3 0.4 0.31 0.42 ; RECT 0.33 0.45 0.35 0.48 ; END
  END data<1>
END reference
"#;
    let first = ModDef::from_lef(lef, &options());
    assert_eq!(first.get_physical_pin("data", 0).layer, "M1");
    assert_eq!(first.get_physical_pin("data", 1).layer, "M2");
    let strict = LefDefOptions {
        multiple_pin_shapes_policy: MultiplePinShapesPolicy::Error,
        ..options()
    };
    assert!(std::panic::catch_unwind(|| ModDef::from_lef(lef, &strict)).is_err());
    let mut opts = LefDefOptions {
        multiple_pin_shapes_policy: MultiplePinShapesPolicy::BoundingBox,
        pin_layer_selections: [
            (("data".to_string(), 0), "M2".to_string()),
            (("data".to_string(), 1), "M1".to_string()),
        ]
        .into(),
        ..options()
    };
    let block = ModDef::from_lef(lef, &opts);
    for (bit, layer, expected) in [
        (0, "M2", (100, 200, 150, 280)),
        (1, "M1", (300, 400, 350, 480)),
    ] {
        let pin = block.get_physical_pin("data", bit);
        assert_eq!(pin.layer, layer);
        assert_eq!(bounds(pin.transformed_polygon().bbox()), expected);
        assert_eq!(pin.polygon.0.len(), 4);
    }
    opts.valid_pin_layers = Some(["M1".to_string()].into());
    assert!(std::panic::catch_unwind(|| ModDef::from_lef(lef, &opts)).is_err());
    opts.valid_pin_layers = None;
    opts.pin_layer_selections.clear();
    assert!(std::panic::catch_unwind(|| ModDef::from_lef(lef, &opts)).is_err());

    let malformed = lef.replace("LAYER M2 ; RECT 0.1 0.2 0.11 0.22", "LAYER ; RECT invalid");
    assert_eq!(
        ModDef::from_lef(&malformed, &options())
            .get_physical_pin("data", 0)
            .layer,
        "M1"
    );
    let ignored = LefDefOptions {
        ignore_pin_names: ["data".to_string()].into(),
        ..strict
    };
    assert!(!ModDef::from_lef(&malformed, &ignored).has_port("data"));
}

#[test]
fn reads_multiple_macros_and_skips_configured_sections() {
    let lef = format!(
        "SKIPPED_SECTION\n MACRO invalid_macro ;\nEND SKIPPED_SECTION\n{}",
        REFERENCE_LEF.replace('<', "(").replace('>', ")").replace(
            "END LIBRARY",
            "MACRO other SIZE 3 BY 4 ; END other\nEND LIBRARY"
        )
    );
    let blocks = ModDef::all_from_lef(
        lef,
        &LefDefOptions {
            skip_lef_sections: ["SKIPPED_SECTION".to_string()].into(),
            ..options()
        },
    );
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].get_name(), "reference");
    assert!(matches!(blocks[0].get_port("data").io(), IO::Input(2)));
    assert_eq!(blocks[1].get_name(), "other");
    assert_eq!(bounds(blocks[1].bbox().unwrap()), (0, 0, 3000, 4000));
}
