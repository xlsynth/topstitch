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
            BlockPort::Input(n) | BlockPort::Output(n) | BlockPort::Inout(n) => block.port_name(*n),
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
    assert!(matches!(
        inst.kind,
        xlsynth_pir::ir::InstantiationKind::OpaqueExternBlock { .. }
    ));
    assert_eq!(inst.block, "Cell");
    assert!(parsed.get_fn(&inst.block).is_none());
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
fn native_inout_hierarchy_preserves_ports_and_resolved_reads() {
    let pad = ModDef::new("Contact");
    pad.add_port("pin", IO::InOut(4)).unused();
    let reader = ModDef::new("Readback");
    reader.feedthrough("d", "q", 4);

    let top = ModDef::new("Upper");
    let pin = top.add_port("pin", IO::InOut(4));
    top.add_port("open", IO::InOut(1)).unused();
    top.instantiate(&pad, Some("pad"), None)
        .get_port("pin")
        .connect(&pin);
    let readback = top.instantiate(&reader, Some("readback"), None);
    readback.get_port("d").connect(&pin);
    readback.get_port("q").export_as("sensed");

    let package = top.to_block_ir().unwrap();
    let block = package.get_top_block().unwrap();
    assert!(matches!(block.ports[0], BlockPort::Inout(_)));
    assert!(matches!(block.ports[1], BlockPort::Inout(_)));
    let ir = package.to_string();
    assert!(ir.contains("ext_inout_read("), "{ir}");
    Parser::new(&ir).parse_and_validate_package().unwrap();
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains("inout wire [3:0] pin"), "{sv}");
    assert!(sv.contains(".pin(pin)"), "{sv}");
    assert!(sv.contains("inout wire open"), "{sv}");
}

#[test]
fn module_input_can_drive_a_private_inout_net_and_read_its_resolved_value() {
    let pad = ModDef::new("Contact");
    pad.add_port("pin", IO::InOut(4)).unused();
    let reader = ModDef::new("Readback");
    reader.feedthrough("d", "q", 4);
    let top = ModDef::new("Upper");
    let data = top.add_port("data", IO::Input(4));
    let pad_instance = top.instantiate(&pad, Some("pad"), None);
    let pin = pad_instance.get_port("pin");
    pin.connect(&data);
    let readback = top.instantiate(&reader, Some("readback"), None);
    readback.get_port("d").connect(&pin);
    readback.get_port("q").export_as("sensed");

    let package = top.to_block_ir().unwrap();
    let ir = package.to_string();
    assert!(ir.contains("ext_inout_drive("), "{ir}");
    assert!(ir.contains("ext_inout_read("), "{ir}");
    Parser::new(&ir).parse_and_validate_package().unwrap();
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains(".pin("), "{sv}");
    assert!(sv.contains("assign"), "{sv}");
}

#[test]
fn native_inout_siblings_share_a_private_electrical_net() {
    let pad = ModDef::new("Contact");
    pad.add_port("pin", IO::InOut(1)).unused();
    let top = ModDef::new("Pair");
    let a = top.instantiate(&pad, Some("a"), None);
    let b = top.instantiate(&pad, Some("b"), None);
    a.get_port("pin").connect(&b.get_port("pin"));

    let package = top.to_block_ir().unwrap();
    assert_eq!(package.get_top_block().unwrap().inout_nets.len(), 1);
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert_eq!(sv.matches(".pin(ts_inout_0)").count(), 2, "{sv}");
}

#[test]
fn specified_private_inout_name_is_shared_by_siblings() {
    let pad = ModDef::new("Contact");
    pad.add_port("pin", IO::InOut(1)).unused();
    let top = ModDef::new("Pair");
    let a = top.instantiate(&pad, Some("a"), None);
    let b = top.instantiate(&pad, Some("b"), None);
    a.get_port("pin").connect(&b.get_port("pin"));
    a.get_port("pin").specify_net_name("physical");
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert_eq!(sv.matches(".pin(physical)").count(), 2, "{sv}");
}

#[test]
fn rejects_requested_private_name_that_would_alias_public_inout() {
    let pad = ModDef::new("Contact");
    pad.add_port("pin", IO::InOut(1)).unused();
    let top = ModDef::new("Upper");
    let public = top.add_port("pin", IO::InOut(1));
    let child = top.instantiate(&pad, Some("pad"), None);
    child.get_port("pin").connect(&public);
    child.get_port("pin").specify_net_name("physical");
    let error = format!("{:#}", top.to_block_ir().unwrap_err());
    assert!(
        error.contains("physical") && error.contains("public inout"),
        "{error}"
    );
}

#[test]
fn native_inout_tieoffs_and_root_stubs_emit() {
    let tied = ModDef::new("Tied");
    tied.add_port("pin", IO::InOut(1)).tieoff(0);
    let sv = tied.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains("assign pin ="), "{sv}");

    let stub = ModDef::new("PadStub");
    stub.add_port("io", IO::InOut(1));
    stub.set_usage(Usage::EmitStubAndStop);
    assert!(
        stub.emit_sv_via_block_ir()
            .unwrap()
            .contains("inout wire io")
    );
}

#[test]
fn ordinary_external_input_can_read_a_public_inout() {
    let top = ModDef::new("Upper");
    let pin = top.add_port("pin", IO::InOut(4));
    let external =
        ModDef::from_verilog("Sink4", "module Sink4(input wire [3:0] data); endmodule").unwrap();
    top.instantiate(&external, Some("sink"), None)
        .get_port("data")
        .connect(&pin);

    let package = top.to_block_ir().unwrap();
    assert!(matches!(
        package.get_top_block().unwrap().instantiations[0].kind,
        xlsynth_pir::ir::InstantiationKind::OpaqueExternBlock { .. }
    ));
    assert!(package.to_string().contains("ext_inout_read("));
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(
        sv.contains("inout wire [3:0] pin") && sv.contains(".data("),
        "{sv}"
    );
}

#[test]
fn parameterized_external_sv_inout_is_bound_as_an_opaque_pin() {
    let source = "module Pad #(parameter WIDTH = 1) (inout wire [WIDTH-1:0] pin); endmodule";
    let external = ModDef::from_verilog("Pad", source)
        .unwrap()
        .parameterize(&[("WIDTH", 4)]);
    let top = ModDef::new("Upper");
    top.instantiate(&external, Some("pad"), None)
        .get_port("pin")
        .export_as("pad_pin");
    let package = top.to_block_ir().unwrap();
    let inst = &package.get_top_block().unwrap().instantiations[0];
    let xlsynth_pir::ir::InstantiationKind::OpaqueExternBlock {
        ports, parameters, ..
    } = &inst.kind
    else {
        panic!("expected opaque external module");
    };
    assert!(matches!(
        ports[0].kind,
        xlsynth_pir::ir::OpaqueExternPortKind::Inout(_)
    ));
    assert!(parameters.contains_key("WIDTH"));
    assert!(package.to_string().contains("ext_instantiation_inout("));
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains(".pin(pad_pin)"), "{sv}");
    ModDef::from_verilog("Upper", format!("{source}\n{sv}")).unwrap();
}

#[test]
fn external_output_can_drive_an_inout_net() {
    let source = "module Owner(output wire q); assign q = 1'b1; endmodule";
    let external = ModDef::from_verilog("Owner", source).unwrap();
    let contact = ModDef::new("Contact");
    contact.add_port("pin", IO::InOut(1)).unused();
    let top = ModDef::new("Upper");
    let pin = top
        .instantiate(&contact, Some("contact"), None)
        .get_port("pin");
    top.instantiate(&external, Some("src"), None)
        .get_port("q")
        .connect(&pin);
    let ir = top.to_block_ir().unwrap().to_string();
    assert!(ir.contains("instantiation_output("), "{ir}");
    assert!(ir.contains("ext_inout_drive("), "{ir}");
    let sv = top.emit_sv_via_block_ir().unwrap();
    ModDef::from_verilog("Upper", format!("{source}\n{sv}")).unwrap();
}

#[test]
fn rejects_native_output_pin_readback_that_requires_an_electrical_binding() {
    let source = ModDef::new("Source");
    source.add_port("q", IO::Output(1)).tieoff(1);
    let sink = ModDef::new("Sink");
    sink.add_port("d", IO::Input(1)).unused();
    let owner = ModDef::new("Owner");
    let q = owner.add_port("q", IO::Output(1));
    let driver = owner
        .instantiate(&source, Some("producer"), None)
        .get_port("q");
    driver.connect(&q);
    owner
        .instantiate(&sink, Some("sink"), None)
        .get_port("d")
        .connect(&driver);
    let contact = ModDef::new("Contact");
    contact.add_port("pin", IO::InOut(1)).unused();
    let top = ModDef::new("Upper");
    let pin = top
        .instantiate(&contact, Some("contact"), None)
        .get_port("pin");
    top.instantiate(&owner, Some("src"), None)
        .get_port("q")
        .connect(&pin);
    let error = format!("{:#}", top.to_block_ir().unwrap_err());
    assert!(
        error.contains("share an electrical net") && error.contains("src.q"),
        "{error}"
    );
}

#[test]
fn sliced_inout_connections_bind_passively_to_the_public_bus() {
    let pad = ModDef::new("Contact");
    pad.add_port("pin", IO::InOut(1)).unused();
    let top = ModDef::new("Bank");
    let pins = top.add_port("pins", IO::InOut(2));
    pins.bit(0).unused();
    top.instantiate(&pad, Some("pad"), None)
        .get_port("pin")
        .connect(&pins.bit(1));
    let package = top.to_block_ir().unwrap();
    let ir = package.to_string();
    assert!(ir.contains("port_name=pin, net=ts_inout_0[1]"), "{ir}");
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains(".pin(pins[1])"), "{sv}");
    ModDef::from_verilog("Bank", sv).unwrap();
}

#[test]
fn external_inout_can_concatenate_disjoint_public_bus_slices() {
    let source = "module Pad(inout wire [3:0] io); endmodule";
    let pad = ModDef::from_verilog("Pad", source).unwrap();
    let top = ModDef::new("Bank");
    let pins = top.add_port("pins", IO::InOut(8));
    pins.slice(5, 2).unused();
    let io = top.instantiate(&pad, Some("pad"), None).get_port("io");
    io.slice(3, 2).connect(&pins.slice(7, 6));
    io.slice(1, 0).connect(&pins.slice(1, 0));
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains(".io({pins[7:6], pins[1:0]})"), "{sv}");
    ModDef::from_verilog("Bank", format!("{source}\n{sv}")).unwrap();
}

#[test]
fn partially_driven_private_inout_splits_only_the_driven_lanes() {
    let pad = ModDef::new("Contact");
    pad.add_port("io", IO::InOut(4)).unused();
    let top = ModDef::new("Bank");
    let data = top.add_port("data", IO::Input(2));
    let a = top.instantiate(&pad, Some("a"), None).get_port("io");
    let b = top.instantiate(&pad, Some("b"), None).get_port("io");
    a.connect(&b);
    a.slice(1, 0).connect(&data);
    top.add_port("sense", IO::Output(4)).connect(&b);
    let package = top.to_block_ir().unwrap();
    let block = package.get_top_block().unwrap();
    assert_eq!(block.inout_nets.len(), 2);
    let ir = package.to_string();
    assert_eq!(ir.matches("ext_inout_drive(").count(), 1, "{ir}");
    let sv = top.emit_sv_via_block_ir().unwrap();
    ModDef::from_verilog("Bank", sv).unwrap();
}

#[test]
fn fully_driven_named_inout_bus_combines_distinct_sources() {
    let pad = ModDef::new("Contact");
    pad.add_port("io", IO::InOut(4)).unused();
    let top = ModDef::new("Bank");
    let low = top.add_port("low", IO::Input(2));
    let high = top.add_port("high", IO::Input(2));
    let driver = ModDef::new("Emitter");
    let d = driver.add_port("d", IO::Input(2));
    driver.add_port("q", IO::Output(2)).connect(&d);
    let lo = top.instantiate(&driver, Some("lo"), None);
    let hi = top.instantiate(&driver, Some("hi"), None);
    lo.get_port("d").connect(&low);
    hi.get_port("d").connect(&high);
    let a = top.instantiate(&pad, Some("a"), None).get_port("io");
    let b = top.instantiate(&pad, Some("b"), None).get_port("io");
    a.connect(&b);
    a.slice(1, 0).connect(&lo.get_port("q"));
    a.slice(3, 2).connect(&hi.get_port("q"));
    a.specify_net_name("physical");
    let package = top.to_block_ir().unwrap();
    let block = package.get_top_block().unwrap();
    assert_eq!(block.inout_nets.len(), 1);
    let ir = package.to_string();
    assert_eq!(ir.matches("ext_inout_drive(").count(), 1, "{ir}");
    let args = block
        .graph
        .nodes
        .iter()
        .find_map(|node| match &node.payload {
            xlsynth_pir::ir::NodePayload::Nary(xlsynth_pir::ir::NaryOp::Concat, args) => Some(args),
            _ => None,
        })
        .unwrap();
    let instances = args
        .iter()
        .map(|arg| match &block.graph.get_node(*arg).payload {
            xlsynth_pir::ir::NodePayload::InstantiationOutput { instantiation, .. } => {
                instantiation.as_str()
            }
            payload => panic!("unexpected concatenation operand {payload:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(instances, ["hi", "lo"]);
    let sv = top.emit_sv_via_block_ir().unwrap();
    assert!(sv.contains(".io(physical)"), "{sv}");
    ModDef::from_verilog("Bank", sv).unwrap();
}

#[test]
fn partially_driven_named_inout_bus_fails_explicitly() {
    let pad = ModDef::new("Contact");
    pad.add_port("io", IO::InOut(4)).unused();
    let driver = ModDef::new("Emitter");
    driver.add_port("q", IO::Output(2)).tieoff(1);
    let top = ModDef::new("Bank");
    let a = top.instantiate(&pad, Some("a"), None).get_port("io");
    let b = top.instantiate(&pad, Some("b"), None).get_port("io");
    a.connect(&b);
    a.slice(1, 0)
        .connect(&top.instantiate(&driver, Some("drv"), None).get_port("q"));
    a.specify_net_name("physical");
    let error = format!("{:#}", top.to_block_ir().unwrap_err());
    assert!(
        error.contains("physical") && error.contains("partially driven"),
        "{error}"
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
