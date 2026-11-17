# Organize a module hierarchy

A `ModDef` describes a module that can be instantiated many times. A `ModInst` identifies one of those instances. In the [adder tree](first-design.md), changing the connections of `i00` affects that use of the adder. Adding a port to a shared definition changes the interface seen at every instance of it.

Put a connection in the module where its two endpoints are visible. To reach inside a grandchild from above, decide what its immediate parent should expose. The exported port gives the next level an interface it can connect without knowing the grandchild's instance name.

## Wire an array of instances

Suppose `Top` has an eight-bit input, `data`, and each `Lane` takes a four-bit input, `sample`. Create two instances from the same `Lane` definition and give each one half of `data`:

```rust
# use topstitch::{EmitOptions, IO, ModDef};
# let lane = ModDef::from_verilog("Lane", "module Lane(input [3:0] sample); endmodule").unwrap();
# let top = ModDef::new("Top");
let data = top.add_port("data", IO::Input(8));
let lanes = top.instantiate_array(&lane, &[2], Some("lane"), None);
let slices = data.subdivide(lanes.len());

for (lane, slice) in lanes.iter().zip(slices) {
    lane.get_port("sample").connect(&slice);
}
# let rtl = top.emit(EmitOptions::default());
# assert!(rtl.contains("Lane lane_0 (\n    .sample(data[3:0])"), "{rtl}");
# assert!(rtl.contains("Lane lane_1 (\n    .sample(data[7:4])"), "{rtl}");
```

`subdivide` returns the low bits first, so `lane_0.sample` gets `data[3:0]` and `lane_1.sample` gets `data[7:4]`.

## Share configuration across modules

Keep choices that several generator functions need in a Rust struct. For example, two modules can expose different stream interfaces while getting their data width and member names from one configuration:

```rust
use topstitch::{IO, Intf, ModDef};

struct StreamConfig {
    data_bits: usize,
}

impl StreamConfig {
    fn add_source(&self, module: &ModDef, name: &str) -> Intf {
        module.add_port(format!("{name}_data"), IO::Output(self.data_bits));
        module.add_port(format!("{name}_valid"), IO::Output(1));
        module.def_intf_from_name_underscore(name)
    }
}

let stream = StreamConfig { data_bits: 16 };
let sensor = ModDef::new("Sensor");
let replay = ModDef::new("Replay");
stream.add_source(&sensor, "samples");
stream.add_source(&replay, "events");
# assert!(matches!(sensor.get_port("samples_data").io(), IO::Output(16)));
# assert!(matches!(replay.get_port("events_data").io(), IO::Output(16)));
# assert!(sensor.has_intf("samples") && replay.has_intf("events"));
```

Pass `&StreamConfig` to builders that need those choices. If later code needs both an endpoint and its configuration, a generator-owned struct can hold the `Intf` and a reference to the config together. The same pattern works with a `ModDef`. For imported RTL, pass the relevant config value to `parameterize` when specializing a SystemVerilog parameter.

## Wrap an existing module

Use `wrap` when you want a new hierarchy level that initially presents the entire original interface. It creates a module, instantiates the original, and connects corresponding ports. That gives you a separate definition to work on. If the new boundary should expose just a few signals, create a `ModDef` and [export](connections.md) those signals individually.

```rust
use topstitch::{EmitOptions, ModDef};

let display = ModDef::from_verilog("Display", "
    module Display(input [6:0] segments); endmodule
").unwrap();
let panel = display.wrap(Some("Panel"), Some("screen"));
let rtl = panel.emit(EmitOptions::default());
assert!(rtl.contains("module Panel("));
assert!(rtl.contains("Display screen ("));
assert!(rtl.contains(".segments(segments)"));
```

## Decide who supplies each module body

An imported RTL block already has an implementation. Supply that RTL alongside the generated wrapper when compiling the design. For modules you construct in Rust, `Usage` controls what gets emitted when the generator reaches the definition:

- `EmitDefinitionAndDescend` emits the body and visits its children. This is the default for a newly constructed module.
- `EmitNothingAndStop` leaves the instance in its parent and expects its module definition to come from elsewhere.
- `EmitStubAndStop` supplies a declaration with ports and an empty body. This is useful for inspecting a proposed interface.
- `EmitDefinitionAndStop` emits this definition and leaves its children to another source.

If a simulator reports an unknown module, find the instance in the emitted parent, then check which source is supposed to define its module. If a simulator accepts a design built with empty stubs, it has established that the interfaces elaborate. To test the computation, provide the functional RTL.

When several modules or packages come from the same SystemVerilog source set, `VerilogLibrary::from_config` lets you compile the set once and look up individual definitions. See the [library import examples](https://github.com/xlsynth/topstitch/blob/99138be87453717c6868c18fb99f2a1c5f7ee3a3/tests/verilog_library.rs).
