// SPDX-License-Identifier: Apache-2.0

// Bedrock-RTL Delay Line (No Reset)
//
// Delays an input signal by a fixed number of clock cycles.
// There are NumStages pipeline registers. If NumStages is 0,
// then the output is the input. The pipeline registers do not
// get reset.

// Test fixture from xlsynth/bedrock-rtl, licensed under Apache-2.0.
// https://github.com/xlsynth/bedrock-rtl/blob/283a06fad75eacf0d45a5936c69c2fdd9728e2ad/delay/rtl/br_delay_nr.sv
// The module body is Slang preprocessor output with blank lines and trailing whitespace removed.
// From the pinned checkout, regenerate the body with:
//   slang -E --comments -I macros delay/rtl/br_delay_nr.sv | sed -n '/^module br_delay_nr /,/^endmodule/p' | sed -e 's/[[:space:]]*$//' -e '/^$/d'

module br_delay_nr #(
    parameter int Width = 1,  // Must be at least 1
    parameter int NumStages = 0  // Must be at least 0
) (
    // Positive edge-triggered. If NumStages is 0, then only used for assertions.
    // ri lint_check_waive INPUT_NOT_READ HIER_NET_NOT_READ HIER_BRANCH_NOT_READ
    input  logic                          clk,
    input  logic [  Width-1:0]            in,
    // Output of last delay stage (delayed by NumStages cycles).
    output logic [  Width-1:0]            out,
    // Output of each delay stage. Note that out_stages[0] == in, and
    // out_stages[NumStages] == out.
    output logic [NumStages:0][Width-1:0] out_stages
);
  //------------------------------------------
  // Integration checks
  //------------------------------------------
if (!(Width >= 1)) begin : gen__bit_width_must_be_at_least_one_a
__BR_ASSERT_STATIC_FAILED__bit_width_must_be_at_least_one_a __BR_ASSERT_STATIC_FAILED__bit_width_must_be_at_least_one_a ();
end
if (!(NumStages >= 0)) begin : gen__num_stages_must_be_at_least_zero_a
__BR_ASSERT_STATIC_FAILED__num_stages_must_be_at_least_zero_a __BR_ASSERT_STATIC_FAILED__num_stages_must_be_at_least_zero_a ();
end
  //------------------------------------------
  // Implementation
  //------------------------------------------
  logic [NumStages:0][Width-1:0] stages;
  assign stages[0] = in;
  for (genvar i = 1; i <= NumStages; i++) begin : gen_stages
    // ri lint_check_waive BA_NBA_REG
always_ff @(posedge clk) begin
    stages[i] <= stages[i-1];
end
  end
  assign out = stages[NumStages];
  assign out_stages = stages;
  //------------------------------------------
  // Implementation checks
  //------------------------------------------
  if (NumStages == 0) begin : gen_zero_delay
    // ri lint_check_waive ALWAYS_COMB
  end else begin : gen_pos_delay
  end
endmodule : br_delay_nr
