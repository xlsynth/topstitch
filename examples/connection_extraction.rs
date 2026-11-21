// SPDX-License-Identifier: Apache-2.0

//! A producer sends samples to a consumer. A monitor observes the high nibble two clocks later.

use topstitch::{
    Connection, ConnectionView, IO, ModDef, PathOptions, PathStep, PipelineConfig, ThroughKind,
    boundary_connections, connections_between, paths_between,
};

fn main() -> Result<(), String> {
    let producer = ModDef::new("Producer");
    producer.add_port("s_sample", IO::Output(8));
    producer.add_port("s_ready", IO::Input(1));
    let consumer = ModDef::new("Consumer");
    producer
        .def_intf_from_prefix("stream", "s_")
        .flip_to(&consumer);

    let monitor = ModDef::new("Monitor");
    monitor.add_port("sample", IO::Input(4));
    let tap = ModDef::new("Tap");
    let observer = tap.instantiate(&monitor, Some("observer"), None);
    tap.add_port("sample", IO::Input(4)).connect_pipeline(
        &observer.get_port("sample"),
        PipelineConfig {
            depth: 2,
            inst_name: Some("delay".into()),
            ..Default::default()
        },
    );

    let top = ModDef::new("Top");
    let tx = top.instantiate(&producer, Some("tx"), None);
    let rx = top.instantiate(&consumer, Some("rx"), None);
    let tap = top.instantiate(&tap, Some("tap"), None);
    tx.get_intf("stream").connect(&rx.get_intf("stream"), false);
    let sample = tx.get_port("s_sample");
    sample.slice(7, 4).connect(&tap.get_port("sample"));
    top.add_port("clk", IO::Input(1))
        .connect(&tap.get_port("clk"));

    println!("Receivers of sample bit 5");
    for wire in sample.bit(5).receivers() {
        println!("  {:?}", wire.route());
    }

    println!("Connections among tx, rx, and tap");
    for link in connections_between(&[&tx, &rx, &tap], ConnectionView::InterfacesAndSignals)? {
        match link {
            Connection::Interface(pair) => {
                let [a, b] = pair.interfaces();
                println!(
                    "  {}.{} <-> {}.{}",
                    a.path.join("."),
                    a.name,
                    b.path.join("."),
                    b.name
                );
                for (name, wire) in pair.member_connections() {
                    let [driver, receiver] = &wire.connection.endpoints;
                    println!(
                        "    {name}  {:?} -> {:?}",
                        driver.endpoint, receiver.endpoint
                    );
                }
            }
            Connection::Signal(wire) => println!("  {:?}", wire.route()),
        }
    }

    println!("Boundary of tx and rx together");
    for wire in boundary_connections(&[&tx, &rx])? {
        println!("  {:?}", wire.route());
    }

    println!("Path from tx to the observer inside tap");
    let options = PathOptions {
        cross_opaque: false,
        ..Default::default()
    };
    let graph = paths_between(&tx, &tap.get_instance("observer"), &options)?;
    for route in graph.routes() {
        for step in route {
            match step {
                PathStep::Wire(wire) => println!("  {:?}", wire.route()),
                PathStep::Through {
                    enter,
                    leave,
                    kind: ThroughKind::Clocked { cycles, clock },
                } => {
                    println!("  {enter:?} -> {leave:?} after {cycles} cycles of {clock}");
                }
                other => println!("  {other:?}"),
            }
        }
    }
    Ok(())
}
