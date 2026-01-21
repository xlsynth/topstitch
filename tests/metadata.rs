// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn metadata_moddef_and_ports() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("in", IO::Input(1));
    leaf.add_port("out", IO::Output(1));

    leaf.set_metadata("author", "name");
    leaf.set_metadata("revision", 3_i64);
    leaf.set_metadata("verified", true);

    let author: String = leaf.get_metadata("author").unwrap().into();
    let revision: i64 = leaf.get_metadata("revision").unwrap().into();
    let verified: bool = leaf.get_metadata("verified").unwrap().into();
    assert_eq!(author, "name");
    assert_eq!(revision, 3);
    assert!(verified);

    leaf.get_port("in").set_metadata("role", "control");
    leaf.get_port("out").set_metadata("gain", 1.25_f64);

    let role: String = leaf.get_port("in").get_metadata("role").unwrap().into();
    let gain: f64 = leaf.get_port("out").get_metadata("gain").unwrap().into();
    assert_eq!(role, "control");
    assert_eq!(gain, 1.25);

    leaf.clear_metadata("revision");
    assert_eq!(leaf.get_metadata("revision"), None);
    leaf.get_port("in").clear_metadata("role");
    assert_eq!(leaf.get_port("in").get_metadata("role"), None);
}

#[test]
fn metadata_modinst_and_ports() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("clk", IO::Input(1));
    leaf.add_port("data", IO::Output(8));

    let top = ModDef::new("Top");
    let inst_a = top.instantiate(&leaf, Some("inst_a"), None);
    let inst_b = top.instantiate(&leaf, Some("inst_b"), None);

    inst_a.set_metadata("owner", "alpha");
    inst_b.set_metadata("owner", "beta");
    inst_a.set_metadata("weight", 7_i64);
    inst_b.set_metadata("weight", 11_i64);

    let owner_a: String = inst_a.get_metadata("owner").unwrap().into();
    let owner_b: String = inst_b.get_metadata("owner").unwrap().into();
    let weight_a: i64 = inst_a.get_metadata("weight").unwrap().into();
    let weight_b: i64 = inst_b.get_metadata("weight").unwrap().into();
    assert_eq!(owner_a, "alpha");
    assert_eq!(owner_b, "beta");
    assert_eq!(weight_a, 7);
    assert_eq!(weight_b, 11);

    inst_a.get_port("clk").set_metadata("speed", "fast");
    inst_b.get_port("clk").set_metadata("speed", "slow");
    inst_a.get_port("data").set_metadata("msb_first", true);
    inst_b.get_port("data").set_metadata("msb_first", false);

    let speed_a: String = inst_a.get_port("clk").get_metadata("speed").unwrap().into();
    let speed_b: String = inst_b.get_port("clk").get_metadata("speed").unwrap().into();
    let msb_first_a: bool = inst_a
        .get_port("data")
        .get_metadata("msb_first")
        .unwrap()
        .into();
    let msb_first_b: bool = inst_b
        .get_port("data")
        .get_metadata("msb_first")
        .unwrap()
        .into();
    assert_eq!(speed_a, "fast");
    assert_eq!(speed_b, "slow");
    assert!(msb_first_a);
    assert!(!msb_first_b);

    inst_a.clear_metadata("owner");
    assert_eq!(inst_a.get_metadata("owner"), None);
    inst_b.get_port("clk").clear_metadata("speed");
    assert_eq!(inst_b.get_port("clk").get_metadata("speed"), None);
}

#[test]
fn metadata_intf_moddef_and_modinst() {
    let leaf = ModDef::new("Leaf");
    leaf.add_port("bus_data", IO::Input(8));
    leaf.add_port("bus_valid", IO::Input(1));
    leaf.def_intf_from_name_underscore("bus");

    leaf.get_intf("bus").set_metadata("protocol", "custom");
    let protocol: String = leaf
        .get_intf("bus")
        .get_metadata("protocol")
        .unwrap()
        .into();
    assert_eq!(protocol, "custom");

    let top = ModDef::new("Top");
    let inst_a = top.instantiate(&leaf, Some("inst_a"), None);
    let inst_b = top.instantiate(&leaf, Some("inst_b"), None);

    inst_a.get_intf("bus").set_metadata("channel", 0_i64);
    inst_b.get_intf("bus").set_metadata("channel", 1_i64);
    inst_a.get_intf("bus").set_metadata("msb_first", true);
    inst_b.get_intf("bus").set_metadata("msb_first", false);

    let channel_a: i64 = inst_a
        .get_intf("bus")
        .get_metadata("channel")
        .unwrap()
        .into();
    let channel_b: i64 = inst_b
        .get_intf("bus")
        .get_metadata("channel")
        .unwrap()
        .into();
    let msb_first_a: bool = inst_a
        .get_intf("bus")
        .get_metadata("msb_first")
        .unwrap()
        .into();
    let msb_first_b: bool = inst_b
        .get_intf("bus")
        .get_metadata("msb_first")
        .unwrap()
        .into();
    assert_eq!(channel_a, 0);
    assert_eq!(channel_b, 1);
    assert!(msb_first_a);
    assert!(!msb_first_b);

    inst_a.get_intf("bus").clear_metadata("msb_first");
    assert_eq!(inst_a.get_intf("bus").get_metadata("msb_first"), None);
}
