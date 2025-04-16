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
        module D;
          logic genblk1;
        endmodule
        module A;
          for (genvar i=0; i<2; i++) begin : blkX
            if (1) begin : blkY
              B b0();
            end
          end
          D d0();
        endmodule
        ",
    )
    .unwrap();

    let cfg = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        include_hierarchy: true,
        ..Default::default()
    };

    let result = ModDef::from_verilog_with_config("A", &cfg).report_all_instances();

    assert_eq!(
        result,
        vec![
            ("C".to_string(), "A.blkX[0].blkY.b0.genblk01.c0".to_string()),
            ("C".to_string(), "A.blkX[0].blkY.b0.c1".to_string()),
            ("B".to_string(), "A.blkX[0].blkY.b0".to_string()),
            ("C".to_string(), "A.blkX[1].blkY.b0.genblk01.c0".to_string()),
            ("C".to_string(), "A.blkX[1].blkY.b0.c1".to_string()),
            ("B".to_string(), "A.blkX[1].blkY.b0".to_string()),
            ("D".to_string(), "A.d0".to_string()),
        ]
    );
}

#[test]
fn test_extract_hierarchy_parameterized() {
    let source = slang_rs::str2tmpfile(
        "
        module C;
        endmodule
        module B #(parameter int ContainsInstance = 1);
          if (ContainsInstance) begin
            C c();
          end
        endmodule
        module A;
          B #(.ContainsInstance(0)) b0();
          B #(.ContainsInstance(1)) b1();
        endmodule
        ",
    )
    .unwrap();

    let cfg = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        include_hierarchy: true,
        ..Default::default()
    };

    let result = ModDef::from_verilog_with_config("A", &cfg).report_all_instances();

    assert_eq!(
        result,
        vec![
            ("B".to_string(), "A.b0".to_string()),
            ("C".to_string(), "A.b1.genblk1.c".to_string()),
            ("B".to_string(), "A.b1".to_string()),
        ]
    );
}

#[test]
fn test_extract_hierarchy_unknown_module() {
    let source = slang_rs::str2tmpfile(
        "
        module E;
        endmodule
        module A(
          input clk
        );
          B b();
          if (1) begin
            C c();
          end
          if (0) begin
            D d();
          end
          E e();
        endmodule
        ",
    )
    .unwrap();

    let cfg = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        include_hierarchy: true,
        ..Default::default()
    };

    let result = ModDef::from_verilog_with_config("A", &cfg).report_all_instances();

    assert_eq!(
        result,
        vec![
            ("B".to_string(), "A.b".to_string()),
            ("C".to_string(), "A.genblk1.c".to_string()),
            ("E".to_string(), "A.e".to_string()),
        ]
    );
}
