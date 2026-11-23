// SPDX-License-Identifier: Apache-2.0

use topstitch::{ModDef, ParserConfig, Source, VerilogLibrary};

/// Re-elaborate the emitted instance with the original leaf and observe its generate branches.
fn linked_markers(source: &str, leaf: &ModDef) -> Vec<String> {
    let rtl = leaf.wrap(None, None).emit(Default::default());
    let sources = [Source::text(source), Source::text(&rtl)];
    let config = ParserConfig {
        sources: &sources,
        tops: &["Leaf_wrapper"],
        include_hierarchy: true,
        ..Default::default()
    };
    VerilogLibrary::from_config(&config)
        .unwrap()
        .module("Leaf_wrapper")
        .unwrap()
        .report_all_instances()
        .into_iter()
        .filter_map(|(name, path)| (name == "Marker").then_some(path))
        .collect()
}

#[test]
fn string_valued_override_retains_its_type() {
    let source = r#"module Marker; endmodule
        module Leaf #(parameter V=string'("d"), parameter W=1)(output [V.len()-1:0] q);
          if (V.len()==3 && V=="abc") begin : matched Marker marker(); end
        endmodule"#;
    for value in [r#"string'("abc")"#, r#"$sformatf("%s", "abc")"#] {
        let sources = [Source::text(source)];
        let leaf = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            parameters: &[("V", value)],
            ..Default::default()
        })
        .unwrap()
        .module("Leaf")
        .unwrap();
        for variant in [&leaf, &leaf.parameterize(&[("W", 2)])] {
            assert_eq!(variant.get_port("q").io().width(), 3);
            assert_eq!(
                linked_markers(source, variant),
                ["Leaf_wrapper.Leaf_i.matched.marker"]
            );
        }
    }
}

#[test]
fn rust_string_literal_stays_packed_for_an_untyped_parameter() {
    let source = r#"module Marker; endmodule
        module Leaf #(parameter V="d", parameter W=1)();
          if ($bits(V)==24 && $typename(V)!="string" && V=="abc") begin : matched Marker marker(); end
        endmodule"#;
    let base = ModDef::from_verilog("Leaf", source).unwrap();
    let variant = base.parameterize(&[("V", "abc")]).parameterize(&[("W", 2)]);
    assert_eq!(
        linked_markers(source, &variant),
        ["Leaf_wrapper.Leaf_i.matched.marker"]
    );
}

#[test]
fn opaque_import_retains_foreign_escaped_names_without_emitting_them() {
    let source = r#"module Marker; endmodule
        module Leaf #(parameter W=3, parameter \V.dot =1, parameter EXTRA=0)();
          if (W==9 || \V.dot ==9) begin : matched Marker marker(); end
        endmodule"#;
    let leaf = ModDef::from_verilog("Leaf", source)
        .unwrap()
        .parameterize(&[("W", 9)])
        .parameterize(&[("EXTRA", 2)]);
    assert_eq!(
        linked_markers(source, &leaf),
        ["Leaf_wrapper.Leaf_i.matched.marker"]
    );
    assert!(
        !leaf
            .wrap(None, None)
            .emit(Default::default())
            .contains("V.dot")
    );
}

#[test]
fn escaped_override_names_are_rejected() {
    let source = r"module Leaf #(parameter W=3, parameter \V.dot =1)(); endmodule";
    for path in [r"Leaf.\W ", r"$root.\Leaf .W", r"Leaf.\V.dot ", r"\W "] {
        let sources = [Source::text(source)];
        let overrides = [(path, "9")];
        let lib = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            parameters: &overrides,
            ..Default::default()
        })
        .unwrap();
        let err = lib.module("Leaf").err().unwrap().to_string();
        assert!(err.contains("unescaped identifier"), "{path}: {err}");
    }
}

#[test]
fn escaped_module_override_path_is_rejected() {
    let sources = [Source::text(
        r"module \Leaf.dot #(parameter W=3)(); endmodule",
    )];
    let lib = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[(r"$root.\Leaf.dot .W", "9")],
        ..Default::default()
    })
    .unwrap();
    let err = lib.module("Leaf.dot").err().unwrap().to_string();
    assert!(
        err.contains("module name") && err.contains("unescaped identifier"),
        "{err}"
    );
}

#[test]
fn equals_in_parameter_path_is_diagnosed_before_slang_splits_the_name() {
    let sources = [Source::text(
        r"module Leaf #(parameter \V=dot =3)(); endmodule",
    )];
    let err = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[(r"Leaf.\V=dot ", "9")],
        ..Default::default()
    })
    .err()
    .unwrap()
    .to_string();
    assert!(
        err.contains("Cannot use '=' in SV parameter override path"),
        "{err}"
    );
}
