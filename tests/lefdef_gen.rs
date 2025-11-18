// SPDX-License-Identifier: Apache-2.0

use topstitch::lefdef::{
    generate_def, generate_lef, DefComponent, DefOrientation, LefComponent, LefShape,
};
use topstitch::LefDefOptions;

#[test]
fn generate_lef_basic() {
    let macros = vec![LefComponent {
        name: "BLOCK_A".to_string(),
        width: 100,
        height: 200,
        shape: LefShape {
            layer: "OUTLINE".to_string(),
            polygon: vec![(0, 0), (100, 0), (100, 200), (0, 200)],
        },
        pins: vec![],
    }];
    let lef = generate_lef(&macros, &LefDefOptions::default());
    assert!(lef.contains("MACRO BLOCK_A"));
    assert!(lef.contains("SIZE 100 BY 200 ;"));
    assert!(lef.contains("POLYGON 0 0 100 0 100 200 0 200 ;"));
}

#[test]
fn generate_def_components() {
    let comps = vec![
        DefComponent {
            inst_name: "top/inst0".to_string(),
            macro_name: "BLOCK_A".to_string(),
            x: 10,
            y: 20,
            orientation: DefOrientation::N,
        },
        DefComponent {
            inst_name: "top/inst1".to_string(),
            macro_name: "BLOCK_B".to_string(),
            x: -5,
            y: 7,
            orientation: DefOrientation::E,
        },
    ];
    let def = generate_def("top", None, &[], &comps, &LefDefOptions::default());
    assert!(def.contains("DESIGN top ;"));
    assert!(def.contains("- top/inst0 BLOCK_A + PLACED ( 10 20 ) N ;"));
    assert!(def.contains("- top/inst1 BLOCK_B + PLACED ( -5 7 ) E ;"));
}
