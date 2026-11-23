// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "block-ir")]

use std::fmt::Write;
use std::time::{Duration, Instant};
use topstitch::{BlockIrLibrary, BlockIrParserConfig, IO};

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_import_shared_block_hierarchy() {
    const DEPTH: usize = 15;
    const BUDGET: Duration = Duration::from_millis(30);
    // Sixteen distinct definitions describe 65,535 instance occurrences including the root.
    let ir = hierarchy(DEPTH, 2);
    let root_name = format!("Level{DEPTH}");

    let start = Instant::now();
    let library = BlockIrLibrary::from_config(&BlockIrParserConfig {
        ir: &ir,
        include_hierarchy: true,
    })
    .unwrap();
    let mut module = library.module(&root_name).unwrap();
    let elapsed = start.elapsed();

    assert!(matches!(module.get_port("clk").io(), IO::Input(1)));
    for _ in 0..DEPTH {
        assert_eq!(module.get_instances().len(), 2);
        module = module.get_instance("child0").get_mod_def();
    }
    assert_eq!(module.get_name(), "Level0");
    eprintln!("Parse and import one shared hierarchy: {elapsed:?} (budget {BUDGET:?})");
    assert!(
        elapsed < BUDGET,
        "Single import took {elapsed:?} (budget {BUDGET:?})"
    );
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_bulk_import_block_hierarchy() {
    const DEPTH: usize = 255;
    const BUDGET: Duration = Duration::from_millis(50);
    let ir = hierarchy(DEPTH, 1);
    let library = BlockIrLibrary::from_config(&BlockIrParserConfig {
        ir: &ir,
        include_hierarchy: true,
    })
    .unwrap();

    // Each root in the chain overlaps every root below it. Time bulk materialization
    // from the parsed package so that rebuilding those descendants dominates the budget.
    let start = Instant::now();
    for _ in 0..4 {
        let modules = library.modules().unwrap();
        assert_eq!(modules.len(), DEPTH + 1);
        assert_eq!(modules[DEPTH].get_instances().len(), 1);
    }
    let elapsed = start.elapsed();
    eprintln!("Four bulk imports of 256 blocks: {elapsed:?} (budget {BUDGET:?})");
    assert!(
        elapsed < BUDGET,
        "Bulk import took {elapsed:?} (budget {BUDGET:?})"
    );
}

/// Each block instantiates the previous level `fanout` times.
fn hierarchy(depth: usize, fanout: usize) -> String {
    let mut ir = String::from("package hierarchy\nblock Level0(clk: clock) {}\n");
    for level in 1..=depth {
        writeln!(ir, "block Level{level}(clk: clock) {{").unwrap();
        for child in 0..fanout {
            writeln!(
                ir,
                "  instantiation child{child}(block=Level{}, kind=block)",
                level - 1
            )
            .unwrap();
        }
        ir.push_str("}\n");
    }
    ir
}
