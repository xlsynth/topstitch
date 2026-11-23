# Extract connections across a hierarchy

A pin query can answer “which inputs receive bit 5?” A group query can answer “which connections would cross the boundary if I kept these instances together?” Both work on the assembled TopStitch design before Verilog emission. Use the existing `get_connections()` API for the attachments directly recorded on a port or slice. Keep the design alive while inspecting extraction results, and run the query again after editing the wiring.

From a TopStitch checkout, run the complete example.

```sh
cargo run --locked --example connection_extraction
```

It prints the two loads of a sample bit, the single wire crossing the producer/consumer boundary, and the two-cycle path to a monitor inside a wrapper.

## Follow a bit to its receivers

Suppose a wrapper feeds the upper four bits of a bus to two readers. Check the mapping from the top-level port all the way to the reader inputs.

```rust
use topstitch::{IO, ModDef};

let reader = ModDef::new("Reader");
reader.add_port("sample", IO::Input(4));
let bank = ModDef::new("Bank");
let samples = bank.add_port("samples", IO::Input(4));
for name in ["left", "right"] {
    let reader = bank.instantiate(&reader, Some(name), None);
    samples.connect(&reader.get_port("sample"));
}

let top = ModDef::new("Top");
let bank = top.instantiate(&bank, Some("bank"), None);
let bus = top.add_port("bus", IO::Input(8));
bus.slice(3, 0).unused();
bus.slice(7, 4).connect(&bank.get_port("samples"));

let loads = bus.bit(5).receivers();
assert_eq!(loads.len(), 2);
for name in ["left", "right"] {
    let input = bank.get_instance(name).get_port("sample").bit(1);
    let trace = loads.iter()
        .find(|trace| trace.peer_endpoint().endpoint.as_pin() == Some(&input))
        .unwrap();
    assert_eq!(trace.route(), &[bus.bit(5), bank.get_port("samples").bit(1), input.clone()]);
    assert_eq!(input.drivers()[0].connection, trace.connection);
}
```

Each trace’s `route()` contains three slices in query order. From the bus, those are `bus[5]`, `bank.samples[1]`, and the selected reader’s `sample[1]`. Compare `trace.connection` to recognize the same mapping when querying from opposite ends. An inout can appear in both directional queries. Use `trace_connections()` to also inspect tieoffs, unused bits, and missing wiring. For the union of several pin queries, `connections_touching` counts each mapped bit pair once.

## Find the wiring between groups or across a boundary

A producer sends a sample to a consumer, which sends a ready signal back. A monitor taps the sample. The interface view can show the complete exchange alongside the monitor’s individual wire.

```rust
use topstitch::{boundary_connections, connections_between, Connection, ConnectionView, IO, ModDef};

let producer = ModDef::new("Producer");
producer.add_port("s_sample", IO::Output(8));
producer.add_port("s_ready", IO::Input(1));
let consumer = ModDef::new("Consumer");
producer.def_intf_from_prefix("stream", "s_").flip_to(&consumer);
let monitor = ModDef::new("Monitor");
monitor.add_port("sample", IO::Input(8));

let top = ModDef::new("Top");
let tx = top.instantiate(&producer, Some("tx"), None);
let rx = top.instantiate(&consumer, Some("rx"), None);
let tap = top.instantiate(&monitor, Some("tap"), None);
tx.get_intf("stream").connect(&rx.get_intf("stream"), false);
tx.get_port("s_sample").connect(&tap.get_port("sample"));

let links = connections_between(&[&tx, &rx, &tap], ConnectionView::InterfacesAndSignals).unwrap();
assert_eq!(links.len(), 2);
assert!(links.iter().any(|link| matches!(link, Connection::Interface(pair)
    if pair.member_connections().len() == 2)));

let channel = [&tx, &rx];
let cut = boundary_connections(&channel).unwrap();
assert_eq!(cut.len(), 1);
assert_eq!(cut[0].peer_endpoint().endpoint.as_pin().unwrap().get_port(), tap.get_port("sample"));
```

With `ConnectionView::Signals`, the three-instance query returns three mappings for sample, ready, and the sample tap. Interface grouping requires the same function keys and complete bit mappings. Members can be shared. If each side declares two names for an identical interface, all four complete interface pairs are reported.

Use `connections_between(&[&channel, &tap], ConnectionView::Signals)` when you know the groups of interest. Each entry can be one instance or several, all from the same design root. Use `boundary_connections(&channel)` when you want to discover *everything* attached outside that group, including loads you did not select. Boundary results also expose tieoffs and incomplete bindings.

## Follow a path through a delay

A structural trace reaches the input of a delay. To continue to the destination and inspect the latency, ask for downstream paths between the two instances.

```rust
use topstitch::{paths_between, IO, ModDef, PathOptions, PathStep, PipelineConfig, ThroughKind};

let source = ModDef::new("Source");
source.add_port("q", IO::Output(8));
let sink = ModDef::new("Sink");
sink.add_port("d", IO::Input(8));
let top = ModDef::new("Top");
let tx = top.instantiate(&source, Some("tx"), None);
let rx = top.instantiate(&sink, Some("rx"), None);
tx.get_port("q").connect_pipeline(&rx.get_port("d"), PipelineConfig {
    depth: 2,
    ..Default::default()
});

let graph = paths_between(&tx, &rx, &PathOptions::default()).unwrap();
let route = graph.routes().next().unwrap();
assert!(matches!(&route[..], [
    PathStep::Wire(_),
    PathStep::Through { kind: ThroughKind::Clocked { cycles: 2, .. }, .. },
    PathStep::Wire(_)
]));
```

Use `graph.through()` for a summary of the instances and their participating entry and exit slices. Use `routes()` when the order of individual crossings matters. With only a starting pin, `pin.trace_paths(&options)` discovers downstream destinations.

TopStitch can follow wiring recorded inside a generated wrapper. An imported Verilog body is opaque to extraction. A path through an opaque instance is marked `ThroughKind::Unknown` because its input-to-output dependency is unverified. Set `cross_opaque: false` in `PathOptions` to exclude those candidates. To control the level of detail for a particular wrapper, use a `HierarchyPolicy` to collapse its internal steps for display or stop at its pins.
