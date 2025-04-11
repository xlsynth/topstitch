// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_extract_hierarchy() {
    let source = slang_rs::str2tmpfile(
        "
        module C;
        endmodule
        module B;
          C c0();
          C c1();
        endmodule
        module A;
          B b0();
        endmodule
        ",
    )
    .unwrap();

    let cfg = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        include_hierarchy: true,
        ..Default::default()
    };

    let result = ModDef::from_verilog_using_config("A", &cfg);

    assert_eq!(
        result.emit(true),
        "\
module C;

endmodule
module B;
  C c0 (
    
  );
  C c1 (
    
  );
endmodule
module A;
  B b0 (
    
  );
endmodule
",
    );
}
