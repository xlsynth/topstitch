// SPDX-License-Identifier: Apache-2.0

use num_bigint::BigInt;
use slang_rs::str2tmpfile;
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

    let cfg = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        incdirs: &[header.path().parent().unwrap().to_str().unwrap()],
        parameters: &[],
        ..Default::default()
    };
    let orig = ModDef::from_verilog_with_config("MyModule", &cfg);
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

    let cfg_no_define = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        ..Default::default()
    };
    let orig_no_define = ModDef::from_verilog_with_config("foo", &cfg_no_define);
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

    let cfg_with_define = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        defines: &[("BAR", "1")],
        ..Default::default()
    };
    let orig_with_define = ModDef::from_verilog_with_config("foo", &cfg_with_define);
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

#[test]
fn test_64bit_param_import() {
    let source = str2tmpfile(
        "
          module bigcounter #(
            parameter longint MaxCount = 1,
            localparam int CountWidth = $clog2(MaxCount + 1)
          ) (
            input logic clk,
            input logic rst,
            input logic incr,
            output logic [CountWidth-1:0] count
          );
            always_ff @(posedge clk) begin
              if (rst) begin
                count <= '0;
              end else begin
                count <= count + incr;
              end
            end
          endmodule
        ",
    )
    .unwrap();

    let base = ModDef::from_verilog_file("bigcounter", source.path(), true, false);
    // Make the largest possible count that will fit in a 64-bit signed integer
    let max_count: BigInt = BigInt::from(2).pow(63) - 1;
    let modified = base.parameterize(&[("MaxCount", max_count.clone())], None, None);
    assert_eq!(
        modified.emit(true),
        format!(
            "\
module bigcounter_MaxCount_{max_count}(
  input wire clk,
  input wire rst,
  input wire incr,
  output wire [62:0] count
);
  bigcounter #(
    .MaxCount(64'h7fff_ffff_ffff_ffff)
  ) bigcounter_i (
    .clk(clk),
    .rst(rst),
    .incr(incr),
    .count(count)
  );
endmodule
",
        )
    );
}

#[test]
fn test_dependent_param_width() {
    let source = str2tmpfile(
        "
          module bigcounter #(
            parameter int MaxCountWidth = 32,
            parameter logic [MaxCountWidth-1:0] MaxCount = 1,
            localparam int CountWidth = $clog2(MaxCount + 1)
          ) (
            input logic clk,
            input logic rst,
            input logic incr,
            output logic [CountWidth-1:0] count
          );
            always_ff @(posedge clk) begin
              if (rst) begin
                count <= '0;
              end else begin
                count <= count + incr;
              end
            end
          endmodule
        ",
    )
    .unwrap();

    let base = ModDef::from_verilog_file("bigcounter", source.path(), true, false);
    let max_count_width = BigInt::from(64);
    let max_count: BigInt = BigInt::from(2).pow(63) - 1;
    let modified = base.parameterize(
        &[
            ("MaxCountWidth", max_count_width),
            ("MaxCount", max_count.clone()),
        ],
        None,
        None,
    );
    assert_eq!(
        modified.emit(true),
        format!(
            "\
module bigcounter_MaxCountWidth_64_MaxCount_{max_count}(
  input wire clk,
  input wire rst,
  input wire incr,
  output wire [62:0] count
);
  bigcounter #(
    .MaxCountWidth(32'h0000_0040),
    .MaxCount(64'h7fff_ffff_ffff_ffff)
  ) bigcounter_i (
    .clk(clk),
    .rst(rst),
    .incr(incr),
    .count(count)
  );
endmodule
",
        )
    );
}
