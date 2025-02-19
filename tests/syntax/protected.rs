// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_protected() {
    let module_a_verilog = "
      module ModuleA (
          input a
      );
      `protected
      asdf
      `endprotected
      endmodule
      ";

    let a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    let top = ModDef::new("TopModule");
    let a_inst = top.instantiate(&a, None, None);
    a_inst.get_port("a").tieoff(0);

    assert_eq!(
        top.emit(false),
        "\
module TopModule;
  ModuleA ModuleA_i (
    .a(1'h0)
  );
endmodule
"
    );
}
