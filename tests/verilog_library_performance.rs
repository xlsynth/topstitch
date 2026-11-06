// SPDX-License-Identifier: Apache-2.0

//! Run with `cargo test --test verilog_library_performance -- --nocapture`.

use std::fmt::Write;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};
use topstitch::{CheckLevel, PackageLookup, ParserConfig, Source, VerilogLibrary};

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
