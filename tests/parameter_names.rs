// SPDX-License-Identifier: Apache-2.0

use topstitch::{ModDef, ParserConfig, Source, VerilogLibrary};

const SOURCE: &str = r#"
    package p;
        typedef enum logic [1:0] { A=0, B=1 } \mode.name ;
    endpackage
    module Leaf #(parameter p::\mode.name MODE=p::A) ();
        wire \inside.bad ;
    endmodule
"#;

#[test]
fn an_opaque_module_can_use_escaped_identifiers_internally() {
    let leaf = ModDef::from_verilog("Leaf", SOURCE).unwrap();
    let rtl = leaf.wrap(Some("Assembly"), None).emit(Default::default());
    assert!(rtl.contains("Leaf Leaf_i"), "{rtl}");
    assert!(!rtl.contains("\\"), "{rtl}");
    let inputs = [Source::text(SOURCE), Source::text(rtl)];
    VerilogLibrary::from_config(&ParserConfig {
        sources: &inputs,
        tops: &["Assembly"],
        ..Default::default()
    })
    .unwrap();
}

#[test]
fn an_override_cannot_require_an_escaped_parameter_cast_type() {
    let inputs = [Source::text(SOURCE)];
    let library = VerilogLibrary::from_config(&ParserConfig {
        sources: &inputs,
        parameters: &[("MODE", "p::B")],
        ..Default::default()
    })
    .unwrap();
    let error = library.module("Leaf").err().unwrap().to_string();
    assert!(error.contains("parameter cast type"), "{error}");
    assert!(error.contains("mode.name"), "{error}");
}

#[test]
#[should_panic(expected = "parameter name \"WIDTH$bits\"")]
fn invalid_parameter_name_is_rejected_before_reelaboration() {
    let leaf =
        ModDef::from_verilog("Leaf", "module Leaf #(parameter WIDTH$bits=1)(); endmodule").unwrap();
    leaf.parameterize(&[("WIDTH$bits", 2)]);
}
