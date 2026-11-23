// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "block-ir")]

use topstitch::{BlockIrLibrary, ModDef};
use xlsynth_pir::ir::{InstantiationKind, NodePayload};

const MODELED_IR: &str = r#"package vendor_library
fn invert(x: bits[1] id=1) -> bits[1] {
  ret result: bits[1] = not(x, id=2)
}
block Vendor(d: bits[1], y: bits[1]) {
  d: bits[1] = input_port(name=d, id=3)
  result: bits[1] = invoke(d, to_apply=invert, id=4)
  y: () = output_port(result, name=y, id=5)
}
top block Root(d: bits[1], y: bits[1]) {
  instantiation vendor(block=Vendor, module_name=Vendor, kind=behavioral_extern_block)
  d: bits[1] = input_port(name=d, id=6)
  connected: () = instantiation_input(d, instantiation=vendor, port_name=d, id=7)
  value: bits[1] = instantiation_output(instantiation=vendor, port_name=y, id=8)
  y: () = output_port(value, name=y, id=9)
}"#;

/// Replacing the original source root must retain only the behavioral model and its dependencies.
#[test]
fn reuses_source_root_name_and_retains_behavioral_model() {
    let ir = MODELED_IR;
    let library = BlockIrLibrary::from_ir(ir).unwrap();
    let vendor = library.extern_module("Root", "vendor").unwrap();
    let root = vendor.wrap(Some("Root"), Some("vendor"));
    let package = root.to_block_ir().unwrap();
    let model = package.get_block("Vendor").unwrap();
    assert!(
        model
            .nodes
            .iter()
            .any(|node| matches!(node.payload, NodePayload::Invoke { .. }))
    );
    assert!(package.get_fn("invert").is_some());
    assert_eq!(package.members.len(), 3);
    assert!(matches!(
        package.get_top_block().unwrap().instantiations[0].kind,
        InstantiationKind::BehavioralExternBlock { .. }
    ));
    let sv = root.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains("Vendor vendor"), "{sv}");
    assert!(!sv.contains("module Vendor"), "{sv}");
}

/// Native imported children use the same dependency closure as external models.
#[test]
fn reuses_source_root_name_with_native_child() {
    let ir = "package native_library\nblock Native() {}\ntop block Root() { instantiation child(block=Native, kind=block) }";
    let library = BlockIrLibrary::from_ir(ir).unwrap();
    let child = library.module("Native").unwrap();
    let root = child.wrap(Some("Root"), Some("child"));
    let package = root.to_block_ir().unwrap();
    assert_eq!(package.members.len(), 2);
    assert!(package.get_block("Native").is_some());
}

/// A separately supplied RTL module cannot share its name with an actually emitted block.
#[test]
fn rejects_external_name_matching_an_emitted_native_child() {
    let ir = r#"package ambiguous
block Vendor() {}
block Model() {}
top block Source() {
  instantiation native(block=Vendor, kind=block)
  instantiation external(block=Model, module_name=Vendor, kind=behavioral_extern_block)
}"#;
    let library = BlockIrLibrary::from_ir(ir).unwrap();
    let root = ModDef::new("Root");
    root.instantiate(&library.module("Vendor").unwrap(), Some("native"), None);
    root.instantiate(
        &library.extern_module("Source", "external").unwrap(),
        Some("external"),
        None,
    );
    let error = root.to_block_ir().unwrap_err().to_string();
    assert!(
        error.contains("conflicts with an emitted Block IR block"),
        "{error}"
    );
}

/// BlockBuilder reserves function names as well as imported block names.
#[test]
fn reports_generated_block_conflicting_with_retained_function() {
    let ir = MODELED_IR.replace("invert", "Root");
    let library = BlockIrLibrary::from_ir(&ir).unwrap();
    let vendor = library.extern_module("Root", "vendor").unwrap();
    let root = vendor.wrap(Some("Root"), Some("vendor"));
    let error = root.to_block_ir().unwrap_err().to_string();
    assert!(
        error.contains("imported Block IR member Root conflicts with a generated module"),
        "{error}"
    );
}
