// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_port_slices() {
    // Define module A
    let a_mod_def = ModDef::new("A");
    a_mod_def.add_port("bus", IO::Input(8));

    // Define module B
    let b_mod_def = ModDef::new("B");
    b_mod_def.add_port("half_bus", IO::Input(4));

    let b0 = a_mod_def.instantiate(&b_mod_def, Some("b0"), None);
    let b1 = a_mod_def.instantiate(&b_mod_def, Some("b1"), None);

    let a_bus = a_mod_def.get_port("bus");
    b0.get_port("half_bus").connect(&a_bus.slice(3, 0));
    a_bus.slice(7, 4).connect(&b1.get_port("half_bus"));

    b_mod_def.set_usage(Usage::EmitStubAndStop);

    assert_eq!(
        a_mod_def.emit(true),
        "\
module B(
  input wire [3:0] half_bus
);

endmodule
module A(
  input wire [7:0] bus
);
  wire [3:0] b0_half_bus;
  wire [3:0] b1_half_bus;
  B b0 (
    .half_bus(b0_half_bus)
  );
  B b1 (
    .half_bus(b1_half_bus)
  );
  assign b0_half_bus[3:0] = bus[3:0];
  assign b1_half_bus[3:0] = bus[7:4];
endmodule
"
    );
}
