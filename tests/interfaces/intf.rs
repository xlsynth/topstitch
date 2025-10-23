// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use topstitch::*;
#[test]
fn test_interfaces() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid,
        input a_ready
    );
    endmodule
    ";

    let module_b_verilog = "
    module ModuleB (
        input [31:0] b_data,
        input b_valid,
        output b_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    module_b.def_intf_from_prefix("b_intf", "b_");

    let top_module = ModDef::new("TopModule");

    let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);
    let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

    let a_intf = a_inst.get_intf("a_intf");
    let b_intf = b_inst.get_intf("b_intf");

    assert_eq!(a_intf.width(), 34);
    assert_eq!(b_intf.width(), 34);

    a_intf.connect(&b_intf, false);

    assert_eq!(
        top_module.emit(true),
        "\
module TopModule;
  wire [31:0] inst_a_a_data;
  wire inst_a_a_valid;
  wire inst_b_b_ready;
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid(inst_a_a_valid),
    .a_ready(inst_b_b_ready)
  );
  ModuleB inst_b (
    .b_data(inst_a_a_data),
    .b_valid(inst_a_a_valid),
    .b_ready(inst_b_b_ready)
  );
endmodule
"
    );
}

#[test]
fn test_interface_connection_moddef_to_modinst() {
    let module_b_verilog = "
        module ModuleB (
            output [31:0] b_data,
            output b_valid,
            input b_ready
        );
        endmodule
        ";

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    module_b.def_intf_from_prefix("b", "b_");

    let module_a = ModDef::new("ModuleA");
    module_a.add_port("a_data", IO::Output(32));
    module_a.add_port("a_valid", IO::Output(1));
    module_a.add_port("a_ready", IO::Input(1));
    module_a.def_intf_from_prefix("a", "a_");

    let b_inst = module_a.instantiate(&module_b, Some("inst_b"), None);

    let mod_a_intf = module_a.get_intf("a");
    let b_intf = b_inst.get_intf("b");
    mod_a_intf.connect(&b_intf, false);

    assert_eq!(
        module_a.emit(true),
        "\
module ModuleA(
  output wire [31:0] a_data,
  output wire a_valid,
  input wire a_ready
);
  ModuleB inst_b (
    .b_data(a_data),
    .b_valid(a_valid),
    .b_ready(a_ready)
  );
endmodule
"
    );
}

#[test]
fn test_interface_connection_within_moddef() {
    let module = ModDef::new("MyModule");

    module.add_port("a_data", IO::Input(32));
    module.add_port("a_valid", IO::Input(1));
    module.add_port("a_ready", IO::Output(1));

    module.add_port("b_data", IO::Output(32));
    module.add_port("b_valid", IO::Output(1));
    module.add_port("b_ready", IO::Input(1));

    module.def_intf_from_prefix("a_intf", "a_");
    module.def_intf_from_prefix("b_intf", "b_");

    let a_intf = module.get_intf("a_intf");
    let b_intf = module.get_intf("b_intf");

    a_intf.connect(&b_intf, false);

    assert_eq!(
        module.emit(true),
        "\
module MyModule(
  input wire [31:0] a_data,
  input wire a_valid,
  output wire a_ready,
  output wire [31:0] b_data,
  output wire b_valid,
  input wire b_ready
);
  assign a_ready = b_ready;
  assign b_data = a_data;
  assign b_valid = a_valid;
endmodule
"
    );
}

#[test]
fn test_export_interface_with_prefix() {
    // Define ModuleA
    let module_a_verilog = "
        module ModuleA (
            output [31:0] a_data,
            output a_valid,
            input a_ready
        );
        endmodule
        ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_prefix("a", "a_");

    let module_b = ModDef::new("ModuleB");
    module_b
        .instantiate(&module_a, None, None)
        .get_intf("a")
        .export_with_prefix("b", "b_");

    let module_c = ModDef::new("ModuleC");
    module_c
        .instantiate(&module_b, None, None)
        .get_intf("b")
        .export_with_name_underscore("c");

    assert_eq!(
        module_c.emit(true),
        "\
module ModuleB(
  output wire [31:0] b_data,
  output wire b_valid,
  input wire b_ready
);
  ModuleA ModuleA_i (
    .a_data(b_data),
    .a_valid(b_valid),
    .a_ready(b_ready)
  );
endmodule
module ModuleC(
  output wire [31:0] c_data,
  output wire c_valid,
  input wire c_ready
);
  ModuleB ModuleB_i (
    .b_data(c_data),
    .b_valid(c_valid),
    .b_ready(c_ready)
  );
endmodule
"
    );
}

#[test]
fn test_intf_slice() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output [3:0] a_valid,
        input [1:0] a_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);

    let mut mapping_lower: IndexMap<String, (String, usize, usize)> = IndexMap::new();
    mapping_lower.insert("data".to_string(), ("a_data".to_string(), 15, 0));
    mapping_lower.insert("valid".to_string(), ("a_valid".to_string(), 1, 0));
    mapping_lower.insert("ready".to_string(), ("a_ready".to_string(), 0, 0));
    module_a.def_intf("lower", mapping_lower);

    let mut mapping_upper: IndexMap<String, (String, usize, usize)> = IndexMap::new();
    mapping_upper.insert("data".to_string(), ("a_data".to_string(), 31, 16));
    mapping_upper.insert("valid".to_string(), ("a_valid".to_string(), 3, 2));
    mapping_upper.insert("ready".to_string(), ("a_ready".to_string(), 1, 1));
    module_a.def_intf("upper", mapping_upper);

    let top_module = ModDef::new("TopModule");
    let a = top_module.instantiate(&module_a, None, None);
    a.get_intf("upper").export_with_prefix("upper", "upper_");
    a.get_intf("lower").export_with_prefix("lower", "lower_");

    assert_eq!(
        top_module.emit(true),
        "\
module TopModule(
  output wire [15:0] upper_data,
  output wire [1:0] upper_valid,
  input wire upper_ready,
  output wire [15:0] lower_data,
  output wire [1:0] lower_valid,
  input wire lower_ready
);
  ModuleA ModuleA_i (
    .a_data({upper_data, lower_data}),
    .a_valid({upper_valid, lower_valid}),
    .a_ready({upper_ready, lower_ready})
  );
endmodule
"
    );
}

#[test]
fn test_intf_subdivide() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output [3:0] a_valid,
        input [1:0] a_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    let a_intf = module_a.def_intf_from_prefix("a_intf", "a_");
    a_intf.subdivide(2);

    let top_module = ModDef::new("TopModule");
    let a = top_module.instantiate(&module_a, None, None);
    a.get_intf("a_intf_0").export_with_prefix("lower", "lower_");
    a.get_intf("a_intf_1").export_with_prefix("upper", "upper_");

    assert_eq!(
        top_module.emit(true),
        "\
module TopModule(
  output wire [15:0] lower_data,
  output wire [1:0] lower_valid,
  input wire lower_ready,
  output wire [15:0] upper_data,
  output wire [1:0] upper_valid,
  input wire upper_ready
);
  ModuleA ModuleA_i (
    .a_data({upper_data, lower_data}),
    .a_valid({upper_valid, lower_valid}),
    .a_ready({upper_ready, lower_ready})
  );
endmodule
"
    );
}

#[test]
fn test_complex_intf() {
    let module_a_verilog = "
    module ModuleA (
        output [7:0] a_data_out,
        output a_valid_out,
        input [7:0] a_data_in,
        input a_valid_in
    );
    endmodule
    ";
    let module_b_verilog = "
    module ModuleB (
        output [15:0] b_data_out,
        output [1:0] b_valid_out,
        input [15:0] b_data_in,
        input [1:0] b_valid_in
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    let b_intf = module_b.def_intf_from_prefix("b_intf", "b_");
    b_intf.subdivide(2);

    let top_module = ModDef::new("TopModule");
    let a0 = top_module.instantiate(&module_a, Some("a0"), None);
    let a1 = top_module.instantiate(&module_a, Some("a1"), None);
    let b = top_module.instantiate(&module_b, None, None);
    b.get_intf("b_intf_0")
        .crossover(&a0.get_intf("a_intf"), "(.*)_in", "(.*)_out");
    b.get_intf("b_intf_1")
        .crossover(&a1.get_intf("a_intf"), "(.*)_in", "(.*)_out");

    assert_eq!(
        top_module.emit(true),
        "\
module TopModule;
  wire [7:0] a0_a_data_out;
  wire a0_a_valid_out;
  wire [15:0] ModuleB_i_b_data_out;
  wire [1:0] ModuleB_i_b_valid_out;
  ModuleA a0 (
    .a_data_out(a0_a_data_out),
    .a_valid_out(a0_a_valid_out),
    .a_data_in(ModuleB_i_b_data_out[7:0]),
    .a_valid_in(ModuleB_i_b_valid_out[0])
  );
  wire [7:0] a1_a_data_out;
  wire a1_a_valid_out;
  ModuleA a1 (
    .a_data_out(a1_a_data_out),
    .a_valid_out(a1_a_valid_out),
    .a_data_in(ModuleB_i_b_data_out[15:8]),
    .a_valid_in(ModuleB_i_b_valid_out[1])
  );
  ModuleB ModuleB_i (
    .b_data_out(ModuleB_i_b_data_out),
    .b_valid_out(ModuleB_i_b_valid_out),
    .b_data_in({a1_a_data_out, a0_a_data_out}),
    .b_valid_in({a1_a_valid_out, a0_a_valid_out})
  );
endmodule
"
    );
}

#[test]
fn test_intf_regex() {
    let module_a_verilog = "
      module ModuleA (
          input [7:0] a_data_in,
          input a_valid_in,
          output [7:0] a_data_out,
          output a_valid_out,
          input [7:0] b_data_in,
          input b_valid_in,
          output [7:0] b_data_out,
          output b_valid_out
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_regexes("left", &[("a_(.*)_in", "a_$1"), ("b_(.*)_out", "b_$1")]);
    module_a.def_intf_from_regexes("right", &[("a_(.*)_out", "a_$1"), ("b_(.*)_in", "b_$1")]);

    let top_module = ModDef::new("TopModule");
    let left = top_module.instantiate(&module_a, Some("left"), None);
    let right = top_module.instantiate(&module_a, Some("right"), None);

    left.get_intf("left").unused_and_tieoff(0);
    left.get_intf("right")
        .connect(&right.get_intf("left"), false);
    right.get_intf("right").unused_and_tieoff(0);

    assert_eq!(
        top_module.emit(true),
        "\
module TopModule;
  wire [7:0] left_a_data_out;
  wire left_a_valid_out;
  wire [7:0] right_b_data_out;
  wire right_b_valid_out;
  ModuleA left (
    .a_data_in(8'h00),
    .a_valid_in(1'h0),
    .a_data_out(left_a_data_out),
    .a_valid_out(left_a_valid_out),
    .b_data_in(right_b_data_out),
    .b_valid_in(right_b_valid_out),
    .b_data_out(),
    .b_valid_out()
  );
  ModuleA right (
    .a_data_in(left_a_data_out),
    .a_valid_in(left_a_valid_out),
    .a_data_out(),
    .a_valid_out(),
    .b_data_in(8'h00),
    .b_valid_in(1'h0),
    .b_data_out(right_b_data_out),
    .b_valid_out(right_b_valid_out)
  );
endmodule
"
    );
}

#[test]
#[should_panic(expected = "Empty interface definition for A.b")]
fn test_empty_prefix_interface() {
    let a_verilog = "\
        module A(
          output a_data,
          output a_valid
        );
        endmodule";

    let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
    a_mod_def.def_intf_from_name_underscore("b");
}

#[test]
#[should_panic(expected = "Empty interface definition for A.b")]
fn test_empty_regex_interface() {
    let a_verilog = "\
        module A(
          output a_data,
          output a_valid
        );
        endmodule";

    let a_mod_def = ModDef::from_verilog("A", a_verilog, true, false);
    a_mod_def.def_intf_from_regex("b", "^b_(.*)$", "${1}");
}

#[test]
fn test_has_intf() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid,
        input a_ready
    );
    endmodule
    ";

    let module_b_verilog = "
    module ModuleB (
        input [31:0] b_data,
        input b_valid,
        output b_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_prefix("a_intf", "a_");

    assert!(module_a.has_intf("a_intf"));
    assert!(!module_a.has_intf("b_intf"));

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    module_b.def_intf_from_prefix("b_intf", "b_");

    let top_module = ModDef::new("TopModule");

    let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

    assert!(b_inst.has_intf("b_intf"));
    assert!(!b_inst.has_intf("a_intf"));
}
