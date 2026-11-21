// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "block-ir")]

use topstitch::{
    BlockIrExternCall, BlockIrLibrary, BlockIrParserConfig, EmitOptions, IO, ModDef, ParserConfig,
    Source, Usage, VerilogLibrary,
};

// XLS emits both fixtures for the one-stage DSLX function `data + u8:1`.
const IR: &str = include_str!("fixtures/block_ir/increment.block.ir");
const SV: &str = include_str!("fixtures/block_ir/increment.sv");

#[test]
fn instantiates_an_ir_interface_with_a_separately_supplied_sv_definition() {
    let child = ModDef::from_block_ir("Increment", IR).unwrap();
    assert_eq!(child.get_usage(), Usage::EmitNothingAndStop);
    assert_eq!(
        child
            .get_ports(None)
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>(),
        ["clk", "data", "out"]
    );
    assert!(matches!(child.get_port("clk").io(), IO::Input(1)));
    assert!(matches!(child.get_port("data").io(), IO::Input(8)));
    assert!(matches!(child.get_port("out").io(), IO::Output(8)));

    let top = ModDef::new("Top");
    let clk = top.add_port("clk", IO::Input(1));
    let input = top.add_port("data", IO::Input(8));
    let output = top.add_port("result", IO::Output(8));
    let first = top.instantiate(&child, Some("first"), None);
    let second = top.instantiate(&child, Some("second"), None);
    first.get_port("clk").connect(&clk);
    first.get_port("data").connect(&input);
    second.get_port("clk").connect(&clk);
    second.get_port("data").connect(&first.get_port("out"));
    second.get_port("out").connect(&output);
    let wrapper = top.emit(EmitOptions::default());
    assert!(!wrapper.contains("module Increment"));

    let sources = [Source::text(SV), Source::text(wrapper)];
    let lib = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        tops: &["Top"],
        include_hierarchy: true,
        ignore_unknown_modules: false,
        ..Default::default()
    })
    .unwrap();
    let mut instances = lib.module("Top").unwrap().report_all_instances();
    instances.sort();
    assert_eq!(
        instances,
        [
            ("Increment".into(), "Top.first".into()),
            ("Increment".into(), "Top.second".into())
        ]
    );
}

#[test]
fn selects_the_named_block_even_when_it_is_not_top() {
    let ir = format!(
        "{IR}\nblock Small(flag: bits[1]) {{ flag: bits[1] = input_port(name=flag, id=101) }}"
    );
    let small = ModDef::from_block_ir("Small", &ir).unwrap();
    assert_eq!(small.get_ports(None).len(), 1);
    assert!(matches!(small.get_port("flag").io(), IO::Input(1)));
    assert!(
        ModDef::from_block_ir("Missing", &ir)
            .err()
            .unwrap()
            .to_string()
            .contains("no block named 'Missing'")
    );
}

#[test]
fn rejects_malformed_ir() {
    assert!(ModDef::from_block_ir("Increment", "not an XLS package").is_err());
}

#[test]
fn rejects_ports_without_a_supported_sv_representation() {
    for (ty, annotation, expected) in [
        ("bits[0]", "", "zero-width"),
        ("(bits[4], bits[4])", "", "unsupported XLS port type"),
        ("bits[4][2]", "", "unsupported XLS port type"),
        (
            "bits[8]",
            ", sv_type=\"p::color_t\"",
            "unsupported SV type override",
        ),
    ] {
        let ir = format!(
            "package example\ntop block Sink(data: {ty}) {{ data: {ty} = input_port(name=data{annotation}, id=1) }}"
        );
        let error = ModDef::from_block_ir("Sink", &ir)
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("Sink.data") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn rejects_names_that_cannot_be_instantiated_verbatim() {
    let reserved_port =
        "package p\ntop block Sink(wire: bits[1]) { wire: bits[1] = input_port(name=wire, id=1) }";
    let error = ModDef::from_block_ir("Sink", reserved_port).err().unwrap();
    assert!(format!("{error:#}").contains("Sink.wire"));
    let error = ModDef::from_block_ir("always", "package p\ntop block always() {}")
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("not a simple SystemVerilog identifier"));
}

#[test]
#[should_panic(expected = "frozen")]
fn imported_interface_is_frozen() {
    let child = ModDef::from_block_ir("Increment", IR).unwrap();
    child.add_port("extra", IO::Output(1));
}

#[test]
fn imports_declared_block_hierarchy_and_external_references() {
    let ir = r#"package hierarchy

#[ffi_proto("""code_template: "OtherModule {fn} ();"
""")]
fn foreign_leaf() -> bits[1] {
  ret value: bits[1] = literal(value=0, id=1)
}

block Leaf() {}
block Branch() {
  instantiation leaf(block=Leaf, kind=block)
  instantiation external(foreign_function=foreign_leaf, kind=extern)
}
top block Root(flag: bits[1]) {
  instantiation a(block=Branch, kind=block)
  instantiation b(block=Branch, kind=block)
  flag: bits[1] = input_port(name=flag, id=2)
}
"#;
    let boundary = BlockIrLibrary::from_ir(ir).unwrap();
    assert!(boundary.module("Root").unwrap().get_instances().is_empty());
    let calls = boundary.extern_calls("Root").unwrap();
    assert_eq!(
        calls,
        ["Root.a.external", "Root.b.external"].map(|path| BlockIrExternCall {
            path: path.to_owned(),
            function: "foreign_leaf",
            sv_template: Some(b"OtherModule {fn} ();"),
        })
    );
    assert!(boundary.extern_calls("Leaf").unwrap().is_empty());
    assert!(boundary.extern_calls("Missing").is_err());

    let library = BlockIrLibrary::from_config(&BlockIrParserConfig {
        ir,
        include_hierarchy: true,
    })
    .unwrap();
    assert_eq!(library.module_names(), ["Leaf", "Branch", "Root"]);
    assert_eq!(library.extern_calls("Root").unwrap(), calls);
    let root = library.module("Root").unwrap();
    assert!(matches!(root.get_port("flag").io(), IO::Input(1)));
    assert_eq!(
        root.report_all_instances(),
        [
            ("Leaf", "Root.a.leaf"),
            ("Branch", "Root.a"),
            ("Leaf", "Root.b.leaf"),
            ("Branch", "Root.b"),
        ]
        .map(|(name, path)| (name.to_owned(), path.to_owned()))
    );
    assert!(
        root.get_instance("a")
            .get_mod_def()
            .get_ports(None)
            .is_empty()
    );
    assert_eq!(root.get_usage(), Usage::EmitNothingAndStop);

    let wrapper = ModDef::new("Wrapper");
    let flag = wrapper.add_port("flag", IO::Input(1));
    wrapper
        .instantiate(&root, Some("root"), None)
        .get_port("flag")
        .connect(&flag);
    let sv = wrapper.emit(EmitOptions::default());
    assert!(sv.contains("Root root ("));
    assert!(!sv.contains("module Root"));
    assert!(!sv.contains("module Branch"));
}

#[test]
fn preserves_present_empty_and_absent_extern_templates() {
    let ir = r#"package templates
#[ffi_proto("""code_template: '\377'""")]
fn binary() -> bits[1] { ret b: bits[1] = literal(value=0, id=1) }
#[ffi_proto("""code_template: ''""")]
fn empty() -> bits[1] { ret e: bits[1] = literal(value=0, id=2) }
fn unspecified() -> bits[1] { ret u: bits[1] = literal(value=0, id=3) }
block Root() {
  instantiation a(foreign_function=binary, kind=extern)
  instantiation b(foreign_function=empty, kind=extern)
  instantiation c(foreign_function=unspecified, kind=extern)
}"#;
    let library = BlockIrLibrary::from_ir(ir).unwrap();
    let calls = library.extern_calls("Root").unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|call| call.sv_template)
            .collect::<Vec<_>>(),
        [Some(&[0xff][..]), Some(&[][..]), None]
    );
}

#[test]
fn bulk_import_shares_nested_definitions_and_preserves_root_interfaces() {
    let ir = "package shared
block Leaf(data: bits[1]) { data: bits[1] = input_port(name=data, id=1) }
block Branch() {
  instantiation leaf(block=Leaf, kind=block)
  value: bits[1] = literal(value=0, id=2)
  drive: () = instantiation_input(value, instantiation=leaf, port_name=data, id=3)
}
block Root() { instantiation branch(block=Branch, kind=block) }";
    let library = BlockIrLibrary::from_config(&BlockIrParserConfig {
        ir,
        include_hierarchy: true,
    })
    .unwrap();
    let modules = library.modules().unwrap();
    let [leaf, branch, root] = modules.as_slice() else {
        panic!("expected three blocks")
    };
    assert!(matches!(leaf.get_port("data").io(), IO::Input(1)));
    let nested_leaf = branch.get_instance("leaf").get_mod_def();
    assert!(nested_leaf.get_ports(None).is_empty());
    nested_leaf.set_metadata("tag", "shared");
    let from_root = root
        .get_instance("branch")
        .get_instance("leaf")
        .get_mod_def();
    assert_eq!(from_root.get_metadata("tag"), Some("shared".into()));
    assert!(leaf.get_metadata("tag").is_none());
    let fresh = library.module("Branch").unwrap();
    assert!(
        fresh
            .get_instance("leaf")
            .get_mod_def()
            .get_metadata("tag")
            .is_none()
    );
}
