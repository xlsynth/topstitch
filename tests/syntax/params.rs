// SPDX-License-Identifier: Apache-2.0

use slang_rs::str2tmpfile;
use slang_rs::SlangConfig;
use topstitch::*;

#[test]
fn test_params() {
    let verilog = str2tmpfile(
        "\
module Orig #(
  parameter W = 8
) (
  output [W-1:0] data
);
endmodule
",
    )
    .unwrap();

    let base = ModDef::from_verilog_file("Orig", verilog.path(), true, false);

    let w16 = base.parameterize(&[("W", 16)], None, None);
    let w32 = base.parameterize(&[("W", 32)], None, None);

    let top = ModDef::new("Top");

    top.instantiate(&w16, Some("inst0"), None)
        .get_port("data")
        .unused();
    top.instantiate(&w16, Some("inst1"), None)
        .get_port("data")
        .unused();

    top.instantiate(&w32, Some("inst2"), None)
        .get_port("data")
        .unused();
    top.instantiate(&w32, Some("inst3"), None)
        .get_port("data")
        .unused();

    assert_eq!(
        top.emit(true),
        "\
module Orig_W_16(
  output wire [15:0] data
);
  Orig #(
    .W(32'h0000_0010)
  ) Orig_i (
    .data(data)
  );
endmodule

module Orig_W_32(
  output wire [31:0] data
);
  Orig #(
    .W(32'h0000_0020)
  ) Orig_i (
    .data(data)
  );
endmodule

module Top;
  Orig_W_16 inst0 (
    .data()
  );
  Orig_W_16 inst1 (
    .data()
  );
  Orig_W_32 inst2 (
    .data()
  );
  Orig_W_32 inst3 (
    .data()
  );
endmodule
"
    );
}

#[test]
fn test_parameterize_with_header() {
    let header = str2tmpfile("`define MY_PARAM_A 12").unwrap();
    let header_name = header.path().file_name().unwrap().to_str().unwrap();

    let source = str2tmpfile(&format!(
        "
      `include \"{header_name}\"
      module MyModule #(
          parameter MY_PARAM_B = 23
      ) (
          input [`MY_PARAM_A-1:0] a,
          output [MY_PARAM_B-1:0] b
      );
      endmodule
      "
    ))
    .unwrap();

    let cfg = SlangConfig {
        sources: &[source.path().to_str().unwrap()],
        incdirs: &[header.path().parent().unwrap().to_str().unwrap()],
        parameters: &[],
        ..Default::default()
    };
    let orig = ModDef::from_verilog_using_slang("MyModule", &cfg, false);
    let modified = orig.parameterize(&[("MY_PARAM_B", 34)], Some("MyModifiedModule"), None);

    assert_eq!(orig.get_port("a").io().width(), 12);
    assert_eq!(orig.get_port("b").io().width(), 23);

    assert_eq!(modified.get_port("a").io().width(), 12);
    assert_eq!(modified.get_port("b").io().width(), 34);
}

#[test]
fn test_define_with_parameterize() {
    let source = str2tmpfile(
        "
          module foo #(
            parameter N=1
          ) (
            `ifdef BAR
            input [N-1:0] a
            `else
            output [N-1:0] b
            `endif
        );
        endmodule",
    )
    .unwrap();

    let cfg_no_define = SlangConfig {
        sources: &[source.path().to_str().unwrap()],
        ..Default::default()
    };
    let orig_no_define = ModDef::from_verilog_using_slang("foo", &cfg_no_define, false);
    let parameterized_no_define = orig_no_define.parameterize(&[("N", 8)], None, None);

    assert_eq!(
        parameterized_no_define.emit(true),
        "\
module foo_N_8(
  output wire [7:0] b
);
  foo #(
    .N(32'h0000_0008)
  ) foo_i (
    .b(b)
  );
endmodule
"
    );

    let cfg_with_define = SlangConfig {
        sources: &[source.path().to_str().unwrap()],
        defines: &[("BAR", "1")],
        ..Default::default()
    };
    let orig_with_define = ModDef::from_verilog_using_slang("foo", &cfg_with_define, false);
    let parameterized_with_define = orig_with_define.parameterize(&[("N", 8)], None, None);

    assert_eq!(
        parameterized_with_define.emit(true),
        "\
module foo_N_8(
  input wire [7:0] a
);
  foo #(
    .N(32'h0000_0008)
  ) foo_i (
    .a(a)
  );
endmodule
"
    );
}
