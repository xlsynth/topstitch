// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_extract_hierarchy() {
    let source = slang_rs::str2tmpfile(
        "
        module C;
        endmodule
        module B;
          logic genblk1;
          if (1) begin
            C c0();
          end
          C c1();
          if (0) begin
            C c2();
          end
        endmodule
        module A;
          for (genvar i=0; i<1; i++) begin : blkX
            if (1) begin : blkY
              B b0();
            end
          end
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

    // TODO: implement the comparison in a cleaner way, since module instance
    // names with dots may not be permitted by TopStitch in the future.

    assert_eq!(
        result.emit(true),
        "\
module C;

endmodule
module B;
  C genblk01.c0 (
    
  );
  C c1 (
    
  );
endmodule
module A;
  B blkX[0].blkY.b0 (
    
  );
endmodule
",
    );
}
