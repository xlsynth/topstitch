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

The block and port names must be simple SystemVerilog identifiers matching the supplied implementation. Import preserves port declaration order, maps an XLS clock to a one-bit input, and accepts input, output, and inout `bits[N]` ports with positive widths. Aggregate ports and input/output SystemVerilog type overrides are unsupported. An inout type override is retained in the original Block IR package; TopStitch wires the pin as a bit vector of the same width.

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

`extern_calls(name)` reports foreign-function calls with either setting of `include_hierarchy`. In the example, the function `foreign_leaf` supplies a template that instantiates `OtherModule`.

Opaque and behavioral external module instances appear in hierarchy reports under their RTL module names. Supported interfaces include reusable ports, raw SV parameter overrides, and input casts. Use `library.extern_module("Parent", "vendor")` to import a particular instance directly, even without hierarchy import; this preserves that instance’s parameter specialization. Unsupported interfaces remain name-only hierarchy entries and cannot be instantiated elsewhere. External clocks become ordinary one-bit inputs so TopStitch can wire them explicitly. Clockless behavioral models are retained for Block IR export; clocked models use an opaque external interface when re-exported.

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

Imported SV modules and stopped generated children become opaque external block instances, with their port directions, widths, parameter overrides, and enum input casts recorded in the IR. Link their separately supplied SV implementations with the generated wrapper. An explicitly requested interface stub is emitted separately through VAST. Analysis and proof paths that require unknown external behavior report an error; no placeholder implementation is invented.

Both emitters preserve port and instance names, bit connectivity, and explicitly requested net names. Calling `q.specify_net_name("word")` names a wire with the full width and bit order of the instance port `q`. Naming a slice creates a wire as wide as that slice, starting at bit zero. For example, `q.slice(3, 2).specify_net_name("lane")` gives a two-bit wire with `lane[1]` connected to `q[3]` and `lane[0]` connected to `q[2]`.

TopStitch accepts unescaped identifiers matching `[A-Za-z_][A-Za-z0-9_]*` and rejects SystemVerilog keywords for the names it emits, including external module and port names. An imported black box may use escaped identifiers internally. Direct SV accepts names such as `top` and `clock`. These are XLS IR keywords, so Block IR export reports an error if one appears as a native module, native port, instance, or explicitly requested net name. External SV module and port names, parameter names, and qualified port types follow the SV naming rules at the external boundary. For automatically generated wires, the exporter can choose an available suffix instead.

Native and external inout pins connect through resolved electrical nets. Whole ports, slices, and concatenations retain passive bidirectional connectivity; a directional source can drive selected lanes while other lanes remain undriven. Ordinary receivers read the resolved value. Private electrical nets can retain requested names when one net can represent the complete request. A fully driven named bus may include ordinary directional lanes as well as lanes attached to inouts. Naming an output pin on an electrical net currently requires a whole native Block IR output; an external or sliced output request fails if its exact pin connection cannot be preserved. Export also fails if a requested name would alias a public inout, if some connected lanes need a drive while others must float, or if a native output depends on electrical readback that PIR cannot preserve. These failures prevent silent changes to physical names or behavior.

Imported definitions share ownership of their original verified package, which stays in memory while those definitions exist. When re-exporting an unclocked Block IR child, TopStitch links its reachable block and function dependencies and instantiates the child natively. One source package is supported per export; use a single `BlockIrLibrary` for multiple imported blocks. A clocked imported child is emitted as an opaque external instance because PIR connects native child clocks implicitly while TopStitch permits arbitrary clock wiring. Supply its separately generated SV implementation; analyses cannot prove behavior across that opaque boundary. An imported block can be exported directly, including its original clock. Export rejects an external RTL module name that conflicts with a module emitted in the same hierarchy, or a generated block name that conflicts with a retained source-package member. Evaluation and formal tools must independently support electrical nets.

Clock-named inputs are ordinary data ports in the structural export: TopStitch does not infer registers, clock domains, or pipeline scheduling.
