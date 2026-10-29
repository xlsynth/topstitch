// SPDX-License-Identifier: Apache-2.0

use crate::common::module_from_text;

use topstitch::*;

#[test]
fn test_find_direct_feedthrough_with_get_connections() {
    let top = ModDef::new("Top");
    let input = top.add_port("input", IO::Input(8));
    let output = top.add_port("output", IO::Output(8));
    input.slice(7, 4).connect(&output.slice(3, 0));

    let buffer = ModDef::new("Buffer");
    buffer.feedthrough("input", "output", 4);
    let inst = top.instantiate(&buffer, Some("buffer"), None);
    input.slice(3, 0).connect(&inst.get_port("input"));
    inst.get_port("output").connect(&output.slice(7, 4));

    let mut direct_feedthroughs = Vec::new();
    for connection in input.get_connections() {
        if let ConnectedItem::PortSlice(other) = connection.other
            && matches!(other.get_port(), Port::ModDef { .. })
        {
            direct_feedthroughs.push((connection.this, other));
        }
    }
    assert_eq!(
        direct_feedthroughs,
        [(input.slice(7, 4), output.slice(3, 0))]
    );
}

#[test]
fn test_get_connections_for_tieoff_unused_and_wire() {
    // Top.left and Top.right are instances of Pair. Changes made through
    // Top.left.dst.data should appear on Pair.dst.data when queried from either instance.

    let leaf = ModDef::new("Leaf");
    leaf.add_port("data", IO::InOut(5));
    let pair = ModDef::new("Pair");
    let dst = pair.instantiate(&leaf, Some("dst"), None);
    let local_data = dst.get_port("data");
    let top = ModDef::new("Top");
    let left = top.instantiate(&pair, Some("left"), None);
    let right = top.instantiate(&pair, Some("right"), None);
    let left_data = left.get_instance("dst").get_port("data");
    let right_data = right.get_instance("dst").get_port("data");
    left_data.slice(1, 0).tieoff(0b10);
    left_data.bit(2).unused();
    left_data.slice(4, 3).specify_net_name("data_net");

    let connections = right_data.slice(3, 1).get_connections();
    assert_eq!(connections, left_data.slice(3, 1).get_connections());
    assert_eq!(connections.len(), 3);
    assert_eq!(connections[0].this, local_data.bit(1));
    assert_eq!(
        connections[0].other,
        ConnectedItem::Tieoff(Tieoff {
            value: 1.into(),
            width: 1,
        })
    );
    assert_eq!(connections[1].this, local_data.bit(2));
    assert!(matches!(connections[1].other, ConnectedItem::Unused(_)));
    assert_eq!(connections[2].this, local_data.bit(3));
    assert_eq!(
        connections[2].other,
        ConnectedItem::Wire(Wire {
            name: "data_net".into(),
            width: 5,
            msb: 3,
            lsb: 3,
        })
    );
}

#[test]
fn test_get_connections_returns_a_snapshot() {
    let top = ModDef::new("Top");
    let input = top.add_port("input", IO::Input(2));
    let output = top.add_port("output", IO::Output(2));
    assert!(input.get_connections().is_empty());

    input.bit(0).connect(&output.bit(1));
    let snapshot: Vec<PortSliceConnection> = input.get_connections();
    assert!(input.bit(1).get_connections().is_empty());

    input.bit(1).connect(&output.bit(0));
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].other, output.bit(1));
    assert_eq!(input.get_connections().len(), 2);
}

#[test]
fn test_get_connections_returns_paths_relative_to_the_containing_definition() {
    // Top.left and Top.right are instances of Pair, which contains src and dst.
    // We connect Top.left.src.out to Top.left.dst.in. Querying Top.right.src.out
    // shows Pair.src.out connected to Pair.dst.in.

    let leaf = ModDef::new("Leaf");
    leaf.add_port("out", IO::Output(1));
    leaf.add_port("in", IO::Input(1));

    let pair = ModDef::new("Pair");
    let pair_input = pair.add_port("input", IO::Input(1));
    let pair_output = pair.add_port("output", IO::Output(1));
    let src = pair.instantiate(&leaf, Some("src"), None);
    let dst = pair.instantiate(&leaf, Some("dst"), None);

    let top = ModDef::new("Top");
    let left = top.instantiate(&pair, Some("left"), None);
    let right = top.instantiate(&pair, Some("right"), None);
    left.get_instance("src")
        .get_port("out")
        .connect(&left.get_instance("dst").get_port("in"));
    pair_input.connect(&left.get_instance("src").get_port("in"));
    left.get_instance("dst")
        .get_port("out")
        .connect(&pair_output);

    let right_src = right.get_instance("src");
    let connections = right_src.get_port("out").get_connections();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].this, src.get_port("out").bit(0));
    assert_eq!(connections[0].other, dst.get_port("in").bit(0));

    // Pair.input is the module's own input and drives Pair.src.in.
    // Asking what drives Top.right.src.in therefore returns Pair.input.
    let connections = right_src.get_port("in").get_connections();
    assert_eq!(connections[0].this, src.get_port("in").bit(0));
    assert_eq!(connections[0].other, pair_input.bit(0));

    assert_eq!(
        pair_output.get_connections()[0].other,
        dst.get_port("out").bit(0)
    );
}

#[test]
fn test_connect_requires_identical_parent_instance_paths() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    // Top.left and Top.right are instances of Wrapper. Each Wrapper contains a
    // Pair named pair, and Pair contains two Leaf instances named src and dst.
    //
    // Pair.src.out can connect to Pair.dst.in. The calls in the loop are rejected.

    let leaf = ModDef::new("Leaf");
    let leaf_out = leaf.add_port("out", IO::Output(1));
    leaf.add_port("in", IO::Input(1));
    let pair = ModDef::new("Pair");
    let local_out = pair.instantiate(&leaf, Some("src"), None).get_port("out");
    let local_in = pair.instantiate(&leaf, Some("dst"), None).get_port("in");

    let wrapper = ModDef::new("Wrapper");
    let partial = wrapper.instantiate(&pair, Some("pair"), None);
    let top = ModDef::new("Top");
    let left = top
        .instantiate(&wrapper, Some("left"), None)
        .get_instance("pair");
    let right = top
        .instantiate(&wrapper, Some("right"), None)
        .get_instance("pair");
    let left_out = left.get_instance("src").get_port("out");
    let right_in = right.get_instance("dst").get_port("in");
    let partial_in = partial.get_instance("dst").get_port("in");

    for (source, destination) in [
        // Top.left.pair.src.out -> Top.right.pair.dst.in
        (&left_out, &right_in),
        // Top.left.pair.src.out -> Wrapper.pair.dst.in
        (&left_out, &partial_in),
        // Pair.src.out -> Top.right.pair.dst.in
        (&local_out, &right_in),
        // Leaf.out -> Pair.src.out
        (&leaf_out, &local_out),
        // Pair.src.out -> Leaf.out
        (&local_out, &leaf_out),
    ] {
        assert!(catch_unwind(AssertUnwindSafe(|| source.connect(destination))).is_err());
        assert!(source.get_connections().is_empty());
        assert!(destination.get_connections().is_empty());
    }

    local_out.connect(&local_in);
    assert_eq!(local_out.get_connections()[0].other, local_in.bit(0));
}

#[test]
fn test_intf_connect_through() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data,
          output a_valid,
          input a_ready
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e_data,
          input e_valid,
          output e_ready
      );
      endmodule
      ";

    let module_a = module_from_text("ModuleA", module_a_verilog);
    module_a.def_intf_from_name_underscore("a");

    let module_e = module_from_text("ModuleE", module_e_verilog);
    module_e.def_intf_from_name_underscore("e");

    let module_b = ModDef::new("ModuleB");
    let module_c = ModDef::new("ModuleC");
    let module_d = ModDef::new("ModuleD");

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);
    let d_inst = top_module.instantiate(&module_d, None, None);
    let e_inst = top_module.instantiate(&module_e, None, None);

    a_inst.get_intf("a").connect_through(
        &e_inst.get_intf("e"),
        &[&b_inst, &c_inst, &d_inst],
        "ft",
        false,
    );

    assert_eq!(
        top_module.emit(EmitOptions::default()),
        "\
module ModuleB(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid,
  output wire ft_flipped_a_ready,
  input wire ft_original_a_ready
);
  assign ft_original_a_data = ft_flipped_a_data;
  assign ft_original_a_valid = ft_flipped_a_valid;
  assign ft_flipped_a_ready = ft_original_a_ready;
endmodule
module ModuleC(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid,
  output wire ft_flipped_a_ready,
  input wire ft_original_a_ready
);
  assign ft_original_a_data = ft_flipped_a_data;
  assign ft_original_a_valid = ft_flipped_a_valid;
  assign ft_flipped_a_ready = ft_original_a_ready;
endmodule
module ModuleD(
  input wire [7:0] ft_flipped_a_data,
  output wire [7:0] ft_original_a_data,
  input wire ft_flipped_a_valid,
  output wire ft_original_a_valid,
  output wire ft_flipped_a_ready,
  input wire ft_original_a_ready
);
  assign ft_original_a_data = ft_flipped_a_data;
  assign ft_original_a_valid = ft_flipped_a_valid;
  assign ft_flipped_a_ready = ft_original_a_ready;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data;
  wire ModuleA_i_a_valid;
  wire ModuleB_i_ft_flipped_a_ready;
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid),
    .a_ready(ModuleB_i_ft_flipped_a_ready)
  );
  wire [7:0] ModuleB_i_ft_original_a_data;
  wire ModuleB_i_ft_original_a_valid;
  wire ModuleC_i_ft_flipped_a_ready;
  ModuleB ModuleB_i (
    .ft_flipped_a_data(ModuleA_i_a_data),
    .ft_original_a_data(ModuleB_i_ft_original_a_data),
    .ft_flipped_a_valid(ModuleA_i_a_valid),
    .ft_original_a_valid(ModuleB_i_ft_original_a_valid),
    .ft_flipped_a_ready(ModuleB_i_ft_flipped_a_ready),
    .ft_original_a_ready(ModuleC_i_ft_flipped_a_ready)
  );
  wire [7:0] ModuleC_i_ft_original_a_data;
  wire ModuleC_i_ft_original_a_valid;
  wire ModuleD_i_ft_flipped_a_ready;
  ModuleC ModuleC_i (
    .ft_flipped_a_data(ModuleB_i_ft_original_a_data),
    .ft_original_a_data(ModuleC_i_ft_original_a_data),
    .ft_flipped_a_valid(ModuleB_i_ft_original_a_valid),
    .ft_original_a_valid(ModuleC_i_ft_original_a_valid),
    .ft_flipped_a_ready(ModuleC_i_ft_flipped_a_ready),
    .ft_original_a_ready(ModuleD_i_ft_flipped_a_ready)
  );
  wire [7:0] ModuleD_i_ft_original_a_data;
  wire ModuleD_i_ft_original_a_valid;
  wire ModuleE_i_e_ready;
  ModuleD ModuleD_i (
    .ft_flipped_a_data(ModuleC_i_ft_original_a_data),
    .ft_original_a_data(ModuleD_i_ft_original_a_data),
    .ft_flipped_a_valid(ModuleC_i_ft_original_a_valid),
    .ft_original_a_valid(ModuleD_i_ft_original_a_valid),
    .ft_flipped_a_ready(ModuleD_i_ft_flipped_a_ready),
    .ft_original_a_ready(ModuleE_i_e_ready)
  );
  ModuleE ModuleE_i (
    .e_data(ModuleD_i_ft_original_a_data),
    .e_valid(ModuleD_i_ft_original_a_valid),
    .e_ready(ModuleE_i_e_ready)
  );
endmodule
"
    );
}

#[test]
fn test_intf_connect_except() {
    let module_a_verilog = "
      module ModuleA (
          output a_data,
          output a_valid,
          input a_ready
      );
      endmodule
      ";

    let module_b_verilog = "
      module ModuleB (
          input b_data,
          input b_valid,
          output b_ready
      );
      endmodule
      ";

    let module_a = module_from_text("ModuleA", module_a_verilog);
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = module_from_text("ModuleB", module_b_verilog);
    module_b.def_intf_from_prefix("b_intf", "b_");

    let top_module = ModDef::new("TopModule");

    let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);
    let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

    let a_intf = a_inst.get_intf("a_intf");
    let b_intf = b_inst.get_intf("b_intf");

    a_intf.connect_except(&b_intf, Some(&["ready"]));
    a_intf.get("ready").unwrap().unused_or_tieoff(0);
    b_intf.get("ready").unwrap().unused_or_tieoff(0);

    let emitted = top_module.emit(EmitOptions::default());
    assert_eq!(
        emitted,
        "\
module TopModule;
  wire inst_a_a_data;
  wire inst_a_a_valid;
  ModuleA inst_a (
    .a_data(inst_a_a_data),
    .a_valid(inst_a_a_valid),
    .a_ready(1'h0)
  );
  ModuleB inst_b (
    .b_data(inst_a_a_data),
    .b_valid(inst_a_a_valid),
    .b_ready()
  );
endmodule
"
    );
}

#[test]
fn test_connect_through() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e
      );
      endmodule
      ";

    let module_a = module_from_text("ModuleA", module_a_verilog);
    let module_e = module_from_text("ModuleE", module_e_verilog);

    let module_b = ModDef::new("ModuleB");
    let module_c = ModDef::new("ModuleC");
    let module_d = ModDef::new("ModuleD");

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);
    let d_inst = top_module.instantiate(&module_d, None, None);
    let e_inst = top_module.instantiate(&module_e, None, None);

    a_inst
        .get_port("a")
        .connect_through(&e_inst.get_port("e"), &[&b_inst, &c_inst, &d_inst], "ft");

    assert_eq!(
        top_module.emit(EmitOptions::default()),
        "\
module ModuleB(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original
);
  assign ft_original = ft_flipped;
endmodule
module ModuleC(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original
);
  assign ft_original = ft_flipped;
endmodule
module ModuleD(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original
);
  assign ft_original = ft_flipped;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a;
  ModuleA ModuleA_i (
    .a(ModuleA_i_a)
  );
  wire [7:0] ModuleB_i_ft_original;
  ModuleB ModuleB_i (
    .ft_flipped(ModuleA_i_a),
    .ft_original(ModuleB_i_ft_original)
  );
  wire [7:0] ModuleC_i_ft_original;
  ModuleC ModuleC_i (
    .ft_flipped(ModuleB_i_ft_original),
    .ft_original(ModuleC_i_ft_original)
  );
  wire [7:0] ModuleD_i_ft_original;
  ModuleD ModuleD_i (
    .ft_flipped(ModuleC_i_ft_original),
    .ft_original(ModuleD_i_ft_original)
  );
  ModuleE ModuleE_i (
    .e(ModuleD_i_ft_original)
  );
endmodule
"
    );
}

#[test]
fn test_connect_through_generic() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e
      );
      endmodule
      ";

    let module_a = module_from_text("ModuleA", module_a_verilog);
    let module_e = module_from_text("ModuleE", module_e_verilog);

    let module_b = ModDef::new("ModuleB");
    let module_c = ModDef::new("ModuleC");
    let module_d = ModDef::new("ModuleD");

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);
    let d_inst = top_module.instantiate(&module_d, None, None);
    let e_inst = top_module.instantiate(&module_e, None, None);

    let cfg = |depth: usize| {
        Some(PipelineConfig {
            clk: "clk".to_string(),
            depth,
            ..Default::default()
        })
    };

    a_inst.get_port("a").connect_through_generic(
        &e_inst.get_port("e"),
        &[(&b_inst, cfg(0xab)), (&c_inst, None), (&d_inst, cfg(0xef))],
        "ft",
    );

    b_inst.get_port("clk").tieoff(0);
    d_inst.get_port("clk").tieoff(0);

    assert_eq!(
        top_module.emit(EmitOptions::default()),
        "\
module ModuleB(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ab)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_flipped),
    .out(ft_original),
    .out_stages()
  );
endmodule
module ModuleC(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original
);
  assign ft_original = ft_flipped;
endmodule
module ModuleD(
  input wire [7:0] ft_flipped,
  output wire [7:0] ft_original,
  input wire clk
);
  br_delay_nr #(
    .Width(32'h0000_0008),
    .NumStages(32'h0000_00ef)
  ) pipeline_conn_0 (
    .clk(clk),
    .in(ft_flipped),
    .out(ft_original),
    .out_stages()
  );
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a;
  ModuleA ModuleA_i (
    .a(ModuleA_i_a)
  );
  wire [7:0] ModuleB_i_ft_original;
  ModuleB ModuleB_i (
    .ft_flipped(ModuleA_i_a),
    .ft_original(ModuleB_i_ft_original),
    .clk(1'h0)
  );
  wire [7:0] ModuleC_i_ft_original;
  ModuleC ModuleC_i (
    .ft_flipped(ModuleB_i_ft_original),
    .ft_original(ModuleC_i_ft_original)
  );
  wire [7:0] ModuleD_i_ft_original;
  ModuleD ModuleD_i (
    .ft_flipped(ModuleC_i_ft_original),
    .ft_original(ModuleD_i_ft_original),
    .clk(1'h0)
  );
  ModuleE ModuleE_i (
    .e(ModuleD_i_ft_original)
  );
endmodule
"
    );
}

#[test]
fn test_todo_jam_connect_port_with_port_list() {
    let top_module = ModDef::new("TopModule");
    top_module.add_port("lhs", IO::Input(6));
    top_module.add_port("rhs0", IO::Output(2));
    top_module.add_port("rhs1", IO::Output(3));

    top_module
        .get_port("lhs")
        .todo_jam_connect(&[top_module.get_port("rhs0"), top_module.get_port("rhs1")]);

    assert_eq!(
        top_module.emit(EmitOptions::default()),
        "\
module TopModule(
  input wire [5:0] lhs,
  output wire [1:0] rhs0,
  output wire [2:0] rhs1
);
  assign rhs0 = lhs[1:0];
  assign rhs1 = lhs[4:2];
endmodule
"
    );
}

#[test]
fn test_todo_jam_connect_port_slice_with_intf() {
    let top_module = ModDef::new("TopModule");
    top_module.add_port("src", IO::Input(3));
    top_module.add_port("r_data", IO::Output(2));
    top_module.add_port("r_flag", IO::Output(1));

    let right_intf = top_module.def_intf_from_prefix("right", "r_");

    top_module.get_port("src").bit(0).unused();
    top_module
        .get_port("src")
        .slice(2, 1)
        .todo_jam_connect(&right_intf);

    assert_eq!(
        top_module.emit(EmitOptions::default()),
        "\
module TopModule(
  input wire [2:0] src,
  output wire [1:0] r_data,
  output wire r_flag
);
  assign r_data = src[2:1];
  assign r_flag = 1'h0;
endmodule
"
    );
}

#[test]
fn test_todo_jam_connect_intf_with_intf_list() {
    let top_module = ModDef::new("TopModule");
    top_module.add_port("l_data", IO::Input(2));
    top_module.add_port("l_valid", IO::Input(2));
    top_module.add_port("r0_data", IO::Output(2));
    top_module.add_port("r1_valid", IO::Output(2));
    top_module.add_port("r2_flag", IO::Output(1));

    let left = top_module.def_intf_from_prefix("left", "l_");
    let right0 = top_module.def_intf_from_prefix("right0", "r0_");
    let right1 = top_module.def_intf_from_prefix("right1", "r1_");
    let right2 = top_module.def_intf_from_prefix("right2", "r2_");

    left.todo_jam_connect(&[right0, right1, right2]);

    assert_eq!(
        top_module.emit(EmitOptions::default()),
        "\
module TopModule(
  input wire [1:0] l_data,
  input wire [1:0] l_valid,
  output wire [1:0] r0_data,
  output wire [1:0] r1_valid,
  output wire r2_flag
);
  assign r0_data = l_data;
  assign r1_valid = l_valid;
  assign r2_flag = 1'h0;
endmodule
"
    );
}
