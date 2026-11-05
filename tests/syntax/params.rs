// SPDX-License-Identifier: Apache-2.0

use super::strict_module_from_text;
use crate::common::{module_from_config, module_from_text};

use num_bigint::BigInt;
use topstitch::*;

#[test]
fn test_params() {
    let verilog = "\
module Orig #(
  parameter W = 8
) (
  output [W-1:0] data
);
endmodule
";

    let base = module_from_text("Orig", verilog);

    let w16 = base.parameterize(&[("W", 16)]);
    let w32 = base.parameterize(&[("W", 32)]);

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
        top.emit(EmitOptions::default()),
        "\
module Top;
  Orig #(
    .W(32'h0000_0010)
  ) inst0 (
    .data()
  );
  Orig #(
    .W(32'h0000_0010)
  ) inst1 (
    .data()
  );
  Orig #(
    .W(32'h0000_0020)
  ) inst2 (
    .data()
  );
  Orig #(
    .W(32'h0000_0020)
  ) inst3 (
    .data()
  );
endmodule
"
    );
}

#[test]
fn test_string_params() {
    let verilog = r#"module Orig #(
  parameter MODE = "narrow",
  localparam int WIDTH = MODE == "wide" ? 16 : 8
) (
  output [WIDTH-1:0] data
);
endmodule
"#;

    let base = module_from_text("Orig", verilog);
    let wide = base.parameterize(&[("MODE", "wide")]);
    let owned = String::from("wide");

    assert_eq!(wide.get_port("data").io().width(), 16);
    assert_eq!(
        base.parameterize(&[("MODE", &owned)])
            .get_port("data")
            .io()
            .width(),
        16
    );
    assert_eq!(
        wide.wrap(None, None).emit(EmitOptions::default()),
        r#"module Orig_wrapper(
  output wire [15:0] data
);
  Orig #(
    .MODE("wide")
  ) Orig_i (
    .data(data)
  );
endmodule
"#
    );
}

#[test]
fn test_parameterize_with_unpacked_localparam() {
    let source = r#"module LocalArray #(
  parameter int BITS = 8,
  parameter logic [BITS-1:0] WIDTH = 3,
  localparam int WIDTHS [2] = '{1, int'(WIDTH)}
) (
  output [WIDTHS[1]-1:0] data
);
endmodule
"#;

    let base = strict_module_from_text("LocalArray", source);
    assert_eq!(base.get_port("data").io().width(), 3);

    let six = base.parameterize(&[("WIDTH", 6)]);
    assert_eq!(six.get_port("data").io().width(), 6);
    let widened = six.parameterize(&[("BITS", 16)]);
    assert_eq!(widened.get_port("data").io().width(), 6);

    let emitted = widened.wrap(None, None).emit(EmitOptions::default());
    assert_eq!(
        emitted,
        "\
module LocalArray_wrapper(
  output wire [5:0] data
);
  LocalArray #(
    .WIDTH(16'h0006),
    .BITS(32'h0000_0010)
  ) LocalArray_i (
    .data(data)
  );
endmodule
"
    );
    let cached = six.parameterize(&[("BITS", 16)]);
    assert_eq!(cached.get_port("data").io().width(), 6);
    assert_eq!(
        cached.wrap(None, None).emit(EmitOptions::default()),
        emitted
    );
}

#[test]
fn test_string_parameterize_with_unpacked_localparam() {
    let source = r#"module StringLocalArray #(
  parameter string MODE = "narrow",
  localparam int WIDTHS [2] = '{8, 16}
) (
  output [WIDTHS[MODE == "wide"]-1:0] data
);
endmodule
"#;

    let base = strict_module_from_text("StringLocalArray", source);
    assert_eq!(base.get_port("data").io().width(), 8);
    let wide = base.parameterize(&[("MODE", "wide")]);
    assert_eq!(wide.get_port("data").io().width(), 16);
    assert_eq!(
        wide.wrap(None, None).emit(EmitOptions::default()),
        r#"module StringLocalArray_wrapper(
  output wire [15:0] data
);
  StringLocalArray #(
    .MODE("wide")
  ) StringLocalArray_i (
    .data(data)
  );
endmodule
"#
    );
}

#[test]
#[should_panic(expected = "Parameter type for 'GAIN' not found")]
fn test_parameterize_rejects_unsupported_override_type() {
    let source = "module RealParameter #(parameter real GAIN = 1.0) (output data); endmodule";
    let base = strict_module_from_text("RealParameter", source);
    base.parameterize(&[("GAIN", 2)]);
}

#[test]
fn test_untyped_string_param_accepts_integer_override() {
    let verilog = r#"module Orig #(
  parameter MODE = "8",
  localparam int WIDTH = MODE == "8" ? 8 : MODE == 256 ? 256 : 16
) (
  output [WIDTH-1:0] data
);
endmodule
"#;

    let base = module_from_text("Orig", verilog);
    let string = base.parameterize(&[("MODE", "8")]);
    let integer = base.parameterize(&[("MODE", 256)]);

    assert_eq!(base.get_port("data").io().width(), 8);
    assert_eq!(string.get_port("data").io().width(), 8);
    assert_eq!(integer.get_port("data").io().width(), 256);
    assert!(
        string
            .wrap(None, None)
            .emit(EmitOptions::default())
            .contains(r#".MODE("8")"#)
    );
    let integer_verilog = integer.wrap(None, None).emit(EmitOptions::default());
    // The 32-bit override must not be truncated to the 8-bit default's width.
    assert!(
        integer_verilog.contains(".MODE(32'h0000_0100)"),
        "{integer_verilog}"
    );

    assert_eq!(
        string
            .parameterize(&[("MODE", 256)])
            .get_port("data")
            .io()
            .width(),
        256
    );
    assert_eq!(
        integer
            .parameterize(&[("MODE", "8")])
            .get_port("data")
            .io()
            .width(),
        8
    );
}

#[test]
fn test_mixed_string_and_integer_params() {
    let verilog = r#"module Orig #(
  parameter string MODE = "narrow",
  parameter int WIDTH = 8,
  parameter int DEPTH = 0
) (
  output [WIDTH + (MODE == "wide" ? 8 : 0) + DEPTH-1:0] data
);
endmodule
"#;

    let base = module_from_text("Orig", verilog);
    let mixed = base.parameterize::<ParameterValue>(&[
        ("WIDTH", 12.into()),
        ("MODE", "wide".into()),
        ("DEPTH", BigInt::from(4).into()),
    ]);

    assert_eq!(mixed.get_port("data").io().width(), 24);
    assert_eq!(
        mixed.wrap(None, None).emit(EmitOptions::default()),
        r#"module Orig_wrapper(
  output wire [23:0] data
);
  Orig #(
    .WIDTH(32'h0000_000c),
    .MODE("wide"),
    .DEPTH(32'h0000_0004)
  ) Orig_i (
    .data(data)
  );
endmodule
"#
    );

    let reversed = base
        .parameterize(&[("MODE", "wide")])
        .parameterize(&[("WIDTH", 12)])
        .parameterize(&[("DEPTH", BigInt::from(4))]);
    assert_eq!(reversed.get_port("data").io().width(), 24);

    let replaced = reversed.parameterize(&[("MODE", "narrow")]);
    assert_eq!(replaced.get_port("data").io().width(), 16);
}

#[test]
fn test_string_param_printable_ascii() {
    let verilog = r#"module Orig #(
  parameter string MODE = "default"
) (
  output data
);
endmodule
"#;

    let base = module_from_text("Orig", verilog);
    let value = "fast mode-2 / path:foo_bar.baz+42=@[]{}!#$%^&*();<>?|`~";
    let parameterized = base.parameterize(&[("MODE", value)]);

    assert!(
        parameterized
            .wrap(None, None)
            .emit(EmitOptions::default())
            .contains(&format!(".MODE(\"{value}\")"))
    );
}

#[test]
fn test_string_param_rejects_unsupported_characters() {
    let verilog = r#"module Orig #(
  parameter string MODE = "default"
) (
  output data
);
endmodule
"#;

    let base = module_from_text("Orig", verilog);

    for value in [
        "bad\"value",
        "bad'value",
        "bad\\value",
        "bad\nvalue",
        "bad\rvalue",
        "bad\tvalue",
        "bad\0value",
        "bad\u{007f}value",
        "badévalue",
    ] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            base.parameterize(&[("MODE", value)])
        }));
        assert!(
            result.is_err(),
            "accepted unsupported parameter value: {value:?}"
        );
    }
}

#[test]
fn test_parameterize_with_header() {
    let directory = tempfile::tempdir().unwrap();
    let include_directory = directory.path().join("include");
    std::fs::create_dir(&include_directory).unwrap();
    let header = include_directory.join("params.svh");
    std::fs::write(&header, "`define MY_PARAM_A 12").unwrap();
    let header_name = header.file_name().unwrap().to_str().unwrap();

    let source = directory.path().join("module.sv");
    std::fs::write(
        &source,
        format!(
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
        ),
    )
    .unwrap();

    let cfg = ParserConfig {
        sources: &[Source::file(source.to_str().unwrap())],
        incdirs: &[include_directory.to_str().unwrap()],
        parameters: &[],
        ..Default::default()
    };
    let orig = module_from_config("MyModule", &cfg);
    let modified = orig.parameterize(&[("MY_PARAM_B", 34)]);

    assert_eq!(orig.get_port("a").io().width(), 12);
    assert_eq!(orig.get_port("b").io().width(), 23);

    assert_eq!(modified.get_port("a").io().width(), 12);
    assert_eq!(modified.get_port("b").io().width(), 34);
}

#[test]
fn test_define_with_parameterize() {
    let source = "
          module foo #(
            parameter N=1
          ) (
            `ifdef BAR
            input [N-1:0] a
            `else
            output [N-1:0] b
            `endif
        );
        endmodule";

    let cfg_no_define = ParserConfig {
        sources: &[Source::text(source)],
        ..Default::default()
    };
    let orig_no_define = module_from_config("foo", &cfg_no_define);
    let parameterized_no_define = orig_no_define.parameterize(&[("N", 8)]).wrap(None, None);

    assert_eq!(
        parameterized_no_define.emit(EmitOptions::default()),
        "\
module foo_wrapper(
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
        sources: &[Source::text(source)],
        defines: &[("BAR", "1")],
        ..Default::default()
    };
    let orig_with_define = module_from_config("foo", &cfg_with_define);
    let parameterized_with_define = orig_with_define.parameterize(&[("N", 8)]).wrap(None, None);

    assert_eq!(
        parameterized_with_define.emit(EmitOptions::default()),
        "\
module foo_wrapper(
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
    let source = "
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
        ";

    let base = module_from_text("bigcounter", source);
    // Make the largest possible count that will fit in a 64-bit signed integer
    let max_count: BigInt = BigInt::from(2).pow(63) - 1;
    let modified = base
        .parameterize(&[("MaxCount", max_count.clone())])
        .wrap(None, None);
    assert_eq!(
        modified.emit(EmitOptions::default()),
        format!(
            "\
module bigcounter_wrapper(
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
    let source = "
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
        ";

    let base = module_from_text("bigcounter", source);
    let max_count_width = BigInt::from(64);
    let max_count: BigInt = BigInt::from(2).pow(63) - 1;
    let modified = base
        .parameterize(&[
            ("MaxCountWidth", max_count_width),
            ("MaxCount", max_count.clone()),
        ])
        .wrap(None, None);
    assert_eq!(
        modified.emit(EmitOptions::default()),
        format!(
            "\
module bigcounter_wrapper(
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

#[test]
fn test_reparameterize() {
    let verilog = "\
module Orig #(
  parameter A = 0,
  parameter B = 1,
  parameter C = 2
) (
  output [A-1:0] a,
  output [B-1:0] b,
  output [C-1:0] c
);
endmodule
";

    let base = module_from_text("Orig", verilog);

    let param1 = base.parameterize(&[("A", 0x12), ("B", 0x34)]);
    let param2 = param1.parameterize(&[("B", 0x56), ("C", 0x78)]);

    assert_eq!(
        param1.wrap(None, None).emit(EmitOptions::default()),
        "\
module Orig_wrapper(
  output wire [17:0] a,
  output wire [51:0] b,
  output wire [1:0] c
);
  Orig #(
    .A(32'h0000_0012),
    .B(32'h0000_0034)
  ) Orig_i (
    .a(a),
    .b(b),
    .c(c)
  );
endmodule
"
    );
    assert_eq!(
        param2.wrap(None, None).emit(EmitOptions::default()),
        "\
module Orig_wrapper(
  output wire [17:0] a,
  output wire [85:0] b,
  output wire [119:0] c
);
  Orig #(
    .A(32'h0000_0012),
    .B(32'h0000_0056),
    .C(32'h0000_0078)
  ) Orig_i (
    .a(a),
    .b(b),
    .c(c)
  );
endmodule
"
    );
}

// Mixed text and file sources must keep their original order so macros resolve
// correctly, even after the input strings and original module are dropped.
#[test]
fn test_mixed_source_order_survives_parameterization() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("middle.sv");
    std::fs::write(
        &file,
        "`ifdef FIRST_WIDTH\n\
         `define COMBINED_WIDTH (`FIRST_WIDTH + 2)\n\
         `else\n\
         `define COMBINED_WIDTH 99\n\
         `endif\n",
    )
    .unwrap();
    let base = {
        let prefix = String::from("`define FIRST_WIDTH 5\n");
        let module = String::from(
            "module OrderedInputs #(parameter MULTIPLIER = 1) \
             (output [MULTIPLIER*`COMBINED_WIDTH-1:0] data); endmodule",
        );
        let cfg = ParserConfig {
            sources: &[
                Source::text(&prefix),
                Source::file(file.to_str().unwrap()),
                Source::text(&module),
            ],
            single_unit: true,
            ..Default::default()
        };
        module_from_config("OrderedInputs", &cfg)
    };

    assert_eq!(base.get_port("data").io().width(), 7);
    let first = base.parameterize(&[("MULTIPLIER", 2)]);
    drop(base);
    let second = first.parameterize(&[("MULTIPLIER", 3)]);
    assert_eq!(first.get_port("data").io().width(), 14);
    assert_eq!(second.get_port("data").io().width(), 21);
}

// Integer overrides for signed vectors and packed arrays must be emitted
// as hex literals with the full width of the parameter type.
#[test]
fn test_signed_parameter_emission() {
    for (param_type, port_width, literal) in [
        ("logic signed [1:0][3:0]", 8, "8'h80"),
        ("logic signed [7:0]", 8, "8'h80"),
        ("signed_word_t [1:0]", 64, "64'h0000_0000_0000_0080"),
    ] {
        let source = format!(
            "typedef int signed_word_t;
             module Signed #(parameter {param_type} P = '0)
               (output [{}:0] data);
               assign data = P;
             endmodule",
            port_width - 1
        );
        let emitted = strict_module_from_text("Signed", source)
            .parameterize(&[("P", 128u64)])
            .wrap(None, None)
            .emit(EmitOptions::default());
        assert!(
            emitted.contains(&format!("    .P({literal})\n")),
            "{param_type}: {emitted}"
        );
    }
}
