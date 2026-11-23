// SPDX-License-Identifier: Apache-2.0

use std::panic::{AssertUnwindSafe, catch_unwind};
use topstitch::{ModDef, ParserConfig, PipelineConfig, Source, Usage, VerilogLibrary};

/// Elaborate an imported module at a particular specialization.
fn leaf(source: &str, parameters: &[(&str, &str)]) -> ModDef {
    let sources = [Source::text(source)];
    VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Leaf"],
        parameters,
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap()
}

/// Link an independently emitted stub and assembly, observing generated instances.
fn markers(root: &str, sources: &[Source]) -> Vec<String> {
    let tops = [root];
    let library = VerilogLibrary::from_config(&ParserConfig {
        sources,
        tops: &tops,
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap();
    let mut paths: Vec<_> = library
        .module(root)
        .unwrap()
        .report_all_instances()
        .into_iter()
        .filter_map(|(module, path)| (module == "Mark").then_some(path))
        .collect();
    paths.sort();
    paths
}

#[test]
fn standalone_stub_keeps_the_original_default_and_can_be_parameterized() {
    let source = "module Leaf #(parameter MODE=0)(); endmodule";
    let base = leaf(source, &[]);
    let on = leaf(source, &[("MODE", "1")]);
    let top = ModDef::new("Assembly");
    top.instantiate(&base, Some("a"), None);
    top.instantiate(&on, Some("b"), None);
    let wrapper = top.emit(Default::default());
    let probe = "module Mark; endmodule module Observer #(parameter V=0)(); if(V==1) begin:on Mark seen(); end endmodule bind Leaf Observer #(.V(MODE)) observer();";
    for stub in [&base, &on] {
        stub.set_usage(Usage::EmitStubAndStop);
        let rtl = stub.emit(Default::default());
        assert_eq!(
            markers(
                "Assembly",
                &[
                    Source::text(rtl),
                    Source::text(&wrapper),
                    Source::text(probe)
                ]
            ),
            ["Assembly.b.observer.on.seen"]
        );
    }
}

#[test]
fn declaration_types_and_dependent_defaults_survive_stubbing() {
    let package = r#"package p;
        typedef struct packed {logic flag; logic [2:0] mode;} cfg_t;
        parameter cfg_t RESET_CFG = '{flag:0,mode:3};
        endpackage"#;
    let source = format!(
        r#"{package}
        module Leaf import p::*; #(parameter cfg_t CFG=RESET_CFG,
            parameter V=string'("abc"), parameter W=4, parameter DEPTH=W+1)(); endmodule"#
    );
    let stub = leaf(&source, &[("W", "8")]);
    stub.set_usage(Usage::EmitStubAndStop);
    let rtl = stub.emit(Default::default());
    let client = r#"module Assembly; Leaf a(); Leaf #(.V(37),.W(8)) b(); endmodule
        module Mark; endmodule
        module Observer #(parameter V="x", parameter M=0, parameter D=0)();
          if (M==3 && (($typename(V)=="string") ? D==5 : D==9)) begin:ok Mark seen(); end
        endmodule
        bind Leaf Observer #(.V(V),.M(CFG.mode),.D(DEPTH)) observer();"#;
    assert_eq!(
        markers(
            "Assembly",
            &[
                Source::text(package),
                Source::text(rtl),
                Source::text(client)
            ]
        ),
        ["Assembly.a.observer.ok.seen", "Assembly.b.observer.ok.seen"]
    );
}

#[test]
fn required_parameters_stay_required_without_losing_optional_defaults() {
    let source = "module Leaf #(parameter W, parameter MODE=0)(output [W-1:0] q); endmodule";
    let a = leaf(source, &[("W", "8")]);
    let b = leaf(source, &[("W", "8"), ("MODE", "1")]);
    b.set_usage(Usage::EmitStubAndStop);
    let top = ModDef::new("Assembly");
    top.instantiate(&a, Some("a"), None).get_port("q").unused();
    top.instantiate(&b, Some("b"), None).get_port("q").unused();
    let rtl = top.emit(Default::default());
    let probe = "module Mark; endmodule module Probe #(parameter M=0)(); if(M==1) begin:on Mark seen(); end endmodule bind Leaf Probe #(.M(MODE)) observer();";
    assert_eq!(
        markers(
            "Assembly",
            &[Source::text(rtl.clone()), Source::text(probe)]
        ),
        ["Assembly.b.observer.on.seen"]
    );
    let invalid = [
        Source::text(rtl),
        Source::text("module MissingWidth; Leaf missing(); endmodule"),
    ];
    assert!(
        VerilogLibrary::from_config(&ParserConfig {
            sources: &invalid,
            tops: &["MissingWidth"],
            ..Default::default()
        })
        .is_err()
    );
}

#[test]
fn native_pipeline_can_emit_a_fixed_standalone_stub() {
    let pipe = PipelineConfig {
        depth: 2,
        ..Default::default()
    }
    .to_mod_def(8);
    pipe.set_usage(Usage::EmitStubAndStop);
    let stub = pipe.emit(Default::default());
    let sources = [
        Source::text(stub),
        Source::text(
            "module Assembly; br_delay_nr p(); endmodule module Mark; endmodule module Probe #(parameter B=0)(); if(B==8) begin:ok Mark seen(); end endmodule bind br_delay_nr Probe #(.B($bits(out))) observer();",
        ),
    ];
    assert_eq!(
        markers("Assembly", &sources),
        ["Assembly.p.observer.ok.seen"]
    );
}

#[test]
fn a_default_depending_on_a_module_body_is_diagnosed() {
    let source = "module Leaf(); function automatic int f(input int a); return a+1; endfunction parameter MODE=f(4); endmodule";
    let stub = leaf(source, &[]);
    stub.set_usage(Usage::EmitStubAndStop);
    let error = catch_unwind(AssertUnwindSafe(|| stub.emit(Default::default()))).unwrap_err();
    assert!(
        error
            .downcast_ref::<String>()
            .unwrap()
            .contains("cannot reproduce value parameter declarations of Leaf")
    );
}

#[test]
fn a_similarly_named_sibling_with_a_different_default_is_diagnosed() {
    let a = leaf("module Leaf #(parameter MODE=4)(); endmodule", &[]);
    let b = leaf(
        "module Leaf #(parameter MODE=5)(); endmodule",
        &[("MODE", "1")],
    );
    b.set_usage(Usage::EmitStubAndStop);
    let top = ModDef::new("Assembly");
    top.instantiate(&a, Some("a"), None);
    top.instantiate(&b, Some("b"), None);
    let error = catch_unwind(AssertUnwindSafe(|| top.emit(Default::default()))).unwrap_err();
    assert!(
        error
            .downcast_ref::<String>()
            .unwrap()
            .contains("changes the value or type of Leaf.MODE")
    );
}

#[test]
fn standalone_header_keeps_type_and_local_parameters_used_by_defaults() {
    let source = "module Leaf #(parameter type T=logic[3:0], localparam L=$bits(T), parameter DEPTH=L+1)(); endmodule";
    let stub = leaf(source, &[]);
    stub.set_usage(Usage::EmitStubAndStop);
    let rtl = stub.emit(Default::default());
    let client = "module Assembly; Leaf #(.T(logic[7:0])) b(); endmodule module Mark; endmodule module Observer #(parameter D=0)(); if (D==9) begin:ok Mark seen(); end endmodule bind Leaf Observer #(.D(DEPTH)) observer();";
    assert_eq!(
        markers("Assembly", &[Source::text(rtl), Source::text(client)]),
        ["Assembly.b.observer.ok.seen"]
    );
}

#[test]
fn parameter_free_stub_emission_needs_no_source_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("Leaf.sv");
    std::fs::write(&file, "module Leaf(input a); endmodule").unwrap();
    let stub = ModDef::from_verilog_file("Leaf", &file).unwrap();
    std::fs::remove_file(file).unwrap();
    stub.set_usage(Usage::EmitStubAndStop);
    let rtl = stub.emit(Default::default());
    assert!(ModDef::from_verilog("Leaf", rtl).unwrap().has_port("a"));
}

#[test]
fn specializing_an_editable_root_preserves_type_and_local_parameters() {
    let sources = [Source::text(
        "module Leaf #(parameter type T=logic[3:0], localparam L=$bits(T), parameter SCALE, parameter COUNT=1)(); endmodule",
    )];
    let leaf = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Leaf"],
        parameters: &[("SCALE", "7"), ("COUNT", "2")],
        include_hierarchy: true,
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    let rtl = leaf.emit(Default::default());
    let client = "module Assembly; Leaf b(); endmodule module Mark; endmodule module Observer #(parameter V=0)(); if(V==13) begin:ok Mark seen(); end endmodule bind Leaf Observer #(.V(L+SCALE+COUNT)) observer();";
    assert_eq!(
        markers("Assembly", &[Source::text(rtl), Source::text(client)]),
        ["Assembly.b.observer.ok.seen"]
    );
}

#[test]
fn header_validation_respects_the_imports_compilation_unit() {
    let sources = [
        Source::text("`define WIDTH 8"),
        Source::text(
            "package p; parameter K=3; endpackage module Leaf import p::*; #(parameter W=`WIDTH+K)(); endmodule",
        ),
    ];
    let leaf = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Leaf"],
        single_unit: true,
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    leaf.set_usage(Usage::EmitStubAndStop);
    let rtl = leaf.emit(Default::default());
    let client = "module Assembly; Leaf b(); endmodule module Mark; endmodule module Observer #(parameter V=0)(); if(V==11) begin:ok Mark seen(); end endmodule bind Leaf Observer #(.V(W)) observer();";
    assert_eq!(
        markers(
            "Assembly",
            &[
                Source::text("package p; parameter K=3; endpackage"),
                Source::text(rtl),
                Source::text(client)
            ]
        ),
        ["Assembly.b.observer.ok.seen"]
    );
}

#[test]
fn single_unit_imports_do_not_leak_global_names_into_a_standalone_stub() {
    let sources = [
        Source::text("localparam int HIDDEN=7;"),
        Source::text("module Leaf #(parameter W=HIDDEN)(); endmodule"),
    ];
    let leaf = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Leaf"],
        single_unit: true,
        allow_use_before_declare: true,
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    leaf.set_usage(Usage::EmitStubAndStop);
    let error = catch_unwind(AssertUnwindSafe(|| leaf.emit(Default::default()))).unwrap_err();
    assert!(
        error
            .downcast_ref::<String>()
            .unwrap()
            .contains("cannot reproduce value parameter declarations of Leaf")
    );
}

#[test]
fn a_parameter_free_stub_cannot_hide_a_siblings_defaulted_parameter() {
    let stub = leaf("module Leaf(); endmodule", &[]);
    stub.set_usage(Usage::EmitStubAndStop);
    let sibling = leaf("module Leaf #(parameter MODE=1)(); endmodule", &[]);
    let top = ModDef::new("Assembly");
    top.instantiate(&stub, Some("a"), None);
    top.instantiate(&sibling, Some("b"), None);
    let error = catch_unwind(AssertUnwindSafe(|| top.emit(Default::default()))).unwrap_err();
    let message = error.downcast_ref::<String>().unwrap();
    assert!(message.contains("Leaf references disagree on their overridable value parameters"));
}

#[test]
fn stub_checks_respect_warnings_as_errors_with_open_ports() {
    let sources = [Source::text(
        "module Leaf #(parameter W=3)(input [W-1:0] d, output [W-1:0] q, inout io); endmodule",
    )];
    let config = ParserConfig {
        sources: &sources,
        tops: &["Leaf"],
        diagnostics: topstitch::DiagnosticOptions {
            warnings_as_errors: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let stub = VerilogLibrary::from_config(&config)
        .unwrap()
        .module("Leaf")
        .unwrap();
    stub.set_usage(Usage::EmitStubAndStop);
    let emitted = ModDef::from_verilog("Leaf", stub.emit(Default::default())).unwrap();
    assert_eq!(emitted.get_port("q").io().width(), 3);
}

#[test]
fn specializing_a_stub_does_not_require_its_default_implementation() {
    let source = r#"module Leaf #(parameter W=0)(output [W-1:0] q);
        if (W==0) $error("W must be positive");
        endmodule"#;
    let stub = leaf(source, &[("W", "8")]);
    stub.set_usage(Usage::EmitStubAndStop);
    let rtl = stub.emit(Default::default());
    let consumer = "module Assembly; Leaf #(.W(8)) leaf(); endmodule module Mark; endmodule module Probe #(parameter OK=0); if(OK) begin:width Mark mark(); end endmodule bind Leaf Probe #(.OK(W==8 && $bits(q)==8)) probe();";
    assert_eq!(
        markers("Assembly", &[Source::text(rtl), Source::text(consumer)]),
        ["Assembly.leaf.probe.width.mark"]
    );
}

#[test]
fn body_declared_type_and_value_parameters_retain_their_scope() {
    let source = "module Leaf(output [7:0] q); parameter type Word=logic[3:0]; parameter W=$bits(q), SUM=W+$bits(Word); endmodule";
    let sources = [Source::text(source)];
    let view = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Leaf"],
        include_hierarchy: true,
        parameters: &[("W", "9")],
        ..Default::default()
    })
    .unwrap()
    .module("Leaf")
    .unwrap();
    let stub = leaf(source, &[("W", "9")]);
    stub.set_usage(Usage::EmitStubAndStop);
    view.get_port("q").tieoff(0);
    let client = "module Assembly; Leaf a(); Leaf #(.Word(logic[7:0])) b(); endmodule";
    for (rtl, width) in [
        (stub.emit(Default::default()), 8),
        (view.emit(Default::default()), 9),
    ] {
        let probe = format!(
            "module Mark; endmodule module Probe #(parameter W=0, S=0); if (W=={width} && S==W+4) begin:narrow Mark mark(); end if (W=={width} && S==W+8) begin:wide Mark mark(); end endmodule bind Leaf Probe #(.W(W),.S(SUM)) probe();"
        );
        assert_eq!(
            markers(
                "Assembly",
                &[Source::text(rtl), Source::text(probe), Source::text(client)]
            ),
            ["Assembly.a.probe.narrow.mark", "Assembly.b.probe.wide.mark"]
        );
    }
}

#[test]
fn sharing_compares_resolved_type_parameters_at_each_instances_bindings() {
    let source = "module Leaf #(parameter W=4, parameter type T=logic[W-1:0])(); endmodule";
    let a = leaf(source, &[]);
    let b = leaf(source, &[("W", "8")]);
    b.set_usage(Usage::EmitStubAndStop);
    let top = ModDef::new("Assembly");
    top.instantiate(&a, Some("a"), None);
    top.instantiate(&b, Some("b"), None);
    let probe = "module Mark; endmodule module P #(parameter B=0); if(B==4) begin:narrow Mark seen(); end if(B==8) begin:wide Mark seen(); end endmodule bind Leaf P #(.B($bits(T))) p();";
    assert_eq!(
        markers(
            "Assembly",
            &[
                Source::text(top.emit(Default::default())),
                Source::text(probe)
            ]
        ),
        ["Assembly.a.p.narrow.seen", "Assembly.b.p.wide.seen"]
    );

    let shared = "module Leaf #(parameter type T=p::word_t)(); endmodule";
    for typedef in ["logic[3:0]", "logic signed[7:0]", "logic[0:7]"] {
        let four = leaf(
            &format!("package p; typedef {typedef} word_t; endpackage {shared}"),
            &[],
        );
        let eight = leaf(
            &format!("package p; typedef logic[7:0] word_t; endpackage {shared}"),
            &[],
        );
        eight.set_usage(Usage::EmitStubAndStop);
        let top = ModDef::new("Assembly");
        top.instantiate(&four, Some("a"), None);
        top.instantiate(&eight, Some("b"), None);
        let error = catch_unwind(AssertUnwindSafe(|| top.emit(Default::default()))).unwrap_err();
        assert!(
            error
                .downcast_ref::<String>()
                .unwrap()
                .contains("resolved type of Leaf.T")
        );
    }
}

#[test]
fn body_declared_value_retains_an_imported_package_typedef() {
    let package = "package p; typedef struct packed {logic valid;} cfg_t; endpackage";
    let source =
        format!("{package} import p::*; module Leaf(); parameter cfg_t CFG=1'b1; endmodule");
    let stub = leaf(&source, &[]);
    stub.set_usage(Usage::EmitStubAndStop);
    let client = "module Assembly; Leaf leaf(); endmodule module Mark; endmodule module P #(parameter V=0); if (V) begin:valid Mark seen(); end endmodule bind Leaf P #(.V(CFG.valid)) obs();";
    assert_eq!(
        markers(
            "Assembly",
            &[
                Source::text(package),
                Source::text(stub.emit(Default::default())),
                Source::text(client)
            ]
        ),
        ["Assembly.leaf.obs.valid.seen"]
    );
}

#[test]
fn timed_parameter_stubs_keep_module_directive_and_compiler_scales_local() {
    use topstitch::{LanguageVersion, TimeScale, TimeScaleMagnitude, TimeScaleValue, TimeUnit};
    let ps = TimeScaleValue {
        unit: TimeUnit::Picoseconds,
        ..Default::default()
    };
    let fs100 = TimeScaleValue {
        unit: TimeUnit::Femtoseconds,
        magnitude: TimeScaleMagnitude::Hundred,
    };
    let sources = [
        (
            "module Leaf #(parameter time T=1ns)(); timeunit 1ps; timeprecision 100fs; endmodule",
            None,
            LanguageVersion::SystemVerilog2017,
        ),
        (
            "`timescale 1ps/100fs\nmodule Leaf #(parameter time T=1ns)(); endmodule",
            None,
            LanguageVersion::Verilog2005,
        ),
        (
            "module Leaf #(parameter time T=1ns)(); endmodule",
            Some(TimeScale {
                base: ps,
                precision: fs100,
            }),
            LanguageVersion::SystemVerilog2017,
        ),
    ];
    for (source, timescale, language_version) in sources {
        let inputs = [Source::text(source)];
        let config = ParserConfig {
            sources: &inputs,
            timescale,
            language_version,
            ..Default::default()
        };
        let import = || {
            VerilogLibrary::from_config(&config)
                .unwrap()
                .module("Leaf")
                .unwrap()
        };
        let leaf = import();
        leaf.set_usage(Usage::EmitStubAndStop);
        let wrapper = ModDef::new("Assembly");
        wrapper.instantiate(&leaf, Some("u"), None);
        // A separate import checks the original instance in its own compilation.
        wrapper.instantiate(&import(), Some("other"), None);
        let rtl = wrapper.emit(Default::default());
        let opts = ParserConfig {
            sources: &[Source::text(&rtl)],
            tops: &["Leaf", "Assembly"],
            ..Default::default()
        };
        let parsed = opts.compile().unwrap();
        let scope = parsed
            .parameter_declarations_for("Leaf")
            .unwrap()
            .timescale
            .unwrap();
        assert_eq!(scope.replace(' ', ""), "1ps/100fs");
        let client = r"module Mark; endmodule
            module Observer #(parameter V=0, parameter time CLIENT=1ns)();
                if (V==1000 && CLIENT==1) begin:ok Mark yes(); end
            endmodule
            bind Leaf Observer #(.V(T)) m();";
        let linked = format!("`timescale 1ns/1ns\n{rtl}\n{client}");
        assert_eq!(
            markers("Assembly", &[Source::text(linked)]),
            ["Assembly.other.m.ok.yes", "Assembly.u.m.ok.yes"]
        );
    }
}

#[test]
fn imported_implicit_timescales_follow_the_consumer_in_a_mixed_assembly() {
    let implicit = "module Leaf #(parameter time T=1ns)(); endmodule";
    let explicit = "module Timed #(parameter time T=1ns)(); timeunit 1ns/1ns; endmodule";
    let ambient = leaf(implicit, &[]);
    ambient.set_usage(Usage::EmitStubAndStop);
    assert!(!ambient.emit(Default::default()).contains("timeunit"));
    let timed = VerilogLibrary::from_config(&ParserConfig {
        sources: &[Source::text(explicit)],
        ..Default::default()
    })
    .unwrap()
    .module("Timed")
    .unwrap();
    timed.set_usage(Usage::EmitStubAndStop);
    let wrapper = ModDef::new("Assembly");
    wrapper.instantiate(&ambient, Some("implicit_leaf"), None);
    wrapper.instantiate(&timed, Some("explicit_leaf"), None);
    let original = format!(
        "{implicit}\n{explicit}\nmodule Assembly; Leaf implicit_leaf(); Timed explicit_leaf(); endmodule"
    );
    let probe = r"module Mark; endmodule
        module Observer #(parameter V=0, EXPECT=0)(); if (V==EXPECT) begin:ok Mark yes(); end endmodule
        bind Leaf Observer #(.V(T),.EXPECT(1000)) m();
        bind Timed Observer #(.V(T),.EXPECT(1)) m();";
    for rtl in [original, wrapper.emit(Default::default())] {
        let linked = format!("`timescale 1ps/1ps\n{rtl}\n{probe}");
        assert_eq!(
            markers("Assembly", &[Source::text(linked)]),
            [
                "Assembly.explicit_leaf.m.ok.yes",
                "Assembly.implicit_leaf.m.ok.yes"
            ]
        );
    }
}

#[test]
fn stub_checks_preserve_module_local_type_names_and_instance_paths() {
    let package = r"package p;
        typedef struct packed { logic valid; logic [2:0] tag; } cfg_t;
        typedef enum logic [1:0] { Idle, Busy } state_t;
    endpackage";
    let module = r#"module Leaf #(parameter type Config=p::cfg_t,
        parameter Config RESET='{valid:1, tag:5}, parameter type State=p::state_t,
        parameter State S=p::Busy, parameter string WHO=$sformatf("%m"),
        parameter string TYPE_NAME=$typename(Config))(); endmodule"#;
    let stub = leaf(&format!("{package}\n{module}"), &[]);
    stub.set_usage(Usage::EmitStubAndStop);
    let client = r#"module Bench; Leaf u(); endmodule
        module Mark; endmodule module Observe #(parameter OK=0)(); if(OK) begin:ok Mark seen(); end endmodule
        bind Leaf Observe #(.OK(RESET.valid && RESET.tag==5 && S==p::Busy && WHO=="Bench.u" && TYPE_NAME==$typename(Config))) observe();"#;
    for rtl in [module.to_owned(), stub.emit(Default::default())] {
        assert_eq!(
            markers(
                "Bench",
                &[
                    Source::text(package),
                    Source::text(rtl),
                    Source::text(client)
                ]
            ),
            ["Bench.u.observe.ok.seen"]
        );
    }
}

#[test]
fn checking_a_stub_keeps_the_source_design_at_its_real_roots() {
    let source = r"module Peer; parameter N=4; endmodule
        module Leaf #(parameter W=4)(output q);
            assign q = $root.Leaf.W==$root.Peer.N;
        endmodule
        module Observer; endmodule
        bind Leaf: Leaf Observer here();";
    let inputs = [Source::text(source)];
    let library = VerilogLibrary::from_config(&ParserConfig {
        sources: &inputs,
        tops: &["Leaf", "Peer"],
        ..Default::default()
    })
    .unwrap();
    let stub = library.module("Leaf").unwrap();
    stub.set_usage(Usage::EmitStubAndStop);
    let rtl = stub.emit(Default::default());
    let client = r"module Bench; Leaf u(); endmodule module Mark; endmodule
        module Observe #(parameter V=0)(); if (V==4) begin:ok Mark seen(); end endmodule
        bind Leaf Observe #(.V(W)) obs();";
    assert_eq!(
        markers("Bench", &[Source::text(rtl), Source::text(client)]),
        ["Bench.u.obs.ok.seen"]
    );
}

#[test]
fn stub_checks_still_diagnose_a_default_changed_by_flattening_a_port() {
    let stub = leaf(
        "module Leaf(output logic [1:0][3:0] q); parameter OUTER=$left(q); endmodule",
        &[],
    );
    stub.set_usage(Usage::EmitStubAndStop);
    let error = catch_unwind(AssertUnwindSafe(|| stub.emit(Default::default()))).unwrap_err();
    assert!(
        error
            .downcast_ref::<String>()
            .unwrap()
            .contains("changes the value or type of Leaf.OUTER")
    );
}

#[test]
fn rust_overrides_keep_their_inferred_unsigned_type_in_a_shared_stub() {
    let base = leaf("module Leaf #(parameter MODE=0, SCALE=1)(); endmodule", &[]);
    let a = base.parameterize(&[("SCALE", 4), ("MODE", 0)]);
    let b = base.parameterize(&[("MODE", 1), ("SCALE", 8)]);
    a.set_usage(Usage::EmitStubAndStop);
    let assembly = ModDef::new("Assembly");
    assembly.instantiate(&a, Some("a"), None);
    assembly.instantiate(&b, Some("b"), None);
    let probe = r"module Mark; endmodule
        module Probe #(parameter OK=0)(); if (OK) begin:yes Mark seen(); end endmodule
        bind Leaf Probe #(.OK(MODE < -1 && ((MODE==0 && SCALE==4) || (MODE==1 && SCALE==8)))) p();";
    assert_eq!(
        markers(
            "Assembly",
            &[
                Source::text(assembly.emit(Default::default())),
                Source::text(probe)
            ]
        ),
        ["Assembly.a.p.yes.seen", "Assembly.b.p.yes.seen"]
    );
}

#[test]
fn observing_all_overrides_still_checks_unbound_localparams() {
    let first = leaf(
        "module Leaf #(parameter MODE=0, localparam OTHER=4)(); endmodule",
        &[],
    )
    .parameterize(&[("MODE", 1)]);
    let second = leaf(
        "module Leaf #(parameter MODE=0, localparam OTHER=8)(); endmodule",
        &[],
    )
    .parameterize(&[("MODE", 2)]);
    second.set_usage(Usage::EmitStubAndStop);
    let assembly = ModDef::new("Assembly");
    assembly.instantiate(&first, Some("a"), None);
    assembly.instantiate(&second, Some("b"), None);
    let error = catch_unwind(AssertUnwindSafe(|| assembly.emit(Default::default()))).unwrap_err();
    assert!(
        error
            .downcast_ref::<String>()
            .unwrap()
            .contains("changes the value or type of Leaf.OTHER")
    );
}

#[test]
fn opaque_imports_allow_escaped_internals_but_stub_declarations_reject_them() {
    for source in [
        r"module Leaf #(parameter \bad.name =0)(); endmodule",
        r"module Leaf #(parameter type \bad.type =logic)(); endmodule",
        r"module Leaf #(parameter W=1, localparam bad$name=2)(); endmodule",
        r"module Leaf(); parameter \bad.name =0; endmodule",
        r"package p; parameter integer \bad.name =3; endpackage module Leaf #(parameter W=p::\bad.name )(); endmodule",
    ] {
        let imported = leaf(source, &[]);
        let top = ModDef::new("Assembly");
        top.instantiate(&imported, Some("leaf"), None);
        assert!(top.emit(Default::default()).contains("Leaf leaf"));
        imported.set_usage(Usage::EmitStubAndStop);
        let error = catch_unwind(AssertUnwindSafe(|| top.emit(Default::default()))).unwrap_err();
        let error = error.downcast_ref::<String>().unwrap();
        assert!(error.contains("unescaped identifier"), "{error}");
    }
}

#[test]
fn copied_declarations_allow_system_names_and_escape_like_comment_or_string_text() {
    let source = r#"module Leaf #(parameter string TEXT="\\bad$name", /* \comment$name */ parameter W=$bits(4'b0011))(); endmodule"#;
    let imported = leaf(source, &[]);
    imported.set_usage(Usage::EmitStubAndStop);
    let rtl = imported.emit(Default::default());
    assert!(rtl.contains("$bits"));
    assert!(rtl.contains("bad$name"));
}
