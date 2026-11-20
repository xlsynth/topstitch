// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "block-ir")]

use num_bigint::BigUint;
use topstitch::{IO, ModDef, Usage};
use xlsynth_pir::IrValue;
use xlsynth_pir::block_inline::inline_all_blocks_in_package;
use xlsynth_pir::block2fn::combinational_block_to_fn;
use xlsynth_pir::ir::BlockPort;
use xlsynth_pir::ir_eval::eval_fn_in_package_value;
use xlsynth_pir::ir_parser::Parser;

/// Uses the same four-bit feedthrough twice. One output drives both a module
/// port and two non-adjacent slices of the other instance's input.
fn routed_pair(upper_nibble: bool) -> ModDef {
    let pass = ModDef::new("Pass");
    let y = pass.add_port("y", IO::Output(4));
    let x = pass.add_port("x", IO::Input(4));
    y.connect(&x);

    let top = ModDef::new("RoutedPair");
    let tap = top.add_port("tap", IO::Output(4));
    let word = top.add_port("word", IO::Input(8));
    let result = top.add_port("result", IO::Output(4));
    let sink = top.instantiate(&pass, Some("sink"), None);
    let source = top.instantiate(&pass, Some("producer"), None);

    let (upper, lower) = (word.slice(7, 4), word.slice(3, 0));
    let (selected, unused) = if upper_nibble {
        (upper, lower)
    } else {
        (lower, upper)
    };
    unused.unused();
    source.get_port("x").connect(&selected);
    let value = source.get_port("y");
    value.connect(&tap);
    let input = sink.get_port("x");
    input.slice(3, 2).connect(&value.slice(1, 0));
    input.bit(1).tieoff(1);
    input.bit(0).connect(&value.bit(3));
    sink.get_port("y").specify_net_name("selected");
    sink.get_port("y").connect(&result);
    top
}

#[test]
fn shared_definition_fanout_and_slices() {
    let pkg = routed_pair(true).to_block_ir().unwrap();
    assert_eq!(pkg.members.len(), 2);
    let block = pkg.get_top_block().unwrap();
    assert_eq!(block.name, "RoutedPair");
    assert_eq!(block.instantiations.len(), 2);
    let ports: Vec<_> = block
        .ports
        .iter()
        .map(|p| match p {
            BlockPort::Input(n) | BlockPort::Output(n) => block.port_name(*n),
            BlockPort::Clock(_) => panic!("unexpected clock"),
        })
        .collect();
    assert_eq!(ports, ["tap", "word", "result"]);

    let mut parsed = Parser::new(&pkg.to_string())
        .parse_and_validate_package()
        .unwrap();
    inline_all_blocks_in_package(&mut parsed).unwrap();
    let function = combinational_block_to_fn(parsed.get_top_block().unwrap()).unwrap();
    for word in 0..256 {
        let input = IrValue::make_ubits(8, word).unwrap();
        let outputs = eval_fn_in_package_value(&parsed, &function, &[input]);
        let tap = word >> 4;
        let result = ((tap & 3) << 2) | 2 | (tap >> 3);
        assert!(outputs.get_element(0).unwrap().bits_equals_u64_value(tap));
        assert!(
            outputs
                .get_element(1)
                .unwrap()
                .bits_equals_u64_value(result)
        );
    }
}

#[test]
fn arbitrary_width_tieoff() {
    let top = ModDef::new("Constant");
    let value = (BigUint::from(1u8) << 69usize) | BigUint::from(3u8);
    top.add_port("value", IO::Output(77)).tieoff(value);
    let pkg = top.to_block_ir().unwrap();
    let function = combinational_block_to_fn(pkg.get_top_block().unwrap()).unwrap();
    let v = eval_fn_in_package_value(&pkg, &function, &[]);
    assert_eq!(
        v.to_bits().unwrap().to_le_bytes(),
        [3, 0, 0, 0, 0, 0, 0, 0, 32, 0]
    );
}

#[test]
fn reports_unconnected_or_multiply_driven_slices() {
    let top = ModDef::new("Gap");
    let a = top.add_port("a", IO::Input(3));
    let y = top.add_port("y", IO::Output(4));
    y.slice(3, 1).connect(&a);
    assert!(format!("{:#}", top.to_block_ir().unwrap_err()).contains("Gap.y[0] is unconnected"));
    y.bit(0).tieoff(1);
    y.bit(0).tieoff(0);
    assert!(
        format!("{:#}", top.to_block_ir().unwrap_err())
            .contains("Gap.y[0:0] has been tied off multiple times")
    );
}

#[test]
fn both_emitters_reject_a_tieoff_and_a_port_driving_the_same_slice() {
    let top = ModDef::new("Conflict");
    let sink = top.add_port("y", IO::Output(4));
    let source = top.add_port("x", IO::Input(4));
    sink.connect(&source);
    sink.bit(2).tieoff(1);

    let expected = "is multiply driven by a tieoff and Conflict.x[2:2]";
    let ir_error = top.to_block_ir().unwrap_err();
    assert!(format!("{ir_error:#}").contains(expected), "{ir_error:#}");
    let sv_error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        top.emit(topstitch::EmitOptions::default())
    }))
    .unwrap_err();
    assert!(
        sv_error
            .downcast_ref::<String>()
            .unwrap()
            .contains(expected)
    );
}

#[test]
fn external_instances_preserve_the_boundary() {
    let top = ModDef::new("Wrapper");
    let cell = ModDef::new("Cell");
    cell.set_usage(Usage::EmitStubAndStop);
    top.instantiate(&cell, Some("leaf"), None);
    let pkg = top.to_block_ir().unwrap();
    let parsed = Parser::new(&pkg.to_string())
        .parse_and_validate_package()
        .unwrap();
    let inst = &parsed.get_top_block().unwrap().instantiations[0];
    assert_eq!(inst.kind, xlsynth_pir::ir::InstantiationKind::Extern);
    assert!(parsed.get_fn(&inst.block).unwrap().ffi_proto().is_some());
    assert!(
        cell.to_block_ir()
            .unwrap_err()
            .to_string()
            .contains("export a parent")
    );

    let conflict = ModDef::new("Cell");
    conflict.instantiate(&cell, Some("leaf"), None);
    assert!(
        format!("{:#}", conflict.to_block_ir().unwrap_err())
            .contains("both an external implementation and a generated block")
    );
}

#[test]
fn rejects_inout_ports_on_native_modules_and_external_children() {
    let native = ModDef::new("Native");
    native.add_port("pad", IO::InOut(1)).unused();
    for error in [
        native.to_block_ir().unwrap_err().to_string(),
        native.emit_sv_via_block_ir().unwrap_err().to_string(),
    ] {
        assert!(error.contains("Native.pad"), "{error}");
        assert!(error.contains("direct SystemVerilog"), "{error}");
    }
    assert!(native.emit(Default::default()).contains("inout wire pad"));

    for usage in [
        Usage::EmitDefinitionAndDescend,
        Usage::EmitNothingAndStop,
        Usage::EmitStubAndStop,
    ] {
        let leaf = ModDef::new("Contact");
        leaf.add_port("pad", IO::InOut(1)).unused();
        leaf.set_usage(usage);
        let top = ModDef::new("Pair");
        let a = top.instantiate(&leaf, Some("a"), None);
        let b = top.instantiate(&leaf, Some("b"), None);
        a.get_port("pad").connect(&b.get_port("pad"));
        for error in [
            format!("{:#}", top.to_block_ir().unwrap_err()),
            format!("{:#}", top.emit_sv_via_block_ir().unwrap_err()),
        ] {
            assert!(error.contains("Contact.pad"), "{error}");
        }
    }
}

#[test]
fn rejects_inout_on_root_stub() {
    let stub = ModDef::new("Pad");
    stub.add_port("io", IO::InOut(1));
    stub.set_usage(Usage::EmitStubAndStop);
    assert!(
        stub.emit_sv_via_block_ir()
            .unwrap_err()
            .to_string()
            .contains("Pad.io")
    );
}

#[cfg(feature = "emission-tests")]
#[path = "emission/parity.rs"]
mod parity;

#[test]
fn rejects_wire_name_colliding_with_port() {
    let source = ModDef::new("Source");
    source.add_port("q", IO::Output(1)).tieoff(1);
    let top = ModDef::new("Top");
    let q = top.instantiate(&source, Some("src"), None).get_port("q");
    q.export_as("external_out");
    q.specify_net_name("external_out");
    let err = format!("{:#}", top.to_block_ir().unwrap_err());
    assert!(err.contains("external_out"), "{err}");
}
