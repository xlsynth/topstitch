# Route a signal through a module

Sometimes the design requires a wire to enter a generated module and leave through another port. Give that module a feedthrough. The module that owns it determines where the new ports appear in the emitted hierarchy.

```rust
use topstitch::{EmitOptions, ModDef};

let bridge = ModDef::new("Bridge");
bridge.feedthrough("west", "east", 6);
let rtl = bridge.emit(EmitOptions::default());
assert!(rtl.contains("assign east = west;"));
```

Instantiate `Bridge` in a parent and connect the upstream endpoint to `west` and the downstream endpoint to `east`. This feedthrough is combinational. The extra module boundary adds no cycles.

## Introduce clocked latency deliberately

Suppose the caller wants each sample to appear two clocks after it arrives. Set `depth` to 2 for that interface. Use `connect_pipeline` between existing endpoints, or `feedthrough_pipeline` to create the two boundary ports and pipeline their connection:

```rust
use topstitch::{EmitOptions, ModDef, PipelineConfig};

let bridge = ModDef::new("ClockedBridge");
bridge.feedthrough_pipeline("west", "east", 6, PipelineConfig {
    clk: "clk".into(),
    depth: 2,
    inst_name: Some("sample_delay".into()),
});
let rtl = bridge.emit(EmitOptions::default());
assert!(rtl.contains("sample_delay ("));
assert!(rtl.contains(".clk(clk)"));
```

Inspect the emitted module for the clock, the delay instance, its width, and its stage count. The generated RTL instantiates `br_delay_nr`. Provide a compatible implementation when simulating or synthesizing it. The [pipeline tests](https://github.com/xlsynth/topstitch/blob/99138be87453717c6868c18fb99f2a1c5f7ee3a3/tests/feedthroughs/pipeline.rs) show the expected instance and parameters.

Choose the latency from the interface contract. For a sampled value, delaying the value and its qualifier together may be enough. For a channel with backpressure, delaying each wire independently changes when the sender sees the receiver's response. If the channel needs storage or flow control, instantiate RTL that implements that behavior and test it with a stalled receiver.
