// SPDX-License-Identifier: Apache-2.0

module demo;
    reg [7:0] in0;
    reg [7:0] in1;
    reg [7:0] in2;
    wire [9:0] sum;

    top top_i (.*);

    initial begin
        in0 = 8'd121;
        in1 = 8'd212;
        in2 = 8'd222;
        $display(sum);
        $finish;
    end
endmodule
