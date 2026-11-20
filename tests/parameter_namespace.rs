// SPDX-License-Identifier: Apache-2.0

use std::panic::{AssertUnwindSafe, catch_unwind};
use topstitch::{EmitOptions, IO, ModDef, ParserConfig, Source, VerilogLibrary};

/// Read an editable root, retaining its original parameter declarations.
fn root(source: &str) -> ModDef {
    let sources = [Source::text(source)];
    VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Root"],
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Root")
    .unwrap()
}

#[test]
fn parameter_declarations_reserve_the_instance_namespace() {
    let source =
        "module Root #(parameter W=4, localparam L=W+1, parameter type Word=logic)(); endmodule";
    for name in ["W", "L", "Word"] {
        let top = root(source);
        top.instantiate(&ModDef::new("Leaf"), Some(name), None);
        let error = catch_unwind(AssertUnwindSafe(|| top.emit(EmitOptions::default())));
        assert!(
            error.is_err(),
            "instance {name} conflicted with an emitted parameter"
        );
    }
}

#[test]
fn generated_wire_avoids_parameter_names() {
    let top = root("module Root #(parameter link_out=0)(); endmodule");
    let source = ModDef::new("Source");
    source.add_port("out", IO::Output(1)).tieoff(0);
    let sink = ModDef::new("Sink");
    sink.add_port("inp", IO::Input(1)).unused();
    let a = top.instantiate(&source, Some("link"), None);
    let b = top.instantiate(&sink, Some("sink"), None);
    a.get_port("out").connect(&b.get_port("inp"));
    let rtl = top.emit(EmitOptions::default());
    assert!(rtl.contains("link_out__1"), "{rtl}");
    let sources = [Source::text(rtl)];
    VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Root"],
        ..Default::default()
    })
    .unwrap();
}
