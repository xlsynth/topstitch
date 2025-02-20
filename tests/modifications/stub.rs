// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_stub_recursive() {
    let a_def = ModDef::new("a");
    let b_def = ModDef::new("b");
    let c_def = ModDef::new("skip_c");
    let d_def = ModDef::new("skip_d");
    let e_def = ModDef::new("e");
    let f_def = ModDef::new("f");
    let g_def = ModDef::new("g");

    a_def.instantiate(&b_def, None, None);
    a_def.instantiate(&c_def, None, None);
    b_def.instantiate(&d_def, None, None);
    c_def.instantiate(&e_def, None, None);
    d_def.instantiate(&f_def, None, None);
    e_def.instantiate(&g_def, None, None);

    a_def.stub_recursive("^skip_(.*)$");

    assert_eq!(
        a_def.emit(true),
        "\
module skip_d;

endmodule
module b;
  skip_d skip_d_i (
    
  );
endmodule
module skip_c;

endmodule
module a;
  b b_i (
    
  );
  skip_c skip_c_i (
    
  );
endmodule
"
    );
}
