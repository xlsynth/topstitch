// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_has_port() {
    let a_verilog = "\
        module A(
          output a
        );
        endmodule";

    let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
    let b_mod_def = ModDef::new("B");
    let a_inst = b_mod_def.instantiate(&a_mod_def, None, None);
    a_inst.get_port("a").export_as("b");

    assert!(a_mod_def.has_port("a"));
    assert!(!a_mod_def.has_port("b"));
    assert!(a_inst.has_port("a"));
    assert!(!a_inst.has_port("b"));
    assert!(b_mod_def.has_port("b"));
    assert!(!b_mod_def.has_port("a"));
}

#[test]
fn test_get_inst_ports() {
    let a_verilog = "\
        module A(
          output a0,
          input a1
        );
        endmodule";

    let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
    let b_mod_def = ModDef::new("B");
    let a_inst = b_mod_def.instantiate(&a_mod_def, None, None);

    for (i, port) in a_inst.get_ports(None).iter().enumerate() {
        port.export_as(format!("b{}", i));
    }

    let ports = a_mod_def.get_ports(None);
    assert_eq!(ports.len(), 2);
    assert_eq!(ports[0].name(), "a0");
    assert_eq!(ports[1].name(), "a1");

    let ports = a_inst.get_ports(None);
    assert_eq!(ports.len(), 2);
    assert_eq!(ports[0].name(), "a0");
    assert_eq!(ports[1].name(), "a1");

    let ports = b_mod_def.get_ports(None);
    assert_eq!(ports.len(), 2);
    assert_eq!(ports[0].name(), "b0");
    assert_eq!(ports[1].name(), "b1");
}
