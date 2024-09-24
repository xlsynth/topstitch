# topstitch
Stitch together Verilog modules with Rust.

Note: the API is currently under development and is subject to frequent changes.

## Installation

Install Rust if you don't have it already:

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then clone this repository:

```shell
git clone https://github.com/xlsynth/topstitch.git
```

## Demo

A basic demo of hierarchy, multiple instantiation, and connection is in [examples/demo.rs](examples/demo.rs).

Run the demo with:

```shell
cargo run --example demo
```

This produces output in `examples/output/top.sv`. Note: the first time that you build this project, it might take several minutes to build dependencies.

If you want to simulate the Verilog code that is produced, first install Icarus Verilog if you don't have it already (via Homebrew, apt, etc.). Then `cd` into `examples/tb` and run:

```shell
./demo.sh
```

This will produce the following output:
```shell
 597
demo.sv:16: $finish called at 0 (1s)
```

The output `597` is expected, as this is the sum of inputs `121`, `212`, and `222`, along with a constant `42`.
