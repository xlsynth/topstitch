// SPDX-License-Identifier: Apache-2.0

module block #(
    parameter M=pack::DefaultM,
    parameter N=pack::DefaultN
) (
    input wire [M-1:0] a,
    output wire [2*M-1:0] b,
    input wire [N-1:0] c,
    output wire [N-1:0] d
);
    assign b[M-1:0] = a;
    assign b[2*M-1:M] = a + 1;
    assign d = c;
endmodule
