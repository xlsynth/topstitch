// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

fn new_mod_def(name: &str) -> ModDef {
    let mod_def = ModDef::new(name);
    mod_def.add_port("in", IO::Input(8)).unused();
    mod_def.add_port("out", IO::Output(8)).tieoff(0);
    mod_def
}

/// Creates a 1D array of modules with the given names, with the output of one
/// module connected to the input of the next.
fn new_top(names: &[&str]) -> (ModDef, Vec<ModInst>, Port, Port) {
    let top = ModDef::new("Top");

    let insts: Vec<_> = names
        .iter()
        .map(|&name| top.instantiate(&new_mod_def(name), None, None))
        .collect();

    let first = insts.first().unwrap().get_port("in");
    let last = insts.last().unwrap().get_port("out");

    for (k, inst) in insts[1..].iter().enumerate() {
        inst.get_port("in").connect(&insts[k].get_port("out"));
        inst.mark_adjacent_to(&insts[k]);
    }

    (top, insts, first, last)
}

#[test]
fn test_abutment_check_loopback() {
    let (top, _insts, first, last) = new_top(&["A", "B", "C"]);
    first.connect(&last);
    assert_eq!(
        top.find_non_abutted_connections(),
        vec![(
            "Top.A_i.in[7:0]".to_string(),
            "Top.C_i.out[7:0]".to_string()
        )]
    );
}

#[test]
fn test_abutment_check_loopback_non_abutted() {
    let (top, _insts, first, last) = new_top(&["A", "B", "C"]);
    first.connect_non_abutted(&last);
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}

#[test]
fn test_abutment_check_loopback_non_abutted_bit_by_bit() {
    let (top, _insts, first, last) = new_top(&["A", "B", "C"]);
    for i in 0..first.io().width() {
        first.bit(i).connect_non_abutted(&last.bit(i));
    }
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}

#[test]
fn test_abutment_check_loopback_intf() {
    let (top, insts, _, _) = new_top(&["A", "B", "C"]);
    let first_inst = insts.first().unwrap();
    let last_inst = insts.last().unwrap();
    first_inst.get_mod_def().def_intf_from_prefix("in", "in");
    last_inst.get_mod_def().def_intf_from_prefix("out", "out");
    first_inst
        .get_intf("in")
        .connect_non_abutted(&last_inst.get_intf("out"), false);
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}

#[test]
fn test_abutment_check_loopback_crossover() {
    let (top, insts, _, _) = new_top(&["A", "B", "C"]);
    let first_inst = insts.first().unwrap();
    let last_inst = insts.last().unwrap();
    first_inst
        .get_mod_def()
        .def_intf_from_regex("in", "in", "in");
    last_inst
        .get_mod_def()
        .def_intf_from_regex("out", "out", "out");
    first_inst
        .get_intf("in")
        .crossover_non_abutted(&last_inst.get_intf("out"), "in", "out");
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}

#[test]
fn test_abutment_check_no_loopback() {
    let (top, _insts, first, last) = new_top(&["A", "B", "C"]);
    first.export();
    last.export();
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}

#[test]
fn test_abutment_check_with_ignore_a() {
    let (top, insts, first, last) = new_top(&["A", "B", "C"]);
    first.connect(&last);
    insts[0].ignore_adjacency();
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}

#[test]
fn test_abutment_check_with_ignore_b() {
    let (top, insts, first, last) = new_top(&["A", "B", "C"]);
    first.connect(&last);
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
    let (top, insts, first, last) = new_top(&["A", "B", "C"]);
    first.connect(&last);
    insts[2].ignore_adjacency();
    assert_eq!(top.find_non_abutted_connections(), vec![]);
}
