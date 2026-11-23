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
            .parameter("W")
            .unwrap()
            .as_ref(),
    )
    .unwrap();
    let module = lib.module("sink").unwrap();
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
    let sink = selected.module("sink").unwrap();
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
    assert!(matches!(
        lib.package("absent"),
        Err(topstitch::LookupError::NotFound {
            kind: "package",
            ..
        })
    ));
    assert!(matches!(
        lib.module("absent"),
        Err(topstitch::LookupError::NotFound { kind: "top", .. })
    ));
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
    let sink = library.module("sink").unwrap();
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
        let module = library.module("configurable").unwrap();
        let variant = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            module.parameterize(&[("ENABLE", 1)])
        }));
        assert_eq!(variant.is_err(), check_level == CheckLevel::Semantic);
    }
}

#[test]
fn an_imported_hierarchy_can_be_instantiated_with_its_bindings() {
    let sources = [Source::text(
        "module Leaf; endmodule module Parent #(parameter W=1) (output [W-1:0] q); if (W == 8) begin : wide Leaf u(); end endmodule",
    )];
    let library = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[("W", "8")],
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap();
    for module in [
        library.module("Parent").unwrap(),
        library.modules().unwrap().pop().unwrap(),
    ] {
        assert_eq!(
            module.report_all_instances(),
            [("Leaf".into(), "Parent.wide.u".into())]
        );
        let narrow = module.parameterize(&[("W", 3)]);
        assert!(narrow.report_all_instances().is_empty());
        let wide = narrow.parameterize(&[("W", 8)]);
        assert_eq!(wide.report_all_instances(), module.report_all_instances());
        module.set_usage(topstitch::Usage::EmitNothingAndStop);
        let rtl = module.wrap(None, None).emit(Default::default());
        assert!(rtl.contains(".W(") && !rtl.contains("module Leaf"), "{rtl}");
        // W=8 is cached from the earlier editable variant. W=16 is a fresh specialization.
        for width in [8, 16] {
            let external = module.parameterize(&[("W", width)]);
            assert_eq!(external.get_usage(), topstitch::Usage::EmitNothingAndStop);
            let wrapper = external
                .wrap(Some("Wrapper"), Some("device"))
                .emit(Default::default());
            let linked_sources = [sources[0].clone(), Source::text(wrapper)];
            let linked = VerilogLibrary::from_config(&ParserConfig {
                sources: &linked_sources,
                tops: &["Wrapper"],
                include_hierarchy: true,
                ..Default::default()
            })
            .unwrap()
            .module("Wrapper")
            .unwrap();
            assert_eq!(linked.get_port("q").io().width(), width as usize);
            let instances = linked.report_all_instances();
            assert_eq!(
                instances
                    .iter()
                    .any(|(_, path)| path == "Wrapper.device.wide.u"),
                width == 8
            );
        }
    }
}

#[test]
fn a_descendant_override_cannot_be_emitted_as_an_instance_parameter() {
    let sources = [Source::text(
        "module Leaf #(parameter W=4) (); endmodule module Parent; Leaf u(); endmodule",
    )];
    let library = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[("Parent.u.W", "8")],
        ..Default::default()
    })
    .unwrap();
    assert!(
        library
            .module("Parent")
            .err()
            .unwrap()
            .to_string()
            .contains("Parent.u.W")
    );
}

#[test]
fn narrow_parameter_is_bound_to_its_elaborated_value() {
    let base = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter logic [3:0] W=0) (output [W:0] q); endmodule",
    )
    .unwrap();
    let leaf = base.parameterize(&[("W", 17)]);
    assert_eq!(leaf.get_port("q").io().width(), 2);
    let wrapper = leaf.wrap(None, None).emit(Default::default());
    assert!(wrapper.contains(".W(4'h1)"), "{wrapper}");
}

#[test]
fn emitted_instances_keep_the_imported_specialization() {
    let source = r#"
        module Match; endmodule
        module Leaf #(parameter W=4, MASK=0, BIAS=0, TAG="a", FLAGS=0, N=W+1)
                     (output [N-1:0] q);
            assign q = MASK;
            if (MASK == 8'ha5 && $bits(BIAS) == 5 && BIAS < 0 && BIAS == -3
                && TAG == "wide" && FLAGS === 4'b1xz0) begin : matched
                if (N == 9) begin : narrow Match ok(); end
                if (N == 11) begin : wide Match ok(); end
            end
        endmodule
    "#;
    let sources = [Source::text(source)];
    let leaf = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[
            ("W", "4 + 4"),
            ("MASK", "8'ha0 | 5"),
            ("BIAS", "5'sh1d"),
            ("TAG", r#""wide""#),
            ("FLAGS", "4'b1xz0"),
        ],
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    let wide = leaf.clone().parameterize(&[("W", 10)]);
    let top = ModDef::new("Assembly");
    for (instance, module) in [("original", &leaf), ("changed", &wide)] {
        top.instantiate(module, Some(instance), None)
            .get_port("q")
            .export_as(instance.to_owned() + "_q");
    }

    // Re-elaborate the actual SV instance bindings against the original implementation.
    let rtl = top.emit(Default::default());
    let linked = [Source::text(source), Source::text(rtl)];
    let assembly = VerilogLibrary::from_config(&ParserConfig {
        sources: &linked,
        tops: &["Assembly"],
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Assembly")
    .unwrap();
    let selected: Vec<_> = assembly
        .report_all_instances()
        .into_iter()
        .filter_map(|(module, path)| (module == "Match").then_some(path))
        .collect();
    assert_eq!(
        selected,
        [
            "Assembly.original.matched.narrow.ok",
            "Assembly.changed.matched.wide.ok"
        ]
    );
}

#[test]
fn omitted_instance_parameter_follows_the_declaration_default() {
    let leaf_sv = "module Leaf #(parameter MODE=0) (); endmodule";
    let a = ModDef::from_verilog("Leaf", leaf_sv).unwrap();
    let sources = [Source::text(leaf_sv)];
    let b = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[("MODE", "1")],
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    let top = ModDef::new("Assembly");
    top.instantiate(&a, Some("a"), None);
    top.instantiate(&b, Some("b"), None);
    let wrapper = top.emit(Default::default());

    // A bound probe observes each instance's parameter even when the leaf has no body.
    let probe = "module Marker; endmodule
                 module Probe #(parameter V=0) ();
                   for (genvar i=V; i<=V; ++i) begin : mode Marker seen(); end
                 endmodule
                 bind Leaf Probe #(.V(MODE)) probe();";
    for default in [0, 2] {
        let linked = [
            Source::text(leaf_sv.replace("MODE=0", &format!("MODE={default}"))),
            Source::text(&wrapper),
            Source::text(probe),
        ];
        let assembly = VerilogLibrary::from_config(&ParserConfig {
            sources: &linked,
            tops: &["Assembly"],
            include_hierarchy: true,
            ..Default::default()
        })
        .unwrap()
        .module("Assembly")
        .unwrap();
        let modes: Vec<_> = assembly
            .report_all_instances()
            .into_iter()
            .filter_map(|(name, path)| (name == "Marker").then_some(path))
            .collect();
        assert_eq!(
            modes,
            [
                format!("Assembly.a.probe.mode[{default}].seen"),
                "Assembly.b.probe.mode[1].seen".to_owned()
            ]
        );
    }
}

#[test]
fn external_enum_parameter_reelaborates_and_emits_with_its_type() {
    let source = r#"
        package p;
            typedef enum logic [1:0] { A=0, B=1 } mode_t;
        endpackage
        module Match; endmodule
        module Leaf #(parameter p::mode_t MODE=p::A, parameter W=4)
                     (output [W-1:0] q);
            if (MODE == p::B) begin : selected Match ok(); end
        endmodule
    "#;
    let sources = [Source::text(source)];
    let leaf = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[("MODE", "p::B")],
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    let wider = leaf.parameterize(&[("W", 8)]);
    let alternate = wider.parameterize(&[("MODE", 0)]);
    let top = ModDef::new("Assembly");
    for (name, module) in [
        ("original", &leaf),
        ("wider", &wider),
        ("alternate", &alternate),
    ] {
        top.instantiate(module, Some(name), None)
            .get_port("q")
            .export_as(format!("{name}_q"));
    }
    let rtl = top.emit(Default::default());
    assert!(rtl.contains("p::mode_t'("), "{rtl}");
    let linked = [Source::text(source), Source::text(rtl)];
    let assembly = VerilogLibrary::from_config(&ParserConfig {
        sources: &linked,
        tops: &["Assembly"],
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Assembly")
    .unwrap();
    let selected: Vec<_> = assembly
        .report_all_instances()
        .into_iter()
        .filter_map(|(module, path)| (module == "Match").then_some(path))
        .collect();
    assert_eq!(
        selected,
        [
            "Assembly.original.selected.ok",
            "Assembly.wider.selected.ok"
        ]
    );
    assert_eq!(assembly.get_port("wider_q").io().width(), 8);
}

#[test]
fn anonymous_enum_binding_is_reported_at_import() {
    let sources = [Source::text(
        "module Leaf #(parameter enum logic [1:0] { A=0, B=1 } MODE=A) (); endmodule",
    )];
    let library = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[("MODE", "B")],
        ..Default::default()
    })
    .unwrap();
    let error = library.module("Leaf").err().unwrap().to_string();
    assert!(
        error.contains("enum requires an accessible scalar typedef"),
        "{error}"
    );
}

#[test]
#[should_panic(expected = "parameter cast type")]
fn escaped_enum_typedef_can_remain_external_but_cannot_be_cast() {
    let source = r#"package p;
                        typedef enum logic [1:0] { A=0, B=1 } \mode.name ;
                    endpackage
                    module Leaf #(parameter p::\mode.name MODE=p::A) (); endmodule"#;
    let leaf = ModDef::from_verilog("Leaf", source).unwrap();
    let rtl = leaf.wrap(None, None).emit(Default::default());
    assert!(!rtl.contains("mode.name"), "{rtl}");
    leaf.parameterize(&[("MODE", 1)]);
}

#[test]
fn implicit_string_with_unknown_bits_retains_conversion_semantics() {
    let source = r#"module Match; endmodule
                    module Leaf #(parameter V="x", parameter W=4) ();
                        localparam string S=V;
                        if ($bits(V)==8 && V === 8'b0111100x) begin : selected Match ok(); end
                    endmodule"#;
    let sources = [Source::text(source)];
    let leaf = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[("V", r#"1'bx ? "x" : "y""#)],
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    let changed = leaf.parameterize(&[("W", 8)]);
    for variant in [&leaf, &changed] {
        let rtl = variant.wrap(None, None).emit(Default::default());
        let sources = [Source::text(source), Source::text(rtl)];
        let wrapper = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            tops: &["Leaf_wrapper"],
            include_hierarchy: true,
            ..Default::default()
        })
        .unwrap()
        .module("Leaf_wrapper")
        .unwrap();
        assert!(
            wrapper
                .report_all_instances()
                .iter()
                .any(|(module, path)| module == "Match" && path.ends_with(".selected.ok"))
        );
    }
}

#[test]
fn chained_parameterization_reapplies_requested_values() {
    let leaf = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter W=4, parameter logic [W-1:0] V=0)(output [V:0] q); endmodule",
    )
    .unwrap();
    let narrow = leaf.parameterize(&[("V", 17)]);
    assert_eq!(narrow.get_port("q").io().width(), 2);
    let wide = narrow.parameterize(&[("W", 8)]);
    assert_eq!(wide.get_port("q").io().width(), 18);
    assert_eq!(
        wide.get_port("q").io().width(),
        leaf.parameterize(&[("V", 17), ("W", 8)])
            .get_port("q")
            .io()
            .width()
    );
}

#[test]
fn root_qualified_overrides_are_preserved_and_descendants_rejected() {
    let source = "module Child #(parameter W=4)(); endmodule module Leaf #(parameter W=4)(output [W-1:0] q); Child child(); endmodule";
    for name in ["$root.Leaf.W", "$root . Leaf . W", "Leaf . W"] {
        let sources = [Source::text(source)];
        let leaf = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            parameters: &[(name, "8")],
            ..Default::default()
        })
        .unwrap()
        .module("Leaf")
        .unwrap();
        let rtl = leaf.wrap(None, None).emit(Default::default());
        let linked = [Source::text(source), Source::text(rtl)];
        let wrapper = VerilogLibrary::from_config(&ParserConfig {
            sources: &linked,
            tops: &["Leaf_wrapper"],
            ..Default::default()
        })
        .unwrap()
        .module("Leaf_wrapper")
        .unwrap();
        assert_eq!(wrapper.get_port("q").io().width(), 8);
    }
    let sources = [Source::text(source)];
    let lib = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[("$root.Leaf.child.W", "8")],
        ..Default::default()
    })
    .unwrap();
    assert!(
        lib.module("Leaf")
            .err()
            .unwrap()
            .to_string()
            .contains("hierarchical override")
    );
}

#[test]
fn carriage_return_parameters_round_trip() {
    for declaration in ["parameter V", "parameter string V"] {
        let source = format!(
            r#"module Match; endmodule module Leaf #({declaration}="x",parameter W=4)(); if(V=="a\015b") begin : selected Match ok(); end endmodule"#
        );
        let sources = [Source::text(&source)];
        let leaf = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            parameters: &[("V", r#""a\015b""#)],
            ..Default::default()
        })
        .unwrap()
        .module("Leaf")
        .unwrap();
        for module in [&leaf, &leaf.parameterize(&[("W", 8)])] {
            let rtl = module.wrap(None, None).emit(Default::default());
            let sources = [Source::text(&source), Source::text(rtl)];
            let wrapper = VerilogLibrary::from_config(&ParserConfig {
                sources: &sources,
                tops: &["Leaf_wrapper"],
                include_hierarchy: true,
                ..Default::default()
            })
            .unwrap()
            .module("Leaf_wrapper")
            .unwrap();
            assert!(
                wrapper
                    .report_all_instances()
                    .iter()
                    .any(|(m, p)| m == "Match" && p.ends_with(".selected.ok"))
            );
        }
    }
}

#[test]
fn imported_hierarchy_stays_editable_and_emittable() {
    let sources = [Source::text("module Leaf; endmodule")];
    let leaf = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    leaf.add_port("extra", topstitch::IO::Input(1)).unused();
    let rtl = leaf.emit(Default::default());
    assert!(
        rtl.contains("module Leaf") && rtl.contains("extra"),
        "{rtl}"
    );
}

/// Import a module specialized by Slang, keeping omitted parameters omitted.
fn specialized(source: &str, parameter: &str, value: &str) -> ModDef {
    let sources = [Source::text(source)];
    VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[(parameter, value)],
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap()
}

/// Construct an assembly with both references to the same external module name.
fn stub_assembly(a: &ModDef, b: &ModDef, reverse: bool) -> ModDef {
    let top = ModDef::new("Assembly");
    for (module, name) in if reverse {
        [(b, "b"), (a, "a")]
    } else {
        [(a, "a"), (b, "b")]
    } {
        let inst = top.instantiate(module, Some(name), None);
        inst.get_port("q").unused();
    }
    top
}

/// Elaborate the emitted definition without the original source, observing an instance expression.
fn stub_values(rtl: &str, expression: &str) -> Vec<String> {
    let probe = format!(
        "module Marker; endmodule module Probe #(parameter V=0) (); for(genvar i=V;i<=V;++i) begin:value Marker seen(); end endmodule bind Leaf Probe #(.V({expression})) probe();"
    );
    let sources = [Source::text(rtl), Source::text(probe)];
    let assembly = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Assembly"],
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Assembly")
    .unwrap();
    let mut modes: Vec<_> = assembly
        .report_all_instances()
        .into_iter()
        .filter_map(|(name, path)| (name == "Marker").then_some(path))
        .collect();
    modes.sort();
    modes
}

#[test]
fn shared_stub_preserves_source_default_and_explicit_override() {
    let source = "module Leaf #(parameter MODE=0)(output q); endmodule";
    for reverse in [false, true] {
        let a = ModDef::from_verilog("Leaf", source).unwrap();
        let b = specialized(source, "MODE", "1");
        b.set_usage(topstitch::Usage::EmitStubAndStop);
        let rtl = stub_assembly(&a, &b, reverse).emit(Default::default());
        assert_eq!(
            stub_values(&rtl, "MODE"),
            [
                "Assembly.a.probe.value[0].seen",
                "Assembly.b.probe.value[1].seen"
            ]
        );
    }
}

#[test]
fn fixed_stub_accepts_matching_shape_and_rejects_other_specializations() {
    let source = "module Leaf #(parameter W=4)(output [W-1:0] q); endmodule";
    for reverse in [false, true] {
        let stub = specialized(source, "W", "8");
        stub.set_usage(topstitch::Usage::EmitStubAndStop);
        let matching = specialized(source, "W", "8");
        let rtl = stub_assembly(&matching, &stub, reverse).emit(Default::default());
        for expression in ["W", "$bits(q)"] {
            assert_eq!(
                stub_values(&rtl, expression),
                [
                    "Assembly.a.probe.value[8].seen",
                    "Assembly.b.probe.value[8].seen"
                ]
            );
        }
        let other = specialized(source, "W", "4");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            stub_assembly(&other, &stub, reverse).emit(Default::default())
        }));
        let error = result.unwrap_err();
        let message = error
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| error.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(message.contains("width mismatch"), "{message}");
    }
}

#[test]
fn editable_imported_specialization_emits_a_parameter_header() {
    let sources = [Source::text("module Leaf #(parameter MODE=0)(); endmodule")];
    let leaf = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap()
    .parameterize(&[("MODE", 1)]);
    let top = ModDef::new("Assembly");
    top.instantiate(&leaf, Some("b"), None);
    let rtl = top.emit(Default::default());
    assert_eq!(
        stub_values(&rtl, "MODE"),
        ["Assembly.b.probe.value[1].seen"]
    );
}

#[test]
fn compilation_unit_enum_binding_is_diagnosed() {
    let sources = [Source::text(
        "typedef enum logic[1:0] {A, B} mode_t; module Leaf #(parameter mode_t MODE=A)(); endmodule",
    )];
    let lib = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        parameters: &[("MODE", "B")],
        ..Default::default()
    })
    .unwrap();
    assert!(
        lib.module("Leaf")
            .err()
            .unwrap()
            .to_string()
            .contains("enum type mode_t is not accessible")
    );
}
