// SPDX-License-Identifier: Apache-2.0

//! Run with `cargo test --locked --release --test extraction_performance -- --test-threads=1 --show-output`.

use std::time::{Duration, Instant};
use topstitch::*;

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_wide_fanout_across_repeated_hierarchy() {
    const WIDTH: usize = 1 << 20;
    const LOADS: usize = 4_096;
    let leaf = ModDef::new("Sink");
    leaf.add_port("d", IO::Input(WIDTH));
    leaf.set_usage(Usage::EmitNothingAndStop);
    let wrap = ModDef::new("Wrap");
    let input = wrap.add_port("bus", IO::Input(WIDTH));
    input.connect(&wrap.instantiate(&leaf, None, None).get_port("d"));
    let top = ModDef::new("Top");
    let driver = top.add_port("q", IO::Input(WIDTH));
    for i in 0..LOADS {
        driver.connect(
            &top.instantiate(&wrap, Some(&format!("w{i}")), None)
                .get_port("bus"),
        );
    }
    let links = timed(
        &format!("{LOADS} loads on a {WIDTH}-bit bus"),
        Duration::from_millis(50),
        || driver.receivers(),
    );
    assert_eq!(links.len(), LOADS);
    assert!(links.iter().all(|l| {
        l.queried_endpoint()
            .endpoint
            .as_pin()
            .is_some_and(|p| p.width() == WIDTH)
            && l.route().len() == 3
    }));
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_named_ranges_on_a_group_boundary() {
    const NAMES: usize = 8_192;
    let top = ModDef::new("Top");
    let pads = ModDef::new("Pads");
    pads.add_port("bus", IO::InOut(2 * NAMES));
    let inst = top.instantiate(&pads, None, None);
    let bus = inst.get_port("bus");
    let external = top.add_port("external", IO::InOut(NAMES));
    for i in 0..NAMES {
        bus.slice(2 * i + 1, 2 * i)
            .specify_net_name(&format!("lane{i}"));
        bus.bit(2 * i).connect(&external.bit(i));
    }
    let cut = timed(
        "8192 partially bound named ranges",
        Duration::from_millis(150),
        || boundary_connections(&inst).unwrap(),
    );
    assert_eq!(cut.len(), 2 * NAMES);
    assert_eq!(
        cut.iter()
            .filter(|w| matches!(&w.peer_endpoint().endpoint,
        Endpoint::NamedWire { lsb, msb, .. } if lsb == msb && lsb % 2 == 1))
            .count(),
        NAMES
    );
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_select_split_bus_endpoints() {
    const RANGES: usize = 8_192;
    let tx = ModDef::new("Tx");
    tx.add_port("q", IO::Output(32 * RANGES));
    tx.set_usage(Usage::EmitNothingAndStop);
    let leaf = ModDef::new("Leaf");
    leaf.add_port("d", IO::Input(32));
    leaf.set_usage(Usage::EmitNothingAndStop);
    let root = ModDef::new("Top");
    let tx = root.instantiate(&tx, None, None);
    let bus = tx.get_port("q");
    let mut loads = Vec::new();
    for i in 0..RANGES {
        let l = root.instantiate(&leaf, Some(&format!("l{i}")), None);
        bus.slice(32 * i + 31, 32 * i).connect(&l.get_port("d"));
        loads.push(l);
    }
    let links = timed(
        &format!("{RANGES} disjoint ranges between two groups"),
        Duration::from_millis(150),
        || connections_between(&[&tx, &loads], ConnectionView::Signals).unwrap(),
    );
    assert_eq!(links.len(), RANGES);
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_opaque_cross_product_stays_lazy() {
    const PORTS: usize = 1_000;
    let (source, opaque, dest) = (
        ModDef::new("Source"),
        ModDef::new("Opaque"),
        ModDef::new("Dest"),
    );
    for i in 0..PORTS {
        source.add_port(format!("q{i}"), IO::Output(1));
        opaque.add_port(format!("i{i}"), IO::Input(1));
        opaque.add_port(format!("o{i}"), IO::Output(1));
        dest.add_port(format!("d{i}"), IO::Input(1));
    }
    for def in [&source, &opaque, &dest] {
        def.set_usage(Usage::EmitNothingAndStop);
    }
    let top = ModDef::new("Top");
    let a = top.instantiate(&source, None, None);
    let x = top.instantiate(&opaque, None, None);
    let b = top.instantiate(&dest, None, None);
    for i in 0..PORTS {
        a.get_port(format!("q{i}"))
            .connect(&x.get_port(format!("i{i}")));
        x.get_port(format!("o{i}"))
            .connect(&b.get_port(format!("d{i}")));
    }
    let (graph, first_route) = timed(
        &format!("{PORTS}x{PORTS} possible paths; extract graph and first route"),
        Duration::from_millis(30),
        || {
            let graph = paths_between(&a, &b, &PathOptions::default()).unwrap();
            let first_route = graph.routes().next().unwrap();
            (graph, first_route)
        },
    );
    assert_eq!(first_route.len(), 3);
    assert_eq!(graph.connections().count(), 2 * PORTS);
    assert_eq!(graph.through().len(), 1);
    assert_eq!(
        (
            graph.through()[0].entries.len(),
            graph.through()[0].exits.len()
        ),
        (PORTS, PORTS)
    );
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_full_bus_tap_overlapping_thousands_of_disjoint_slices() {
    const BITS: usize = 12_000;
    let root = ModDef::new("Top");
    let bus = root.add_port("input", IO::Input(BITS));
    let tap = root.add_port("tap", IO::Output(BITS));
    bus.connect(&tap);
    let output = root.add_port("output", IO::Output(BITS));
    for i in 0..BITS {
        bus.bit(i).connect(&output.bit(i));
    }
    let selected = (0..BITS).map(|i| bus.bit(i)).collect::<Vec<_>>();
    let result = timed(
        &format!("{BITS} bit queries with one full-width tap"),
        Duration::from_millis(85),
        || connections_touching(&selected),
    );
    assert_eq!(result.len(), BITS * 2);
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_bundle_interfaces_across_many_occurrences() {
    const PAIRS: usize = 1_200;
    let tx = ModDef::new("Tx");
    let rx = ModDef::new("Rx");
    for (def, drives) in [(&tx, true), (&rx, false)] {
        def.add_port(
            "bus_payload",
            if drives {
                IO::Output(128)
            } else {
                IO::Input(128)
            },
        );
        def.add_port(
            "bus_ready",
            if drives { IO::Input(1) } else { IO::Output(1) },
        );
        def.def_intf_from_prefix("bus", "bus_");
        def.set_usage(Usage::EmitNothingAndStop);
    }
    let top = ModDef::new("Top");
    let mut senders = Vec::new();
    let mut receivers = Vec::new();
    for i in 0..PAIRS {
        let a = top.instantiate(&tx, Some(&format!("a{i}")), None);
        let b = top.instantiate(&rx, Some(&format!("b{i}")), None);
        a.get_intf("bus").connect(&b.get_intf("bus"), false);
        senders.push(a);
        receivers.push(b);
    }
    let bundles = timed(
        &format!("{PAIRS} complete bidirectional interface pairs"),
        Duration::from_millis(85),
        || {
            connections_between(
                &[&senders, &receivers],
                ConnectionView::InterfacesAndSignals,
            )
            .unwrap()
        },
    );
    assert_eq!(bundles.len(), PAIRS);
    assert!(
        bundles
            .iter()
            .all(|c| matches!(c,Connection::Interface(i) if i.member_connections().len()==2))
    );
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_interface_slices_on_one_wide_port() {
    const LANES: usize = 16_384;
    let tx = ModDef::new("Tx");
    let rx = ModDef::new("Rx");
    for (def, io) in [(&tx, IO::Output(LANES)), (&rx, IO::Input(LANES))] {
        def.add_port("bus", io);
        def.set_usage(Usage::EmitNothingAndStop);
        for bit in 0..LANES {
            def.def_intf(
                format!("lane_{bit}"),
                [("data".into(), ("bus".into(), bit, bit))].into(),
            );
        }
    }
    let top = ModDef::new("Top");
    let tx = top.instantiate(&tx, None, None);
    let rx = top.instantiate(&rx, None, None);
    let q = tx.get_port("bus");
    let d = rx.get_port("bus");
    // Exercise both many narrow wires and a single wire spanning many interface members.
    for bit in 0..LANES / 2 {
        q.bit(bit).connect(&d.bit(bit));
    }
    q.slice(LANES - 1, LANES / 2)
        .connect(&d.slice(LANES - 1, LANES / 2));

    for view in [
        ConnectionView::Signals,
        ConnectionView::InterfacesAndSignals,
    ] {
        let result = timed(
            &format!("{LANES} interface slices on one port ({view:?})"),
            Duration::from_millis(250),
            || connections_between(&[&tx, &rx], view).unwrap(),
        );
        assert_eq!(result.len(), LANES);
        assert!(result.iter().all(|connection| match (view, connection) {
            (ConnectionView::Signals, Connection::Signal(wire)) =>
                wire.peer_endpoint().endpoint.as_pin().unwrap().width() == 1,
            (ConnectionView::InterfacesAndSignals, Connection::Interface(pair)) => {
                pair.member_connections().len() == 1
            }
            _ => false,
        }));
    }
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_interface_lanes_sharing_a_control_pin() {
    const LANES: usize = 1_024;
    let tx = ModDef::new("Tx");
    let rx = ModDef::new("Rx");
    for (def, control, data) in [
        (&tx, IO::Output(1), IO::Output(LANES)),
        (&rx, IO::Input(1), IO::Input(LANES)),
    ] {
        def.add_port("valid", control);
        def.add_port("payload", data);
        for lane in 0..LANES {
            // Declare the shared member first to exercise selection independent of member order.
            def.def_intf(
                format!("lane{lane}"),
                [
                    ("valid".into(), ("valid".into(), 0, 0)),
                    ("data".into(), ("payload".into(), lane, lane)),
                ]
                .into(),
            );
        }
        def.set_usage(Usage::EmitNothingAndStop);
    }
    let top = ModDef::new("Top");
    let tx = top.instantiate(&tx, None, None);
    let rx = top.instantiate(&rx, None, None);
    tx.get_port("valid").connect(&rx.get_port("valid"));
    tx.get_port("payload").connect(&rx.get_port("payload"));
    let pairs = timed(
        &format!("{LANES} interface pairs sharing a control wire"),
        Duration::from_millis(40),
        || connections_between(&[&tx, &rx], ConnectionView::InterfacesAndSignals).unwrap(),
    );
    assert_eq!(pairs.len(), LANES);
    assert!(pairs.iter().all(|c| matches!(c, Connection::Interface(pair)
        if pair.interfaces()[0].name == pair.interfaces()[1].name && pair.member_connections().len() == 2)));
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release mode")]
fn performance_targeted_lanes_through_reconvergent_paths() {
    const LAYERS: usize = 14;
    let top = ModDef::new("Top");
    let source = ModDef::new("Source");
    source.add_port("q", IO::Output(32));
    let dest = ModDef::new("Dest");
    dest.add_port("d", IO::Input(1));
    let merge = ModDef::new("Merge");
    merge.add_port("a", IO::Input(1));
    merge.add_port("b", IO::Input(1));
    merge.add_port("q", IO::Output(1));
    for def in [&source, &dest, &merge] {
        def.set_usage(Usage::EmitNothingAndStop);
    }
    let source = top.instantiate(&source, None, None);
    let dest = top.instantiate(&dest, None, None);
    let delay = top.instantiate(
        &PipelineConfig {
            depth: 1,
            ..Default::default()
        }
        .to_mod_def(32),
        None,
        None,
    );
    source.get_port("q").connect(&delay.get_port("in"));
    let mut q = delay.get_port("out").bit(9);
    for i in 0..LAYERS {
        let stage = top.instantiate(&merge, Some(&format!("m{i}")), None);
        q.connect(&stage.get_port("a"));
        q.connect(&stage.get_port("b"));
        q = stage.get_port("q").to_port_slice();
    }
    q.connect(&dest.get_port("d"));

    // Two arrivals per layer yield 2^LAYERS candidate routes on a linear number of wires.
    let graph = timed(
        "target lanes across 16,384 candidate routes",
        Duration::from_millis(20),
        || paths_between(&source, &dest, &PathOptions::default()).unwrap(),
    );
    assert_eq!(graph.connections().count(), 2 * LAYERS + 2);
    assert!(graph.connections().any(
        |wire| wire.queried_endpoint().endpoint.as_pin() == Some(&source.get_port("q").bit(9))
    ));
    let stages = graph.through();
    assert_eq!(stages.len(), LAYERS + 1);
    assert!(
        stages
            .iter()
            .any(|s| s.entries.contains(&delay.get_port("in").bit(9)))
    );
}

/// Measures one query, including its indexes and result construction, on a prepared design.
fn timed<T>(label: &str, budget: Duration, query: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = query();
    let elapsed = start.elapsed();
    eprintln!("{label} took {elapsed:?} (budget {budget:?})");
    assert!(
        elapsed <= budget,
        "{label} took {elapsed:?}, exceeding {budget:?}"
    );
    result
}
