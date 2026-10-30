# topstitch

TopStitch is a Rust tool for top-level hardware integration: block connectivity,
floorplanning, and pin placement.

Note: the API is currently under development and is subject to frequent changes.

## Installation

Install Rust if you don't have it already:

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Install these build prerequisites:

- CMake 3.20 or newer and Python 3.
- A C++20 compiler: GCC 11, clang 17, or Xcode 16 or newer.
- Make or Ninja.

For example, on Ubuntu 24.04:

```shell
sudo apt-get update
sudo apt-get install -y build-essential cmake ninja-build python3
```

Add TopStitch to your `Cargo.toml`:

```toml
[dependencies]
topstitch = "0.103.0"
```

The first build can take several minutes because it builds Slang automatically.
To use a prebuilt library, see the [slang-rs documentation](https://github.com/xlsynth/slang-rs/blob/v0.24.0/README.md#optional-precompiled-slang-installation).

## Basic usage

Define two module stubs, instantiate them in a top-level module, and connect
their ports:

```rust
use topstitch::{EmitOptions, IO, ModDef, Usage};

let a_def = ModDef::new("A");
a_def.add_port("data", IO::Output(8));
a_def.set_usage(Usage::EmitStubAndStop);

let b_def = ModDef::new("B");
b_def.add_port("data", IO::Input(8));
b_def.set_usage(Usage::EmitStubAndStop);

let top = ModDef::new("Top");
let a = top.instantiate(&a_def, Some("a"), None);
let b = top.instantiate(&b_def, Some("b"), None);
a.get_port("data").connect(&b.get_port("data"));

println!("{}", top.emit(EmitOptions::default()));
```

This emits stubs for `A` and `B` and a `Top` module connecting them.

```verilog
module A(
  output wire [7:0] data
);

endmodule
module B(
  input wire [7:0] data
);

endmodule
module Top;
  wire [7:0] a_data;
  A a (
    .data(a_data)
  );
  B b (
    .data(a_data)
  );
endmodule
```

## Demo

The [demo](examples/demo.rs) connects three adders to compute `in0 + in1 + in2 + 42`.
Its testbench supplies `121`, `212`, and `222` as inputs.

Clone this repository and run the demo:

```shell
git clone https://github.com/xlsynth/topstitch.git
cd topstitch
cargo run --example demo
```

This produces output in `examples/output/top.sv`.

If you want to simulate the Verilog code that is produced, first install Icarus Verilog if you don't have it already (via Homebrew, apt, etc.). Then `cd` into `examples/tb` and run:

```shell
./demo.sh
```

The simulation prints the sum:

```text
597
```

## Development

We use [pre-commit](https://pre-commit.com/) as part of our CI pipeline.

If you haven't already installed `pre-commit`, you can do so with:

```shell
pip install pre-commit
```

Then install the pre-commit hooks for this repository with:

```shell
pre-commit install
```

The pre-commit hooks will run automatically when you attempt to commit code. You can also run pre-commit checks on-demand with:

```shell
pre-commit run
```
