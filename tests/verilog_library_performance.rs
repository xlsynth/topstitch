// SPDX-License-Identifier: Apache-2.0

//! Run with `cargo test --test verilog_library_performance -- --nocapture`.

use std::fmt::Write;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};
use topstitch::{CheckLevel, ModDef, PackageLookup, ParserConfig, Source, Usage, VerilogLibrary};

#[test]
fn module_and_package_imports_reuse_the_compilation() {
    let mut sv = String::from(
        "package paper; localparam int WIDTH = 8; endpackage
         module canvas(input logic [paper::WIDTH-1:0] color);\n",
    );
    for i in 0..4096 {
        writeln!(sv, "wire [7:0] swatch_{i} = color ^ 8'd{};", i % 256).unwrap();
    }
    sv.push_str("endmodule");

    check_import(sv, |lib| {
        // Importing ports and constants is cheap; parsing the wires on every query is not.
        for _ in 0..64 {
            let modules = lib.modules().unwrap();
            assert_eq!(modules[0].get_port("color").io().width(), 8);
            assert_eq!(width(&lib.packages().unwrap()["paper"]), 8);
        }
    });
}

#[test]
fn performance_named_module_import_skips_internal_wires() {
    let measure = |wire_count| {
        let mut sv = String::from("module Leaf(input d);\n");
        for i in 0..wire_count {
            writeln!(sv, "wire w_{i}=d;").unwrap();
        }
        sv.push_str("endmodule");
        let sources = [Source::text(sv)];
        let library = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            num_threads: NonZeroU32::new(1),
            ..Default::default()
        })
        .unwrap();
        (0..3)
            .map(|_| {
                let start = Instant::now();
                for _ in 0..100 {
                    std::hint::black_box(library.module("Leaf").unwrap());
                }
                start.elapsed()
            })
            .min()
            .unwrap()
    };
    let small = measure(0);
    let large = measure(32768);
    assert!(
        large < small * 8 + Duration::from_millis(1),
        "Named imports took {small:?} without internal wires and {large:?} with 32768"
    );
}

#[test]
fn importing_many_modules_scales_with_the_number_of_tops() {
    let small = fastest_batch_import(1024);
    let large = fastest_batch_import(8192);
    // Eight times as many modules leaves room for allocation and cache effects.
    // A linear scan per top takes roughly 64 times as long on the larger fixture.
    assert!(
        large < small * 30 + Duration::from_millis(5),
        "Batch import grew from {small:?} for 1024 tops to {large:?} for 8192 tops"
    );
}

/// Time only the batch query on an already elaborated library.
fn fastest_batch_import(count: usize) -> Duration {
    let mut sv = String::new();
    for i in 0..count {
        writeln!(sv, "module Leaf_{i}(input x); endmodule").unwrap();
    }
    let sources = [Source::text(sv)];
    let config = ParserConfig {
        sources: &sources,
        num_threads: NonZeroU32::new(1),
        ..Default::default()
    };
    let lib = VerilogLibrary::from_config(&config).unwrap();

    // Use the fastest sample to limit the effect of unrelated activity on CI.
    (0..3)
        .map(|_| {
            let start = Instant::now();
            assert_eq!(lib.modules().unwrap().len(), count);
            start.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
fn performance_parameterized_stub_emission_scales_with_definition_count() {
    for import_by_name in [false, true] {
        let small = fastest_stub_emission(32, import_by_name);
        let large = fastest_stub_emission(256, import_by_name);
        assert!(
            large < small * 20 + Duration::from_millis(3),
            "Stub emission grew from {small:?} for 32 definitions to {large:?} for 256 (import_by_name={import_by_name})"
        );
    }
}

/// Measure the first emission of a design imported from one library.
fn fastest_stub_emission(count: usize, import_by_name: bool) -> Duration {
    let mut sv = String::new();
    for i in 0..count {
        writeln!(sv, "module Leaf_{i} #(parameter W=8)(); endmodule").unwrap();
    }
    let sources = [Source::text(sv)];
    let config = ParserConfig {
        sources: &sources,
        num_threads: NonZeroU32::new(1),
        ..Default::default()
    };
    (0..3)
        .map(|_| {
            let lib = VerilogLibrary::from_config(&config).unwrap();
            let top = ModDef::new("Assembly");
            let leaves = if import_by_name {
                (0..count)
                    .map(|i| lib.module(&format!("Leaf_{i}")).unwrap())
                    .collect()
            } else {
                lib.modules().unwrap()
            };
            for (i, leaf) in leaves.into_iter().enumerate() {
                leaf.set_usage(Usage::EmitStubAndStop);
                top.instantiate(&leaf, Some(&format!("u{i}")), None);
            }
            let start = Instant::now();
            let rtl = top.emit(Default::default());
            let elapsed = start.elapsed();
            assert_eq!(rtl.matches("parameter").count(), count);
            elapsed
        })
        .min()
        .unwrap()
}

#[test]
fn package_discovery_skips_unrequested_packages() {
    // Listing names and reading paper should never copy the other packages' tables.
    let mut sv = String::from("package paper; localparam int WIDTH = 8; endpackage\n");
    for i in 0..32 {
        writeln!(
            sv,
            "package palette_{i}; localparam logic [255:0] shades[16384] = '{{default:'1}}; endpackage"
        )
        .unwrap();
    }

    check_import(sv, |lib| {
        let names = lib.package_names().unwrap();
        assert_eq!(names.len(), 33);
        let name = names.iter().find(|n| *n == "paper").unwrap();
        assert_eq!(width(&lib.package(name).unwrap()), 8);
    });
}

#[test]
fn member_lookup_skips_other_values_in_the_same_package() {
    // Looking up WIDTH should skip the tables even when they belong to paper.
    let mut sv = String::from("package paper; localparam int WIDTH = 8;\n");
    for i in 0..32 {
        writeln!(
            sv,
            "localparam logic [255:0] shades_{i}[16384] = '{{default:'1}};"
        )
        .unwrap();
    }
    sv.push_str("endpackage");

    check_import(sv, |lib| {
        assert_eq!(width(&lib.package("paper").unwrap()), 8);
    });
}

/// Read the scalar that the fixture deliberately makes cheap to import.
fn width(package: &impl PackageLookup) -> usize {
    usize::try_from(package.parameter("WIDTH").unwrap().as_ref()).unwrap()
}

/// Include compilation in the deadline so eager work cannot hide in library construction.
fn check_import(sv: String, check: impl FnOnce(&VerilogLibrary<'_>)) {
    const BUDGET: Duration = Duration::from_millis(150);
    let sources = [Source::text(sv)];
    let config = ParserConfig {
        sources: &sources,
        num_threads: NonZeroU32::new(1),
        // Check syntax up front; elaborate on demand so the test measures what the imports request.
        check_level: CheckLevel::Parse,
        ..Default::default()
    };
    let start = Instant::now();
    let lib = VerilogLibrary::from_config(&config).unwrap();
    check(&lib);
    let elapsed = start.elapsed();
    eprintln!("Import took {elapsed:?} (budget {BUDGET:?})");
    assert!(
        elapsed < BUDGET,
        "Import took {elapsed:?} (budget {BUDGET:?})"
    );
}
