// SPDX-License-Identifier: Apache-2.0

use topstitch::{
    CheckLevel, HierarchyMode, IO, ModDef, PackageLookup, ParserConfig, Source, VerilogLibrary,
};

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
    let width = usize::try_from(
        lib.package("sizes")
            .unwrap()
            .unwrap()
            .parameter("W")
            .unwrap()
            .unwrap()
            .as_ref(),
    )
    .unwrap();
    let module = lib.module("sink").unwrap().unwrap();
    assert_eq!(width, 7);
    assert_eq!(lib.modules().unwrap().len(), 1);
    assert!(matches!(module.get_port("data").io(), IO::Input(w) if w == width));
}

#[test]
fn library_inputs_expose_packages_without_inferring_module_roots() {
    let sources = [Source::text(
        "package sizes; localparam int W = 5; endpackage
         module sink(input logic [sizes::W-1:0] data); endmodule",
    )];
    let config = ParserConfig {
        sources: &sources,
        hierarchy_mode: HierarchyMode::ExplicitTopsOnly,
        ..Default::default()
    };
    let library = VerilogLibrary::from_config(&config).unwrap();
    assert_eq!(library.package_names().unwrap(), ["sizes"]);
    assert!(library.module_names().unwrap().is_empty());

    let selected = VerilogLibrary::from_config(&ParserConfig {
        tops: &["sink"],
        ..config
    })
    .unwrap();
    let sink = selected.module("sink").unwrap().unwrap();
    assert_eq!(sink.get_port("data").io().width(), 5);
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

#[test]
fn default_import_checks_constant_indices_but_skips_driver_analysis() {
    let sources = [Source::text(
        "module sink #(parameter int IDX = 0) (input logic [1:0] data, output logic q);
            localparam logic [1:0] MASK = 2'b01;
            always_comb q = data[0];
            always_comb q = MASK[IDX];
         endmodule",
    )];
    let config = ParserConfig {
        sources: &sources,
        tops: &["sink"],
        ..Default::default()
    };
    let library = VerilogLibrary::from_config(&config).unwrap();
    let sink = library.module("sink").unwrap().unwrap();
    assert_eq!(sink.get_port("data").io().width(), 2);

    let invalid_index = VerilogLibrary::from_config(&ParserConfig {
        parameters: &[("IDX", "7")],
        ..config
    })
    .err()
    .expect("an out-of-bounds constant index must fail import");
    assert!(
        invalid_index
            .diagnostics
            .iter()
            .any(|d| d.code == "IndexOOB")
    );

    let conflicting_drivers = VerilogLibrary::from_config(&ParserConfig {
        check_level: CheckLevel::Analysis,
        ..config
    })
    .err()
    .expect("analysis must reject conflicting always_comb drivers");
    assert!(
        conflicting_drivers
            .diagnostics
            .iter()
            .any(|d| d.code == "MultipleAlwaysAssigns")
    );
}

#[test]
fn parameterized_import_preserves_semantic_checking() {
    let sources = [Source::text(
        "module configurable #(parameter bit ENABLE = 0) (input logic data);
            if (ENABLE) begin localparam int WIDTH = MISSING; end
         endmodule",
    )];
    for check_level in [CheckLevel::Parse, CheckLevel::Semantic] {
        let library = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            check_level,
            ..Default::default()
        })
        .unwrap();
        let module = library.module("configurable").unwrap().unwrap();
        let variant = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            module.parameterize(&[("ENABLE", 1)])
        }));
        assert_eq!(variant.is_err(), check_level == CheckLevel::Semantic);
    }
}
