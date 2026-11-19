// SPDX-License-Identifier: Apache-2.0

module And2(
  input logic A,
  input logic B,
  output logic Y
);
  assign Y = A & B;
endmodule

module Or2(input A, B, output Y);
  assign Y = A | B;
endmodule

module Mux2(input S, A, B, output Y);
  assign Y = S ? B : A;
endmodule

module Dff(input D, CLK, output logic Q);
  always_ff @(posedge CLK) Q <= D;
endmodule

module ResetBank #(
  parameter int WIDTH = 1,
  parameter logic [WIDTH-1:0] RESET_VALUE = '0
)(
  input wire CLK, RN,
  input wire [WIDTH-1:0] D,
  output logic [WIDTH-1:0] Q
);
  always_ff @(negedge CLK or negedge RN) begin
    if (!RN) Q <= RESET_VALUE;
    else Q <= D;
  end
endmodule
