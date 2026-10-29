// SPDX-License-Identifier: Apache-2.0

use topstitch::{IO, ModDef, ParserConfig, Source, VerilogLibrary};

#[test]
fn imports_ports_and_the_package_that_defines_their_width() {
    let sources = [Source::text(
        "package sizes; localparam int W = 7; endpackage
         module sink(input logic [sizes::W-1:0] data); endmodule",
    )];
    let lib = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(lib.package_names().unwrap(), ["sizes"]);
    assert_eq!(lib.module_names().unwrap(), ["sink"]);
    let width = usize::try_from(&lib.package("sizes").unwrap().unwrap()["W"]).unwrap();
    let module = lib.module("sink").unwrap().unwrap();
    assert_eq!(width, 7);
    assert_eq!(lib.modules().unwrap().len(), 1);
    assert!(matches!(module.get_port("data").io(), IO::Input(w) if w == width));
}

#[test]
fn imports_a_package_without_modules() {
    let sources = [Source::text(
        "package setting; localparam int N = 2; endpackage",
    )];
    let lib = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        ..Default::default()
    })
    .unwrap();
    assert!(lib.modules().unwrap().is_empty());
    assert!(lib.package("absent").unwrap().is_none());
    assert!(lib.module("absent").unwrap().is_none());
    assert_eq!(
        i32::try_from(&lib.packages().unwrap()["setting"]["N"]).unwrap(),
        2
    );
}

#[test]
fn imports_the_named_module_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("modules.sv");
    std::fs::write(
        &path,
        "module leaf(input logic [3:0] data); endmodule
        module wrapper(input logic [3:0] value); leaf inst(.data(value)); endmodule",
    )
    .unwrap();

    let leaf = ModDef::from_verilog_file("leaf", &path).unwrap();
    assert_eq!(leaf.get_port("data").io().width(), 4);

    let error = ModDef::from_verilog_file("absent", &path)
        .err()
        .expect("missing module must fail");
    assert!(error.to_string().contains("absent"));
    assert!(error.to_string().contains(path.to_str().unwrap()));
}
