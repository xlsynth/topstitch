# Connect ports and interfaces

First decide where the signal ends. If both endpoints already exist in the same parent, connect them. If a caller of the parent needs access to a child signal, export it from the child instance. A connection joins existing endpoints. An export creates a port on the enclosing module as well as the wiring to it.

## Expose a child's ports

Suppose an imported counter has a one-bit input and a four-bit result. We want the wrapper to present these as `tick` and `count`:

```rust
use topstitch::{EmitOptions, ModDef};

let counter = ModDef::from_verilog("Counter", "
    module Counter(input step, output [3:0] q); endmodule
").unwrap();
let top = ModDef::new("Top");
let digits = top.instantiate(&counter, Some("digits"), None);
digits.get_port("step").export_as("tick");
digits.get_port("q").export_as("count");

let rtl = top.emit(EmitOptions::default());
assert!(rtl.contains(".step(tick)"));
assert!(rtl.contains(".q(count)"));
```

Export from the instance because the new ports belong to its parent. In the emitted `Top`, `tick` is an input and `count` is a four-bit output. If the parent already had a `tick` input, you would connect that existing port to `digits.get_port("step")` instead.

## Connect a group by signal role

An interface belongs to a module definition and maps signal roles (called *function keys*) to its port slices. Interface names are unique within a module definition. Function keys are local to each interface, so `apb0` and `apb1` on the same module can both have a `PADDR` key. On an instance, `get_intf("apb0")` selects which mapping to use.

In this example the meter sends a four-bit `sample` and the reader sends `take` back. The meter’s ports share a prefix, so `def_intf_from_prefix` can infer the keys. The reader’s ports are named `value` and `ack`, so we define the same keys explicitly with `def_intf`:

```rust
use topstitch::{EmitOptions, ModDef};

let meter = ModDef::from_verilog("Meter", "
    module Meter(output [3:0] m_sample, input m_take); endmodule
").unwrap();
let reader = ModDef::from_verilog("Reader", "
    module Reader(input [3:0] value, output ack); endmodule
").unwrap();
meter.def_intf_from_prefix("reading", "m_");
reader.def_intf("measurement", [
    ("sample".into(), ("value".into(), 3, 0)),
    ("take".into(), ("ack".into(), 0, 0)),
].into());

let top = ModDef::new("Instrument");
let dial = top.instantiate(&meter, Some("dial"), None);
let log = top.instantiate(&reader, Some("log"), None);
dial.get_intf("reading").connect(&log.get_intf("measurement"), false);
let rtl = top.emit(EmitOptions::default());
assert!(rtl.contains(".value(dial_m_sample)"));
assert!(rtl.contains(".m_take(log_ack)"));
```

Each entry in the explicit map names a port and its inclusive high and low bit indices. Connecting by key wires `m_sample` to `value[3:0]` and `ack` back to `m_take`. For names that follow a more complex pattern, `def_intf_from_regex` can derive the keys with a replacement string.

When the generator runs, `connect(&other, false)` requires the same keys on both sides. With `true`, it connects only the keys the two sides share. In either case each matched pair must have equal widths and compatible directions. If both definitions omit `take`, their keys can still match because the connection checks the two mappings against each other. A shared protocol helper can check each module against the signals the protocol requires.

For an interface that ends at the parent boundary, call `export_with_name_underscore("panel")` on the child instance's interface instead. For the meter that would create `panel_sample` and `panel_take` with opposite directions.

### Reuse an interface on another module

For a new module that needs the meter’s ports, `meter.get_intf("reading").copy_to(&other)` creates those ports and the interface on `other`. Use `flip_to(&other)` to create the opposite endpoint with the directions reversed. If the target already has ports, define the mapping over those ports with `def_intf`. A Rust helper taking a `&ModDef` can apply the same mapping to several definitions. For configurable widths or optional members, see [Share configuration across modules](hierarchy.md#share-configuration-across-modules).

## Connect part of a bus

Use `slice(msb, lsb)` when the bit positions carry meaning. Use `subdivide(n)` for equal groups. For example, subdividing a 12-bit port into three gives bits 3:0, 7:4, and 11:8 in that order. Connect each slice to a port of the corresponding width. When both slices have the same width, TopStitch can check their electrical compatibility but only your design determines which slice goes to which instance.
