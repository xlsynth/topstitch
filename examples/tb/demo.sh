#!/bin/bash
# SPDX-License-Identifier: Apache-2.0

set -e

iverilog -o ../output/demo.vvp ../input/adder.sv ../output/top.sv demo.sv
vvp ../output/demo.vvp
