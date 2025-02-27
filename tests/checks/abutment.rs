// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

fn new_mod_def(name: &str) -> ModDef {
    let mod_def = ModDef::new(name);
    mod_def.add_port("in", IO::Input(8)).unused();
    mod_def.add_port("out", IO::Output(8)).tieoff(0);
    mod_def
}

/// Creates a 1D array of modules with the given names, with the output of one
/// module connected to the input of the next. If `loopback` is true, the output
/// of the last module is connected to the input of the first module.
fn new_top(names: &[&str], loopback: bool) -> (ModDef, Vec<ModInst>) {
    let top = ModDef::new("Top");

    let insts: Vec<_> = names
        .iter()
        .map(|&name| top.instantiate(&new_mod_def(name), None, None))
        .collect();

    for (k, inst) in insts[1..].iter().enumerate() {
        inst.get_port("in").connect(&insts[k].get_port("out"));
        inst.mark_adjacent_to(&insts[k]);
    }

    if loopback {
        insts
            .first()
            .unwrap()
            .get_port("in")
            .connect(&insts.last().unwrap().get_port("out"));
    } else {
        insts.first().unwrap().get_port("in").tieoff(0);
        insts.last().unwrap().get_port("out").unused();
    }

    (top, insts)
}

#[test]
fn test_abutment_check_loopback() {
    let (top, _insts) = new_top(&["A", "B", "C"], true);
    assert_eq!(
        top.find_non_abutted_connections(),
        vec![(
            "Top.A_i.in[7:0]".to_string(),
            "Top.C_i.out[7:0]".to_string()
        )]
    );
}

#[test]
fn test_abutment_check_no_loopback() {
    let (top, _insts) = new_top(&["A", "B", "C"], false);
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}

#[test]
fn test_abutment_check_with_ignore_a() {
    let (top, insts) = new_top(&["A", "B", "C"], true);
    insts[0].ignore_adjacency();
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}

#[test]
fn test_abutment_check_with_ignore_b() {
    let (top, insts) = new_top(&["A", "B", "C"], true);
    insts[1].ignore_adjacency();
    assert_eq!(
        top.find_non_abutted_connections(),
        vec![(
            "Top.A_i.in[7:0]".to_string(),
            "Top.C_i.out[7:0]".to_string()
        )]
    );
}

#[test]
fn test_abutment_check_with_ignore_c() {
    let (top, insts) = new_top(&["A", "B", "C"], true);
    insts[2].ignore_adjacency();
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}
