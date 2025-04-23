// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_extract_packages() {
    let verilog = "
      package pkg_a;
        localparam int a=22;
      endpackage
      package pkg_b;
        localparam int b=123;
        localparam int c=b+pkg_a::a;
        typedef logic [33:22] my_t;
      endpackage
    ";

    let pkgs = extract_packages_from_verilog(verilog, false).unwrap();

    assert_eq!(pkgs["pkg_a"]["a"].parse::<i32>().unwrap(), 22);
    assert_eq!(pkgs["pkg_b"]["b"].parse::<i32>().unwrap(), 123);
    assert_eq!(pkgs["pkg_b"]["c"].parse::<i32>().unwrap(), 145);
    assert_eq!(pkgs["pkg_b"]["my_t"].calc_type_width().unwrap(), 12);
}

#[test]
fn test_extract_packages_with_typedefs() {
    let verilog = "
      package my_pkg;
        typedef struct packed {
          logic [2:0] a;
          logic [1:0] b;
          logic c;
        } my_struct_t;
        typedef enum logic [1:0] {
          Red = 0,
          Green = 1,
          Blue = 2
        } my_enum_t;
        typedef my_struct_t [3:0] my_array_t;
      endpackage
    ";

    let pkgs = extract_packages_from_verilog(verilog, false).unwrap();

    assert_eq!(pkgs["my_pkg"]["my_struct_t"].calc_type_width().unwrap(), 6);
    assert_eq!(pkgs["my_pkg"]["my_enum_t"].calc_type_width().unwrap(), 2);
    assert_eq!(pkgs["my_pkg"]["my_array_t"].calc_type_width().unwrap(), 24);
}
