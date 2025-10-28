// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_wrap() {
    let original_mod = ModDef::new("OriginalModule");
    original_mod.add_port("data_in", IO::Input(16));
    original_mod.add_port("data_out", IO::Output(16));

    original_mod.def_intf_from_prefix("data_intf", "data_");

    let wrapped_mod = original_mod.wrap(None, None);

    let top_mod = ModDef::new("TopModule");
    let wrapped_inst = top_mod.instantiate(&wrapped_mod, Some("wrapped_inst"), None);

    wrapped_inst
        .get_intf("data_intf")
        .export_with_prefix("top", "top_");

    original_mod.set_usage(Usage::EmitNothingAndStop);

    assert_eq!(
        top_mod.emit(true),
        "\
module OriginalModule_wrapper(
  input wire [15:0] data_in,
  output wire [15:0] data_out
);
  OriginalModule OriginalModule_i (
    .data_in(data_in),
    .data_out(data_out)
  );
endmodule
module TopModule(
  input wire [15:0] top_in,
  output wire [15:0] top_out
);
  OriginalModule_wrapper wrapped_inst (
    .data_in(top_in),
    .data_out(top_out)
  );
endmodule
"
    );
}
