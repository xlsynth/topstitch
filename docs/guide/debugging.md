# Inspect and trace connections

## Which bits did the generator connect?

In the [adder demo](first-design.md), `i01.b` is an eight-bit input tied to 42. To reproduce a partial connection, change `i01.get_port("b").tieoff(42)` to `i01.get_port("b").slice(6, 0).tieoff(42)`, then run `cargo run --locked --example demo`. Verilog emission stops with:

```text
top.i01.b[7] is unconnected
```

To see what the generator recorded, put this probe just before emission:

```rust,ignore
for connection in i01.get_port("b").get_connections() {
    eprintln!("{:?} = {:?}", connection.this, connection.other);
}
```

```text
top.i01.b[6:0] = Tieoff(0x(2A))
```

The constant has the right value. The slice leaves bit 7 floating. Restore the whole-port `tieoff(42)` and rerun. The probe now reports `[7:0]` and emission succeeds. In your own generator, `top.emit(EmitOptions::default())` runs the same connectivity checks in memory. In this version, `ModDef::validate()` is a no-op.

## Where does this bit go through a wrapper?

The demo exports two eight-bit inputs as `in0` and `in2`. Swapping those export names still produces valid Verilog. To check which adder actually receives `in2`, wrap the finished tree and trace bit 0 from the new boundary:

```rust,ignore
let system = top.wrap(Some("System"), Some("tree"));
let bit = system.get_port("in2").bit(0);
eprintln!("{:?}", bit.trace_through_hierarchy());
```

With the original exports, the trace prints `Some(System.tree.i01.a[0:0])`. Swap the names and it prints `Some(System.tree.i00.a[0:0])`. A direct `bit.get_connections()` query sees the same first hop, `System.tree.in2[0:0]`, in both cases.

Use the trace for a point-to-point path. It follows wiring through generated modules marked `EmitDefinitionAndDescend` and stops at the imported adder’s port. If a signal fans out, tracing can panic. Use `get_connections()` at the branch to see each directly connected endpoint.
