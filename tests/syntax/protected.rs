// SPDX-License-Identifier: Apache-2.0

use crate::common::module_from_text;

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

    let a = module_from_text("ModuleA", module_a_verilog);
    let top = ModDef::new("TopModule");
    let a_inst = top.instantiate(&a, None, None);
    a_inst.get_port("a").tieoff(0);

    assert_eq!(
        top.emit(EmitOptions {
            validate: false,
            ..Default::default()
        }),
        "\
module TopModule;
  ModuleA ModuleA_i (
    .a(1'h0)
  );
endmodule
"
    );
}
