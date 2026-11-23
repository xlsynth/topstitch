// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "block-ir")]

use std::collections::{HashMap, HashSet};

use regex::Regex;
use topstitch::{EmitOptions, IO, ModDef, ParserConfig, Source, VerilogLibrary};
use xlsynth_pir::ir::{Block, InoutBinding, InoutPiece, NodePayload};

const SINGLE_PAD: &str = "module ExternalPad(inout wire pad); endmodule";
const BUS_PAD: &str =
    "module ExternalBus #(parameter WIDTH=4) (inout wire [WIDTH-1:0] pad); endmodule";

#[test]
fn public_bus_connects_to_native_and_external_scalar_pins() {
    let native = ModDef::new("NativePad");
    native.add_port("pad", IO::InOut(1)).unused();
    let external = ModDef::from_verilog("ExternalPad", SINGLE_PAD).unwrap();
    let top = ModDef::new("MixedBank");
    let pins = top.add_port("pins", IO::InOut(4));
    pins.bit(1).unused();
    pins.bit(3).unused();
    top.instantiate(&native, Some("native"), None)
        .get_port("pad")
        .connect(&pins.bit(0));
    top.instantiate(&external, Some("external"), None)
        .get_port("pad")
        .connect(&pins.bit(2));
    top.instantiate(&external, Some("shared"), None)
        .get_port("pad")
        .connect(&pins.bit(0));

    check_electrical_connections(
        &top,
        SINGLE_PAD,
        &[
            ("native", "pad", &[0]),
            ("external", "pad", &[2]),
            ("shared", "pad", &[0]),
        ],
        "pins",
    );
}

#[test]
fn permuted_external_bus_keeps_msb_and_lsb_order() {
    let external = ModDef::from_verilog("ExternalBus", BUS_PAD).unwrap();
    let top = ModDef::new("PermutedBank");
    let pins = top.add_port("pins", IO::InOut(8));
    for unused in [2, 3, 5, 7] {
        pins.bit(unused).unused();
    }
    let bus = top
        .instantiate(&external, Some("external"), None)
        .get_port("pad");
    for (pin_bit, public_bit) in [(0, 6), (1, 1), (2, 4), (3, 0)] {
        bus.bit(pin_bit).connect(&pins.bit(public_bit));
    }

    check_electrical_connections(&top, BUS_PAD, &[("external", "pad", &[6, 1, 4, 0])], "pins");
}

#[test]
fn named_internal_slice_is_compact_and_zero_based() {
    let external = ModDef::from_verilog("ExternalBus", BUS_PAD).unwrap();
    let top = ModDef::new("NamedBank");
    let a = top.instantiate(&external, Some("a"), None).get_port("pad");
    let b = top.instantiate(&external, Some("b"), None).get_port("pad");
    a.connect(&b);
    a.slice(3, 2).specify_net_name("lane");
    for sv in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir().unwrap(),
    ] {
        ModDef::from_verilog("NamedBank", format!("{BUS_PAD}\n{sv}")).unwrap();
        let topology = SvTopology::new(&sv, "NamedBank");
        assert_eq!(topology.widths["lane"], 2, "{sv}");
        for instance in ["a", "b"] {
            let pins = topology.instance_bits(instance, "pad");
            for bit in 2..=3 {
                assert!(
                    topology.same(&pins[bit], &format!("lane[{}]", bit - 2)),
                    "{sv}"
                );
            }
        }
    }
}

#[test]
fn named_directional_bus_can_include_lanes_without_inout_pins() {
    let source = ModDef::new("Source");
    source.add_port("q", IO::Output(4)).tieoff(10);
    let pad = ModDef::new("Pad");
    pad.add_port("io", IO::InOut(2)).unused();
    let reader = ModDef::new("Reader");
    reader.feedthrough("d", "q", 2);
    let top = ModDef::new("PartialNamedNet");
    let q = top
        .instantiate(&source, Some("src_inst"), None)
        .get_port("q");
    let io = top.instantiate(&pad, Some("pad"), None).get_port("io");
    q.slice(1, 0).connect(&io);
    q.slice(3, 2).export_as("sample");
    let reader = top.instantiate(&reader, Some("reader"), None);
    reader.get_port("d").connect(&io);
    reader.get_port("q").export_as("sensed");
    q.specify_net_name("physical");

    let package = top.to_block_ir().unwrap();
    let block = package.get_top_block().unwrap();
    assert!(
        block
            .inout_nets
            .iter()
            .any(|net| net.name == "physical" && net.ty == xlsynth_pir::ir::Type::Bits(4)),
        "{package}"
    );
    for sv in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir().unwrap(),
    ] {
        ModDef::from_verilog("PartialNamedNet", &sv).unwrap();
        let topology = SvTopology::new(&sv, "PartialNamedNet");
        assert_eq!(topology.widths["physical"], 4, "{sv}");
        let pin = topology.instance_bits("pad", "io");
        let source_pin = topology.instance_bits("src_inst", "q");
        for bit in 0..4 {
            assert!(
                topology.same(&source_pin[bit], &format!("physical[{bit}]")),
                "{sv}"
            );
        }
        for bit in 0..2 {
            assert!(
                topology.same(&pin[bit], &format!("physical[{bit}]")),
                "{sv}"
            );
        }
        #[cfg(feature = "emission-tests")]
        simulate(&format!(
            r#"{sv}
module tb;
  wire [1:0] sample, sensed;
  reg [1:0] external_drive = 2'bzz;
  PartialNamedNet dut(.sample(sample), .sensed(sensed));
  assign dut.pad.io = external_drive;
  initial begin
    #1;
    if (dut.physical !== 4'b1010 || sample !== 2'b10 || sensed !== 2'b10) $fatal(1, "mixed bus mismatch");
    external_drive = 2'b01; #1;
    if (dut.physical !== 4'b10xx || sample !== 2'b10 || sensed !== 2'bxx) $fatal(1, "resolved readback mismatch");
    $finish;
  end
endmodule"#
        ));
    }
}

#[test]
fn named_external_output_cannot_silently_use_a_different_pin_net() {
    let source = ModDef::from_verilog(
        "ExternalSource",
        "module ExternalSource(output wire [3:0] q); assign q = 4'b1010; endmodule",
    )
    .unwrap();
    let pad = ModDef::new("Pad");
    pad.add_port("io", IO::InOut(2)).unused();
    let top = ModDef::new("ExternalNamedBus");
    let q = top
        .instantiate(&source, Some("src_inst"), None)
        .get_port("q");
    q.slice(1, 0)
        .connect(&top.instantiate(&pad, Some("pad"), None).get_port("io"));
    q.slice(3, 2).export_as("sample");
    q.specify_net_name("physical");

    let error = format!("{:#}", top.to_block_ir().unwrap_err());
    assert!(
        error.contains("physical")
            && error.contains("src_inst.q")
            && error.contains("whole native block output"),
        "{error}"
    );
}

#[test]
fn named_output_slice_cannot_silently_use_a_different_pin_net() {
    let source = ModDef::new("Source");
    source.add_port("q", IO::Output(4)).tieoff(10);
    let pad = ModDef::new("Pad");
    pad.add_port("io", IO::InOut(2)).unused();
    let top = ModDef::new("SlicedNamedBus");
    let q = top
        .instantiate(&source, Some("src_inst"), None)
        .get_port("q");
    q.slice(1, 0)
        .connect(&top.instantiate(&pad, Some("pad"), None).get_port("io"));
    q.slice(3, 2).export_as("sample");
    q.slice(1, 0).specify_net_name("physical");

    let error = format!("{:#}", top.to_block_ir().unwrap_err());
    assert!(
        error.contains("physical")
            && error.contains("src_inst.q")
            && error.contains("whole native block output"),
        "{error}"
    );
}

#[test]
fn requested_name_on_a_directional_pin_labels_its_electrical_net() {
    for name_driver in [true, false] {
        let source = ModDef::new("Source");
        source.add_port("q", IO::Output(4)).tieoff(10);
        let pad = ModDef::new("Pad");
        pad.add_port("io", IO::InOut(4)).unused();
        let receiver = ModDef::new("Receiver");
        receiver.feedthrough("d", "q", 4);
        let top = ModDef::new("NamedBank");
        let q = top
            .instantiate(&source, Some("src_inst"), None)
            .get_port("q");
        let io = top.instantiate(&pad, Some("pad"), None).get_port("io");
        let receiver = top.instantiate(&receiver, Some("receiver"), None);
        let d = receiver.get_port("d");
        q.connect(&io);
        d.connect(&io);
        receiver.get_port("q").export_as("sample");
        if name_driver {
            q.specify_net_name("physical");
        } else {
            d.specify_net_name("physical");
        }
        for sv in [
            top.emit(EmitOptions::default()),
            top.emit_sv_via_block_ir().unwrap(),
        ] {
            ModDef::from_verilog("NamedBank", &sv).unwrap();
            let topology = SvTopology::new(&sv, "NamedBank");
            assert_eq!(topology.widths["physical"], 4, "{sv}");
            let pin = topology.instance_bits("pad", "io");
            for bit in 0..4 {
                assert!(
                    topology.same(&pin[bit], &format!("physical[{bit}]")),
                    "{sv}"
                );
            }
        }
    }
}

#[test]
fn partially_driven_public_bus_drives_only_selected_lanes() {
    let top = partial_bank();
    let package = top.to_block_ir().unwrap();
    let block = package.get_top_block().unwrap();
    let public = public_binding(block, "pins");
    let driven_nets: HashSet<_> = block
        .graph
        .nodes
        .iter()
        .filter_map(|node| match &node.payload {
            NodePayload::ExtInoutDrive { net, .. } => Some(net.as_str()),
            _ => None,
        })
        .collect();
    let driven_public_bits: Vec<_> = public
        .iter()
        .enumerate()
        .filter_map(|(i, (net, _))| driven_nets.contains(net.as_str()).then_some(i))
        .collect();
    assert_eq!(driven_public_bits, [0, 1], "{}", package);

    check_electrical_connections(&top, BUS_PAD, &[("external", "pad", &[0, 1, 2, 3])], "pins");
}

#[test]
fn neighboring_lanes_with_different_drivers_share_one_electrical_net() {
    let contact = ModDef::new("Contact");
    contact.add_port("io", IO::InOut(4)).unused();
    let top = ModDef::new("PackedDrivers");
    let pins = top.add_port("pins", IO::InOut(4));
    let lower = top.add_port("lower_data", IO::Input(2));
    let upper = top.add_port("upper_data", IO::Input(2));
    let contact = top.instantiate(&contact, Some("contact"), None);
    let io = contact.get_port("io");
    io.connect(&pins);
    let driver = ModDef::new("LaneDriver");
    driver.feedthrough("data", "value", 2);
    let lower_driver = top.instantiate(&driver, Some("lower_driver"), None);
    lower_driver.get_port("data").connect(&lower);
    lower_driver.get_port("value").connect(&io.slice(1, 0));
    let upper_driver = top.instantiate(&driver, Some("upper_driver"), None);
    upper_driver.get_port("data").connect(&upper);
    upper_driver.get_port("value").connect(&io.slice(3, 2));

    let package = top.to_block_ir().unwrap();
    let block = package.get_top_block().unwrap();
    assert_eq!(block.inout_nets.len(), 1, "{package}");
    assert_eq!(
        block
            .graph
            .nodes
            .iter()
            .filter(|node| matches!(&node.payload, NodePayload::ExtInoutDrive { .. }))
            .count(),
        1,
        "{package}"
    );

    #[cfg(feature = "emission-tests")]
    {
        let testbench = r#"module tb;
  reg [1:0] lower_data = 0, upper_data = 0;
  wire [3:0] pins;
  PackedDrivers dut(.pins(pins), .lower_data(lower_data), .upper_data(upper_data));
  initial begin
    lower_data = 2'b01; upper_data = 2'b10; #1;
    if (pins !== 4'b1001) $fatal(1, "separate lane drivers changed order");
    lower_data = 2'b11; upper_data = 2'b00; #1;
    if (pins !== 4'b0011) $fatal(1, "separate lane drivers changed values");
    $finish;
  end
endmodule"#;
        for wrapper in [
            top.emit(EmitOptions::default()),
            top.emit_sv_via_block_ir().unwrap(),
        ] {
            simulate(&format!("{wrapper}\n{testbench}"));
        }
    }
}

/// Creates a bus with a native directional driver on its low lanes and a bidirectional external leaf.
fn partial_bank() -> ModDef {
    let external = ModDef::from_verilog("ExternalBus", BUS_PAD).unwrap();
    let driver = ModDef::new("LaneDriver");
    driver.feedthrough("data", "value", 2);
    let reader = ModDef::new("BusReader");
    reader.feedthrough("pins", "sample", 4);
    let top = ModDef::new("PartialBank");
    let pins = top.add_port("pins", IO::InOut(4));
    let data = top.add_port("data", IO::Input(2));
    let pad = top
        .instantiate(&external, Some("external"), None)
        .get_port("pad");
    pad.connect(&pins);
    let driver = top.instantiate(&driver, Some("driver"), None);
    driver.get_port("data").connect(&data);
    driver.get_port("value").connect(&pad.slice(1, 0));
    let reader = top.instantiate(&reader, Some("reader"), None);
    reader.get_port("pins").connect(&pad);
    reader.get_port("sample").export_as("sample");
    top
}

#[cfg(feature = "emission-tests")]
#[test]
fn both_emitters_simulate_partial_drive_and_bidirectional_readback() {
    let top = partial_bank();
    let testbench = r#"module tb;
  reg [1:0] data = 0, external = 0;
  wire [3:0] pins, sample;
  assign pins[3:2] = external;
  PartialBank dut(.pins(pins), .data(data), .sample(sample));
  initial begin
    data = 2'b10; external = 2'b01; #1;
    if (pins !== 4'b0110 || sample !== 4'b0110) $fatal(1, "partial drive/readback mismatch");
    external = 2'bzz; #1;
    if (pins !== 4'bzz10 || sample !== 4'bzz10) $fatal(1, "undriven lanes lost Z");
    data = 2'b01; external = 2'b11; #1;
    if (pins !== 4'b1101 || sample !== 4'b1101) $fatal(1, "changed drive/read mismatch");
    $finish;
  end
endmodule"#;
    for wrapper in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir().unwrap(),
    ] {
        simulate(&format!("{BUS_PAD}\n{wrapper}\n{testbench}"));
    }
}

#[cfg(feature = "emission-tests")]
#[test]
fn offset_driver_slice_maps_to_the_right_resolved_lanes() {
    let pad = ModDef::from_verilog("ExternalBus", BUS_PAD)
        .unwrap()
        .parameterize(&[("WIDTH", 5)]);
    let driver = ModDef::new("OffsetDriver");
    driver.feedthrough("data", "value", 8);
    let reader = ModDef::new("OffsetReader");
    reader.feedthrough("pins", "sample", 5);
    let top = ModDef::new("OffsetBank");
    let pins = top.add_port("pins", IO::InOut(5));
    let data = top.add_port("data", IO::Input(8));
    let pad = top.instantiate(&pad, Some("pad"), None).get_port("pad");
    pad.connect(&pins);
    let driver = top.instantiate(&driver, Some("driver"), None);
    driver.get_port("data").connect(&data);
    let value = driver.get_port("value");
    value.bit(7).unused();
    value.slice(3, 0).unused();
    value.slice(6, 4).connect(&pad.slice(3, 1));
    let reader = top.instantiate(&reader, Some("reader"), None);
    reader.get_port("pins").connect(&pad);
    reader.get_port("sample").export_as("sample");
    let testbench = r#"module tb;
  reg [7:0] data = 0;
  reg [1:0] external = 0;
  wire [4:0] pins, sample;
  assign pins[4] = external[1];
  assign pins[0] = external[0];
  OffsetBank dut(.pins(pins), .data(data), .sample(sample));
  initial begin
    data = 8'b11010011; external = 2'b10; #1;
    if (pins !== 5'b11010 || sample !== 5'b11010) $fatal(1, "nonzero source/destination offset mismatch");
    data = 8'b00101100; external = 2'bzz; #1;
    if (pins !== 5'bz010z || sample !== 5'bz010z) $fatal(1, "offset drive changed undriven lanes");
    $finish;
  end
endmodule"#;
    for wrapper in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir().unwrap(),
    ] {
        simulate(&format!("{BUS_PAD}\n{wrapper}\n{testbench}"));
    }
}

#[cfg(feature = "emission-tests")]
#[test]
fn external_output_can_drive_a_resolved_inout_net() {
    let source = r#"module ExternalBus #(parameter WIDTH=4) (inout wire [WIDTH-1:0] pad); endmodule
module ExternalDriver(input wire data, output wire value); assign value = data; endmodule"#;
    let pad = ModDef::from_verilog("ExternalBus", source)
        .unwrap()
        .parameterize(&[("WIDTH", 2)]);
    let driver = ModDef::from_verilog("ExternalDriver", source).unwrap();
    let reader = ModDef::new("ExternalDriveReader");
    reader.feedthrough("pins", "sample", 2);
    let top = ModDef::new("ExternalDriveBank");
    let pins = top.add_port("pins", IO::InOut(2));
    let data = top.add_port("data", IO::Input(1));
    let pad = top.instantiate(&pad, Some("pad"), None).get_port("pad");
    pad.connect(&pins);
    let driver = top.instantiate(&driver, Some("driver"), None);
    driver.get_port("data").connect(&data);
    driver.get_port("value").connect(&pad.bit(0));
    let reader = top.instantiate(&reader, Some("reader"), None);
    reader.get_port("pins").connect(&pad);
    reader.get_port("sample").export_as("sample");
    check_electrical_connections(&top, source, &[("pad", "pad", &[0, 1])], "pins");
    let testbench = r#"module tb;
  reg data = 0, external = 0;
  wire [1:0] pins, sample;
  assign pins[1] = external;
  ExternalDriveBank dut(.pins(pins), .data(data), .sample(sample));
  initial begin
    data = 1'b1; external = 1'b0; #1;
    if (pins !== 2'b01 || sample !== 2'b01) $fatal(1, "external output did not drive inout");
    data = 1'b0; external = 1'bz; #1;
    if (pins !== 2'bz0 || sample !== 2'bz0) $fatal(1, "external output/readback mismatch");
    $finish;
  end
endmodule"#;
    for wrapper in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir().unwrap(),
    ] {
        simulate(&format!("{source}\n{wrapper}\n{testbench}"));
    }
}

/// Compiles and runs a small electrical testbench with the CI-provided Icarus tools.
#[cfg(feature = "emission-tests")]
fn simulate(source: &str) {
    use std::{fs, process::Command};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("design.sv");
    let output = dir.path().join("design.vvp");
    fs::write(&input, source).unwrap();
    let compile = Command::new("iverilog")
        .args(["-g2012", "-s", "tb", "-o"])
        .arg(&output)
        .arg(&input)
        .output()
        .expect("iverilog is required for emission-tests");
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new("vvp")
        .arg(output)
        .output()
        .expect("vvp is required for emission-tests");
    assert!(
        run.status.success(),
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn parameterized_opaque_pad_preserves_enum_input_cast() {
    let source = r#"package pad_types; typedef enum logic [1:0] {Idle, Active} mode_t; endpackage
module ConfigurablePad #(parameter WIDTH=1) (inout wire [WIDTH-1:0] pad, input pad_types::mode_t mode); endmodule"#;
    let external = ModDef::from_verilog("ConfigurablePad", source)
        .unwrap()
        .parameterize(&[("WIDTH", 4)]);
    let top = ModDef::new("ConfiguredBank");
    let pins = top.add_port("pins", IO::InOut(4));
    let mode = top.add_port("mode", IO::Input(2));
    let child = top.instantiate(&external, Some("external"), None);
    child.get_port("pad").connect(&pins);
    child.get_port("mode").connect(&mode);

    let ir_sv = top.emit_sv_via_block_ir().unwrap();
    assert!(ir_sv.contains(".mode(pad_types::mode_t'(mode))"), "{ir_sv}");
    check_electrical_connections(&top, source, &[("external", "pad", &[0, 1, 2, 3])], "pins");
}

/// Checks the electrical identity of each child pin bit in both emitted representations.
fn check_electrical_connections(
    top: &ModDef,
    external: &str,
    expected: &[(&str, &str, &[usize])],
    public: &str,
) {
    let direct = top.emit(EmitOptions::default());
    let ir = top.emit_sv_via_block_ir().unwrap();
    for sv in [&direct, &ir] {
        let sources = [Source::text(external), Source::text(sv)];
        let name = top.get_name();
        let tops = [name.as_str()];
        let library = VerilogLibrary::from_config(&ParserConfig {
            sources: &sources,
            tops: &tops,
            include_hierarchy: true,
            ignore_unknown_modules: false,
            ..Default::default()
        })
        .unwrap();
        let reported: HashSet<_> = library
            .module(&name)
            .unwrap()
            .report_all_instances()
            .into_iter()
            .map(|(_, path)| path)
            .collect();
        let topology = SvTopology::new(sv, &name);
        for (instance, port, expected_bits) in expected {
            assert!(
                reported.contains(&format!("{name}.{instance}")),
                "missing {instance}: {sv}"
            );
            let actual = topology.instance_bits(instance, port);
            assert_eq!(actual.len(), expected_bits.len(), "{sv}");
            for (bit, public_bit) in actual.iter().zip(*expected_bits) {
                assert!(
                    topology.same(bit, &format!("{public}[{public_bit}]")),
                    "{instance}.{port} bit {bit} is not {public}[{public_bit}]: {sv}"
                );
            }
        }
    }
    let package = top.to_block_ir().unwrap();
    let block = package.get_top_block().unwrap();
    let public_bits = public_binding(block, public);
    for (instance, port, expected_bits) in expected {
        let binding = block
            .graph
            .nodes
            .iter()
            .find_map(|n| match &n.payload {
                NodePayload::ExtInstantiationInout {
                    instantiation,
                    port_name,
                    net,
                } if instantiation == instance && port_name == port => Some(net),
                _ => None,
            })
            .expect("inout instance binding");
        let actual = binding_bits(block, binding);
        let expected: Vec<_> = expected_bits
            .iter()
            .map(|i| public_bits[*i].clone())
            .collect();
        assert_eq!(actual, expected, "{package}");
    }
}

/// Expands a public bidirectional binding from least to most significant bit.
fn public_binding(block: &Block, name: &str) -> Vec<(String, usize)> {
    let binding = block
        .graph
        .nodes
        .iter()
        .find_map(|node| match &node.payload {
            NodePayload::ExtInoutPort {
                name: port, net, ..
            } if port == name => Some(net),
            _ => None,
        })
        .expect("public inout binding");
    binding_bits(block, binding)
}

/// Expands a PIR binding without assuming any particular decomposition into nets.
fn binding_bits(block: &Block, binding: &InoutBinding) -> Vec<(String, usize)> {
    binding
        .pieces()
        .iter()
        .rev()
        .flat_map(|piece| {
            let (net, lo, hi) = match piece {
                InoutPiece::Whole(net) => (
                    net,
                    0,
                    block
                        .inout_nets
                        .iter()
                        .find(|n| n.name == *net)
                        .unwrap()
                        .ty
                        .bit_count()
                        - 1,
                ),
                InoutPiece::Select { net, hi, lo } => (net, *lo, *hi),
            };
            (lo..=hi).map(move |i| (net.clone(), i))
        })
        .collect()
}

/// Models only passive SV connectivity: shared nets and `tran` primitives, never assignments.
struct SvTopology {
    body: String,
    widths: HashMap<String, usize>,
    edges: HashMap<String, Vec<String>>,
}

impl SvTopology {
    /// Reads static wire declarations and passive connections from one emitted module.
    fn new(sv: &str, module: &str) -> Self {
        let module_re = Regex::new(&format!(
            r"(?s)\bmodule\s+{}\b(.*?)\bendmodule\b",
            regex::escape(module)
        ))
        .unwrap();
        let body = module_re.captures(sv).unwrap()[1].to_owned();
        let decl = Regex::new(r"(?m)^\s*(?:(?:input|output|inout)(?:\s+(?:wire|logic))?|wire|logic)\s*(?:\[(\d+)\s*:\s*(\d+)\]\s*)?([A-Za-z_][A-Za-z_0-9]*)\b").unwrap();
        let mut widths = HashMap::new();
        for x in decl.captures_iter(&body) {
            let width = match (x.get(1), x.get(2)) {
                (Some(hi), Some(lo)) => {
                    hi.as_str().parse::<usize>().unwrap() - lo.as_str().parse::<usize>().unwrap()
                        + 1
                }
                _ => 1,
            };
            widths.insert(x[3].to_owned(), width);
        }
        let mut topology = Self {
            body,
            widths,
            edges: HashMap::new(),
        };
        let tran =
            Regex::new(r"\btran(?:\s+[A-Za-z_][A-Za-z_0-9]*)?\s*\(([^,]+),([^\)]+)\)\s*;").unwrap();
        let pairs: Vec<_> = tran
            .captures_iter(&topology.body)
            .map(|x| (x[1].to_owned(), x[2].to_owned()))
            .collect();
        for (a, b) in pairs {
            let a = topology.expr_bits(&a);
            let b = topology.expr_bits(&b);
            assert_eq!(a.len(), b.len());
            for (a, b) in a.into_iter().zip(b) {
                topology.edges.entry(a.clone()).or_default().push(b.clone());
                topology.edges.entry(b).or_default().push(a);
            }
        }
        topology
    }

    /// Returns an instance inout connection in least-significant-bit order.
    fn instance_bits(&self, instance: &str, port: &str) -> Vec<String> {
        let instance_re = Regex::new(&format!(r"\b{}\s*\(", regex::escape(instance))).unwrap();
        let found = instance_re.find(&self.body).unwrap();
        let inner = self.balanced(&self.body, found.end() - 1);
        let port_re = Regex::new(&format!(r"\.{}\s*\(", regex::escape(port))).unwrap();
        let found = port_re.find(inner).unwrap();
        self.expr_bits(self.balanced(inner, found.end() - 1))
    }

    /// Extracts the text inside a balanced pair of parentheses.
    fn balanced<'a>(&self, text: &'a str, open: usize) -> &'a str {
        let mut depth = 0;
        for (offset, ch) in text[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return &text[open + 1..open + offset];
                    }
                }
                _ => {}
            }
        }
        panic!("unbalanced emitted SV: {text}")
    }

    /// Expands a static wire expression into individual wire bits.
    fn expr_bits(&self, expr: &str) -> Vec<String> {
        let expr: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
        if expr.starts_with('{') && expr.ends_with('}') {
            return expr[1..expr.len() - 1]
                .split(',')
                .rev()
                .flat_map(|x| self.expr_bits(x))
                .collect();
        }
        let term = Regex::new(r"^([A-Za-z_][A-Za-z_0-9]*)(?:\[(\d+)(?::(\d+))?\])?$").unwrap();
        let x = term
            .captures(&expr)
            .unwrap_or_else(|| panic!("unexpected inout expression: {expr}"));
        let name = &x[1];
        let (lo, hi) = match (x.get(2), x.get(3)) {
            (Some(hi), Some(lo)) => (lo.as_str().parse().unwrap(), hi.as_str().parse().unwrap()),
            (Some(i), None) => {
                let i = i.as_str().parse().unwrap();
                (i, i)
            }
            _ => (0, self.widths[name] - 1),
        };
        (lo..=hi).map(|i| format!("{name}[{i}]")).collect()
    }

    /// Returns whether two wire bits share a passive electrical connection.
    fn same(&self, a: &str, b: &str) -> bool {
        let mut pending = vec![a.to_owned()];
        let mut seen = HashSet::new();
        while let Some(x) = pending.pop() {
            if x == b {
                return true;
            }
            if seen.insert(x.clone()) {
                pending.extend(self.edges.get(&x).into_iter().flatten().cloned());
            }
        }
        false
    }
}
