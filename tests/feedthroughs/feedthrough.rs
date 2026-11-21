// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_feedthrough() {
    let mod_def = ModDef::new("TestModule");
    mod_def.feedthrough("input_signal", "output_signal", 8);
    assert_eq!(
        mod_def.emit(EmitOptions::default()),
        "\
module TestModule(
  input wire [7:0] input_signal,
  output wire [7:0] output_signal
);
  assign output_signal = input_signal;
endmodule
"
    );
}

#[test]
fn test_intf_feedthrough() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data_out,
          output a_valid_out
      );
      endmodule
      ";

    let module_c_verilog = "
      module ModuleC (
          input [7:0] c_data_in,
          input c_valid_in
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    let a_intf = module_a.def_intf_from_name_underscore("a");

    let module_c = ModDef::from_verilog("ModuleC", module_c_verilog).unwrap();
    module_c.def_intf_from_name_underscore("c");

    let module_b = ModDef::new("ModuleB");
    a_intf.feedthrough(&module_b, "ft_left", "ft_right");

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);

    a_inst
        .get_intf("a")
        .connect(&b_inst.get_intf("ft_left"), false);
    c_inst
        .get_intf("c")
        .crossover(&b_inst.get_intf("ft_right"), "(.*)_in", "(.*)_out");

    assert_eq!(
        top_module.emit(EmitOptions::default()),
        "\
module ModuleB(
  input wire [7:0] ft_left_data_out,
  output wire [7:0] ft_right_data_out,
  input wire ft_left_valid_out,
  output wire ft_right_valid_out
);
  assign ft_right_data_out = ft_left_data_out;
  assign ft_right_valid_out = ft_left_valid_out;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out)
  );
  wire [7:0] ModuleB_i_ft_right_data_out;
  wire ModuleB_i_ft_right_valid_out;
  ModuleB ModuleB_i (
    .ft_left_data_out(ModuleA_i_a_data_out),
    .ft_right_data_out(ModuleB_i_ft_right_data_out),
    .ft_left_valid_out(ModuleA_i_a_valid_out),
    .ft_right_valid_out(ModuleB_i_ft_right_valid_out)
  );
  ModuleC ModuleC_i (
    .c_data_in(ModuleB_i_ft_right_data_out),
    .c_valid_in(ModuleB_i_ft_right_valid_out)
  );
endmodule
"
    );
}

#[test]
fn test_port_feedthrough() {
    let a = ModDef::new("A");
    a.add_port("a", IO::Input(8)).unused();

    let b = ModDef::new("B");
    a.get_port("a").feedthrough(&b, "flipped", "original");

    assert_eq!(
        b.emit(EmitOptions::default()),
        "\
module B(
  output wire [7:0] flipped,
  input wire [7:0] original
);
  assign flipped = original;
endmodule
"
    );
}

#[test]
fn test_port_slice_feedthrough() {
    let a = ModDef::new("A");
    a.add_port("a", IO::Input(8)).unused();

    let b = ModDef::new("B");
    a.get_port("a")
        .slice(7, 4)
        .feedthrough(&b, "flipped", "original");

    assert_eq!(
        b.emit(EmitOptions::default()),
        "\
module B(
  output wire [3:0] flipped,
  input wire [3:0] original
);
  assign flipped = original;
endmodule
"
    );
}

#[test]
fn test_port_feedthrough_pipeline() {
    let a = ModDef::new("A");
    a.add_port("a", IO::Input(8)).unused();

    let b = ModDef::new("B");
    a.get_port("a").feedthrough_pipeline(
        &b,
        "flipped",
        "original",
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 1,
            ..Default::default()
        },
    );

    assert_eq!(
        b.emit(EmitOptions::default()),
        "\
module B(
  output wire [7:0] flipped,
  input wire [7:0] original,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_0001)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(original),
    .out(flipped),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
fn test_port_slice_feedthrough_pipeline() {
    let a = ModDef::new("A");
    a.add_port("a", IO::Input(8)).unused();

    let b = ModDef::new("B");
    a.get_port("a").slice(7, 4).feedthrough_pipeline(
        &b,
        "flipped",
        "original",
        PipelineConfig {
            clk: "clk".to_string(),
            depth: 1,
            ..Default::default()
        },
    );

    assert_eq!(
        b.emit(EmitOptions::default()),
        "\
module B(
  output wire [3:0] flipped,
  input wire [3:0] original,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0004),
    .NumStages(32'h0000_0001)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(original),
    .out(flipped),
    .out_stages()
  );
endmodule
"
    );
}

#[test]
fn selected_array_element_retains_its_fields_on_both_feedthrough_ports() {
    let item = PackedLayout::structure(
        "Item",
        [
            ("tag", PackedLayout::bits(3)),
            ("valid", PackedLayout::bits(1)),
        ],
    );
    let source = ModDef::new("Source");
    let items = source.add_port("items", PackedLayout::array_of(&item, 1).input());
    let relay = ModDef::new("Relay");
    // The element and the array cover the same four wires.
    let element = items.element(0);
    let direct = element.feedthrough(&relay, "direct_out", "direct_in");
    let piped = element.feedthrough_pipeline(
        &relay,
        "piped_out",
        "piped_in",
        PipelineConfig {
            clk: "clk".into(),
            depth: 1,
            ..Default::default()
        },
    );
    for (output, input) in [direct, piped] {
        for port in [output, input] {
            assert_eq!(port.field("tag").to_port_slice(), port.slice(3, 1));
        }
    }
    let sv = relay.emit(EmitOptions::default());
    for name in ["direct", "piped"] {
        assert!(sv.contains(&format!("input wire [3:0] {name}_in")));
        assert!(sv.contains(&format!("output wire [3:0] {name}_out")));
    }
    assert!(sv.contains("assign direct_out = direct_in;"));
    assert!(sv.contains(".in(piped_in)"));
    assert!(sv.contains(".out(piped_out)"));
}

#[test]
fn feedthroughs_return_ports_on_the_target() {
    let source = ModDef::new("Source");
    let items = source.add_port(
        "items",
        PackedLayout::array_of(&PackedLayout::bits(2), 2).input(),
    );
    let relay = ModDef::new("Relay");
    let top = ModDef::new("Top");
    let first = top.instantiate(&relay, Some("first"), None);
    let second = top.instantiate(&relay, Some("second"), None);

    check_feedthrough_ports(&items, &ModDef::new("Standalone"));
    check_feedthrough_ports(&items, &second);
    assert_ne!(first.get_port("port_out"), second.get_port("port_out"));
}

/// Checks both returned handles against the caller's target.
fn check_feedthrough_ports(items: &Port, target: &impl ConvertibleToModDef) {
    let slice = items.slice(1, 0);
    let view = items.element(0);
    let pipe = PipelineConfig {
        depth: 1,
        ..Default::default()
    };
    let routes = [
        items.feedthrough(target, "port_out", "port_in"),
        slice.feedthrough(target, "slice_out", "slice_in"),
        view.feedthrough(target, "view_out", "view_in"),
        items.feedthrough_pipeline(target, "piped_port_out", "piped_port_in", pipe.clone()),
        slice.feedthrough_pipeline(target, "piped_slice_out", "piped_slice_in", pipe.clone()),
        view.feedthrough_pipeline(target, "piped_view_out", "piped_view_in", pipe),
    ];
    for (flipped, original) in routes {
        assert_eq!(flipped, target.get_port(flipped.name()));
        assert_eq!(original, target.get_port(original.name()));
    }
}
