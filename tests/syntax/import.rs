// SPDX-License-Identifier: Apache-2.0

use slang_rs::str2tmpfile;
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
    let a = ModDef::from_verilog("A", format!("{structs}\n{module_a_verilog}"), false, false);
    let b = ModDef::from_verilog("B", format!("{structs}\n{module_b_verilog}"), false, false);

    let top = ModDef::new("Top");
    let a0 = top.instantiate(&a, Some("a0"), None);
    let a1 = top.instantiate(&a, Some("a1"), None);
    let b0 = top.instantiate(&b, Some("b0"), None);

    let b0_inputs = b0.get_port("y").subdivide(2);

    a0.get_port("x").connect(&b0_inputs[0]);
    a1.get_port("x").connect(&b0_inputs[1]);

    assert_eq!(
        top.emit(true),
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
        typedef union {
          logic [1:0] a; // width: 2
          logic [2:0] b; // width: 3
        } my_union_t;
      endpackage
      ";
    let module_a_verilog = "
      module A (
        output my_pack::my_union_t x // width: 3
      );
      endmodule";
    let module_b_verilog = "
      module B (
        input my_pack::my_union_t y // width: 3
      );
      endmodule";
    let a = ModDef::from_verilog("A", format!("{unions}\n{module_a_verilog}"), false, false);
    let b = ModDef::from_verilog("B", format!("{unions}\n{module_b_verilog}"), false, false);

    let top = ModDef::new("Top");
    let a0 = top.instantiate(&a, Some("a0"), None);
    let b0 = top.instantiate(&b, Some("b0"), None);

    a0.get_port("x").connect(&b0.get_port("y"));

    assert_eq!(
        top.emit(true),
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
    let a = ModDef::from_verilog("A", format!("{unions}\n{module_a_verilog}"), false, false);
    let b = ModDef::from_verilog("B", format!("{unions}\n{module_b_verilog}"), false, false);

    let top = ModDef::new("Top");
    let a0 = top.instantiate(&a, Some("a0"), None);
    let b0 = top.instantiate(&b, Some("b0"), None);

    a0.get_port("x").connect(&b0.get_port("y"));

    assert_eq!(
        top.emit(true),
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
}

#[test]
fn test_multiple_modules_1() {
    let source = str2tmpfile(
        "\
module A;
endmodule
module B;
A A_i();
C C_i();
endmodule
module C;
endmodule
      ",
    )
    .unwrap();

    let cfg = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        ..Default::default()
    };
    let results = ModDef::all_from_verilog_with_config(&cfg);

    let module_names: Vec<String> = results.iter().map(|mod_def| mod_def.get_name()).collect();
    let mut sorted_module_names = module_names.clone();
    sorted_module_names.sort();
    assert_eq!(sorted_module_names, vec!["B"]);
}

#[test]
fn test_multiple_modules_2() {
    let source = str2tmpfile(
        "\
module A;
endmodule
module B;
endmodule
module C;
endmodule
  ",
    )
    .unwrap();

    let cfg = ParserConfig {
        sources: &[source.path().to_str().unwrap()],
        ..Default::default()
    };
    let results = ModDef::all_from_verilog_with_config(&cfg);

    let module_names: Vec<String> = results.iter().map(|mod_def| mod_def.get_name()).collect();
    let mut sorted_module_names = module_names.clone();
    sorted_module_names.sort();
    assert_eq!(sorted_module_names, vec!["A", "B", "C"]);
}

#[test]
fn test_negative_indices() {
    let verilog = str2tmpfile(
        "\
        module foo (
          output [-4:-2] a
        );
        endmodule",
    )
    .unwrap();

    let foo = ModDef::from_verilog_file("foo", verilog.path(), true, false);
    let bar = foo.stub("bar");
    bar.get_port("a").tieoff(0);

    assert_eq!(
        bar.emit(true),
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
    let verilog = str2tmpfile(
        "\
        module foo #(
            parameter N=1
        ) (
            input [N-1:0] a
        );
        endmodule",
    )
    .unwrap();

    let foo = ModDef::from_verilog_file("foo", verilog.path(), true, false);

    let parameterized = foo.parameterize(&[("N", 0)]).wrap(None, None);

    assert_eq!(
        parameterized.emit(true),
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
