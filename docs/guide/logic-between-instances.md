# Add logic between instances

Suppose a partition receives five byte-wide samples. A three-bit index picks one sample, a mask extracts its payload, and a register pipeline aligns it with a downstream block. You can build the whole path from ports and slices. Each operation returns a `Port` you can connect, slice, or pass to another Rust function.

## Select, tag, and delay a sample

```rust
use topstitch::{EmitOptions, IO, LogicPort, ModDef, Port, concat};

/// Puts a nonzero flag in bit 7 and the seven payload bits in bits 6:0.
fn tag_payload(sample: &Port) -> Port {
    let payload = sample & 0x7f;
    concat([payload.or().bit(0), payload.slice(6, 0)])
}

let top = ModDef::new("Readout");
let clk = top.add_port("clk", IO::Input(1));
top.set_default_clock(&clk.declare_clock().unwrap()).unwrap();
let lanes = top.add_port("lanes", IO::Input(40)).subdivide(5);
let slot = top.add_port("slot", IO::Input(3));
let q = top.add_port("q", IO::Output(8));

let sample = slot.select_from_or(&lanes, 0x80);
q.connect(&tag_payload(&sample).delay(3));

let rtl = top.emit(EmitOptions::default());
assert!(rtl.contains("[39:0] lanes"));
assert_eq!(rtl.matches("always_ff").count(), 1);
```

`lanes[0]` supplies the byte for slot zero. Slots five through seven supply the idle byte `0x80`. The mask clears bit 7, and `payload.or()` reduces the remaining payload to a one-bit flag. For example, a selected byte of `0x05` produces `0x85`; an idle byte produces `0x00`. The output shows that tagged byte after three rising edges.

The helper gets its wiring scope from `sample`. Data operands can come from an enclosing module input, a child output, or an `inout` on either boundary. For example, `!&pad` inverts the pad’s resolved value, including any external drive. Calling `pad.tieoff(0xa)` adds a constant driver to a four-bit pad. If another driver supplies `0x5`, both the pad and its inverse read as four unknown bits. All operands in an expression must be visible in the same wiring scope. Borrowing a handle, as in `&a & &b`, lets you reuse the Rust variables for another connection.

## Work with widths and constants

`&`, `|`, `^`, and `!` operate on each bit of a word. The methods `.and()`, `.or()`, and `.xor()` reduce the entire word to a single bit, like SystemVerilog's unary `&x`, `|x`, and `^x`. Use `.or()` for “any bit set,” `.and()` for “all bits set,” and `.xor()` for odd parity.

A signal or slice supplies the hardware width for an integer on the other input. The same rule works for a `BigUint` mask on a bus wider than any native Rust integer. If both data arms are literals, give one an explicit width with `bits`.

```rust
use topstitch::{IO, LogicPort, ModDef, bits, hex};

let top = ModDef::new("MaskedStatus");
let bus = top.add_port("bus", IO::Input(500));
let enabled = top.add_port("enabled", IO::Input(1));
let mask = hex("a".repeat(125)); // 125 hex digits fill the 500-bit word.
let filtered = &bus & mask;
top.add_port("data", IO::Output(500)).connect(&enabled.select(filtered, 0));
top.add_port("status", IO::Output(8)).connect(&enabled.select(bits(8, 3), 0));
```

Literal values must be nonnegative and fit the inferred or explicit width. TopStitch preserves a gate even when an operand is a constant. For example, `&bus | 0` creates an OR with a tied input that a connection query can find.

`concat([high, low])` assembles a new bus in SystemVerilog order. The last part occupies the least significant bits. It accepts a nonempty iterator of ports or slices from the same scope. In the tagging helper both parts are slices, so Rust can put them in the same array. Concatenation is wiring under every primitive policy.

A single part covering a whole port returns the original port, so `concat([&pads])` preserves an `inout` connection. Multiple parts or a partial slice produce an output that reads their values. If a child drives that output, the drive participates in resolution on the output wire and reaches only the output's consumers.

To swap pairs of pins between a four-bit module `inout` (`pads`) and a child `inout` (`io`) while carrying drives in both directions, connect their slices directly.

```rust
let io = child.get_port("io");
io.slice(3, 2).connect(&pads.slice(1, 0));
io.slice(1, 0).connect(&pads.slice(3, 2));
```

## Choose a mux or a priority chain

Use `s.select(when_true, when_false)` for a one-bit control. For an unsigned index, pass the arms in address order. A three-bit selector takes eight arms with `select_from`. With fewer arms, use `select_from_or` and choose the value for every omitted index. Both methods take an iterator, so a vector of slices or `lanes.iter().take(5)` works. An empty iterator with `select_from_or` selects the fallback for every address.

Under `Allow`, an indexed table emits one word-wide `always_comb` block with a plain `case`. Each address has its own arm. For `slot = 3'b00x`, no numeric arm matches: `select_from_or` returns its fallback (the idle byte here), and a full multi-bit `select_from` returns zero. A complete one-bit `select_from([low, high])` emits the same ternary as `s.select(high, low)`. An empty table with a fallback needs only a wire.

Under `RequireMapping`, the same table uses a binary tree of library mux cells. Bit 0 chooses between adjacent arms, bit 1 between pairs, and so on. Eight arms take seven muxes per output bit in three levels. The five-arm example takes six per bit because both indices in the 6–7 subtree return the fallback. The case and the tree select the same word for every known address. For an X or Z select bit, the mapped result depends on the cells. A ternary mux selecting between `8'hA0` and `8'hA1` with an X select produces `8'b1010_000x`.

If each lane has an independent request bit, an index would express a different selection rule. To give the lowest-numbered requester priority, walk the lanes in reverse order and make each request select between its lane and the previous result. Run `cargo run --example primitive_logic` to see that generator alongside an indexed selector.

## Sample on a clock edge

Declare a one-bit clock pin and set it as the owning definition's default, as in the first example. A bare explicit clock selects the rising edge. The reset constructor specifies timing, polarity, and the value loaded into the register.

| Hardware | Expression |
| --- | --- |
| One register on the default clock | `d.flop()` |
| Three cycles on the default clock | `d.delay(3)` |
| One register on a different clock | `d.flop_with(&io_clk)` |
| Synchronous reset on the default clock | `d.flop_with(Reset::sync_high(&rst, 0))` |
| Falling edge and asynchronous active-low reset | `d.flop_with((io_clk.negedge(), Reset::async_low(&rst_n, 0xa5)))` |
| Three stages with the same clock and reset | `d.delay_with(3, (io_clk.negedge(), Reset::async_low(&rst_n, 0xa5)))` |

A reset applies to every stage of a delay. For repeated uses, bind the pair to a local variable and pass `&options` to each operation. An external clock domain describes timing but has no wire that can drive storage. A flop or positive-depth delay needs a physical one-bit clock source.

`d.delay(0)` needs no clock and follows the single-part `concat` rule. A whole port keeps its direction. On a partial slice, such as `pads.slice(3, 0).delay(0)` of a wider bus, the result is an output that reads those bits. `d.delay(1)` samples once. Under the default policy, an eight-bit `d.delay(3)` uses one packed shift register. On the first edge the new sample enters stage 0, and on the third it reaches stage 2.

```systemverilog
logic [2:0][7:0] delay_stages;
always_ff @(posedge clk) begin
  delay_stages <= {delay_stages[1:0], d};
end
assign q = delay_stages[2];
```

Generated sequential definitions declare their `clk` pin and associate their data inputs and outputs with it. A synchronous reset is associated with the same clock, and an asynchronous reset has the `Async` role.

For the final value, use the port returned by `let q = d.delay(3);`. If you also need an intermediate value, `q.get_mod_inst().unwrap().delay_stage(1)` returns the first registered word as a `PortSlice` from that same delay. For a delay with taps, stage zero is the undelayed input and stage three carries the final value. The logical `out_stages` bus holds these words from least to most significant, and unused taps can be left open.

## Set the policy at a partition boundary

Choose the policy on the `ModDef` that owns the operation, before creating its first gate or register. Every instance of that definition shares the policy. Each child definition controls its own body, so a wiring-only parent can instantiate a child that uses RTL internally.

| Policy | What the generator builds |
| --- | --- |
| `PrimitivePolicy::Allow` (default) | Word-wide bitwise operators, mux data paths, and inferred registers. Anonymous operations can appear inline in SV. |
| `PrimitivePolicy::RequireMapping(lib)` | A structural body built from the registered library cells. A missing binding fails when the generator requests the operation. |
| `PrimitivePolicy::Forbid` | Wiring and ordinary module instances. Requesting primitive logic in this definition fails. Connections, tieoffs, concatenation, and zero-cycle delays are legal. |

The example cell library defines a one-bit AND in [logic_cells.sv](../../examples/input/logic_cells.sv).

```systemverilog
module And2(
  input logic A,
  input logic B,
  output logic Y
);
  assign Y = A & B;
endmodule
```

Import that module and bind its two input pins and output pin to `GateMap::and2`. For the eight-bit mask below, TopStitch instantiates eight copies of `And2`, including one with an input tied to zero for bit 7. Compile the cell library alongside the generated SV to supply the `And2` definition.

```rust
use topstitch::{GateMap, IO, ModDef, Primitive, PrimitivePolicy};

// Run from the repository root.
let and2 = ModDef::from_verilog_file("And2", "examples/input/logic_cells.sv").unwrap();
let mut lib = GateMap::new();
lib.and2(and2, ["A", "B"], "Y").unwrap();
let top = ModDef::new("Filter");
top.set_primitive_policy(PrimitivePolicy::RequireMapping(lib)).unwrap();

let data = top.add_port("data", IO::Input(8));
let masked = &data & 0x7f;
top.add_port("q", IO::Output(8)).connect(&masked);

let gate = masked.get_mod_inst().unwrap();
assert_eq!(gate.get_mod_def().primitive_kind(), Some(Primitive::And));
assert_eq!(gate.get_instances().len(), 8);
```

The returned port belongs to an ordinary `ModInst` in the logical hierarchy. Connection extraction can visit the gate and its pins even when default RTL folds it into an expression. Under a map, its physical cell children are already available when the expression returns. If a net needs a recognizable name, use the existing `specify_net_name` API. To name the instance itself, instantiate the primitive explicitly and connect its formal ports. This takes the same policy path as the operator. Wire the instance pins in the parent module. The primitive’s definition can be shared by multiple instances, so its internal wiring is read-only.

The default emitter keeps word-wide operations such as `assign y = a & b;`. It introduces intermediate wires for long expression chains and preserves a shared net when a child’s `inout` pin can drive it. With `EmitOptions::bitblast_assignments`, each continuous assignment drives one bit, including primitive results. A four-bit AND then emits four one-bit AND assignments. This formatting option preserves the primitive graph, gate mapping, and number of clocked processes. An explicitly unused gate result still emits the requested operation.

A hierarchy trace ends at the pin of a behavioral gate or register. Through mapped logic it can reach a physical cell. On a delay mapped to a chain of flops, leaving a stage tap open preserves the point-to-point trace between adjacent flops. Connecting a receiver to the tap creates fanout on the observed bits of that delay instance. If a wrapper forwards the tap, tracing follows the connected bits through the enclosing instances. An unused wrapper output or a net name alone leaves the stage-to-stage path point-to-point. A tap that reaches a receiver or a top-level output counts as fanout. Concatenations and zero-cycle delays preserve bit positions while tracing in either direction. A slice spanning several independently connected concat parts has no single endpoint.

To preserve an implementation for reuse, call `wrap` on the instance's `get_mod_def()`. The wrapper retains its cells. Passing a primitive definition directly to `instantiate` requests the operation under the new parent's policy. Naming a behavioral primitive gives it an identity in the Rust hierarchy. Its logic still emits inside its parent. To give it a separate SV module, emit a stub, or stop descent there, apply the usage setting to an explicit wrapper. Setting a stopping usage directly on the primitive is rejected, and `stub_recursive` skips primitive definitions. A wrapper using `EmitDefinitionAndStop` emits its local expressions and any generated mapped bodies they require, then stops at ordinary child instances. SV and placement use the same generated instance names. In SV, generated module names also avoid collisions with other mapped definitions and referenced library cells.

```rust
use topstitch::{IO, ModDef, Primitive};

let top = ModDef::new("Enable");
let request = top.add_port("request", IO::Input(1));
let ready = top.add_port("ready", IO::Input(1));
let gate = top.instantiate(&top.primitive_def(Primitive::And, 1), Some("accept"), None);
gate.get_port("a").connect(&request);
gate.get_port("b").connect(&ready);
top.add_port("valid", IO::Output(1)).connect(&gate.get_port("y"));
```

The `and2`, `or2`, and `xor2` bindings also implement the corresponding reductions. For six input bits, a reduction uses five cells. Splitting at the midpoint gives paths of two or three levels. For a single input bit, the cell's other input is tied to the identity value (one for AND, zero for OR or XOR), preserving SV reduction behavior on a high-impedance input. Bind `inv` for bitwise inversion and `mux2` for selection. The mux binding names the select pin, both data pins by their true/false roles, and the output. All bits of a word share the selector.

## Map a pipeline to flops or a delay macro

Use `flop` to register a one-bit cell. Its flavor specifies the clock edge and, for resettable cells, the reset timing, polarity, and bit value. A scalar implementation of an eight-bit flop with reset value `0xa5` uses preset cells for bits 7, 5, 2, and 0, and clear cells for the other four bits.

Use `flop_by_width` when the library has a register bank. A plain flavor takes a one-argument factory, such as `|width| Ok(bank.parameterize(&[("WIDTH", width)]))`. For a bank with a reset parameter, use `reset_by_value(kind)`. Its factory receives the width and the entire reset word as a borrowed `BigUint`. Leading zeros are implicit in the value and the width gives the exact number of bits.

```rust
use topstitch::{FlopFlavor, FlopPins, GateMap, ModDef, ParameterValue, ResetKind};

fn register_reset_bank(lib: &mut GateMap, bank: ModDef) -> Result<(), String> {
    lib.flop_by_width(
        FlopFlavor::negedge().reset_by_value(ResetKind::AsyncLow),
        FlopPins { d: "D", clk: "CLK", q: "Q", reset: Some("RN") },
        move |width, value| Ok(bank.parameterize::<ParameterValue>(&[
            ("WIDTH", width.into()),
            ("RESET_VALUE", value.clone().into()),
        ])),
    )
}
```

Finish adding ports to a Rust-built library cell before registering it or returning it from a factory. The binding checks the pins at that point. Every occurrence shares the supplied `ModDef`, so adding a pin afterward would also add an unconnected pin to existing instances.

A shared `GateMap` reuses each successful factory binding across parents, flops, and delay stages. The arguments identify the physical definition: width for a fixed bank, width and the full reset value for a value-aware bank, or width and depth for a whole delay. Give different generated modules distinct names when their implementations differ. If you use `stub_recursive` on parameterized library cells, each emitted stub has its elaborated port widths. Distinct stubs receive distinct module names, and references to the external library keep its original name and parameter overrides.

A factory can build a wide bank from narrower banks. If it requests the same mapping with the same arguments while building it, TopStitch reports a recursive mapping. To use a one-bit library flop as the base case, instantiate that cell directly. TopStitch caches the binding after validating its pins, including any delay stage output. A subsequent request can retry a failed factory or pin check.

Declare the library parameter as `parameter logic [WIDTH-1:0] RESET_VALUE = '0` to carry all data bits. With this mapping, `d.flop_with((clk.negedge(), Reset::async_low(&rst_n, 0xa5)))` on an eight-bit port creates one eight-bit bank with `RESET_VALUE = 8'ha5`. A 500-bit data port can use a distinct reset value for every bit through the same factory.

For a bank that always clears to zero or presets to all ones, register the fixed flavor, such as `FlopFlavor::negedge().reset(ResetKind::AsyncLow, false)`, and a width-only factory. A matching fixed-value bank has priority over the general reset-value factory. When neither bank matches, TopStitch uses the scalar binding for each bit. Once a factory is selected, any factory or pin error is reported to the generator.

A positive-depth delay first checks for a whole-delay macro registered with `lib.delay(flavor, pins, |width, cycles| ...)`. This rule matches an unreset word or a uniform reset matching the flavor. Otherwise each stage uses the same bank or scalar binding as a flop. An eight-bit three-stage delay can therefore use one delay macro, three eight-bit banks, or 24 scalar flops. Reset applies to every stage. A whole-delay binding registered with `delay` supplies only the final output. If the library module also exposes the complete stage bus, register it with `delay_with_stages(flavor, pins, "out_stages", factory)`. Flop-based delay mappings automatically expose every stage. Accessing taps on a final-output-only library mapping fails because the intermediate values are unavailable. A zero-cycle expression returns its input as wiring, so use that input directly for stage zero.

`connect_pipeline` and the interface and feedthrough pipeline helpers use the policy of the module where each pipeline is inserted. A positive-depth connection exposes `in`, `clk`, and `out`. Its `PipelineConfig` names the physical clock port. An explicit mapped instance name must be distinct from ports and wires in the same module. With an automatic name, the Rust handle remains stable even if a later port or wire requires a different SV instance name. For an interface pipeline or a route through several modules, TopStitch checks the signal pairs, clock ports, feedthrough names, and requested mappings before wiring the first signal. If a later signal has a width mismatch or a hop has a clock-name conflict, the generator can correct the request and retry.

From the repository root, run the same five-lane generator both ways to inspect the result. The cell models used by the mapped example are in `examples/input/logic_cells.sv`.

```sh
cargo run --example mapped_logic
cargo run --example mapped_logic -- --mapped
```
