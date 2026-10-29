// SPDX-License-Identifier: Apache-2.0

//! Run with `cargo test --test verilog_library_performance -- --nocapture`.

use std::fmt::Write;
use std::num::NonZeroU32;
use std::time::Instant;
use topstitch::{ModDef, Package, ParserConfig, Source, VerilogLibrary};

#[test]
fn import_performance() {
    // Make parsing dominate the first comparison: module internals take work
    // to compile but do not appear in the imported ports or packages.
    let mut sv = String::from(
        "package paper; localparam int WIDTH = 8; endpackage
         module canvas(input logic [paper::WIDTH-1:0] color);\n",
    );
    for i in 0..2048 {
        writeln!(sv, "wire [7:0] swatch_{i} = color ^ 8'd{};", i % 256).unwrap();
    }
    sv.push_str("endmodule");
    let sources = [Source::text(sv)];
    let cfg = ParserConfig {
        sources: &sources,
        num_threads: NonZeroU32::new(1),
        ..Default::default()
    };
    compare(
        "shared compilation vs separate imports",
        1.65,
        || {
            let lib = VerilogLibrary::from_config(&cfg).unwrap();
            widths(&lib.modules().unwrap(), &lib.packages().unwrap()["paper"])
        },
        || {
            let modules = VerilogLibrary::from_config(&cfg)
                .unwrap()
                .modules()
                .unwrap();
            let packages = VerilogLibrary::from_config(&cfg)
                .unwrap()
                .packages()
                .unwrap();
            widths(&modules, &packages["paper"])
        },
    );

    // Make snapshotting dominate the second comparison: the extra packages
    // are short to declare, but each contains a table of constant values.
    let mut sv = String::from(
        "package paper; localparam int WIDTH = 8; endpackage
         module canvas(input logic [paper::WIDTH-1:0] color); endmodule\n",
    );
    for i in 0..32 {
        writeln!(
            sv,
            "package palette_{i}; localparam int shades[1024] = '{{default:{i}}}; endpackage"
        )
        .unwrap();
    }
    let sources = [Source::text(sv)];
    let cfg = ParserConfig {
        sources: &sources,
        ..cfg
    };
    compare(
        "discover and read one package vs extract all",
        3.0,
        || {
            let lib = VerilogLibrary::from_config(&cfg).unwrap();
            let names = lib.package_names().unwrap();
            assert_eq!(names.len(), 33);
            let name = names.iter().find(|n| *n == "paper").unwrap();
            widths(
                &lib.modules().unwrap(),
                &lib.package(name).unwrap().unwrap(),
            )
        },
        || {
            let lib = VerilogLibrary::from_config(&cfg).unwrap();
            widths(&lib.modules().unwrap(), &lib.packages().unwrap()["paper"])
        },
    );
}

/// Read the same port and package constant through either import path.
fn widths(modules: &[ModDef], paper: &Package) -> (usize, usize) {
    (
        modules[0].get_port("color").io().width(),
        usize::try_from(&paper["WIDTH"]).unwrap(),
    )
}

/// Compare complete imports on the same machine. Warm both paths, alternate
/// which runs first, and take the median to tolerate scheduling noise.
fn compare(
    label: &str,
    min_speedup: f64,
    fast: impl Fn() -> (usize, usize),
    reference: impl Fn() -> (usize, usize),
) {
    let time = |import: &dyn Fn() -> (usize, usize)| {
        let start = Instant::now();
        let widths = import();
        let elapsed = start.elapsed();
        assert_eq!(widths, (8, 8));
        elapsed
    };
    time(&fast);
    time(&reference);
    let mut samples = Vec::new();
    for i in 0..7 {
        let (a, b) = if i % 2 == 0 {
            (time(&fast), time(&reference))
        } else {
            let b = time(&reference);
            (time(&fast), b)
        };
        samples.push((b.as_secs_f64() / a.as_secs_f64(), a, b));
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (speedup, fast, reference) = samples[samples.len() / 2];
    eprintln!("{label}: {fast:?} vs {reference:?} ({speedup:.2}x)");
    assert!(
        speedup >= min_speedup,
        "{label}: expected at least {min_speedup}x, got {speedup:.2}x; samples: {samples:?}"
    );
}
