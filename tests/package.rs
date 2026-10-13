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

    let a: &ConstantValue = pkgs["pkg_a"].get("a").unwrap();
    assert_eq!(i32::try_from(a).unwrap(), 22);
    assert_eq!(a, &pkgs["pkg_a"].parameters["a"]);
    assert_eq!(a, &pkgs["pkg_a"]["a"]);
    assert!(pkgs["pkg_b"].get("missing").is_none());
    assert!(pkgs["pkg_b"].get("my_t").is_none());
    let b: i32 = (&pkgs["pkg_b"]["b"]).try_into().unwrap();
    assert_eq!(b, 123);
    assert_eq!(i32::try_from(&pkgs["pkg_b"]["c"]).unwrap(), 145);
    assert_eq!(
        pkgs["pkg_b"].types["my_t"]
            .as_ref()
            .unwrap()
            .width()
            .unwrap(),
        12
    );
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

    for (name, width) in [("my_struct_t", 6), ("my_enum_t", 2), ("my_array_t", 24)] {
        assert_eq!(
            pkgs["my_pkg"].types[name]
                .as_ref()
                .unwrap()
                .width()
                .unwrap(),
            width
        );
    }
}
