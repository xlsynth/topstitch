// SPDX-License-Identifier: Apache-2.0

use std::{fs, process::Command};

use anyhow::{Result, ensure};
use topstitch::{EmitOptions, IO, ModDef, ParameterValue, Usage};

use super::ir_sv;

const REG: &str = r#"
module RegisterBank #(parameter WIDTH = 4, parameter MASK = 0)
    (input clk, input [WIDTH-1:0] d, output reg [WIDTH-1:0] q, output odd);
    always @(posedge clk) q <= d ^ MASK;
    assign odd = ^q;
endmodule
"#;

#[test]
fn stock_codegen_binds_parameterized_clocked_leaves_in_place() -> Result<()> {
    let register = ModDef::from_verilog("RegisterBank", REG)?;
    let byte = register.parameterize(&[("WIDTH", 8), ("MASK", 0)]);
    let duplicate = register.parameterize(&[("MASK", 0), ("WIDTH", 8)]);
    let invert = register.parameterize(&[("WIDTH", 8), ("MASK", 255)]);
    let wide = register.parameterize(&[("WIDTH", 12), ("MASK", 0)]);

    // The exporter's first candidate for a private helper is already a user-owned symbol.
    let top = ModDef::new("ts_external_0");
    let clk = top.add_port("clk", IO::Input(1));
    let data = top.add_port("data", IO::Input(12));
    for (name, leaf) in [
        ("first", &byte),
        ("repeat_byte", &duplicate),
        ("invert", &invert),
        ("wide", &wide),
    ] {
        let inst = top.instantiate(leaf, Some(name), None);
        inst.get_port("clk").connect(&clk);
        let d = inst.get_port("d");
        d.connect(&data.slice(d.io().width() - 1, 0));
        let q = inst.get_port("q");
        q.export_as(format!("{name}_out").as_str());
        if name == "first" {
            q.specify_net_name("boundary");
        }
        inst.get_port("odd")
            .export_as(format!("{name}_odd").as_str());
    }

    let package = top.to_block_ir()?;
    let cells = &package.get_top_block().unwrap().instantiations;
    assert_eq!(cells[0].block, cells[1].block);
    assert_ne!(cells[0].block, cells[2].block);
    assert_ne!(cells[0].block, cells[3].block);
    assert_eq!(package.members.len(), 4);
    let linked = ir_sv(&top)?;
    assert_eq!(linked.matches("module ").count(), 1);
    assert!(linked.contains(".q(boundary)"));
    let bench = r#"
module tb;
  reg clk = 0;
  reg [11:0] data = 0;
  wire [7:0] a, b, c;
  wire [11:0] z;
  wire pa, pb, pc, pz;
  ts_external_0 dut (.clk(clk), .data(data), .first_out(a), .repeat_byte_out(b),
    .invert_out(c), .wide_out(z), .first_odd(pa), .repeat_byte_odd(pb), .invert_odd(pc), .wide_odd(pz));
  task check(input [11:0] expected);
    if ({a,b,c,z} !== {expected[7:0],expected[7:0],~expected[7:0],expected}
        || {pa,pb,pc,pz} !== {^a,^b,^c,^z} || dut.boundary !== dut.first.q)
      $fatal(1, "bad register outputs or boundary");
  endtask
  initial begin
    if (dut.first.WIDTH != 8 || dut.invert.MASK != 255 || dut.wide.WIDTH != 12) $fatal;
    data=12'hf31; #1; clk=1; #1; check(12'hf31);
    clk=0; data=12'h842; #1; check(12'hf31);
    clk=1; #1; check(12'h842); $finish;
  end
endmodule
"#;
    for rtl in [top.emit(EmitOptions::default()), linked] {
        simulate(&format!("{REG}\n{rtl}\n{bench}"))?;
    }
    Ok(())
}

#[test]
fn feedback_through_an_external_register() -> Result<()> {
    let source = r#"
        module ToggleRegister(input clk, rst, d, output reg q);
          always @(posedge clk) q <= rst ? 1'b0 : ~d;
        endmodule"#;
    let register = ModDef::from_verilog("ToggleRegister", source)?;
    let top = ModDef::new("Feedback");
    let inst = top.instantiate(&register, Some("r"), None);
    inst.get_port("clk").export_as("clk");
    inst.get_port("rst").export_as("rst");
    let q = inst.get_port("q");
    inst.get_port("d").connect(&q);
    q.export_as("q");

    let bench = r#"
        module tb;
          reg clk = 0, rst = 1;
          wire q;
          Feedback dut(.*);
          task tick;
            #1; clk = 1; #1; clk = 0;
          endtask
          initial begin
            tick; if (q !== 1'b0) $fatal(1, "reset");
            rst = 0;
            tick; if (q !== 1'b1) $fatal(1, "first toggle");
            tick; if (q !== 1'b0) $fatal(1, "second toggle");
          end
        endmodule"#;
    for rtl in [
        top.emit(EmitOptions::default()),
        top.emit_sv_via_block_ir()?,
    ] {
        simulate(&format!("{source}\n{rtl}\n{bench}"))?;
    }
    Ok(())
}

#[test]
fn ffi_quotes_literal_braces_and_keeps_enum_input_casts() -> Result<()> {
    let source = r#"
        package types; typedef enum logic [1:0] {Idle, Busy} mode_t; endpackage
        module Cell #(parameter string LABEL="idle") (input types::mode_t mode, output y);
        assign y = mode == types::Busy; endmodule
    "#;
    let leaf = ModDef::from_verilog("Cell", source)?
        .parameterize::<ParameterValue>(&[("LABEL", "{fn} {return} xlsynth_pir_ext=test".into())]);
    let top = ModDef::new("Top");
    let inst = top.instantiate(&leaf, Some("cell_i"), None);
    inst.get_port("mode").export_as("mode");
    inst.get_port("y").export_as("y");
    let rtl = ir_sv(&top)?;
    assert!(
        rtl.contains(r#".LABEL("{fn} {return} xlsynth_pir_ext\075test")"#),
        "{rtl}"
    );
    assert!(rtl.contains(".mode(types::mode_t'(mode))"), "{rtl}");
    ModDef::from_verilog("Top", format!("{source}\n{rtl}"))?;
    Ok(())
}

#[test]
fn zero_input_and_zero_output_externals_emit_both_instances() -> Result<()> {
    let source = ModDef::new("Generator");
    source.add_port("value", IO::Output(3));
    source.set_usage(Usage::EmitNothingAndStop);
    let sink = ModDef::new("Consumer");
    sink.add_port("value", IO::Input(3));
    sink.set_usage(Usage::EmitStubAndStop);
    let top = ModDef::new("Top");
    let a = top.instantiate(&source, Some("a"), None);
    let b = top.instantiate(&sink, Some("b"), None);
    a.get_port("value").connect(&b.get_port("value"));
    let rtl = ir_sv(&top)?;
    assert!(
        rtl.contains("Generator a (") && rtl.contains("Consumer b ("),
        "{rtl}"
    );
    Ok(())
}

#[test]
fn linked_combinational_leaf_in_native_hierarchy_matches_spec() -> Result<()> {
    const LEAF: &str = "module Rotate(input [3:0] d, output [3:0] q);
        assign q = {d[1:0], d[3:2]}; endmodule";
    let leaf = ModDef::from_verilog("Rotate", LEAF)?;
    let inner = leaf.wrap(Some("Inner"), Some("rotate"));
    inner
        .get_instance("rotate")
        .get_port("q")
        .specify_net_name("rotated");
    let top = inner.wrap(Some("Top"), Some("adapter"));
    let spec = "module Top(input [3:0] d, output [3:0] q);
        assign q = {d[1:0], d[3:2]}; endmodule";
    let direct = format!("{LEAF}\n{}", top.emit(EmitOptions::default()));
    let linked = format!("{LEAF}\n{}", ir_sv(&top)?);
    super::compare(&top, spec, &linked, false)?;
    super::compare(&top, &direct, &linked, true)
}

#[test]
fn vast_stub_links_with_a_block_ir_parent_and_a_real_external_module() -> Result<()> {
    let stub = ModDef::new("HeaderOnly");
    stub.add_port("d", IO::Input(4));
    stub.add_port("q", IO::Output(4));
    stub.set_usage(Usage::EmitStubAndStop);
    let implementation = "module Inverter(input [3:0] d, output [3:0] q);
                          assign q = ~d; endmodule";
    let external = ModDef::from_verilog("Inverter", implementation)?;
    let top = ModDef::new("Top");
    let d = top.add_port("d", IO::Input(4));
    for (instance, leaf, output) in [
        ("a", &stub, "floating_a"),
        ("b", &stub, "floating_b"),
        ("real_cell", &external, "inverted"),
    ] {
        let inst = top.instantiate(leaf, Some(instance), None);
        inst.get_port("d").connect(&d);
        inst.get_port("q").export_as(output);
    }
    let parent = top.emit_sv_via_block_ir()?;
    let header = stub.emit_sv_via_block_ir()?;
    assert_eq!(header, stub.emit(EmitOptions::default()));
    assert!(!header.contains("assign "));
    assert_eq!(parent.matches("module HeaderOnly(").count(), 1);
    assert!(!parent.contains("module Inverter("));
    let bench = r#"
        module tb;
          logic [3:0] d;
          wire [3:0] floating_a, floating_b, inverted;
          Top dut(.*);
          initial begin
            for (int i = 0; i < 16; i++) begin
              d = i; #1;
              if (inverted !== ~d) $fatal(1, "bad inverter output");
            end
          end
        endmodule"#;
    simulate(&format!("{parent}\n{implementation}\n{bench}"))
}

#[test]
#[ignore = "requires preserving imported SV parameter bindings in TopStitch and slang-rs"]
fn imported_bindings_link_to_the_requested_external_specialization() -> Result<()> {
    use topstitch::{ParserConfig, Source, VerilogLibrary};
    let source = r#"module Leaf #(parameter W=4, MASK=0, STEP=1, N=W+STEP,
                                  SIGNED_VAL=0, TAG="a", FLAGS=0)
                    (output [N-1:0] q);
                      assign q = MASK;
                    endmodule
                    module Other(output q); assign q=1; endmodule"#;
    let sources = [Source::text(source)];
    let overrides = [
        ("W", "4 + 4"),
        ("MASK", "8'ha5"),
        ("SIGNED_VAL", "5'sh1d"),
        ("TAG", r#""word""#),
        ("FLAGS", "4'b1xz0"),
    ];
    let config = ParserConfig {
        sources: &sources,
        parameters: &overrides,
        ..Default::default()
    };
    let library = VerilogLibrary::from_config(&config)?;
    let mut modules = library.modules()?;
    let from_batch = modules.remove(modules.iter().position(|m| m.get_name() == "Leaf").unwrap());
    let other = modules.pop().unwrap();
    let single = library.module("Leaf")?;
    drop(library);
    let variant = single.clone().parameterize(&[("W", 10)]);
    let top = ModDef::new("Assembly");
    for (module, instance, port, width) in [
        (&single, "u", "q", 9),
        (&from_batch, "v", "r", 9),
        (&variant, "w", "s", 11),
        (&other, "o", "t", 1),
    ] {
        assert_eq!(module.get_port("q").io().width(), width);
        top.instantiate(module, Some(instance), None)
            .get_port("q")
            .export_as(port);
    }
    let bench = r#"module tb;
        wire [8:0] q,r; wire [10:0] s; wire t;
        Assembly dut(.*);
        initial begin #1;
          if (q !== 9'ha5 || r !== 9'ha5 || s !== 11'ha5 || t !== 1) $fatal;
          if (dut.u.W != 8 || dut.w.W != 10 || dut.w.N != 11 || dut.u.SIGNED_VAL >= 0
              || $bits(dut.w.SIGNED_VAL) != 5 || dut.w.SIGNED_VAL != -3
              || dut.w.TAG != "word" || dut.w.FLAGS !== 4'b1xz0) $fatal;
        end endmodule"#;
    for rtl in [top.emit(Default::default()), ir_sv(&top)?] {
        simulate(&format!("{source}\n{rtl}\n{bench}"))?;
    }
    Ok(())
}

#[test]
fn invalid_stub_width_is_an_emission_error() {
    let stub = ModDef::new("EmptyPort");
    stub.add_port("q", IO::Output(0));
    stub.set_usage(Usage::EmitStubAndStop);
    let error = stub.emit_sv_via_block_ir().unwrap_err().to_string();
    assert!(error.contains("EmptyPort.q"), "{error}");
    let parent = ModDef::new("Parent");
    parent.instantiate(&stub, Some("u"), None);
    let error = format!("{:#}", parent.emit_sv_via_block_ir().unwrap_err());
    assert!(error.contains("EmptyPort.q"), "{error}");
}

/// Compiles and runs a self-checking SystemVerilog testbench.
fn simulate(source: &str) -> Result<()> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("design.sv"), source)?;
    for (tool, args) in [
        (
            "iverilog",
            &["-g2012", "-s", "tb", "-o", "sim", "design.sv"][..],
        ),
        ("vvp", &["sim"][..]),
    ] {
        let out = Command::new(tool)
            .args(args)
            .current_dir(dir.path())
            .output()?;
        ensure!(
            out.status.success(),
            "{tool}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

#[test]
fn elaborated_stub_accepts_its_instances_parameter_override() -> Result<()> {
    let base = ModDef::from_verilog(
        "Leaf",
        "module Leaf #(parameter W=1)(output [W-1:0] q); endmodule",
    )?;
    let leaf = base.parameterize(&[("W", 5)]);
    leaf.set_usage(Usage::EmitStubAndStop);
    let inner = leaf.wrap(Some("Inner"), Some("u"));
    let top = inner.wrap(Some("Top"), Some("nested"));
    let rtl = top.emit_sv_via_block_ir()?;
    simulate(&format!(
        "{rtl}\nmodule tb; wire [4:0] q; Top dut(.*); initial if (dut.nested.u.W != 5 || $bits(dut.nested.u.q) != 5) $fatal; endmodule"
    ))
}
