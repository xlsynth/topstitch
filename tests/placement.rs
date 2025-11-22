// SPDX-License-Identifier: Apache-2.0

use topstitch::{LefDefOptions, ModDef, Orientation, Polygon, Usage};

#[test]
fn placement_basic() {
    let top = ModDef::new("top");

    let block = ModDef::new("block");
    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);

    // Instantiate and place block in top
    let b_inst = top.instantiate(&block, Some("b_inst_0"), None);
    b_inst.place((10, 20), Orientation::R0);

    // Compute placements and verify absolute shape
    let (placements, _) = top.collect_placements_and_mod_defs(&LefDefOptions::default());
    let b_placed = placements
        .get("b_inst_0")
        .expect("instance b_inst_0 not found");
    let abs_shape = block
        .get_shape()
        .unwrap()
        .apply_transform(&b_placed.transform);
    assert_eq!(
        abs_shape,
        Polygon::new(vec![
            (10, 20).into(),
            (10, 220).into(),
            (110, 220).into(),
            (110, 20).into(),
        ])
    );
}

#[test]
fn placement_skip_level() {
    let top = ModDef::new("top");
    let intermediate = ModDef::new("intermediate");
    let block = ModDef::new("block");

    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);

    // Instantiate and place block in intermediate hierarchy level
    top.instantiate(&intermediate, Some("i_inst_0"), None);
    let b_inst = intermediate.instantiate(&block, Some("b_inst_0"), None);
    b_inst.place((10, 20), Orientation::R0);

    // Compute placements and verify absolute shape
    let (placements, _) = top.collect_placements_and_mod_defs(&LefDefOptions {
        omit_top_module_in_hierarchy: false,
        divider_char: ".".to_string(),
        ..LefDefOptions::default()
    });
    let b_placed = placements
        .get("top.i_inst_0.b_inst_0")
        .expect("instance top.i_inst_0.b_inst_0 not found");
    let abs_shape = block
        .get_shape()
        .unwrap()
        .apply_transform(&b_placed.transform);
    assert_eq!(
        abs_shape,
        Polygon::new(vec![
            (10, 20).into(),
            (10, 220).into(),
            (110, 220).into(),
            (110, 20).into(),
        ])
    );
}

#[test]
fn placement_relative_basic() {
    let top = ModDef::new("top");
    let intermediate = ModDef::new("intermediate");
    let block = ModDef::new("block");

    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(100, 200);

    // Instantiate and place block in intermediate
    let b_inst = intermediate.instantiate(&block, Some("b_inst_0"), None);
    b_inst.place((12, 34), Orientation::R270);

    let i_inst = top.instantiate(&intermediate, Some("i_inst_0"), None);
    i_inst.place((56, 78), Orientation::MY);

    // Compute placements and verify absolute shape
    let (placements, _) = top.collect_placements_and_mod_defs(&LefDefOptions {
        omit_top_module_in_hierarchy: false,
        ..LefDefOptions::default()
    });
    let b_placed = placements
        .get("top/i_inst_0/b_inst_0")
        .expect("instance top/i_inst_0/b_inst_0 not found");
    let abs_shape = block
        .get_shape()
        .unwrap()
        .apply_transform(&b_placed.transform);
    assert_eq!(
        abs_shape,
        Polygon::new(vec![
            (44, 112).into(),
            (-156, 112).into(),
            (-156, 12).into(),
            (44, 12).into(),
        ])
    );
}

#[test]
fn placement_relative_to_parent() {
    let top = ModDef::new("top");
    let intermediate = ModDef::new("intermediate");
    let block = ModDef::new("block");

    block.set_usage(Usage::EmitStubAndStop);
    block.set_width_height(400, 300);

    let b_inst = intermediate.instantiate(&block, Some("b_inst_0"), None);
    b_inst.place((100, 200), Orientation::R0);

    for (index, orientation) in [
        Orientation::R0,
        Orientation::MX,
        Orientation::R180,
        Orientation::MY,
    ]
    .iter()
    .enumerate()
    {
        let i_inst = top.instantiate(&intermediate, Some(&format!("i_inst_{index}")), None);
        i_inst.place((0, 0), *orientation);
    }

    // Compute placements and verify absolute shape
    let (placements, _) = top.collect_placements_and_mod_defs(&LefDefOptions::default());

    let b_placed: std::collections::HashMap<_, _> = placements
        .iter()
        .map(|(inst_name, p)| {
            let abs_shape = block.get_shape().unwrap().apply_transform(&p.transform);
            (inst_name.clone(), abs_shape)
        })
        .collect();

    assert_eq!(
        b_placed.get("i_inst_0/b_inst_0"),
        Some(&Polygon::new(vec![
            (100, 200).into(),
            (100, 500).into(),
            (500, 500).into(),
            (500, 200).into(),
        ]))
    );

    assert_eq!(
        b_placed.get("i_inst_1/b_inst_0"),
        Some(&Polygon::new(vec![
            (100, -200).into(),
            (100, -500).into(),
            (500, -500).into(),
            (500, -200).into(),
        ]))
    );

    assert_eq!(
        b_placed.get("i_inst_2/b_inst_0"),
        Some(&Polygon::new(vec![
            (-100, -200).into(),
            (-100, -500).into(),
            (-500, -500).into(),
            (-500, -200).into(),
        ]))
    );

    assert_eq!(
        b_placed.get("i_inst_3/b_inst_0"),
        Some(&Polygon::new(vec![
            (-100, 200).into(),
            (-100, 500).into(),
            (-500, 500).into(),
            (-500, 200).into(),
        ]))
    );
}
