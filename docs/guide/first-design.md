# Build your first design

The adder demo starts with a SystemVerilog module and generates a parent that wires three instances together. The excerpts below follow one input and one internal wire through the [full Rust generator](demo.rs).

## Import a definition and create an instance

```rust,ignore
let adder_8_bit = ModDef::from_verilog_file("adder", examples.join("input/adder.sv"))?;
let top = ModDef::new("top");
let i00 = top.instantiate(&adder_8_bit, Some("i00"), None);
```

`ModDef::from_verilog_file` reads the ports of the existing `adder` module. `ModDef::new` creates the parent module you’ll generate. `instantiate` adds a child named `i00` to that parent. Reuse the `ModDef` to instantiate another adder. Wire each child through its own `ModInst` handle.

## Expose a port on the parent

The demo exposes the child’s input `a` as `in0` on the parent:

```rust,ignore
i00.get_port("a").export_as("in0");
```

That one call creates a parent port with the child port’s width and direction and connects the two. Written out, the same operation takes two calls:

```rust,ignore
let a = top.add_port("in0", i00.get_port("a").io());
a.connect(&i00.get_port("a"));
```

For the final adder’s `sum`, the demo uses `i11.get_port("sum").export()` to keep the same name on the parent. Use `add_port` when the parent defines its own interface. If the parent port already exists, use `connect` to wire it.

## Wire two instances

```rust,ignore
i00.get_port("sum").connect(&i11.get_port("a"));
i01.get_port("b").tieoff(42);
```

The first line connects two child ports inside `top`. In the generated Verilog, look for the same wire, `i00_sum`, on both instances. The second line drives an input with a constant. The demo uses `parameterize` to give the final adder 9-bit inputs for the first-stage sums.

## Emit and check the design

The generator writes the parent module with `top.emit_to_file(&output_file, EmitOptions::default())`. After [installing the build tools](installation.md), install the checkout example’s additional Ubuntu packages (`git`, `pkg-config`, `libssl-dev`, `libcurl4-openssl-dev`, and `iverilog`). Run these commands from the root of a [TopStitch checkout](https://github.com/xlsynth/topstitch):

```sh
cargo run --locked --example demo
(cd examples/tb && ./demo.sh)
```

Open `examples/output/top.sv` to inspect the connections. The testbench compiles that file together with the original adder RTL and prints `597` on success. Continue with [Connect ports and interfaces](connections.md) to wire groups of signals.
