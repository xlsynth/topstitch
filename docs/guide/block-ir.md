# Export XLS Block IR

Use Block IR when another XLS tool needs to inspect or process the structural hierarchy. You can also generate SystemVerilog from that IR instead of using TopStitch's direct SV emitter.

Enable the optional Cargo feature:

```sh
cargo add topstitch --features block-ir
```

The feature requires nightly Rust. TopStitch tests it with `nightly-2026-04-22`; the default features work on stable Rust.

## Export a design

Build and connect the design using the same TopStitch API as for direct SV emission:

```rust
use topstitch::{IO, ModDef};

let top = ModDef::new("UpperNibble");
let byte = top.add_port("byte_in", IO::Input(8));
byte.slice(3, 0).unused();
top.add_port("nibble", IO::Output(4))
    .connect(&byte.slice(7, 4));

let package = top.to_block_ir().unwrap();
assert_eq!(package.get_top_block().unwrap().name, "UpperNibble");
println!("{package}");

let rtl = top.emit_sv_via_block_ir().unwrap();
assert!(rtl.contains("module UpperNibble"));
```

`to_block_ir()` returns an XLSynth PIR package with the selected module as its top block. `emit_sv_via_block_ir()` uses XLSynth codegen to generate SV and includes any explicitly requested interface stubs. Use `ModDef::emit(EmitOptions::default())` for direct SV emission; its formatting options apply to that path.

## External modules and limitations

Imported SV modules and children where `Usage` stops traversal are external instances in the IR. Link their separately supplied SV implementations with the generated wrapper. An interface stub declares the module's ports; it does not implement the module. Likewise, the IR extern's placeholder function is not a behavioral model. Proofs involving an external implementation require separately validated models or the linked RTL.

XLSynth 0.84.0 treats every external output as combinationally dependent on every external input for cycle detection. This can reject valid feedback through a clocked external module, such as wiring a register’s output back to its data input. Use the direct SV emitter for that wiring.

The exporter preserves port and instance names, bit connectivity, and explicitly requested net names. When a name is requested for part of an instance port, the IR-backed SV aliases the whole port. Only the requested bit range is guaranteed to match the direct emitter's named wire.

TopStitch accepts unescaped identifiers matching `[A-Za-z_][A-Za-z0-9_]*` and rejects SystemVerilog keywords for the names it emits, including external module and port names. An imported black box may use escaped identifiers internally. Direct SV accepts names such as `top` and `clock`. These are XLS IR keywords, so Block IR export reports an error if one appears as a native module, native port, instance, or explicitly requested net name. External SV module and port names, parameter names, and qualified port types appear only in the SV binding template and follow the SV rules. For automatically generated wires, the exporter can choose an available suffix instead.

Inout ports are currently rejected; use direct SV emission for designs containing them. Clock-named inputs are ordinary data ports in the structural export: TopStitch does not infer registers, clock domains, or pipeline scheduling.
