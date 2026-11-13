// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

fn block(name: &str, ports: &[(&str, IO)]) -> ModDef {
    let def = ModDef::new(name);
    for (name, io) in ports {
        def.add_port(*name, io.clone());
    }
    def.set_usage(Usage::EmitNothingAndStop);
    def
}
fn peer(c: &ConnectionTrace) -> &PortSlice {
    c.peer_endpoint()
        .endpoint
        .as_pin()
        .expect("expected a peer pin")
}

#[test]
fn the_same_connection_from_either_end_has_one_identity() {
    use std::collections::HashSet;

    let top = ModDef::new("Top");
    let source = top.add_port("src", IO::Input(8));
    let rx = top.instantiate(&block("Rx", &[("data", IO::Input(4))]), None, None);
    let data = rx.get_port("data");
    let high = source.slice(7, 4);
    high.connect(&data);

    let downstream = high.receivers().pop().unwrap();
    let upstream = data.drivers().pop().unwrap();
    assert_eq!(downstream.stop_reason, StopReason::Opaque);
    assert_eq!(upstream.stop_reason, StopReason::Boundary);
    assert!(downstream.route().iter().eq(upstream.route().iter().rev()));

    let expected = [
        ConnectionEndpoint {
            endpoint: Endpoint::Pin(high),
            role: Some(Role::Driver),
        },
        ConnectionEndpoint {
            endpoint: Endpoint::Pin(data.to_port_slice()),
            role: Some(Role::Receiver),
        },
    ];
    for trace in [&downstream, &upstream] {
        assert_eq!(trace.connection.endpoints, expected);
    }
    assert_eq!(downstream.queried_endpoint(), &expected[0]);
    assert_eq!(downstream.peer_endpoint(), &expected[1]);
    assert_eq!(upstream.queried_endpoint(), &expected[1]);
    assert_eq!(upstream.peer_endpoint(), &expected[0]);

    let mut unique = HashSet::from([downstream.connection]);
    assert!(!unique.insert(upstream.connection));
    // The one-bit connection overlaps the four-bit connection but is a distinct mapping.
    assert!(unique.insert(data.bit(0).drivers().pop().unwrap().connection));
}

#[test]
fn repeated_wrapper_fanout_and_clipped_snapshots() {
    let leaf = block("Sink", &[("d", IO::Input(4))]);
    let wrapper = ModDef::new("Wrapper");
    let bus = wrapper.add_port("renamed", IO::Input(8));
    let a = wrapper.instantiate(&leaf, Some("a"), None);
    let b = wrapper.instantiate(&leaf, Some("b"), None);
    bus.slice(7, 4).connect(&a.get_port("d"));
    bus.slice(7, 4).connect(&b.get_port("d"));
    let top = ModDef::new("Top");
    let left = top.instantiate(&wrapper, Some("left"), None);
    let right = top.instantiate(&wrapper, Some("right"), None);
    let source = top.add_port("source", IO::Input(8));
    source.connect(&left.get_port("renamed"));
    let spare = top.add_port("spare", IO::Input(8));
    spare.connect(&right.get_port("renamed"));
    let links = source.bit(5).receivers();
    let first = left.get_instance("a").get_port("d").bit(1);
    let second = left.get_instance("b").get_port("d").bit(1);
    assert_eq!(
        links.iter().map(peer).collect::<Vec<_>>(),
        [&first, &second]
    );
    assert_eq!(
        links[0].route(),
        [
            source.bit(5),
            left.get_port("renamed").bit(5),
            first.clone()
        ]
    );
    // A returned endpoint supports the same queries as the original design handle.
    assert_eq!(peer(&peer(&links[0]).drivers()[0]), &source.bit(5));
    assert_eq!(
        peer(&right.get_instance("a").get_port("d").bit(1).drivers()[0]),
        &spare.bit(5)
    );
    let stop = HierarchyPolicy::expand_all().for_definition(&wrapper, HierarchyAction::Stop);
    assert_eq!(
        peer(&source.bit(5).receivers_with(&stop)[0]),
        &left.get_port("renamed").bit(5)
    );
    let override_rule = stop.for_instance(&left, HierarchyAction::Expand);
    assert_eq!(source.bit(5).receivers_with(&override_rule).len(), 2);
    source
        .slice(7, 4)
        .connect(&top.instantiate(&leaf, Some("extra"), None).get_port("d"));
    assert_eq!(links.len(), 2);
    assert_eq!(source.bit(5).receivers().len(), 3);
    assert_eq!(peer(&links[0]), &first);
}

#[test]
fn splits_fanout_and_groups_keep_external_branch() {
    let top = ModDef::new("Top");
    let tx = top.instantiate(&block("Send", &[("q", IO::Output(8))]), Some("tx"), None);
    let sink = block("Sink", &[("d", IO::Input(4))]);
    let a = top.instantiate(&sink, Some("a"), None);
    let b = top.instantiate(&sink, Some("b"), None);
    let monitor = top.instantiate(&sink, Some("monitor"), None);
    let q = tx.get_port("q");
    q.slice(7, 4).connect(&a.get_port("d"));
    q.slice(3, 0).connect(&b.get_port("d"));
    q.slice(7, 4).connect(&monitor.get_port("d"));
    let group = [&tx, &a];
    let links = boundary_connections(&group).unwrap();
    assert_eq!(links.len(), 2);
    assert!(links.iter().any(
        |c| c.queried_endpoint().endpoint.as_pin().unwrap().lsb() == 4
            && peer(c) == &monitor.get_port("d").to_port_slice()
    ));
    assert_eq!(
        connections_between(&[&group, &b, &monitor], ConnectionView::Signals)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(connections_touching([&q, &a.get_port("d")]).len(), 3);
    let other = ModDef::new("Other");
    let c = other.instantiate(&sink, None, None);
    assert!(connections_between(&[&a, &c], ConnectionView::Signals).is_err());
    assert!(connections_between(&[&group, &tx], ConnectionView::Signals).is_err());
}

#[test]
fn interfaces_include_reverse_members_and_leave_extra_branches_visible() {
    let txdef = block(
        "Tx",
        &[
            ("a_d", IO::Output(8)),
            ("a_r", IO::Input(1)),
            ("irq", IO::Output(1)),
        ],
    );
    let rxdef = block(
        "Rx",
        &[
            ("b_d", IO::Input(8)),
            ("b_r", IO::Output(1)),
            ("irq", IO::Input(1)),
        ],
    );
    txdef.def_intf_from_prefix("tx", "a_");
    rxdef.def_intf_from_prefix("rx", "b_");
    let top = ModDef::new("Top");
    let tx = top.instantiate(&txdef, None, None);
    let rx = top.instantiate(&rxdef, None, None);
    let monitor = top.instantiate(&block("Monitor", &[("d", IO::Input(4))]), None, None);
    tx.get_intf("tx").connect(&rx.get_intf("rx"), false);
    tx.get_port("irq").connect(&rx.get_port("irq"));
    tx.get_port("a_d")
        .slice(7, 4)
        .connect(&monitor.get_port("d"));
    let flat = connections_between(&[&tx, &rx, &monitor], ConnectionView::Signals).unwrap();
    let bundled =
        connections_between(&[&tx, &rx, &monitor], ConnectionView::InterfacesAndSignals).unwrap();
    assert_eq!(flat.len(), 4);
    assert_eq!(bundled.len(), 3);
    let Connection::Interface(intf) = &bundled[0] else {
        panic!()
    };
    assert_eq!(intf.interfaces()[0].name, "tx");
    assert_eq!(intf.member_connections().len(), 2);
    let (_, ready) = intf
        .member_connections()
        .iter()
        .find(|(n, _)| n == "r")
        .unwrap();
    let [driver, receiver] = &ready.connection.endpoints;
    assert_eq!(
        driver.endpoint,
        Endpoint::Pin(rx.get_port("b_r").to_port_slice())
    );
    assert_eq!(driver.role, Some(Role::Driver));
    assert_eq!(
        receiver.endpoint,
        Endpoint::Pin(tx.get_port("a_r").to_port_slice())
    );
    assert_eq!(receiver.role, Some(Role::Receiver));
}

#[test]
fn partial_members_are_individual_wires() {
    let a = block("A", &[("bus_d", IO::Output(8))]);
    let b = block("B", &[("bus_d", IO::Input(8))]);
    for def in [&a, &b] {
        def.def_intf_from_prefix("bus", "bus_");
    }
    let top = ModDef::new("Top");
    let a = top.instantiate(&a, None, None);
    let b = top.instantiate(&b, None, None);
    a.get_port("bus_d")
        .slice(7, 4)
        .connect(&b.get_port("bus_d").slice(7, 4));
    let links = connections_between(&[&a, &b], ConnectionView::InterfacesAndSignals).unwrap();
    assert!(
        matches!(&links[..],[Connection::Signal(c)] if c.queried_endpoint().endpoint.as_pin().unwrap().width()==4)
    );
}

#[test]
fn reports_constants_named_wires_opens_and_missing_ranges() {
    let top = ModDef::new("Top");
    let dst = top.instantiate(
        &block("D", &[("d", IO::Input(8)), ("p", IO::InOut(2))]),
        None,
        None,
    );
    let d = dst.get_port("d");
    d.slice(7, 4).tieoff(11);
    d.slice(2, 1).tieoff(0);
    let p = dst.get_port("p");
    p.bit(1).specify_net_name("named");
    p.bit(0).unused();
    let links = boundary_connections(&dst).unwrap();
    assert!(links.iter().any(
        |c| matches!(&c.peer_endpoint().endpoint,Endpoint::Constant{value,..} if value==&11.into())
    ));
    let drivers = d.bit(5).drivers();
    assert_eq!(
        drivers[0].queried_endpoint().endpoint.as_pin(),
        Some(&d.bit(5))
    );
    assert_eq!(drivers[0].peer_endpoint().endpoint.as_pin(), None);
    assert_eq!(
        drivers[0].peer_endpoint(),
        &drivers[0].connection.endpoints[0]
    );
    assert_eq!(drivers[0].peer_endpoint().role, Some(Role::Driver));
    assert!(
        links
            .iter()
            .filter(|c| matches!(
                c.stop_reason,
                StopReason::Unused | StopReason::NamedWire | StopReason::Unconnected
            ))
            .all(|c| c.peer_endpoint().role.is_none()
                && c.peer_endpoint().endpoint.as_pin().is_none())
    );
    assert_eq!(
        links
            .iter()
            .filter(|c| c.stop_reason == StopReason::Unconnected)
            .count(),
        2
    );
    assert!(links.iter().any(|c| c.stop_reason == StopReason::Unused));
    assert!(links.iter().any(
        |c| matches!(&c.peer_endpoint().endpoint,Endpoint::NamedWire{name,..} if name=="named")
    ));
}
#[test]
fn inout_reports_possible_roles() {
    let dev = block("Pad", &[("pad", IO::InOut(1))]);
    let top = ModDef::new("Top");
    let a = top.instantiate(&dev, Some("a"), None);
    let b = top.instantiate(&dev, Some("b"), None);
    a.get_port("pad").connect(&b.get_port("pad"));
    for results in [a.get_port("pad").drivers(), a.get_port("pad").receivers()] {
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].peer_endpoint().role, Some(Role::InOut));
        assert_eq!(peer(&results[0]), &b.get_port("pad").to_port_slice());
    }
}
#[test]
fn inout_connection_hashing_ignores_order_and_role_annotations() {
    use std::collections::HashSet;

    let top = ModDef::new("Top");
    let pad = block("Pad", &[("io", IO::InOut(1))]);
    let a = top.instantiate(&pad, Some("a"), None).get_port("io");
    let b = top.instantiate(&pad, Some("b"), None).get_port("io");
    a.connect(&b);
    let from_a = a.receivers().pop().unwrap();
    let from_b = b.drivers().pop().unwrap();
    assert_eq!(
        from_a.peer_endpoint().endpoint,
        Endpoint::Pin(b.to_port_slice())
    );
    assert_eq!(
        from_b.peer_endpoint().endpoint,
        Endpoint::Pin(a.to_port_slice())
    );
    let mut swapped = from_a.connection.clone();
    swapped.endpoints.swap(0, 1);
    swapped.endpoints[0].role = None;
    let mut unique = HashSet::from([from_a.connection]);
    assert!(!unique.insert(from_b.connection));
    assert!(!unique.insert(swapped));
}

#[test]
fn pipeline_is_an_explicit_clocked_step() {
    let a = block("A", &[("q", IO::Output(8))]);
    let b = block("B", &[("d", IO::Input(8))]);
    let top = ModDef::new("Top");
    let a = top.instantiate(&a, None, None);
    let b = top.instantiate(&b, None, None);
    a.get_port("q").connect_pipeline(
        &b.get_port("d"),
        PipelineConfig {
            depth: 2,
            inst_name: Some("delay".into()),
            ..Default::default()
        },
    );
    assert!(
        connections_between(&[&a, &b], ConnectionView::Signals)
            .unwrap()
            .is_empty()
    );
    let graph = paths_between(&a, &b, &PathOptions::default()).unwrap();
    assert_eq!(graph.through().len(), 1);
    assert_eq!(graph.connections().count(), 2);
    let route = graph.routes().next().unwrap();
    assert!(matches!(
        &route[..],
        [
            PathStep::Wire(_),
            PathStep::Through {
                kind: ThroughKind::Clocked { cycles: 2, .. },
                ..
            },
            PathStep::Wire(_)
        ]
    ));
    let mut opts = PathOptions::default();
    opts.cross_registers = false;
    assert_eq!(paths_between(&a, &b, &opts).unwrap().routes().count(), 0);
}
#[test]
fn path_from_delay_input_starts_with_the_actual_crossing() {
    for depth in [0, 2] {
        let top = ModDef::new("Top");
        let delay = top.instantiate(
            &PipelineConfig {
                depth,
                ..Default::default()
            }
            .to_mod_def(8),
            None,
            None,
        );
        let input = delay.get_port("in");
        let output = delay.get_port("out");
        let sink = top.add_port("sink", IO::Output(2));
        output.slice(6, 5).connect(&sink);
        let options = PathOptions::default();
        let graph = input.slice(6, 5).trace_paths(&options);
        let routes: Vec<_> = graph.expanded_routes().collect();
        assert_eq!(routes.len(), 1);
        let [
            PathStep::Through { enter, leave, kind },
            PathStep::Wire(wire),
        ] = routes[0].as_slice()
        else {
            panic!("expected the transfer followed by its outgoing wire");
        };
        assert_eq!(enter, &input.slice(6, 5));
        assert_eq!(leave, &output.slice(6, 5));
        let expected = if depth == 0 {
            ThroughKind::Feedthrough
        } else {
            ThroughKind::Clocked {
                clock: "clk".into(),
                cycles: depth,
            }
        };
        assert_eq!(kind, &expected);
        assert_eq!(peer(wire), &sink.to_port_slice());
        assert_eq!(graph.connections().count(), 1);
        assert_eq!(graph.through().len(), 1);
        assert!(graph.through()[0].entries.contains(enter));
        assert!(graph.through()[0].exits.contains(leave));

        let no_registers = PathOptions {
            cross_registers: false,
            ..Default::default()
        };
        assert_eq!(
            input.bit(5).trace_paths(&no_registers).routes().count(),
            usize::from(depth == 0)
        );
        let stopped = options
            .hierarchy(HierarchyPolicy::expand_all().for_instance(&delay, HierarchyAction::Stop));
        assert_eq!(input.bit(5).trace_paths(&stopped).routes().count(), 0);
    }
}

#[test]
fn explicit_delay_parameters_determine_path_latency() {
    // This import exposes the primary data and clock ports only.
    let import = |name: &str| {
        ModDef::from_verilog(name, format!(
        "module {name} #(parameter int NumStages = 7)(input clk, input [3:0] in, output [3:0] out); endmodule"
    )).unwrap()
    };
    let delay = import("br_delay_nr");
    let other = import("Other");
    let clocked = |cycles| ThroughKind::Clocked {
        clock: "clk".into(),
        cycles,
    };
    for (def, expected) in [
        (delay.parameterize(&[("NumStages", 2)]), clocked(2)),
        (delay.parameterize(&[("NumStages", 5)]), clocked(5)),
        (
            delay.parameterize(&[("NumStages", 0)]),
            ThroughKind::Feedthrough,
        ),
        (delay, ThroughKind::Unknown),
        (
            other.parameterize(&[("NumStages", 2)]),
            ThroughKind::Unknown,
        ),
    ] {
        let top = ModDef::new("Top");
        let source = top.instantiate(&block("Source", &[("q", IO::Output(4))]), None, None);
        let sink = top.instantiate(&block("Sink", &[("d", IO::Input(4))]), None, None);
        let stage = top.instantiate(&def, Some("stage"), None);
        source.get_port("q").connect(&stage.get_port("in"));
        stage.get_port("out").connect(&sink.get_port("d"));
        let paths = paths_between(&source, &sink, &PathOptions::default()).unwrap();
        let mut routes = paths.routes();
        let route = routes.next().unwrap();
        let [
            PathStep::Wire(_),
            PathStep::Through { kind, .. },
            PathStep::Wire(_),
        ] = &route[..]
        else {
            panic!("unexpected route: {route:?}");
        };
        assert_eq!(kind, &expected);
        assert!(routes.next().is_none());
    }
}

#[test]
fn opaque_crossings_are_candidates_and_can_be_pruned() {
    let top = ModDef::new("Top");
    let a = top.instantiate(&block("A", &[("q", IO::Output(8))]), None, None);
    let x = top.instantiate(
        &block(
            "X",
            &[
                ("i", IO::Input(8)),
                ("x", IO::Output(2)),
                ("y", IO::Output(4)),
            ],
        ),
        None,
        None,
    );
    let dest = block("Dest", &[("d", IO::Input(4))]);
    let b = top.instantiate(&dest, None, None);
    a.get_port("q").connect(&x.get_port("i"));
    x.get_port("y").connect(&b.get_port("d"));
    let graph = paths_between(&a, &b, &PathOptions::default()).unwrap();
    assert_eq!(graph.through()[0].exits.len(), 1);
    assert!(
        matches!(&graph.routes().next().unwrap()[1],PathStep::Through{enter,leave,kind:ThroughKind::Unknown} if enter.width()==8 && leave.width()==4)
    );
    let opts = PathOptions::default()
        .hierarchy(HierarchyPolicy::expand_all().for_instance(&x, HierarchyAction::Stop));
    assert_eq!(paths_between(&a, &b, &opts).unwrap().routes().count(), 0);
    assert_eq!(a.get_port("q").bit(3).trace_paths(&opts).through().len(), 0);
}

#[test]
fn nested_selection_observes_the_ports_on_both_faces() {
    let top = ModDef::new("Top");
    let mid = ModDef::new("Mid");
    let leaf = block("Leaf", &[("d", IO::Input(4))]);
    mid.add_port("pass", IO::Input(8))
        .slice(7, 4)
        .connect(&mid.instantiate(&leaf, Some("leaf"), None).get_port("d"));
    let mid = top.instantiate(&mid, Some("mid"), None);
    let q = top.add_port("q", IO::Input(8));
    q.connect(&mid.get_port("pass"));
    let leaf = mid.get_instance("leaf");
    let links = connections_between(&[&top, &mid, &leaf], ConnectionView::Signals).unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(
        connections_between(&[&top, &leaf], ConnectionView::Signals)
            .unwrap()
            .len(),
        1
    );
    let group = [&mid, &leaf];
    let boundary = boundary_connections(&group).unwrap();
    assert_eq!(boundary.len(), 1);
    assert!(peer(&boundary[0]).get_port().get_mod_inst().is_none());
}

#[test]
fn instance_loopback_is_visible_from_its_pins_but_internal_to_its_group() {
    let top = ModDef::new("Top");
    let cell = block("Cell", &[("q", IO::Output(4)), ("d", IO::Input(4))]);
    let inst = top.instantiate(&cell, None, None);
    let q = inst.get_port("q");
    let d = inst.get_port("d");
    let tap = top.add_port("tap", IO::Output(4));
    q.connect(&d);
    q.connect(&tap);

    let drivers = d.drivers();
    assert_eq!(drivers.len(), 1);
    assert_eq!(peer(&drivers[0]), &q.to_port_slice());
    let loads = q.receivers();
    assert_eq!(loads.len(), 2);
    assert!(loads.iter().any(|c| c.connection == drivers[0].connection));
    assert!(loads.iter().any(|c| peer(c) == &tap.to_port_slice()));

    let external = boundary_connections(&inst).unwrap();
    assert_eq!(external.len(), 1);
    assert_eq!(peer(&external[0]), &tap.to_port_slice());
}

#[test]
fn group_boundary_keeps_both_attachments_of_an_external_feedthrough() {
    let top = ModDef::new("Top");
    let cell = block("Cell", &[("q", IO::Output(4)), ("d", IO::Input(4))]);
    let a = top.instantiate(&cell, Some("a"), None);
    let feed = ModDef::new("Feed");
    feed.add_port("i", IO::Input(2))
        .connect(&feed.add_port("o", IO::Output(2)));
    let external = top.instantiate(&feed, Some("external"), None);

    let q = a.get_port("q");
    let d = a.get_port("d");
    let i = external.get_port("i");
    let o = external.get_port("o");
    q.slice(3, 2).connect(&i);
    o.connect(&d.slice(1, 0));
    q.slice(1, 0).connect(&d.slice(3, 2));

    let cut = boundary_connections(&top.get_instance("a")).unwrap();
    assert_eq!(cut.len(), 2);
    for (inside, outside, role) in [
        (q.slice(3, 2), i, Role::Receiver),
        (d.slice(1, 0), o, Role::Driver),
    ] {
        let route = [inside, outside.to_port_slice()];
        assert!(
            cut.iter()
                .any(|c| c.route() == route && c.peer_endpoint().role == Some(role))
        );
    }
    assert!(boundary_connections(&[&a, &external]).unwrap().is_empty());
    assert!(boundary_connections(&top).unwrap().is_empty());
}

#[test]
fn complete_inout_interface_survives_overlapping_routes() {
    for direct_is_wide in [true, false] {
        let top = ModDef::new("Top");
        let pad = block("Pad", &[("bus_data", IO::InOut(4))]);
        pad.def_intf_from_prefix("bus", "bus_");
        let a = top.instantiate(&pad, Some("a"), None);
        let b = top.instantiate(&pad, Some("b"), None);
        let p = a.get_port("bus_data");
        let q = b.get_port("bus_data");
        let width = if direct_is_wide { 2 } else { 4 };
        let bridge = ModDef::new("Bridge");
        bridge
            .add_port("i", IO::Input(width))
            .connect(&bridge.add_port("o", IO::Output(width)));
        let bridge = top.instantiate(&bridge, None, None);
        if direct_is_wide {
            p.connect(&q);
            p.slice(2, 1).connect(&bridge.get_port("i"));
            bridge.get_port("o").connect(&q.slice(2, 1));
        } else {
            p.slice(2, 1).connect(&q.slice(2, 1));
            p.connect(&bridge.get_port("i"));
            bridge.get_port("o").connect(&q);
        }
        top.validate();
        for [first, second] in [[&a, &b], [&b, &a]] {
            let groups: [&dyn ModuleGroup; 2] = [first, second];
            let flat = connections_between(&groups, ConnectionView::Signals).unwrap();
            let grouped =
                connections_between(&groups, ConnectionView::InterfacesAndSignals).unwrap();
            let [Connection::Interface(pair)] = grouped.as_slice() else {
                panic!("expected one complete interface pair");
            };
            let mut bits = [0; 4];
            for flat_wire in &flat {
                let Connection::Signal(w) = flat_wire else {
                    panic!("expected a signal");
                };
                let from = w.queried_endpoint().endpoint.as_pin().unwrap();
                let to = peer(w);
                let (u, v) = (from.get_port(), to.get_port());
                assert!((u == p && v == q) || (u == q && v == p));
                assert_eq!((from.lsb(), from.msb()), (to.lsb(), to.msb()));
                for count in &mut bits[from.lsb()..=from.msb()] {
                    *count += 1;
                }
                assert!(
                    pair.member_connections()
                        .iter()
                        .any(|(name, member)| name == "data" && member.connection == w.connection)
                );
            }
            assert_eq!(bits, [1; 4]);
            assert_eq!(pair.member_connections().len(), flat.len());
        }
    }
}

#[test]
fn naming_a_connected_net_preserves_the_boundary_cut() {
    let top = ModDef::new("Top");
    let tx = top.instantiate(&block("Tx", &[("q", IO::Output(4))]), None, None);
    let sink = block("Rx", &[("d", IO::Input(4))]);
    let rx = top.instantiate(&sink, Some("rx"), None);
    let q = tx.get_port("q");
    q.connect(&rx.get_port("d"));
    assert!(boundary_connections(&[&tx, &rx]).unwrap().is_empty());
    q.specify_net_name("link");
    top.validate();
    assert!(boundary_connections(&[&tx, &rx]).unwrap().is_empty());
    assert!(q.trace_connections().iter().any(|w|
        matches!(&w.peer_endpoint().endpoint, Endpoint::NamedWire { name, msb: 3, lsb: 0, .. } if name == "link")));

    let tap = top.instantiate(&sink, Some("tap"), None);
    q.connect(&tap.get_port("d"));
    let cut = boundary_connections(&[&tx, &rx]).unwrap();
    let [crossing] = cut.as_slice() else {
        panic!("expected just the external load");
    };
    assert_eq!(peer(crossing), &tap.get_port("d").to_port_slice());
}

#[test]
fn boundary_retains_only_otherwise_unbound_bits_of_a_net_name() {
    let top = ModDef::new("Top");
    let pad = block("Pad", &[("p", IO::InOut(8))]);
    let a = top.instantiate(&pad, Some("a"), None);
    let p = a.get_port("p");
    p.slice(6, 1).specify_net_name("link");
    let bus = top.add_port("bus", IO::InOut(2));
    p.slice(4, 3).connect(&bus);
    p.bit(6).unused();
    let mut names = Vec::new();
    for link in boundary_connections(&a).unwrap() {
        let start = link.queried_endpoint().endpoint.as_pin().unwrap();
        if let Endpoint::NamedWire { name, lsb, msb, .. } = &link.peer_endpoint().endpoint {
            assert_eq!(name, "link");
            assert_eq!((*lsb, *msb), (start.lsb(), start.msb()));
            names.push((*lsb, *msb));
        }
    }
    names.sort_unstable();
    assert_eq!(names, [(1, 2), (5, 5)]);
}

#[test]
fn disjoint_slices_of_one_inout_connect_through_its_instance() {
    let wrapper = ModDef::new("Wrapper");
    let internal = wrapper.add_port("bus", IO::InOut(8));
    internal.slice(2, 1).connect(&internal.slice(6, 5));

    let top = ModDef::new("Top");
    let child = top.instantiate(&wrapper, Some("child"), None);
    let bus = top.add_port("bus", IO::InOut(8));
    bus.connect(&child.get_port("bus"));

    for (from, to) in [(1, 5), (2, 6), (5, 1), (6, 2)] {
        for traces in [bus.bit(from).drivers(), bus.bit(from).receivers()] {
            assert_eq!(traces.len(), 1);
            let trace = &traces[0];
            assert_eq!(trace.stop_reason, StopReason::Boundary);
            assert_eq!(trace.peer_endpoint().role, Some(Role::InOut));
            assert_eq!(
                trace.route(),
                &[
                    bus.bit(from),
                    child.get_port("bus").bit(from),
                    child.get_port("bus").bit(to),
                    bus.bit(to),
                ]
            );
        }
    }

    let union = connections_touching(&[bus.slice(2, 1), bus.slice(6, 5)]);
    assert_eq!(union.len(), 1);
    assert_eq!(
        union[0].queried_endpoint().endpoint.as_pin(),
        Some(&bus.slice(2, 1))
    );
    assert_eq!(
        union[0].peer_endpoint().endpoint.as_pin(),
        Some(&bus.slice(6, 5))
    );
}

#[test]
fn overlapping_slices_of_one_inout_join_all_bits_without_a_cycle() {
    use std::collections::BTreeSet;

    let top = ModDef::new("Top");
    let pad = top.add_port("pad", IO::InOut(4));
    pad.slice(2, 0).connect(&pad.slice(3, 1));

    // The physical edges are 0-1, 1-2 and 2-3. Every pair is on the same net.
    let mut pairs = BTreeSet::new();
    for trace in connections_touching(&[pad.to_port_slice(), pad.bit(2)]) {
        assert_eq!(trace.stop_reason, StopReason::Boundary);
        let start = trace.queried_endpoint().endpoint.as_pin().unwrap();
        let end = peer(&trace);
        assert_eq!(start.width(), end.width());
        for offset in 0..start.width() {
            let a = start.lsb() + offset;
            let b = end.lsb() + offset;
            assert!(pairs.insert((a.min(b), a.max(b))), "duplicate bit pair");
        }
    }
    assert_eq!(
        pairs,
        BTreeSet::from([(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)])
    );
}

#[test]
fn group_query_on_a_shared_inout_reports_only_the_overlapping_lanes() {
    let top = ModDef::new("Top");
    let bus = top.add_port("backplane", IO::InOut(12));
    let leaf = block("Leaf", &[("pads", IO::InOut(8))]);
    let left = top.instantiate(&leaf, Some("left"), None);
    let right = top.instantiate(&leaf, Some("right"), None);
    let a = left.get_port("pads");
    let b = right.get_port("pads");
    a.connect(&bus.slice(7, 0));
    b.connect(&bus.slice(10, 3));

    for (first, second, start, end) in [
        (&left, &right, a.slice(7, 3), b.slice(4, 0)),
        (&right, &left, b.slice(4, 0), a.slice(7, 3)),
    ] {
        let links = connections_between(&[first, second], ConnectionView::Signals).unwrap();
        let [Connection::Signal(trace)] = links.as_slice() else {
            panic!("expected one shared range");
        };
        assert_eq!(trace.route(), &[start, bus.slice(7, 3), end]);
    }
}

#[test]
fn wire_cycle_is_reported_and_bidirectional_chain_does_not_backtrack() {
    let wrap = ModDef::new("Feed");
    wrap.add_port("i", IO::Input(4))
        .connect(&wrap.add_port("o", IO::Output(4)));
    let top = ModDef::new("Top");
    let a = top.instantiate(&wrap, Some("a"), None);
    let b = top.instantiate(&wrap, Some("b"), None);
    a.get_port("o").connect(&b.get_port("i"));
    b.get_port("o").connect(&a.get_port("i"));
    let tap = top.add_port("tap", IO::Output(4));
    a.get_port("o").connect(&tap);
    let receivers = a.get_port("o").receivers();
    assert_eq!(receivers.len(), 1);
    assert_eq!(peer(&receivers[0]), &tap.to_port_slice());
    assert!(
        a.get_port("o")
            .trace_connections()
            .iter()
            .any(|c| c.stop_reason == StopReason::Cycle && c.peer_endpoint().role.is_none())
    );

    let graph = a.get_port("o").trace_paths(&PathOptions::default());
    assert!(graph.routes().any(|route| matches!(route.as_slice(),
        [PathStep::Wire(w), PathStep::Feedback { at }]
        if w.stop_reason == StopReason::Cycle && w.route().last() == Some(at)
    )));

    let pad = block("Pad", &[("io", IO::InOut(1))]);
    let pads: Vec<_> = (0..3)
        .map(|i| top.instantiate(&pad, Some(&format!("p{i}")), None))
        .collect();
    for pair in pads.windows(2) {
        pair[0].get_port("io").connect(&pair[1].get_port("io"));
    }
    let links = pads[0].get_port("io").drivers();
    assert_eq!(links.len(), 2);
    assert!(links.iter().all(
        |c| peer(c).get_port().get_mod_inst().unwrap().name() != "p0"
            && c.peer_endpoint().role == Some(Role::InOut)
    ));
}

#[test]
fn collapsed_wrapper_visibility_is_independent_of_the_starting_pin() {
    let feed = ModDef::new("Feed");
    feed.add_port("i", IO::Input(1))
        .connect(&feed.add_port("o", IO::Output(1)));
    let wrap = ModDef::new("Wrapper");
    let first = wrap.instantiate(&feed, Some("first"), None);
    let second = wrap.instantiate(&feed, Some("second"), None);
    wrap.add_port("i", IO::Input(1))
        .connect(&first.get_port("i"));
    first.get_port("o").connect(&second.get_port("i"));
    second
        .get_port("o")
        .connect(&wrap.add_port("o", IO::Output(1)));

    let top = ModDef::new("Top");
    let w = top.instantiate(&wrap, Some("w"), None);
    let a = top.add_port("a", IO::Input(1));
    let b = top.add_port("b", IO::Output(1));
    let i = w.get_port("i");
    let o = w.get_port("o");
    a.connect(&i);
    o.connect(&b);
    let first = w.get_instance("first");
    let second = w.get_instance("second");
    let first_out = first.get_port("o");
    let second_in = second.get_port("i");
    let policy = HierarchyPolicy::expand_all().for_instance(&w, HierarchyAction::Collapse);

    for (hits, expected) in [
        (a.receivers_with(&policy), vec![&a, &i, &o, &b]),
        (i.receivers_with(&policy), vec![&i, &o, &b]),
        (first_out.receivers_with(&policy), vec![&first_out, &o, &b]),
        (o.drivers_with(&policy), vec![&o, &i, &a]),
        (second_in.drivers_with(&policy), vec![&second_in, &i, &a]),
    ] {
        let [wire] = hits.as_slice() else {
            panic!("expected one wire")
        };
        let pins: Vec<_> = expected.into_iter().map(|p| p.to_port_slice()).collect();
        assert!(
            wire.visible_route().eq(&pins),
            "visible route from {:?}",
            pins[0]
        );
        assert!(wire.route().len() > pins.len());
    }

    // Both endpoints are inside w. The route never visits either wrapper port.
    let graph = paths_between(&first, &second, &PathOptions::default().hierarchy(policy)).unwrap();
    let route = graph.routes().next().unwrap();
    assert!(
        matches!(&route[..], [PathStep::Collapsed { instance, enter, leave, .. }]
        if instance.last().unwrap() == "w" && enter == &first_out.to_port_slice()
            && leave == &second_in.to_port_slice())
    );
    assert!(matches!(
        &graph.expanded_routes().next().unwrap()[..],
        [PathStep::Wire(_)]
    ));
}

#[test]
fn collapsed_register_wrapper_keeps_its_clocked_evidence() {
    let wrap = ModDef::new("Retimed");
    let input = wrap.add_port("i", IO::Input(4));
    let output = wrap.add_port("o", IO::Output(4));
    input.connect_pipeline(
        &output,
        PipelineConfig {
            depth: 2,
            inst_name: Some("delay".into()),
            ..Default::default()
        },
    );
    let top = ModDef::new("Top");
    let a = top.instantiate(&block("A", &[("q", IO::Output(4))]), None, None);
    let b = top.instantiate(&block("B", &[("d", IO::Input(4))]), None, None);
    let retimed = top.instantiate(&wrap, Some("retimed"), None);
    a.get_port("q").connect(&retimed.get_port("i"));
    retimed.get_port("o").connect(&b.get_port("d"));
    let policy = HierarchyPolicy::expand_all().for_definition(&wrap, HierarchyAction::Collapse);
    let g = paths_between(&a, &b, &PathOptions::default().hierarchy(policy)).unwrap();
    assert_eq!(g.through().len(), 1);
    assert_eq!(g.through()[0].path.last().unwrap(), "retimed");
    let route = g.routes().next().unwrap();
    assert!(
        matches!(&route[..],[PathStep::WireHop{..},PathStep::Collapsed{enter,leave,steps,..},PathStep::WireHop{..}]
        if enter.get_port().name()=="i" && leave.get_port().name()=="o" && steps.iter().any(|s|matches!(s,PathStep::Through{kind:ThroughKind::Clocked{cycles:2,..},..})))
    );
}

#[test]
fn opaque_mux_inputs_have_unknown_paths_to_the_output() {
    let top = ModDef::new("Top");
    let mux = block(
        "Mux",
        &[
            ("a", IO::Input(8)),
            ("b", IO::Input(8)),
            ("sel", IO::Input(1)),
            ("y", IO::Output(8)),
        ],
    );
    let inst = top.instantiate(&mux, None, None);
    let clk = top.add_port("clock_a", IO::Input(8));
    clk.connect(&inst.get_port("a"));
    let sel = top.add_port("mode", IO::Input(1));
    sel.connect(&inst.get_port("sel"));
    inst.get_port("y")
        .connect(&top.add_port("selected", IO::Output(8)));
    for (seed, input) in [
        (clk.bit(5), inst.get_port("a").bit(5)),
        (sel.to_port_slice(), inst.get_port("sel").to_port_slice()),
    ] {
        let graph = seed.trace_paths(&PathOptions::default());
        let route = graph.routes().next().unwrap();
        let [
            PathStep::Wire(_),
            PathStep::Through { enter, leave, kind },
            PathStep::Wire(_),
        ] = route.as_slice()
        else {
            panic!("expected one crossing through the opaque instance");
        };
        assert_eq!(*kind, ThroughKind::Unknown);
        assert_eq!(enter, &input);
        assert_eq!(leave, &inst.get_port("y").to_port_slice());

        let graph = input.trace_paths(&PathOptions::default());
        let route = graph.routes().next().unwrap();
        assert!(
            matches!(&route[..], [PathStep::Through { enter, leave, kind: ThroughKind::Unknown }, PathStep::Wire(_)]
            if enter == &input && leave == &inst.get_port("y").to_port_slice())
        );
        let no_opaque = PathOptions {
            cross_opaque: false,
            ..Default::default()
        };
        assert_eq!(input.trace_paths(&no_opaque).routes().count(), 0);
    }
}

#[test]
fn interval_mapping_matches_independent_bit_oracle() {
    use std::collections::BTreeSet;
    let n = 31;
    let core = ModDef::new("Scatter");
    let src = core.add_port("i", IO::Input(n * 3));
    let out = core.add_port("o", IO::Output(n * 3));
    for lane in 0..n {
        let j = (7 * lane + 11) % n;
        src.slice(lane * 3 + 2, lane * 3)
            .connect(&out.slice(j * 3 + 2, j * 3));
    }
    let top = ModDef::new("Top");
    let i = top.add_port("q", IO::Input(n * 3));
    let mid = top.instantiate(&core, None, None);
    i.connect(&mid.get_port("i"));
    for sink in ["a", "b"] {
        mid.get_port("o")
            .connect(&top.add_port(sink, IO::Output(n * 3)));
    }
    for (lo, hi) in [(0, n * 3 - 1), (2, 73), (17, 17), (n * 3 - 5, n * 3 - 2)] {
        let mut got = BTreeSet::new();
        for link in i.slice(hi, lo).receivers() {
            let start = link.queried_endpoint().endpoint.as_pin().unwrap();
            let peer = peer(&link);
            for k in 0..start.width() {
                assert!(got.insert((
                    start.lsb() + k,
                    peer.get_port().name().to_owned(),
                    peer.lsb() + k
                )));
            }
        }
        let mut want = BTreeSet::new();
        for bit in lo..=hi {
            for sink in ["a", "b"] {
                want.insert((bit, sink.to_owned(), ((bit / 3 * 7 + 11) % n) * 3 + bit % 3));
            }
        }
        assert_eq!(got, want);
    }
}

#[test]
fn bundling_a_subrange_preserves_the_entire_flat_result() {
    let top = ModDef::new("Top");
    let a = block("A", &[("q", IO::Output(8))]);
    let b = block("B", &[("d", IO::Input(8))]);
    a.def_intf(
        "middle",
        indexmap::IndexMap::from([("payload".into(), ("q".into(), 5, 2))]),
    );
    b.def_intf(
        "middle",
        indexmap::IndexMap::from([("payload".into(), ("d".into(), 5, 2))]),
    );
    let a = top.instantiate(&a, None, None);
    let b = top.instantiate(&b, None, None);
    a.get_port("q").connect(&b.get_port("d"));
    let flat = connections_between(&[&a, &b], ConnectionView::Signals).unwrap();
    let grouped = connections_between(&[&a, &b], ConnectionView::InterfacesAndSignals).unwrap();
    let expanded = |connections: &[Connection]| {
        let mut wires = Vec::new();
        for c in connections {
            match c {
                Connection::Signal(s) => wires.push(s),
                Connection::Interface(i) => {
                    wires.extend(i.member_connections().iter().map(|(_, s)| s))
                }
            }
        }
        wires.sort_by_key(|w| w.queried_endpoint().endpoint.as_pin().unwrap().lsb());
        wires
            .into_iter()
            .map(|s| (s.connection.clone(), s.route().to_vec()))
            .collect::<Vec<_>>()
    };
    assert_eq!(expanded(&flat), expanded(&grouped));
    assert_eq!(
        grouped
            .iter()
            .filter(|c| matches!(c, Connection::Interface(_)))
            .count(),
        1
    );
}

#[test]
fn touching_overlapping_slices_from_opposite_ends_is_a_bitwise_union() {
    let top = ModDef::new("Top");
    let a = top.add_port("a", IO::Input(16));
    let b = top.add_port("b", IO::Output(8));
    a.slice(11, 4).connect(&b);
    let links = connections_touching(&[a.slice(8, 4), b.slice(7, 2), a.slice(11, 6)]);
    let mut bits = std::collections::BTreeSet::new();
    for c in links {
        let start = c.queried_endpoint().endpoint.as_pin().unwrap();
        let dst = peer(&c);
        for i in 0..start.width() {
            let pair = if start.get_port().name() == "a" {
                (start.lsb() + i, dst.lsb() + i)
            } else {
                (dst.lsb() + i, start.lsb() + i)
            };
            assert!(bits.insert(pair), "duplicated {pair:?}");
        }
    }
    assert_eq!(bits, (0..8).map(|i| (i + 4, i)).collect());
}

#[test]
fn imported_module_is_a_known_terminal_with_an_uninspected_body() {
    let cell = ModDef::from_verilog(
        "External",
        "module External(input d, output q); assign q=d; endmodule",
    )
    .unwrap();
    let native = ModDef::new("Native");
    let ext = native.instantiate(&cell, Some("ext"), None);
    let src = native.add_port("src", IO::Input(1));
    src.connect(&ext.get_port("d"));
    let c = src.receivers().pop().unwrap();
    assert_eq!(c.stop_reason, StopReason::Opaque);
    assert_eq!(c.peer_endpoint().role, Some(Role::Receiver));
    assert_eq!(peer(&c).get_port().name(), "d");
    assert!(
        cell.get_port("d")
            .trace_connections()
            .iter()
            .any(|c| c.stop_reason == StopReason::Opaque)
    );
}

#[test]
fn narrowed_output_keeps_clocked_route_bit_precise_back_to_source() {
    let top = ModDef::new("Top");
    let a = top.instantiate(&block("A", &[("q", IO::Output(8))]), None, None);
    let b = top.instantiate(&block("B", &[("d", IO::Input(4))]), None, None);
    let pipeline = PipelineConfig {
        depth: 3,
        ..Default::default()
    }
    .to_mod_def(8);
    let delay = top.instantiate(&pipeline, Some("delay"), None);
    a.get_port("q").connect(&delay.get_port("in"));
    delay.get_port("out").slice(7, 4).connect(&b.get_port("d"));
    let graph = paths_between(&a, &b, &PathOptions::default()).unwrap();
    let route = graph.routes().next().unwrap();
    let [
        PathStep::Wire(first),
        PathStep::Through { enter, leave, .. },
        PathStep::Wire(last),
    ] = &route[..]
    else {
        panic!("expected a wire on each side of the pipeline");
    };
    assert_eq!(
        first.queried_endpoint().endpoint.as_pin(),
        Some(&a.get_port("q").slice(7, 4))
    );
    assert_eq!((enter.lsb(), enter.width()), (4, 4));
    assert_eq!((leave.lsb(), leave.width()), (4, 4));
    assert_eq!(peer(last).lsb(), 0);
    let wires: Vec<_> = graph.connections().map(|wire| &wire.connection).collect();
    assert_eq!(wires, [&first.connection, &last.connection]);
    let step = &graph.through()[0];
    assert_eq!(step.entries.iter().collect::<Vec<_>>(), [enter]);
    assert_eq!(step.exits.iter().collect::<Vec<_>>(), [leave]);
}

#[test]
fn targeted_summaries_keep_disjoint_lanes_through_wrappers_and_stop_narrowing_at_unknowns() {
    let top = ModDef::new("Top");
    let src = top.instantiate(&block("Src", &[("q", IO::Output(12))]), None, None);
    let dst = top.instantiate(
        &block(
            "Dst",
            &[
                ("low", IO::Input(2)),
                ("repeat", IO::Input(2)),
                ("high", IO::Input(1)),
            ],
        ),
        None,
        None,
    );
    let opaque = top.instantiate(
        &block("Opaque", &[("i", IO::Input(12)), ("o", IO::Output(8))]),
        None,
        None,
    );
    let wrap = ModDef::new("Wrap");
    let delay = wrap.instantiate(
        &PipelineConfig {
            depth: 2,
            ..Default::default()
        }
        .to_mod_def(8),
        Some("delay"),
        None,
    );
    wrap.add_port("i", IO::Input(8))
        .connect(&delay.get_port("in"));
    delay
        .get_port("out")
        .connect(&wrap.add_port("o", IO::Output(8)));
    let w = top.instantiate(&wrap, Some("w"), None);
    let pass = top.instantiate(
        &PipelineConfig {
            depth: 0,
            ..Default::default()
        }
        .to_mod_def(6),
        Some("pass"),
        None,
    );
    src.get_port("q").connect(&opaque.get_port("i"));
    opaque.get_port("o").connect(&w.get_port("i"));
    w.get_port("o").slice(7, 2).connect(&pass.get_port("in"));
    let out = pass.get_port("out");
    out.slice(2, 1).connect(&dst.get_port("low"));
    out.slice(2, 1).connect(&dst.get_port("repeat"));
    out.bit(5).connect(&dst.get_port("high"));
    out.slice(4, 3)
        .connect(&top.add_port("other", IO::Output(2)));

    for action in [HierarchyAction::Expand, HierarchyAction::Collapse] {
        let options = PathOptions::default()
            .hierarchy(HierarchyPolicy::expand_all().for_instance(&w, action));
        let graph = paths_between(&src, &dst, &options).unwrap();
        let routes: Vec<_> = graph.expanded_routes().collect();
        assert_eq!(routes.len(), 3);
        let expected: std::collections::HashSet<_> = routes
            .iter()
            .flatten()
            .filter_map(|step| match step {
                PathStep::Wire(wire) => Some(wire.connection.clone()),
                _ => None,
            })
            .collect();
        let wires: Vec<_> = graph.connections().collect();
        assert_eq!(wires.len(), expected.len());
        assert!(wires.iter().all(|wire| expected.contains(&wire.connection)));

        let instance = |name: &str| {
            graph
                .through()
                .iter()
                .find(|step| step.path.last().unwrap() == name)
                .unwrap()
        };
        let pins = |ranges: &indexmap::IndexSet<PortSlice>| {
            let mut bits: Vec<_> = ranges.iter().map(|p| (p.lsb(), p.msb())).collect();
            bits.sort_unstable();
            bits
        };
        let unknown = instance(&opaque.name());
        assert_eq!(pins(&unknown.entries), [(0, 11)]);
        assert_eq!(pins(&unknown.exits), [(3, 4), (7, 7)]);
        assert_eq!(pins(&instance("w").entries), [(3, 4), (7, 7)]);
        assert_eq!(pins(&instance("w").exits), [(3, 4), (7, 7)]);
        assert_eq!(pins(&instance("pass").entries), [(1, 2), (5, 5)]);
        assert_eq!(pins(&instance("pass").exits), [(1, 2), (5, 5)]);
    }
}

#[test]
fn identical_interface_aliases_report_all_four_declaration_pairs() {
    use std::collections::BTreeSet;

    let tx = block("Tx", &[("s_d", IO::Output(8))]);
    let rx = block("Rx", &[("s_d", IO::Input(8))]);
    for def in [&tx, &rx] {
        for alias in ["first", "second"] {
            def.def_intf_from_prefix(alias, "s_");
        }
    }
    let top = ModDef::new("Top");
    let tx = top.instantiate(&tx, None, None);
    let rx = top.instantiate(&rx, None, None);
    tx.get_port("s_d").connect(&rx.get_port("s_d"));
    let flat = connections_between(&[&tx, &rx], ConnectionView::Signals).unwrap();
    let [Connection::Signal(physical)] = flat.as_slice() else {
        panic!("expected one physical wire")
    };
    let grouped = connections_between(&[&tx, &rx], ConnectionView::InterfacesAndSignals).unwrap();
    let mut pairs = BTreeSet::new();
    for connection in &grouped {
        let Connection::Interface(pair) = connection else {
            panic!("wire is fully covered")
        };
        let [left, right] = pair.interfaces();
        assert!(pairs.insert((left.name.as_str(), right.name.as_str())));
        let [(name, wire)] = pair.member_connections() else {
            panic!("expected one member")
        };
        assert_eq!(name, "d");
        assert_eq!(wire.connection, physical.connection);
    }
    assert_eq!(
        pairs,
        BTreeSet::from([
            ("first", "first"),
            ("first", "second"),
            ("second", "first"),
            ("second", "second")
        ])
    );
}

#[test]
fn a_private_lane_only_groups_when_its_shared_member_reaches_the_same_peer() {
    let tx = block("Tx", &[("valid", IO::Output(1)), ("data", IO::Output(3))]);
    let rx = block(
        "Rx",
        &[
            ("valid", IO::Input(1)),
            ("other", IO::Input(1)),
            ("data", IO::Input(3)),
        ],
    );
    for lane in 0..3 {
        for (def, control) in [
            (&tx, "valid"),
            (&rx, if lane == 2 { "other" } else { "valid" }),
        ] {
            def.def_intf(
                format!("lane{lane}"),
                [
                    ("valid".into(), (control.into(), 0, 0)),
                    ("data".into(), ("data".into(), lane, lane)),
                ]
                .into(),
            );
        }
    }
    let top = ModDef::new("Top");
    let tx = top.instantiate(&tx, None, None);
    let rx = top.instantiate(&rx, None, None);
    tx.get_port("valid").connect(&rx.get_port("valid"));
    tx.get_port("data").connect(&rx.get_port("data"));
    rx.get_port("other").tieoff(0);
    let mut lanes = std::collections::BTreeSet::new();
    let mut controls = Vec::new();
    let mut leftovers = Vec::new();
    for conn in connections_between(&[&tx, &rx], ConnectionView::InterfacesAndSignals).unwrap() {
        match conn {
            Connection::Interface(pair) => {
                let [a, b] = pair.interfaces();
                assert_eq!(a.name, b.name);
                assert!(lanes.insert(a.name.clone()));
                let [(valid, wire), (data, _)] = pair.member_connections() else {
                    panic!("expected both members")
                };
                assert_eq!((valid.as_str(), data.as_str()), ("valid", "data"));
                controls.push(wire.connection.clone());
            }
            Connection::Signal(w) => leftovers.push(peer(&w).clone()),
        }
    }
    assert_eq!(lanes.into_iter().collect::<Vec<_>>(), ["lane0", "lane1"]);
    assert_eq!(controls[0], controls[1]);
    assert_eq!(leftovers, [rx.get_port("data").bit(2)]);
}

#[test]
fn overlapping_interface_ranges_share_only_their_covered_pieces() {
    let tx = block("Tx", &[("q", IO::Output(16))]);
    let rx = block("Rx", &[("d", IO::Input(14))]);
    for (name, hi, lo) in [
        ("separate", 14, 12),
        ("nested", 5, 3),
        ("crossing", 7, 5),
        ("wide", 9, 3),
    ] {
        tx.def_intf(name, [("data".into(), ("q".into(), hi, lo))].into());
        rx.def_intf(name, [("data".into(), ("d".into(), hi - 2, lo - 2))].into());
    }
    tx.def_intf("tx_only", [("a".into(), ("q".into(), 11, 10))].into());
    rx.def_intf("rx_only", [("b".into(), ("d".into(), 9, 9))].into());
    let top = ModDef::new("Top");
    let tx = top.instantiate(&tx, None, None);
    let rx = top.instantiate(&rx, None, None);
    tx.get_port("q").slice(15, 2).connect(&rx.get_port("d"));

    let range = |wire: &ConnectionTrace| {
        let [tx_pin, rx_pin] = wire
            .connection
            .endpoints
            .each_ref()
            .map(|e| e.endpoint.as_pin().unwrap());
        assert_eq!(tx_pin.lsb(), rx_pin.lsb() + 2);
        assert_eq!(tx_pin.msb(), rx_pin.msb() + 2);
        (tx_pin.lsb(), tx_pin.msb())
    };
    let flat = connections_between(&[&tx, &rx], ConnectionView::Signals).unwrap();
    let mut ranges: Vec<_> = flat
        .iter()
        .map(|c| match c {
            Connection::Signal(w) => range(w),
            _ => panic!("expected a signal"),
        })
        .collect();
    ranges.sort_unstable();
    assert_eq!(
        ranges,
        [
            (2, 2),
            (3, 4),
            (5, 5),
            (6, 7),
            (8, 9),
            (10, 10),
            (11, 11),
            (12, 14),
            (15, 15)
        ]
    );

    let mut pairs = std::collections::BTreeMap::new();
    let mut ungrouped = Vec::new();
    for c in connections_between(&[&tx, &rx], ConnectionView::InterfacesAndSignals).unwrap() {
        match c {
            Connection::Signal(w) => ungrouped.push(range(&w)),
            Connection::Interface(pair) => {
                let [left, right] = pair.interfaces();
                assert_eq!(left.name, right.name);
                let bits: Vec<_> = pair
                    .member_connections()
                    .iter()
                    .map(|(_, w)| range(w))
                    .collect();
                assert!(pairs.insert(left.name.clone(), bits).is_none());
            }
        }
    }
    assert_eq!(pairs.len(), 4);
    assert_eq!(pairs["wide"], [(3, 4), (5, 5), (6, 7), (8, 9)]);
    assert_eq!(pairs["nested"], [(3, 4), (5, 5)]);
    assert_eq!(pairs["crossing"], [(5, 5), (6, 7)]);
    assert_eq!(pairs["separate"], [(12, 14)]);
    ungrouped.sort_unstable();
    assert_eq!(ungrouped, [(2, 2), (10, 10), (11, 11), (15, 15)]);
}

#[test]
fn connected_pipeline_stage_tap_is_an_explicit_candidate() {
    let top = ModDef::new("Top");
    let a = top.instantiate(&block("Source", &[("q", IO::Output(8))]), None, None);
    let b = top.instantiate(&block("Consumer", &[("d", IO::Input(2))]), None, None);
    let delay = top.instantiate(
        &PipelineConfig {
            depth: 2,
            ..Default::default()
        }
        .to_mod_def(8),
        None,
        None,
    );
    a.get_port("q").connect(&delay.get_port("in"));
    delay
        .get_port("out_stages")
        .slice(11, 10)
        .connect(&b.get_port("d"));
    let mut options = PathOptions::default();
    let g = paths_between(&a, &b, &options).unwrap();
    assert!(
        matches!(&g.routes().next().unwrap()[1],PathStep::Through{kind:ThroughKind::Unknown,leave,..} if leave.get_port().name()=="out_stages" && leave.lsb()==10 && leave.msb()==11)
    );
    options.cross_opaque = false;
    assert_eq!(paths_between(&a, &b, &options).unwrap().routes().count(), 0);
}

#[test]
fn feedback_routes_close_at_a_pin_or_continue_to_the_requested_target() {
    use std::collections::HashSet;

    let top = ModDef::new("Top");
    let source = top.instantiate(&block("Source", &[("q", IO::Output(1))]), None, None);
    let merge = top.instantiate(
        &block(
            "Merge",
            &[
                ("start", IO::Input(1)),
                ("feedback", IO::Input(1)),
                ("q", IO::Output(1)),
            ],
        ),
        None,
        None,
    );
    let fork = top.instantiate(
        &block("Fork", &[("d", IO::Input(1)), ("q", IO::Output(1))]),
        None,
        None,
    );
    let delay = top.instantiate(
        &PipelineConfig {
            depth: 1,
            ..Default::default()
        }
        .to_mod_def(1),
        Some("delay"),
        None,
    );
    let seed = source.get_port("q");
    let feedback = merge.get_port("feedback");
    let delayed = delay.get_port("out");
    seed.connect(&merge.get_port("start"));
    merge.get_port("q").connect(&fork.get_port("d"));
    fork.get_port("q").connect(&delay.get_port("in"));
    delayed.connect(&feedback);
    top.add_port("clk", IO::Input(1))
        .connect(&delay.get_port("clk"));
    delay.get_port("out_stages").unused();
    top.validate();

    let options = PathOptions::default();
    // From the source, the merge crossing repeats. From the delay output, its outgoing wire repeats.
    for (start, at) in [(&seed, &feedback), (&delayed, &delayed)] {
        let routes: Vec<_> = start.trace_paths(&options).expanded_routes().collect();
        assert_eq!(routes.len(), 1);
        assert!(
            matches!(routes[0].last(), Some(PathStep::Feedback { at: pin }) if pin == &at.to_port_slice())
        );
        assert!(routes[0].iter().any(|s| matches!(
            s,
            PathStep::Through {
                kind: ThroughKind::Clocked { cycles: 1, .. },
                ..
            }
        )));
    }
    let collapsed = PathOptions::default().hierarchy(HierarchyPolicy::collapse_all());
    let route = seed.trace_paths(&collapsed).routes().next().unwrap();
    assert!(
        matches!(route.last(), Some(PathStep::Feedback { at }) if at == &feedback.to_port_slice())
    );

    let dest = top.instantiate(&block("Dest", &[("d", IO::Input(1))]), None, None);
    assert!(
        paths_between(&source, &dest, &options)
            .unwrap()
            .routes()
            .next()
            .is_none()
    );
    let target = dest.get_port("d");
    fork.get_port("q").connect(&target);
    top.validate();

    let graph = paths_between(&source, &dest, &options).unwrap();
    let routes: Vec<_> = graph.expanded_routes().collect();
    assert_eq!(routes.len(), 2);
    let mut clocked_crossings = Vec::new();
    let mut wires = HashSet::new();
    for route in routes {
        assert!(
            matches!(route.last(), Some(PathStep::Wire(w)) if peer(w) == &target.to_port_slice())
        );
        clocked_crossings.push(
            route
                .iter()
                .filter(|s| {
                    matches!(
                        s,
                        PathStep::Through {
                            kind: ThroughKind::Clocked { cycles: 1, .. },
                            ..
                        }
                    )
                })
                .count(),
        );
        for step in route {
            if let PathStep::Wire(wire) = step {
                wires.insert(wire.connection);
            }
        }
    }
    clocked_crossings.sort_unstable();
    assert_eq!(clocked_crossings, [0, 1]);
    assert_eq!(
        wires,
        graph.connections().map(|w| w.connection.clone()).collect()
    );
    assert!(
        graph
            .through()
            .iter()
            .any(|step| step.path.last().unwrap() == "delay")
    );
    let no_registers = PathOptions {
        cross_registers: false,
        ..Default::default()
    };
    assert_eq!(
        paths_between(&source, &dest, &no_registers)
            .unwrap()
            .routes()
            .count(),
        1
    );
}

#[test]
fn whole_port_queries_skip_a_zero_depth_pipeline_tap() {
    let top = ModDef::new("Top");
    let delay = top.instantiate(&PipelineConfig::default().to_mod_def(4), None, None);
    let tap = delay.get_port("out_stages");
    let q = top.add_port("q", IO::Input(4));
    q.connect(&delay.get_port("in"));
    assert_eq!(tap.io().width(), 0);
    assert!(tap.drivers().is_empty());
    assert!(tap.receivers().is_empty());
    assert!(tap.trace_connections().is_empty());
    let graph = tap.trace_paths(&PathOptions::default());
    assert_eq!(graph.connections().count(), 0);
    assert!(graph.through().is_empty());
    assert_eq!(graph.routes().count(), 0);
    assert!(connections_touching([&tap]).is_empty());
    let mixed = connections_touching([&tap, &q, &tap]);
    assert_eq!(mixed.len(), 1);
    assert_eq!(mixed[0].connection, q.receivers()[0].connection);
}

#[test]
fn extraction_uses_the_modeled_body_independently_of_emission_usage() {
    let top = ModDef::new("Top");
    let def = ModDef::new("Cell");
    let input = def.add_port("i", IO::Input(2));
    let output = def.add_port("o", IO::Output(1));
    let cell = top.instantiate(&def, None, None);
    let source = top.add_port("source", IO::Input(2));
    let sink = top.add_port("sink", IO::Output(1));
    source.connect(&cell.get_port("i"));
    cell.get_port("o").connect(&sink);
    let usages = [
        Usage::EmitDefinitionAndDescend,
        Usage::EmitDefinitionAndStop,
        Usage::EmitNothingAndStop,
        Usage::EmitStubAndStop,
    ];

    for usage in &usages {
        def.set_usage(usage.clone());
        let links = source.receivers();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].stop_reason, StopReason::Opaque);
        let graph = source.trace_paths(&PathOptions::default());
        assert!(graph.expanded_routes().flatten().any(|step| matches!(
            step,
            PathStep::Through {
                kind: ThroughKind::Unknown,
                ..
            }
        )));
    }
    input.bit(0).connect(&output);
    for usage in &usages {
        def.set_usage(usage.clone());
        let wired = source.bit(0).receivers();
        assert_eq!(wired.len(), 1);
        assert_eq!(peer(&wired[0]), &sink.to_port_slice());
        let gap = source.bit(1).receivers();
        assert_eq!(gap.len(), 1);
        assert_eq!(gap[0].stop_reason, StopReason::Unconnected);
        assert_eq!(peer(&gap[0]), &cell.get_port("i").bit(1));
    }
}
