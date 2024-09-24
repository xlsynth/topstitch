// SPDX-License-Identifier: Apache-2.0

module adder #(
    parameter W=8
) (
    input wire [W-1:0] a,
    input wire [W-1:0] b,
    output wire [W:0] sum
);
    assign sum = a + b;
endmodule
