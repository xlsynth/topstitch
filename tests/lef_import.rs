// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use topstitch::{IO, LefDefOptions, ModDef};

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
