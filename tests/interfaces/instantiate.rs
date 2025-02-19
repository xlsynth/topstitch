// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_autoconnect() {
    let parent_mod = ModDef::new("ParentModule");
    parent_mod.add_port("clk", IO::Input(1));
    parent_mod.add_port("unused", IO::Input(1)).unused();

    let child_mod = ModDef::new("ChildModule");
    child_mod.add_port("clk", IO::Input(1));
    child_mod.add_port("rst", IO::Input(1));
    child_mod.add_port("data", IO::Output(8));

    let autoconnect_ports = ["clk", "rst", "nonexistent"];
    let child_inst =
        parent_mod.instantiate(&child_mod, Some("child_inst"), Some(&autoconnect_ports));
    child_inst.get_port("data").unused();

    child_mod.set_usage(Usage::EmitStubAndStop);

    assert_eq!(
        parent_mod.emit(true),
        "\
module ChildModule(
  input wire clk,
  input wire rst,
  output wire [7:0] data
);

endmodule
module ParentModule(
  input wire clk,
  input wire unused,
  input wire rst
);
  wire child_inst_clk;
  wire child_inst_rst;
  ChildModule child_inst (
    .clk(child_inst_clk),
    .rst(child_inst_rst),
    .data()
  );
  assign child_inst_clk = clk;
  assign child_inst_rst = rst;
endmodule
"
    );
}

#[test]
fn test_instantiate_array() {
    let child_moddef = ModDef::new("child");
    let child_data_out = child_moddef.add_port("data_out", IO::Output(1));
    child_data_out.tieoff(0);

    let parent_moddef = ModDef::new("parent");
    let parent_data_out = parent_moddef.add_port("parent_data_out", IO::Output(6));

    let instances = parent_moddef.instantiate_array(&child_moddef, &[2, 3], None, None);

    // Connect the data_out port of each child instance to a bit in the
    // parent_data_out port
    for (idx, inst) in instances.iter().enumerate() {
        inst.get_port("data_out")
            .connect(&parent_data_out.slice(idx, idx));
    }

    let expected_verilog = "\
module child(
  output wire data_out
);
  assign data_out = 1'h0;
endmodule
module parent(
  output wire [5:0] parent_data_out
);
  wire child_i_0_0_data_out;
  wire child_i_0_1_data_out;
  wire child_i_0_2_data_out;
  wire child_i_1_0_data_out;
  wire child_i_1_1_data_out;
  wire child_i_1_2_data_out;
  child child_i_0_0 (
    .data_out(child_i_0_0_data_out)
  );
  child child_i_0_1 (
    .data_out(child_i_0_1_data_out)
  );
  child child_i_0_2 (
    .data_out(child_i_0_2_data_out)
  );
  child child_i_1_0 (
    .data_out(child_i_1_0_data_out)
  );
  child child_i_1_1 (
    .data_out(child_i_1_1_data_out)
  );
  child child_i_1_2 (
    .data_out(child_i_1_2_data_out)
  );
  assign parent_data_out[0:0] = child_i_0_0_data_out;
  assign parent_data_out[1:1] = child_i_0_1_data_out;
  assign parent_data_out[2:2] = child_i_0_2_data_out;
  assign parent_data_out[3:3] = child_i_1_0_data_out;
  assign parent_data_out[4:4] = child_i_1_1_data_out;
  assign parent_data_out[5:5] = child_i_1_2_data_out;
endmodule
    ";

    // Emit the Verilog code from the parent module
    let emitted_verilog = parent_moddef.emit(true);

    // Assert that the emitted Verilog matches the expected Verilog
    assert_eq!(emitted_verilog.trim(), expected_verilog.trim());
}
