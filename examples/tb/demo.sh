#!/bin/bash
# SPDX-License-Identifier: Apache-2.0

set -e

iverilog ../input/adder.sv ../output/top.sv demo.sv
vvp a.out
