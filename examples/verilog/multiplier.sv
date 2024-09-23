// SPDX-License-Identifier: Apache-2.0

module multiplier (
    input  wire [15:0] a,
    input  wire [15:0] b,
    output wire [31:0] prod
);
    assign prod = a * b;
endmodule
