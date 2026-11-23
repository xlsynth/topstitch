// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn copying_and_flipping_selected_views_keeps_the_selected_layout() {
    let item = PackedLayout::structure(
        "Item",
        [
            ("tag", PackedLayout::bits(3)),
            ("valid", PackedLayout::bits(1)),
        ],
    );
    let items = PackedLayout::array_of(&item, 2);
    let message = PackedLayout::structure("Message", [("items", items)]);
    let source = ModDef::new("Source");
    let port = source.add_port("message", message.input());
    let copied = ModDef::new("Copied");
    let flipped = ModDef::new("Flipped");
    let top = ModDef::new("Top");
    let inst = top.instantiate(&flipped, None, None);

    // The field covers the same bits as the original port, but has an array layout.
    let copy = port.field("items").copy_to(&copied);
    let flip = port.field("items").element(1).flip_to(&inst);
    assert_eq!(copy, copied.get_port("message"));
    assert_eq!(flip, inst.get_port("message"));
    assert!(matches!(copy.io(), IO::Input(8)));
    assert!(matches!(flip.io(), IO::Output(4)));
    assert_eq!(
        copy.element(1).field("tag").to_port_slice(),
        copy.slice(7, 5)
    );
    assert_eq!(flip.field("valid").to_port_slice(), flip.bit(0));
}

#[test]
fn test_flip_and_copy() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] bus_data_out,
          output bus_valid_out,
          input bus_ready_in
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    let a_intf = module_a.def_intf_from_name_underscore("bus");

    let module_b = ModDef::new("ModuleB");
    let b_intf = a_intf.copy_to(&module_b);
    b_intf.unused_and_tieoff(0);

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    a_inst.get_intf("bus").unused_and_tieoff(0);

    let b_inst = top_module.instantiate(&module_b, None, None);
    b_inst.get_intf("bus").unused_and_tieoff(0);

    let module_c = ModDef::new("ModuleC");
    let c_intf = a_inst.get_intf("bus").flip_to(&module_c);
    c_intf.unused_and_tieoff(0);
    let c_inst = top_module.instantiate(&module_c, None, None);
    c_inst.get_intf("bus").unused_and_tieoff(0);

    assert_eq!(
        top_module.emit(EmitOptions::default()),
        "\
module ModuleB(
  output wire [7:0] bus_data_out,
  output wire bus_valid_out,
  input wire bus_ready_in
);
  assign bus_data_out = 8'h00;
  assign bus_valid_out = 1'h0;
endmodule
module ModuleC(
  input wire [7:0] bus_data_out,
  input wire bus_valid_out,
  output wire bus_ready_in
);
  assign bus_ready_in = 1'h0;
endmodule
module TopModule;
  ModuleA ModuleA_i (
    .bus_data_out(),
    .bus_valid_out(),
    .bus_ready_in(1'h0)
  );
  ModuleB ModuleB_i (
    .bus_data_out(),
    .bus_valid_out(),
    .bus_ready_in(1'h0)
  );
  ModuleC ModuleC_i (
    .bus_data_out(8'h00),
    .bus_valid_out(1'h0),
    .bus_ready_in()
  );
endmodule
"
    );
}

#[test]
fn test_intf_flip_to_uses_slice_width() {
    let source = ModDef::new("Source");
    source.add_port("bus", IO::Input(16));
    let intf = source.def_intf("channel", [("data".into(), ("bus".into(), 7, 4))].into());

    let target = ModDef::new("Target");
    let flipped = intf.flip_to(&target);
    let bus = target.get_port("bus");
    let io = bus.io();
    assert!(
        matches!(io, IO::Output(4)),
        "expected Output(4), got {io:?}"
    );
    assert_eq!(flipped.get("data").unwrap(), bus.slice(3, 0));
}

#[test]
fn test_intf_copy_to_with_prefix() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid,
        input a_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    module_a.def_intf_from_prefix("a", "a_");

    let module_b = ModDef::new("ModuleB");
    let b_intf = module_a
        .get_intf("a")
        .copy_to_with_prefix(&module_b, "b", "");
    b_intf.unused_and_tieoff(0);

    assert_eq!(
        module_b.emit(EmitOptions::default()),
        "\
module ModuleB(
  output wire [31:0] data,
  output wire valid,
  input wire ready
);
  assign data = 32'h0000_0000;
  assign valid = 1'h0;
endmodule
"
    );
}

#[test]
fn test_intf_copy_to_with_name_underscore() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid,
        input a_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog).unwrap();
    module_a.def_intf_from_name_underscore("a");

    let module_b = ModDef::new("ModuleB");
    let b_intf = module_a
        .get_intf("a")
        .copy_to_with_name_underscore(&module_b, "b");
    b_intf.unused_and_tieoff(0);

    assert_eq!(
        module_b.emit(EmitOptions::default()),
        "\
module ModuleB(
  output wire [31:0] b_data,
  output wire b_valid,
  input wire b_ready
);
  assign b_data = 32'h0000_0000;
  assign b_valid = 1'h0;
endmodule
"
    );
}

#[test]
fn interface_copies_preserve_whole_arrays_and_flatten_slices() {
    let source = ModDef::new("Source");
    source.add_port_array("src_lanes", IO::Input(8), [4]);
    source.add_port_array("src_field", IO::Output(8), [4]);
    let intf = source.def_intf(
        "channel",
        [
            ("lanes".into(), ("src_lanes".into(), 31, 0)),
            ("field".into(), ("src_field".into(), 12, 4)),
        ]
        .into(),
    );

    let parent = ModDef::new("Parent");
    let instance = parent.instantiate(&source, None, None);
    let exported = instance.get_intf("channel").export();
    let pass = ModDef::new("Pass");
    let (flipped, original) = intf.feedthrough(&pass, "out", "in");
    for interface in [exported, flipped, original] {
        let lanes = interface.get("lanes").unwrap().get_port();
        let field = interface.get("field").unwrap().get_port();
        assert_eq!(lanes.packed_dimensions(), [4, 8]);
        assert_eq!(lanes.element(1).to_port_slice(), lanes.slice(15, 8));
        assert_eq!(field.packed_dimensions(), [9]);
    }

    for flip in [false, true] {
        let target = ModDef::new("Target");
        let copies = if flip {
            [
                intf.flip_to(&target),
                intf.flip_to_with_prefix(&target, "prefixed", "bus_"),
                intf.flip_to_with_name_underscore(&target, "named"),
            ]
        } else {
            [
                intf.copy_to(&target),
                intf.copy_to_with_prefix(&target, "prefixed", "bus_"),
                intf.copy_to_with_name_underscore(&target, "named"),
            ]
        };
        for (copy, prefix) in copies.into_iter().zip(["src_", "bus_", "named_"]) {
            let lanes = target.get_port(format!("{prefix}lanes"));
            let field = target.get_port(format!("{prefix}field"));
            assert_eq!(copy.get("lanes").unwrap(), lanes.slice(31, 0));
            assert_eq!(copy.get("field").unwrap(), field.slice(8, 0));
            assert_eq!(lanes.packed_dimensions(), [4, 8]);
            assert_eq!(field.packed_dimensions(), [9]);
            assert!(matches!(
                (flip, lanes.io(), field.io()),
                (true, IO::Output(32), IO::Input(9)) | (false, IO::Input(32), IO::Output(9))
            ));
        }
    }
}
