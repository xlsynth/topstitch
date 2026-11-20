# XLS Block IR

Enable the optional `block-ir` feature in your generator project.

```sh
cargo add topstitch --features block-ir
```

The feature requires nightly Rust. TopStitch tests it with `nightly-2026-04-22`. The default features work on stable Rust.

## Import a block

When XLS produces both Block IR and SystemVerilog, TopStitch can read the block's ports from the IR and use them to wire an instance. For this example, place [increment.block.ir](../../tests/fixtures/block_ir/increment.block.ir) next to your Rust source.

```rust
use topstitch::{EmitOptions, ModDef};

let ir = include_str!("increment.block.ir");
let increment = ModDef::from_block_ir("Increment", ir).unwrap();
let top = increment.wrap(Some("Top"), Some("increment"));
println!("{}", top.emit(EmitOptions::default()));
```

`wrap` creates `Top` with the same `clk`, `data`, and `out` ports and connects them to the `Increment` instance. Compile the emitted wrapper alongside the matching [increment.sv](../../tests/fixtures/block_ir/increment.sv) implementation. To wire the block into an existing parent, pass `&increment` to the parent's `instantiate` method and connect its instance ports as usual.

The block and port names must be simple SystemVerilog identifiers matching the supplied implementation. Import preserves port declaration order, maps an XLS clock to a one-bit input, and accepts input and output `bits[N]` ports with positive widths. Inouts, aggregate ports, and SystemVerilog type overrides are unsupported.

## Inspect an imported hierarchy

Use `BlockIrLibrary` to parse a package once and select blocks by name. By default, `module(name)` imports just that block's interface. Enable `include_hierarchy` to inspect the instance paths declared in the IR.

```rust
use topstitch::{BlockIrLibrary, BlockIrParserConfig};

let ir = r#"package example
#[ffi_proto("""code_template: "OtherModule {fn} ();"
""")]
fn foreign_leaf() -> bits[1] { ret value: bits[1] = literal(value=0, id=1) }
block Leaf() {}
top block Parent() {
  instantiation child(block=Leaf, kind=block)
  instantiation external(foreign_function=foreign_leaf, kind=extern)
}"#;

let library = BlockIrLibrary::from_config(&BlockIrParserConfig {
    ir,
    include_hierarchy: true,
})
.unwrap();
let parent = library.module("Parent").unwrap();
assert_eq!(
    parent.report_all_instances(),
    [("Leaf".into(), "Parent.child".into())]
);

let calls = library.extern_calls("Parent").unwrap();
assert_eq!(calls[0].path, "Parent.external");
assert_eq!(calls[0].function, "foreign_leaf");
assert_eq!(calls[0].sv_template, Some(b"OtherModule {fn} ();".as_slice()));
```

Nested definitions record names and hierarchy. To wire a nested block elsewhere, request its own port-bearing definition with `library.module("Leaf")`. When importing every block with `modules()`, roots have their own interfaces and share nested definitions within that result.

`extern_calls(name)` works with either setting of `include_hierarchy`. It returns each declared extern's instance path, IR function name, and optional SystemVerilog template as bytes. In the example, the function `foreign_leaf` supplies a template that instantiates `OtherModule`.

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
