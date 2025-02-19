// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_unused_bits_marked_correctly() {
    let mod_def = ModDef::new("TestMod");
    let in_port = mod_def.add_port("in", IO::Input(8));
    let out_port = mod_def.add_port("out", IO::Output(8));

    out_port.bit(0).connect(&in_port.bit(0));
    out_port.bit(7).connect(&in_port.bit(7));
    out_port.slice(6, 1).tieoff(0);

    in_port.slice(6, 1).unused();

    mod_def.validate(); // Should pass
}

#[test]
#[should_panic(expected = "TestMod.in[6:1] (ModDef Input) is unused")]
fn test_unused_bits_not_marked() {
    let mod_def = ModDef::new("TestMod");
    let in_port = mod_def.add_port("in", IO::Input(8));
    let out_port = mod_def.add_port("out", IO::Output(8));

    out_port.bit(0).connect(&in_port.bit(0));
    out_port.bit(7).connect(&in_port.bit(7));
    out_port.slice(6, 1).tieoff(0);

    mod_def.validate(); // Should panic
}
