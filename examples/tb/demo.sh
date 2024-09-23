# SPDX-License-Identifier: Apache-2.0

set -e

iverilog ../output/top.sv demo.sv
vvp a.out
