# Installation

These steps build a Rust program that imports SystemVerilog and emits a wrapper on Ubuntu 26.04 (x86-64). Cargo builds Slang automatically through the `slang-rs` dependency.

## Install the build tools

Run these commands as root, or prefix each with `sudo`:

```sh
apt-get update
apt-get install -y build-essential cmake python3 curl ca-certificates
```

`build-essential` provides GCC, G++ and Make. On other platforms, building Slang requires CMake 3.20 or newer, Python 3, Make or Ninja, and a C++20 compiler (GCC 11+, Clang 17+, or Xcode 16+).

## Install Rust

As your regular user, follow the [official Rust installation instructions](https://rust-lang.org/tools/install/) for the stable toolchain. Open a new terminal and check that both commands print a version:

```sh
rustc --version
cargo --version
```

## Create a project

This example uses the latest published crate. The first Cargo build fetches and verifies the pinned Slang and fmt sources, compiles them with CMake, and links Slang into your program. Allow several minutes for that first build.

```sh
cargo new --bin stitch-demo
cd stitch-demo
cargo add topstitch
```

Replace `src/main.rs` with this generator:

```rust
use topstitch::{EmitOptions, ModDef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let led = ModDef::from_verilog("Led", "module Led(input [2:0] rgb); endmodule")?;
    let panel = led.wrap(Some("Panel"), Some("led"));
    print!("{}", panel.emit(EmitOptions::default()));
    Ok(())
}
```

## Run the generator

```sh
cargo run --locked
```

Look for `module Panel(` and the connection `.rgb(rgb)` on the `Led led` instance. Seeing both means Slang parsed the input port and TopStitch wired it through the wrapper. Continue with [Build your first design](first-design.md) to generate and simulate an adder tree.
