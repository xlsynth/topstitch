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

#[test]
fn imports_inout_interfaces_without_losing_width_or_port_order() {
    let ir = r#"package pins

top block Contact(data: bits[4], pin: bits[4], observed: bits[4]) {
  ext_inout_net pad_net(bits[4])
  data: bits[4] = input_port(name=data, id=1)
  physical: () = ext_inout_port(name=pin, net=pad_net, id=2)
  sample: bits[4] = ext_inout_read(net=pad_net, id=3)
  observed: () = output_port(sample, name=observed, id=4)
}
"#;
    let contact = ModDef::from_block_ir("Contact", ir).unwrap();
    assert_eq!(
        contact
            .get_ports(None)
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>(),
        ["data", "pin", "observed"]
    );
    assert_eq!(contact.get_port("data").io(), IO::Input(4));
    assert_eq!(contact.get_port("pin").io(), IO::InOut(4));
    assert_eq!(contact.get_port("observed").io(), IO::Output(4));

    let top = ModDef::new("Upper");
    let instance = top.instantiate(&contact, Some("contact"), None);
    instance.get_port("data").export_as("data");
    instance.get_port("pin").export_as("pin");
    instance.get_port("observed").export_as("observed");
    let sv = top.emit(EmitOptions::default());
    assert!(sv.contains("inout wire [3:0] pin"), "{sv}");
    assert!(sv.contains(".pin(pin)"), "{sv}");
}

#[test]
fn links_a_clockless_imported_inout_block_into_block_ir_export() {
    let ir = r#"package pins

top block Contact(pin: bits[1]) {
  ext_inout_net pad_net(bits[1])
  physical: () = ext_inout_port(name=pin, net=pad_net, id=1)
}
"#;
    let contact = ModDef::from_block_ir("Contact", ir).unwrap();
    let top = ModDef::new("Upper");
    top.instantiate(&contact, Some("contact"), None)
        .get_port("pin")
        .export_as("pin");

    let package = top.to_block_ir().unwrap();
    assert!(package.get_block("Contact").is_some());
    let instance = &package.get_top_block().unwrap().instantiations[0];
    assert_eq!(instance.kind, xlsynth_pir::ir::InstantiationKind::Block);
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains("module Contact("), "{sv}");
    assert!(sv.contains(".pin(pin)"), "{sv}");
}

#[test]
fn exports_clocked_imported_roots_and_uses_opaque_children() {
    use xlsynth_pir::ir::{InstantiationKind, OpaqueExternPortKind, Type};

    let clocked = ModDef::from_block_ir("Increment", IR).unwrap();
    let package = clocked.to_block_ir().unwrap();
    assert_eq!(package.get_top_block().unwrap().name, "Increment");
    let leaf_sv = clocked.emit_sv_via_block_ir().unwrap();
    assert!(leaf_sv.contains("module Increment("), "{leaf_sv}");

    let top = ModDef::new("Upper");
    let child = top.instantiate(&clocked, Some("increment"), None);
    for port in child.get_ports(None) {
        port.export_as(port.name());
    }
    let package = top.to_block_ir().unwrap();
    assert!(package.get_block("Increment").is_none());
    let inst = &package.get_top_block().unwrap().instantiations[0];
    let InstantiationKind::OpaqueExternBlock { ports, .. } = &inst.kind else {
        panic!("expected opaque imported child, got {}", package);
    };
    assert_eq!(inst.rtl_module_name(), Some("Increment"));
    assert!(matches!(
        ports[0].kind,
        OpaqueExternPortKind::Input(Type::Bits(1))
    ));
    assert_eq!(ports[0].name, "clk");
    let wrapper_sv = top.emit_sv_via_block_ir().unwrap();
    assert!(!wrapper_sv.contains("module Increment("), "{wrapper_sv}");
    ModDef::from_verilog("Upper", format!("{leaf_sv}\n{wrapper_sv}")).unwrap();
}

#[test]
fn preserves_clock_and_inout_connections_on_an_opaque_imported_child() {
    use xlsynth_pir::ir::{InstantiationKind, NodePayload, OpaqueExternPortKind, Type};

    let ir = r#"package clocked_inout

top block Contact(clk: clock, pad: bits[1]) {
  ext_inout_net bus(bits[1])
  pad: () = ext_inout_port(name=pad, net=bus, id=1)
}
"#;
    let leaf = ModDef::from_block_ir("Contact", ir).unwrap();
    let top = ModDef::new("Upper");
    let child = top.instantiate(&leaf, Some("contact"), None);
    child.get_port("clk").export_as("clock_in");
    child.get_port("pad").export_as("pad");
    let package = top.to_block_ir().unwrap();
    let block = package.get_top_block().unwrap();
    assert!(block.clock_port_name().is_none());
    let InstantiationKind::OpaqueExternBlock { ports, .. } = &block.instantiations[0].kind else {
        panic!("expected opaque imported child");
    };
    assert!(matches!(
        ports[0].kind,
        OpaqueExternPortKind::Input(Type::Bits(1))
    ));
    assert!(matches!(
        ports[1].kind,
        OpaqueExternPortKind::Inout(Type::Bits(1))
    ));
    assert!(block.graph.nodes.iter().any(|node| matches!(
        &node.payload,
        NodePayload::InstantiationInput { instantiation, port_name, .. }
            if instantiation == "contact" && port_name == "clk"
    )));
    assert!(block.graph.nodes.iter().any(|node| matches!(
        &node.payload,
        NodePayload::ExtInstantiationInout { instantiation, port_name, .. }
            if instantiation == "contact" && port_name == "pad"
    )));
    let wrapper_sv = top.emit_sv_via_block_ir().unwrap();
    assert!(wrapper_sv.contains(".clk(clock_in)"), "{wrapper_sv}");
    assert!(wrapper_sv.contains(".pad(pad)"), "{wrapper_sv}");
    let leaf_sv = leaf.emit_sv_via_block_ir().unwrap();
    ModDef::from_verilog("Upper", format!("{leaf_sv}\n{wrapper_sv}")).unwrap();
}

#[test]
fn prunes_an_unused_clocked_source_member_before_binding_its_opaque_boundary() {
    let ir = "package shared\nblock Clockless() {}\nblock Clocked(clk: clock) {}";
    let library = BlockIrLibrary::from_ir(ir).unwrap();
    let top = ModDef::new("Upper");
    top.instantiate(&library.module("Clockless").unwrap(), Some("plain"), None);
    top.instantiate(&library.module("Clocked").unwrap(), Some("clocked"), None)
        .get_port("clk")
        .export_as("clk");
    let package = top.to_block_ir().unwrap();
    assert!(package.get_block("Clockless").is_some());
    assert!(package.get_block("Clocked").is_none());
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains("Clocked clocked"), "{sv}");
}

#[test]
fn rejects_linking_two_independently_imported_packages() {
    let first = ModDef::from_block_ir("First", "package first\ntop block First() {}").unwrap();
    let second = ModDef::from_block_ir("Second", "package second\ntop block Second() {}").unwrap();
    let top = ModDef::new("Upper");
    top.instantiate(&first, Some("first"), None);
    top.instantiate(&second, Some("second"), None);
    let error = format!("{:#}", top.to_block_ir().unwrap_err());
    assert!(
        error.contains("cannot link distinct imported Block IR packages"),
        "{error}"
    );
}

#[test]
fn deduplicates_blocks_imported_repeatedly_from_one_library() {
    let library =
        BlockIrLibrary::from_ir("package shared\nblock First() {}\ntop block Second() {}").unwrap();
    let first = library.module("First").unwrap();
    let another_first = library.module("First").unwrap();
    let second = library.module("Second").unwrap();
    let top = ModDef::new("Upper");
    top.instantiate(&first, Some("a"), None);
    top.instantiate(&another_first, Some("b"), None);
    top.instantiate(&second, Some("c"), None);

    let package = top.to_block_ir().unwrap();
    assert_eq!(package.members.len(), 3);
    assert!(package.get_block("First").is_some());
    assert!(package.get_block("Second").is_some());
    assert_eq!(package.get_top_block().unwrap().instantiations.len(), 3);
}

#[test]
fn imports_reusable_opaque_and_behavioral_external_interfaces() {
    use xlsynth_pir::ir::InstantiationKind;

    for declaration in [
        "instantiation vendor(block=Model, module_name=Vendor, parameters={WIDTH:\"1 << 2\"}, kind=behavioral_extern_block)",
        "instantiation vendor(module_name=Vendor, ports={pin:inout bits[1]}, parameters={WIDTH:\"1 << 2\"}, kind=opaque_extern_block)",
    ] {
        let ir = format!(
            "package external\n\nblock Model(pin: bits[1]) {{\n  ext_inout_net model_pin(bits[1])\n  port: () = ext_inout_port(name=pin, net=model_pin, id=1)\n}}\n\ntop block Root(pin: bits[1]) {{\n  ext_inout_net root_pin(bits[1])\n  {declaration}\n  port: () = ext_inout_port(name=pin, net=root_pin, id=2)\n  child: () = ext_instantiation_inout(instantiation=vendor, port_name=pin, net=root_pin, id=3)\n}}\n"
        );
        let boundary = BlockIrLibrary::from_ir(&ir).unwrap();
        assert!(boundary.extern_calls("Root").unwrap().is_empty());
        let explicit = boundary.extern_module("Root", "vendor").unwrap();

        let hierarchy = BlockIrLibrary::from_config(&BlockIrParserConfig {
            ir: &ir,
            include_hierarchy: true,
        })
        .unwrap();
        let root = hierarchy.module("Root").unwrap();
        assert_eq!(
            root.report_all_instances(),
            [("Vendor".to_owned(), "Root.vendor".to_owned())]
        );
        let nested = root.get_instance("vendor").get_mod_def();
        for vendor in [explicit, nested] {
            assert_eq!(vendor.get_name(), "Vendor");
            assert_eq!(vendor.get_port("pin").io(), IO::InOut(1));
            assert_eq!(vendor.get_usage(), Usage::EmitNothingAndStop);

            let top = ModDef::new("Standalone");
            top.instantiate(&vendor, Some("vendor"), None)
                .get_port("pin")
                .export_as("pin");
            let package = top.to_block_ir().unwrap();
            let instance = &package.get_top_block().unwrap().instantiations[0];
            let parameters = match &instance.kind {
                InstantiationKind::BehavioralExternBlock { parameters, .. } => {
                    assert!(declaration.contains("behavioral_extern_block"));
                    assert!(package.get_block("Model").is_some());
                    parameters
                }
                InstantiationKind::OpaqueExternBlock { parameters, .. } => {
                    assert!(declaration.contains("opaque_extern_block"));
                    parameters
                }
                other => panic!("expected external block, got {other:?}"),
            };
            assert_eq!(parameters["WIDTH"], "1 << 2");
            assert_eq!(instance.rtl_module_name(), Some("Vendor"));
            let vendor_sv = "module Vendor #(parameter WIDTH = 1) (inout wire pin); endmodule";
            for rtl in [
                top.emit(EmitOptions::default()),
                top.emit_sv_via_block_ir().unwrap(),
            ] {
                assert!(rtl.contains(".WIDTH(1 << 2)"), "{rtl}");
                ModDef::from_verilog("Standalone", format!("{vendor_sv}\n{rtl}")).unwrap();
            }
        }
        let original = root.to_block_ir().unwrap().to_string();
        assert!(
            original.contains("parameters={WIDTH:\"1 << 2\"}"),
            "{original}"
        );
    }
}

#[test]
fn preserves_unwireable_external_interfaces_as_hierarchy_only() {
    let ir = r#"package hierarchy_only

top block Root() {
  instantiation unsupported(module_name=AggregateVendor, ports={data:output (bits[1], bits[1])}, kind=opaque_extern_block)
  instantiation supported(module_name=EmptyVendor, ports={}, kind=opaque_extern_block)
  data: (bits[1], bits[1]) = instantiation_output(instantiation=unsupported, port_name=data, id=1)
}"#;
    let library = BlockIrLibrary::from_config(&BlockIrParserConfig {
        ir,
        include_hierarchy: true,
    })
    .unwrap();
    let root = library.module("Root").unwrap();
    assert_eq!(
        root.report_all_instances(),
        [
            ("AggregateVendor".to_owned(), "Root.unsupported".to_owned()),
            ("EmptyVendor".to_owned(), "Root.supported".to_owned()),
        ]
    );
    let unsupported = root.get_instance("unsupported").get_mod_def();
    assert!(unsupported.get_ports(None).is_empty());
    assert_eq!(unsupported.get_usage(), Usage::EmitNothingAndStop);
    let error = library.extern_module("Root", "unsupported").err().unwrap();
    assert!(format!("{error:#}").contains("unsupported XLS port type"));
    let top = ModDef::new("Top");
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            top.instantiate(&unsupported, Some("bad"), None);
        }))
        .is_err()
    );
    top.instantiate(
        &root.get_instance("supported").get_mod_def(),
        Some("ok"),
        None,
    );
    assert_eq!(top.get_instances().len(), 1);
}

#[test]
fn rejects_external_sv_keywords_at_the_import_boundary() {
    for (declaration, node, expected) in [
        (
            "module_name=always, ports={}",
            "",
            "cannot instantiate external module 'always'",
        ),
        (
            "module_name=Vendor, ports={wire:output bits[1]}",
            "value: bits[1] = instantiation_output(instantiation=vendor, port_name=wire, id=1)",
            "cannot connect external port 'Vendor.wire'",
        ),
        (
            r#"module_name=Vendor, ports={}, parameters={wire:"1"}"#,
            "",
            "cannot bind parameter Vendor.wire",
        ),
    ] {
        let ir = format!(
            "package invalid_names\ntop block Root() {{\ninstantiation vendor({declaration}, kind=opaque_extern_block)\n{node}\n}}"
        );
        let library = BlockIrLibrary::from_ir(&ir).unwrap();
        let error = library.extern_module("Root", "vendor").err().unwrap();
        let message = format!("{error:#}");
        assert!(message.contains(expected), "{message}");
        assert!(
            message.contains("reserved SystemVerilog keyword"),
            "{message}"
        );
    }
}

#[test]
fn keeps_parameterized_external_specializations_separate() {
    use xlsynth_pir::ir::InstantiationKind;

    let ir = r#"package variants

top block Root() {
  ext_inout_net narrow(bits[4])
  ext_inout_net wide(bits[8])
  instantiation a(module_name=Vendor, ports={pin:inout bits[4]}, parameters={WIDTH:"4"}, kind=opaque_extern_block)
  instantiation b(module_name=Vendor, ports={pin:inout bits[8]}, parameters={WIDTH:"8"}, kind=opaque_extern_block)
  a_pin: () = ext_instantiation_inout(instantiation=a, port_name=pin, net=narrow, id=1)
  b_pin: () = ext_instantiation_inout(instantiation=b, port_name=pin, net=wide, id=2)
}"#;
    let library = BlockIrLibrary::from_ir(ir).unwrap();
    let small = library.extern_module("Root", "a").unwrap();
    let large = library.extern_module("Root", "b").unwrap();
    assert_eq!(small.get_name(), large.get_name());
    assert_eq!(small.get_port("pin").io(), IO::InOut(4));
    assert_eq!(large.get_port("pin").io(), IO::InOut(8));
    let top = ModDef::new("Assembly");
    top.instantiate(&small, Some("narrow_device"), None)
        .get_port("pin")
        .export_as("small_pin");
    top.instantiate(&large, Some("wide_device"), None)
        .get_port("pin")
        .export_as("large_pin");
    let package = top.to_block_ir().unwrap();
    let instances = &package.get_top_block().unwrap().instantiations;
    for (instance, width) in instances.iter().zip(["4", "8"]) {
        let InstantiationKind::OpaqueExternBlock { parameters, .. } = &instance.kind else {
            panic!("expected opaque external specialization")
        };
        assert_eq!(parameters["WIDTH"], width);
    }
    let vendor = "module Vendor #(parameter WIDTH = 1) (inout wire [WIDTH-1:0] pin); endmodule";
    for rtl in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir().unwrap(),
    ] {
        assert!(
            rtl.contains(".WIDTH(4)") && rtl.contains(".WIDTH(8)"),
            "{rtl}"
        );
        ModDef::from_verilog("Assembly", format!("{vendor}\n{rtl}")).unwrap();
    }
}

#[test]
fn imports_external_clock_and_enum_input_cast() {
    let ir = r#"package external_clock

top block Root(clk: clock, mode: bits[2], pad: bits[1], active: bits[1]) {
  ext_inout_net pad_net(bits[1])
  instantiation vendor(module_name=Vendor, ports={clk:clock, mode:input bits[2], pad:inout bits[1], active:output bits[1]}, parameters={ENABLE:"1'b1"}, input_casts={mode:"types::mode_t"}, kind=opaque_extern_block)
  mode: bits[2] = input_port(name=mode, id=1)
  pad: () = ext_inout_port(name=pad, net=pad_net, id=2)
  mode_input: () = instantiation_input(mode, instantiation=vendor, port_name=mode, id=3)
  connected_pad: () = ext_instantiation_inout(instantiation=vendor, port_name=pad, net=pad_net, id=4)
  value: bits[1] = instantiation_output(instantiation=vendor, port_name=active, id=5)
  active: () = output_port(value, name=active, id=6)
}"#;
    let vendor = BlockIrLibrary::from_ir(ir)
        .unwrap()
        .extern_module("Root", "vendor")
        .unwrap();
    assert_eq!(vendor.get_port("clk").io(), IO::Input(1));
    assert_eq!(vendor.get_port("mode").io(), IO::Input(2));
    assert_eq!(vendor.get_port("pad").io(), IO::InOut(1));
    assert_eq!(vendor.get_port("active").io(), IO::Output(1));
    let top = vendor.wrap(Some("Standalone"), Some("vendor"));
    let supplied = r#"package types; typedef enum logic [1:0] {Idle, Busy} mode_t; endpackage
module Vendor #(parameter ENABLE = 0) (input clk, input types::mode_t mode, inout wire pad, output active);
assign active = ENABLE && mode == types::Busy; endmodule"#;
    for rtl in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir().unwrap(),
    ] {
        assert!(rtl.contains(".ENABLE(1'b1)"), "{rtl}");
        assert!(rtl.contains("types::mode_t'(mode)"), "{rtl}");
        ModDef::from_verilog("Standalone", format!("{supplied}\n{rtl}")).unwrap();
    }
}

#[test]
fn clocked_behavioral_external_falls_back_to_opaque_binding() {
    use xlsynth_pir::ir::InstantiationKind;

    let ir = r#"package clocked_external
block Model(clk: clock, d: bits[1], q: bits[1]) {
  d: bits[1] = input_port(name=d, id=1)
  q: () = output_port(d, name=q, id=2)
}
top block Root(clk: clock, d: bits[1], q: bits[1]) {
  instantiation vendor(block=Model, module_name=Vendor, kind=behavioral_extern_block)
  d: bits[1] = input_port(name=d, id=3)
  connected: () = instantiation_input(d, instantiation=vendor, port_name=d, id=4)
  value: bits[1] = instantiation_output(instantiation=vendor, port_name=q, id=5)
  q: () = output_port(value, name=q, id=6)
}"#;
    let vendor = BlockIrLibrary::from_ir(ir)
        .unwrap()
        .extern_module("Root", "vendor")
        .unwrap();
    let top = vendor.wrap(Some("Standalone"), Some("vendor"));
    let package = top.to_block_ir().unwrap();
    assert!(package.get_block("Model").is_none());
    assert!(matches!(
        package.get_top_block().unwrap().instantiations[0].kind,
        InstantiationKind::OpaqueExternBlock { .. }
    ));
    let supplied = "module Vendor(input clk, input d, output q); assign q=d; endmodule";
    for rtl in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir().unwrap(),
    ] {
        ModDef::from_verilog("Standalone", format!("{supplied}\n{rtl}")).unwrap();
    }
}

#[test]
fn rejects_non_external_or_missing_instance_requests() {
    let ir = r#"package requests
fn foreign() -> bits[1] { ret value: bits[1] = literal(value=0, id=1) }
block Plain() {}
top block Root() {
  instantiation native(block=Plain, kind=block)
  instantiation ffi(foreign_function=foreign, kind=extern)
}"#;
    let library = BlockIrLibrary::from_ir(ir).unwrap();
    for (block, instance) in [
        ("Missing", "ffi"),
        ("Root", "missing"),
        ("Root", "native"),
        ("Root", "ffi"),
    ] {
        assert!(
            library.extern_module(block, instance).is_err(),
            "{block}.{instance}"
        );
    }
}

#[test]
#[should_panic(expected = "frozen")]
fn imported_external_interface_is_frozen() {
    let ir = "package external\ntop block Root() { instantiation vendor(module_name=Vendor, ports={}, kind=opaque_extern_block) }";
    let vendor = BlockIrLibrary::from_ir(ir)
        .unwrap()
        .extern_module("Root", "vendor")
        .unwrap();
    vendor.add_port("extra", IO::Output(1));
}

#[test]
fn imports_typed_inout_and_preserves_its_sv_type_in_native_ir() {
    let ir = r#"package typed

top block Contact(pin: bits[2]) {
  ext_inout_net bus(bits[2])
  pin: () = ext_inout_port(name=pin, net=bus, sv_type="types::mode_t", id=1)
}
"#;
    let contact = ModDef::from_block_ir("Contact", ir).unwrap();
    assert_eq!(contact.get_port("pin").io(), IO::InOut(2));
    let upper = ModDef::new("Upper");
    upper
        .instantiate(&contact, Some("contact"), None)
        .get_port("pin")
        .export_as("pin");
    let package = upper.to_block_ir().unwrap();
    assert!(package.to_string().contains("sv_type=\"types::mode_t\""));
    let rtl = upper.emit_sv_via_block_ir().unwrap();
    let package_sv = "package types; typedef enum logic [1:0] {Idle, Busy} mode_t; endpackage";
    ModDef::from_verilog("Upper", format!("{package_sv}\n{rtl}")).unwrap();
}

#[test]
fn reexports_imported_opaque_external_with_sliced_inout_binding_losslessly() {
    let ir = r#"package external_slices

top block Root(pad: bits[8]) {
  ext_inout_net bus(bits[8])
  instantiation vendor(module_name=Vendor, ports={io:inout bits[4]}, parameters={WIDTH:"4"}, kind=opaque_extern_block)
  pad: () = ext_inout_port(name=pad, net=bus, id=1)
  io: () = ext_instantiation_inout(instantiation=vendor, port_name=io, net={bus[7:6], bus[1:0]}, id=2)
}
"#;
    let root = ModDef::from_block_ir("Root", ir).unwrap();
    let reexported = root.to_block_ir().unwrap();
    let text = reexported.to_string();
    assert!(text.contains("parameters={WIDTH:\"4\"}"), "{text}");
    assert!(text.contains("net={bus[7:6], bus[1:0]}"), "{text}");
    let rtl = root.emit_sv_via_block_ir().unwrap();
    assert!(rtl.contains(".WIDTH(4)"), "{rtl}");
    assert!(rtl.contains(".io({pad[7:6], pad[1:0]})"), "{rtl}");
}
