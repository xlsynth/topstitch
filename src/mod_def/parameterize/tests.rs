// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Barrier};

use super::ImportContext;
use crate::test_utils::module_from_sources;
use crate::{EmitOptions, ModDef, ParameterValue, ParserConfig, Source, Usage, VerilogLibrary};

fn context(module: &ModDef) -> Arc<ImportContext> {
    module.core.read().verilog_import.as_ref().unwrap().clone()
}

// Reusing earlier parameter values must return a fresh module unaffected by changes
// to the previous result's interfaces, metadata, or emission settings. Deleting the
// source file proves this uses the cache.
#[test]
fn chained_cache_hits_create_independent_modules() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cached.sv");
    std::fs::write(
        &path,
        "module Cached #(parameter WIDTH = 8, DEPTH = 4)
         (output [WIDTH+DEPTH-1:0] data); endmodule",
    )
    .unwrap();
    let base = ModDef::from_verilog_file("Cached", &path).unwrap();
    let import = context(&base);
    let first = base.parameterize(&[("WIDTH", 16), ("DEPTH", 4)]);
    let chained = first.parameterize(&[("DEPTH", 6)]);
    assert_eq!(import.cache.read().len(), 2);
    assert!(Arc::ptr_eq(&import, &context(&chained)));
    assert_eq!(chained.get_port("data").io().width(), 22);

    first.set_usage(Usage::EmitStubAndStop);
    first.set_metadata("variant", "modified");
    first.def_intf_from_prefix("output", "");

    // A hit must work without reopening the source, and must not reuse the
    // mutable definition returned by the original parameterization.
    std::fs::remove_file(path).unwrap();
    let cached = chained.parameterize(&[("DEPTH", 4)]);
    assert_eq!(import.cache.read().len(), 2);
    assert_eq!(cached.get_usage(), Usage::EmitNothingAndStop);
    assert_eq!(cached.get_metadata("variant"), None);
    assert!(!cached.has_intf("output"));
    assert!(!Arc::ptr_eq(&first.core, &cached.core));
    assert_eq!(
        cached.wrap(None, None).emit(EmitOptions::default()),
        "\
module Cached_wrapper(
  output wire [19:0] data
);
  Cached #(
    .WIDTH(32'h0000_0010),
    .DEPTH(32'h0000_0004)
  ) Cached_i (
    .data(data)
  );
endmodule
"
    );
}

// Modules from one import share a cache, but each module gets its own entries.
// Importing the same source again starts a separate, empty cache.
#[test]
fn shared_import_cache_distinguishes_modules() {
    let sources = [Source::text(
        "module Single #(parameter WIDTH = 8) (output [WIDTH-1:0] data); endmodule
         module Double #(parameter WIDTH = 8) (output [2*WIDTH-1:0] data); endmodule",
    )];
    let config = ParserConfig {
        sources: &sources,
        ..Default::default()
    };
    let modules = VerilogLibrary::from_config(&config)
        .unwrap()
        .modules()
        .unwrap();
    let single = modules.iter().find(|m| m.get_name() == "Single").unwrap();
    let double = modules.iter().find(|m| m.get_name() == "Double").unwrap();
    let import = context(single);
    assert!(Arc::ptr_eq(&import, &context(double)));

    assert_eq!(
        single
            .parameterize(&[("WIDTH", 12)])
            .get_port("data")
            .io()
            .width(),
        12
    );
    assert_eq!(
        double
            .parameterize(&[("WIDTH", 12)])
            .get_port("data")
            .io()
            .width(),
        24
    );
    assert_eq!(import.cache.read().len(), 2);

    let separate = VerilogLibrary::from_config(&config)
        .unwrap()
        .module("Single")
        .unwrap()
        .unwrap();
    let separate_import = context(&separate);
    assert!(!Arc::ptr_eq(&import, &separate_import));
    assert!(separate_import.cache.read().is_empty());
}

// Equal integers share a cache entry across Rust types, while strings stay distinct.
// Changing override order gets a separate entry and preserves that order in output.
#[test]
fn cache_preserves_override_types_and_order() {
    let base = module_from_sources(
        "Typed",
        &[Source::text(
            "module Typed #(parameter VALUE = 0, EXTRA = 0)
         (output [$bits(VALUE)+EXTRA-1:0] data); endmodule",
        )],
    );
    let numeric = base.parameterize(&[("VALUE", 16), ("EXTRA", 0)]);
    let from_u8 = base.parameterize(&[("VALUE", 16u8), ("EXTRA", 0)]);
    assert_eq!(from_u8.get_port("data").io().width(), 32);
    assert_eq!(context(&base).cache.read().len(), 1);

    let text = base.parameterize::<ParameterValue>(&[("VALUE", "16".into()), ("EXTRA", 0.into())]);
    assert_eq!(numeric.get_port("data").io().width(), 32);
    assert_eq!(text.get_port("data").io().width(), 16);
    assert_eq!(context(&base).cache.read().len(), 2);

    let reversed = base.parameterize(&[("EXTRA", 0), ("VALUE", 16)]);
    assert_eq!(reversed.get_port("data").io().width(), 32);
    assert_eq!(context(&base).cache.read().len(), 3);
    let original_output = numeric.wrap(None, None).emit(EmitOptions::default());
    let reversed_output = reversed.wrap(None, None).emit(EmitOptions::default());
    assert!(original_output.find(".VALUE(").unwrap() < original_output.find(".EXTRA(").unwrap());
    assert!(reversed_output.find(".EXTRA(").unwrap() < reversed_output.find(".VALUE(").unwrap());
}

// Concurrent requests for repeated and different widths must each get the right
// result, leaving one cache entry per distinct width.
#[test]
fn parallel_parameterizations_share_the_cache_without_mixing_results() {
    let base = module_from_sources(
        "Parallel",
        &[Source::text(
            "module Parallel #(parameter int WIDTH = 8)
         (output [WIDTH-1:0] data); endmodule",
        )],
    );
    let widths = [12, 24, 12, 24];
    let barrier = Barrier::new(widths.len());
    std::thread::scope(|scope| {
        for width in widths {
            let base = &base;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                let variant = base.parameterize(&[("WIDTH", width)]);
                assert_eq!(variant.get_port("data").io().width(), width as usize);
            });
        }
    });
    assert_eq!(context(&base).cache.read().len(), 2);
}

// Imports and parameterized variants must retain the original owned text buffer
// without copying it, including when multiple imports use the same source.
#[test]
fn imports_and_variants_keep_owned_text_allocations() {
    const VERILOG: &str =
        "module Owned #(parameter WIDTH = 8) (output [WIDTH-1:0] data); endmodule";
    let assert_source_pointer = |module: &ModDef, expected| {
        let import = context(module);
        let slang_rs::Source::Text(text) = import.config.sources[0].as_slang_source() else {
            panic!("expected text source");
        };
        assert_eq!(text.as_ptr(), expected);
    };

    let direct_text = VERILOG.to_owned();
    let direct_pointer = direct_text.as_ptr();
    let direct = module_from_sources("Owned", &[Source::text(direct_text)]);
    assert_source_pointer(&direct, direct_pointer);
    assert_source_pointer(&direct.parameterize(&[("WIDTH", 16)]), direct_pointer);

    let config_text = VERILOG.to_owned();
    let config_pointer = config_text.as_ptr();
    let sources = [Source::text(config_text)];
    let first = module_from_sources("Owned", &sources);
    let second = module_from_sources("Owned", &sources);
    drop(sources);
    let variant = first.parameterize(&[("WIDTH", 16)]);
    for module in [&first, &second, &variant] {
        assert_source_pointer(module, config_pointer);
    }
}

// Variants keep their import and source text alive after the original module is
// dropped, and release both when the last variant is dropped.
#[test]
fn variants_retain_then_release_the_import_and_shared_source() {
    let source = Arc::new(String::from(
        "module Retained #(parameter WIDTH = 8) (output [WIDTH-1:0] data); endmodule",
    ));
    let weak_source = Arc::downgrade(&source);
    let sources = [Source::text(source.clone())];
    let base = module_from_sources("Retained", &sources);
    let weak_import = Arc::downgrade(&context(&base));
    drop(source);
    drop(sources);

    let first = base.parameterize(&[("WIDTH", 16)]);
    drop(base);
    assert!(weak_import.upgrade().is_some());
    assert!(weak_source.upgrade().is_some());
    let second = first.parameterize(&[("WIDTH", 24)]);
    drop(first);
    assert_eq!(second.get_port("data").io().width(), 24);
    assert_eq!(weak_import.upgrade().unwrap().cache.read().len(), 2);

    drop(second);
    assert!(weak_import.upgrade().is_none());
    assert!(weak_source.upgrade().is_none());
}
