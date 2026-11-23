// SPDX-License-Identifier: Apache-2.0

module Increment(
  input wire clk,
  input wire [7:0] data,
  output wire [7:0] out
);
  // ===== Pipe stage 0:
  wire [7:0] p0_add_17_comb;
  assign p0_add_17_comb = data + 8'h01;

  // Registers for pipe stage 0:
  reg [7:0] out__output_flop;
  always_ff @ (posedge clk) begin
    out__output_flop <= p0_add_17_comb;
  end
  assign out = out__output_flop;
endmodule
