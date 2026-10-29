// SPDX-License-Identifier: Apache-2.0

use super::strict_module_from_text;
use crate::common::{module_from_config, module_from_text};

use topstitch::*;

#[test]
fn test_structs() {
    let structs = "
      package my_pack;
        typedef struct packed {
          logic [1:0] a; // width: 2
          logic [2:0] b; // width: 3
        } my_struct_t;
      endpackage
      ";
    let module_a_verilog = "
      module A (
        output my_pack::my_struct_t [3:0] x // width: 20
      );
      endmodule";
    let module_b_verilog = "
      module B (
        input my_pack::my_struct_t [1:0][1:0][1:0] y // width: 40
      );
      endmodule";
    let a = strict_module_from_text("A", format!("{structs}\n{module_a_verilog}"));
    let b = strict_module_from_text("B", format!("{structs}\n{module_b_verilog}"));

    let top = ModDef::new("Top");
    let a0 = top.instantiate(&a, Some("a0"), None);
    let a1 = top.instantiate(&a, Some("a1"), None);
    let b0 = top.instantiate(&b, Some("b0"), None);

    let b0_inputs = b0.get_port("y").subdivide(2);

    a0.get_port("x").connect(&b0_inputs[0]);
    a1.get_port("x").connect(&b0_inputs[1]);

    assert_eq!(
        top.emit(EmitOptions::default()),
        "\
module Top;
  wire [19:0] a0_x;
  A a0 (
    .x(a0_x)
  );
  wire [19:0] a1_x;
  A a1 (
    .x(a1_x)
  );
  B b0 (
    .y({a1_x, a0_x})
  );
endmodule
"
    );
}

#[test]
fn test_unions() {
    let unions = "
      package my_pack;
        typedef union packed {
          logic [2:0] a;
          logic [2:0] b;
        } my_union_t;
      endpackage
      ";
    let module_a_verilog = "
      module A (
        output my_pack::my_union_t x
      );
      endmodule";
    let module_b_verilog = "
      module B (
        input my_pack::my_union_t y
      );
      endmodule";
    let a = strict_module_from_text("A", format!("{unions}\n{module_a_verilog}"));
    let b = strict_module_from_text("B", format!("{unions}\n{module_b_verilog}"));

    let top = ModDef::new("Top");
    let a0 = top.instantiate(&a, Some("a0"), None);
    let b0 = top.instantiate(&b, Some("b0"), None);

    a0.get_port("x").connect(&b0.get_port("y"));

    assert_eq!(
        top.emit(EmitOptions::default()),
        "\
module Top;
  wire [2:0] a0_x;
  A a0 (
    .x(a0_x)
  );
  B b0 (
    .y(a0_x)
  );
endmodule
"
    );
}

#[test]
fn test_unions_complex() {
    let unions = "
      package my_pack;
        typedef struct packed {
          logic [1:0] a; // width: 2
          logic [1:0] b; // width: 2
        } my_struct_t; // width: 4
        typedef union packed {
          my_struct_t [1:0] c; // width: 8
          logic [7:0] d; // width: 8
        } my_union_t; // width: 8
      endpackage
      ";
    let module_a_verilog = "
      module A (
        output my_pack::my_union_t [1:0] x // width: 16
      );
      endmodule";
    let module_b_verilog = "
      module B (
        input my_pack::my_union_t [1:0] y // width: 16
      );
      endmodule";
    let a = strict_module_from_text("A", format!("{unions}\n{module_a_verilog}"));
    let b = strict_module_from_text("B", format!("{unions}\n{module_b_verilog}"));

    let top = ModDef::new("Top");
    let a0 = top.instantiate(&a, Some("a0"), None);
    let b0 = top.instantiate(&b, Some("b0"), None);

    a0.get_port("x").connect(&b0.get_port("y"));

    let emitted = top.emit(EmitOptions::default());
    assert_eq!(
        emitted,
        "\
module Top;
  wire [15:0] a0_x;
  A a0 (
    .x(a0_x)
  );
  B b0 (
    .y(a0_x)
  );
endmodule
"
    );

    VerilogLibrary::from_config(&ParserConfig {
        sources: &[Source::text(format!(
            "{unions}\n{module_a_verilog}\n{module_b_verilog}\n{emitted}"
        ))],
        tops: &["Top"],
        ignore_unknown_modules: false,
        ..Default::default()
    })
    .unwrap()
    .module("Top")
    .unwrap()
    .unwrap();
}

#[test]
fn test_unpacked_union_port_is_skipped() {
    let source = "typedef union {
            logic [1:0] a;
            logic [2:0] b;
         } unpacked_union_t;
         module MixedPorts (
            input unpacked_union_t unsupported,
            input logic [3:0] supported
         );
         endmodule";
    let module = module_from_config(
        "MixedPorts",
        &ParserConfig {
            sources: &[Source::text(source)],
            ignore_unknown_modules: false,
            skip_unsupported: true,
            ..Default::default()
        },
    );

    assert!(!module.has_port("unsupported"));
    assert_eq!(module.get_port("supported").io().width(), 4);
}

#[test]
fn test_non_ansi_ports_import_in_header_order() {
    let source = "module Legacy(data_out, clk, data_in, pad);
  parameter WIDTH = 8;
  input clk;
  inout [1:0] pad;
  input [WIDTH-1:0] data_in;
  output [WIDTH-1:0] data_out;
endmodule
";

    for skip_unsupported in [false, true] {
        let module = module_from_config(
            "Legacy",
            &ParserConfig {
                sources: &[Source::text(source)],
                ignore_unknown_modules: false,
                skip_unsupported,
                ..Default::default()
            },
        );
        assert_eq!(
            module
                .get_ports(None)
                .iter()
                .map(|port| port.name().to_owned())
                .collect::<Vec<_>>(),
            ["data_out", "clk", "data_in", "pad"]
        );
        assert!(matches!(module.get_port("data_out").io(), IO::Output(8)));
        assert!(matches!(module.get_port("clk").io(), IO::Input(1)));
        assert!(matches!(module.get_port("data_in").io(), IO::Input(8)));
        assert!(matches!(module.get_port("pad").io(), IO::InOut(2)));

        let wide = module.parameterize(&[("WIDTH", 16)]);
        assert_eq!(
            wide.wrap(None, None).emit(EmitOptions::default()),
            "module Legacy_wrapper(
  output wire [15:0] data_out,
  input wire clk,
  input wire [15:0] data_in,
  inout wire [1:0] pad
);
  Legacy #(
    .WIDTH(32'h0000_0010)
  ) Legacy_i (
    .data_out(data_out),
    .clk(clk),
    .data_in(data_in),
    .pad(pad)
  );
endmodule
"
        );
    }
}

#[test]
#[should_panic(expected = "Unnamed Verilog ports are not supported")]
fn test_unnamed_ports_are_rejected() {
    strict_module_from_text(
        "Slices",
        "module Slices(bus[3:2], bus[0], clk); input [3:0] bus; input clk; endmodule",
    );
}

#[test]
fn test_unnamed_ports_are_skipped() {
    let module = module_from_config(
        "Slices",
        &ParserConfig {
            sources: &[Source::text(
                "module Slices(bus[3:2], bus[0], clk); input [3:0] bus; input clk; endmodule",
            )],
            ignore_unknown_modules: false,
            skip_unsupported: true,
            ..Default::default()
        },
    );
    assert!(!module.has_port(""));
    assert!(module.has_port("clk"));
    assert_eq!(module.get_ports(None).len(), 1);
}

#[test]
fn test_internal_unpacked_types() {
    let module = strict_module_from_text(
        "UnpackedInternals",
        "module UnpackedInternals (input logic [3:0] data);
            union {
                logic [1:0] a;
                logic [2:0] b;
            } value;
            logic [7:0] memory [0:15];
         endmodule",
    );

    assert_eq!(module.get_port("data").io().width(), 4);
}

#[test]
fn test_multiple_modules_1() {
    let source = "\
module A;
endmodule
module B;
A A_i();
C C_i();
endmodule
module C;
endmodule
      ";

    let cfg = ParserConfig {
        sources: &[Source::text(source)],
        ..Default::default()
    };
    let results = VerilogLibrary::from_config(&cfg)
        .unwrap()
        .modules()
        .unwrap();

    let module_names: Vec<String> = results.iter().map(|mod_def| mod_def.get_name()).collect();
    let mut sorted_module_names = module_names.clone();
    sorted_module_names.sort();
    assert_eq!(sorted_module_names, vec!["B"]);
}

#[test]
fn test_multiple_modules_2() {
    let source = "\
module A;
endmodule
module B;
endmodule
module C;
endmodule
  ";

    let cfg = ParserConfig {
        sources: &[Source::text(source)],
        ..Default::default()
    };
    let results = VerilogLibrary::from_config(&cfg)
        .unwrap()
        .modules()
        .unwrap();

    let module_names: Vec<String> = results.iter().map(|mod_def| mod_def.get_name()).collect();
    let mut sorted_module_names = module_names.clone();
    sorted_module_names.sort();
    assert_eq!(sorted_module_names, vec!["A", "B", "C"]);
}

#[test]
fn test_negative_indices() {
    let verilog = "\
        module foo (
          output [-4:-2] a
        );
        endmodule";

    let foo = module_from_text("foo", verilog);
    let bar = foo.stub("bar");
    bar.get_port("a").tieoff(0);

    assert_eq!(
        bar.emit(EmitOptions::default()),
        "\
module bar(
  output wire [2:0] a
);
  assign a = 3'h0;
endmodule
"
    );
}

#[test]
fn test_negative_indices_parameterized() {
    let verilog = "\
        module foo #(
            parameter N=1
        ) (
            input [N-1:0] a
        );
        endmodule";

    let foo = module_from_text("foo", verilog);

    let parameterized = foo.parameterize(&[("N", 0)]).wrap(None, None);

    assert_eq!(
        parameterized.emit(EmitOptions::default()),
        "\
module foo_wrapper(
  input wire [1:0] a
);
  foo #(
    .N(32'h0000_0000)
  ) foo_i (
    .a(a)
  );
endmodule
"
    );
}

#[test]
fn test_multiple_file_import() {
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("width_pkg.sv");
    let module = dir.path().join("module.sv");
    std::fs::write(
        &package,
        "package width_pkg; parameter WIDTH = 12; endpackage",
    )
    .unwrap();
    std::fs::write(
        &module,
        "module FileModule (input [width_pkg::WIDTH-1:0] data); endmodule",
    )
    .unwrap();

    let imported = VerilogLibrary::from_config(&ParserConfig {
        sources: &[
            Source::file(package.to_str().unwrap()),
            Source::file(module.to_str().unwrap()),
        ],
        ..Default::default()
    })
    .unwrap()
    .module("FileModule")
    .unwrap()
    .unwrap();
    assert_eq!(imported.get_port("data").io().width(), 12);
}
