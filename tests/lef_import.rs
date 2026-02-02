// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use topstitch::{Coordinate, IO, LefDefOptions, ModDef};

#[test]
fn import_lef_macro_into_moddef() {
    let lef = r#"VERSION 5.8 ;
BUSBITCHARS "<>" ;

MACRO my_macro
  CLASS BLOCK ;
  ORIGIN 0.0 0.0 ;
  SIZE 1.0 BY 2.0 ;
  PIN a<0>
    DIRECTION INPUT ;
  END a<0>
  PIN a<1>
    DIRECTION INPUT ;
  END a<1>
  PIN a<2>
    DIRECTION INPUT ;
  END a<2>
  PIN b
    DIRECTION OUTPUT ;
  END b
  PIN VDD
    DIRECTION INOUT ;
  END VDD
END my_macro
END LIBRARY
"#;

    let opts = LefDefOptions {
        bus_bit_chars: "<>".to_string(),
        units_microns: 100,
        ignore_pin_names: HashSet::from(["VDD".to_string()]),
        ..LefDefOptions::default()
    };
    let md = ModDef::from_lef(lef, &opts);

    assert_eq!(md.get_name(), "my_macro");
    let bbox = md.get_shape().expect("shape missing").bbox();
    assert_eq!(bbox.max_x, 100);
    assert_eq!(bbox.max_y, 200);

    assert!(matches!(md.get_port("a").io(), IO::Input(3)));
    assert!(matches!(md.get_port("b").io(), IO::Output(1)));
    assert!(!md.has_port("VDD"));
}

#[test]
fn import_lef_with_skipped_section() {
    let lef = r#"VERSION 5.8 ;
SKIPPED_SECTION
  MACRO invalid_macro ;
END SKIPPED_SECTION

MACRO my_macro
  SIZE 1.0 BY 2.0 ;
  PIN a
    DIRECTION INPUT ;
  END a
END my_macro
END LIBRARY
"#;

    let opts = LefDefOptions {
        skip_lef_sections: HashSet::from(["SKIPPED_SECTION".to_string()]),
        ..LefDefOptions::default()
    };
    let moddefs = ModDef::all_from_lef(lef, &opts);

    assert_eq!(moddefs.len(), 1);
    assert_eq!(moddefs[0].get_name(), "my_macro");
    assert!(moddefs[0].has_port("a"));
    assert!(!moddefs[0].has_port("invalid_macro"));
}

#[test]
fn import_lef_pin_geometry() {
    let lef = r#"VERSION 5.8 ;
BUSBITCHARS "<>" ;

MACRO my_macro
  CLASS BLOCK ;
  ORIGIN 0.0 0.0 ;
  SIZE 1.0 BY 1.0 ;
  PIN a
    DIRECTION INPUT ;
    PORT
      LAYER M1 ;
      RECT 0.1 0.2 0.3 0.4 ;
    END
  END a
END my_macro
END LIBRARY
"#;

    let opts = LefDefOptions {
        units_microns: 100,
        ..LefDefOptions::default()
    };
    let md = ModDef::from_lef(lef, &opts);
    let pin = md.get_physical_pin("a", 0);
    assert_eq!(pin.layer, "M1");
    let points = pin.transformed_polygon().0;
    assert_eq!(
        points,
        vec![
            Coordinate { x: 10, y: 20 },
            Coordinate { x: 10, y: 40 },
            Coordinate { x: 30, y: 40 },
            Coordinate { x: 30, y: 20 },
        ]
    );
}
