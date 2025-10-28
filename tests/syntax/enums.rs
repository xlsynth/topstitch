// SPDX-License-Identifier: Apache-2.0

use slang_rs::str2tmpfile;
use topstitch::*;

#[test]
fn test_enum_type_remap() {
    let input_verilog = "
        package color_pkg;
            typedef enum bit[1:0] {RED, GREEN, BLUE} rgb_t;
        endpackage
        module ModA import color_pkg::*; (
            input rgb_t portA,
            output rgb_t portB,
            input rgb_t [3:0] portC,
            output rgb_t [3:0] portD
        );
        endmodule
        ";

    let mod_a = ModDef::from_verilog("ModA", input_verilog, true, false);
    let wrapped = mod_a.wrap(None, None);

    assert_eq!(
        wrapped.emit(true),
        "\
module ModA_wrapper(
  input wire [1:0] portA,
  output wire [1:0] portB,
  input wire [7:0] portC,
  output wire [7:0] portD
);
  ModA ModA_i (
    .portA(color_pkg::rgb_t'(portA)),
    .portB(portB),
    .portC(portC),
    .portD(portD)
  );
endmodule
"
    );
}

#[test]
fn test_enum_type_remap_parameterized() {
    let input_verilog = str2tmpfile(
        "
        package color_pkg;
            typedef enum bit[1:0] {RED, GREEN, BLUE} rgb_t;
        endpackage
        module ModA import color_pkg::*; #(
            parameter MY_PARAM = 8
        ) (
            input rgb_t portA,
            output rgb_t portB,
            input rgb_t [3:0] portC,
            output rgb_t [3:0] portD,
            input [MY_PARAM-1:0] portE
        );
        endmodule
        ",
    )
    .unwrap();

    let mod_a = ModDef::from_verilog_file("ModA", input_verilog.path(), true, false);
    let mod_a_parameterized = mod_a.parameterize(&[("MY_PARAM", 16)], None, None);
    let wrapped = mod_a_parameterized.wrap(None, None);

    assert_eq!(
        wrapped.emit(true),
        "\
module ModA_MY_PARAM_16(
  input wire [1:0] portA,
  output wire [1:0] portB,
  input wire [7:0] portC,
  output wire [7:0] portD,
  input wire [15:0] portE
);
  ModA #(
    .MY_PARAM(32'h0000_0010)
  ) ModA_i (
    .portA(color_pkg::rgb_t'(portA)),
    .portB(portB),
    .portC(portC),
    .portD(portD),
    .portE(portE)
  );
endmodule

module ModA_MY_PARAM_16_wrapper(
  input wire [1:0] portA,
  output wire [1:0] portB,
  input wire [7:0] portC,
  output wire [7:0] portD,
  input wire [15:0] portE
);
  ModA_MY_PARAM_16 ModA_MY_PARAM_16_i (
    .portA(portA),
    .portB(portB),
    .portC(portC),
    .portD(portD),
    .portE(portE)
  );
endmodule
"
    );
}

#[test]
fn test_enum_relaxed() {
    let input_verilog = str2tmpfile(
        "
        typedef enum logic [1:0] {
            A=0,
            B=1,
            C=2
        } enum_t;

        module foo (
            output enum_t a,
            input logic [1:0] b
        );
            assign a = b;
        endmodule
        ",
    )
    .unwrap();

    let config = ParserConfig {
        sources: &[input_verilog.path().to_str().unwrap()],
        extra_arguments: &["--relax-enum-conversions"],
        ..Default::default()
    };

    let foo = ModDef::from_verilog_with_config("foo", &config);

    assert_eq!(foo.get_ports(None).len(), 2);
    assert_eq!(foo.get_port("a").io().width(), 2);
    assert_eq!(foo.get_port("b").io().width(), 2);
}
