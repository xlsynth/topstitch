// SPDX-License-Identifier: Apache-2.0

use topstitch::{Clock, ClockOrigin, ClockRole, IO, ModDef};

#[test]
fn resolves_defaults_and_overrides_by_range() {
    let m = ModDef::new("device");
    let clocks = m.add_port("clocks", IO::Input(2));
    let data = m.add_port("data", IO::Output(16));
    let ready = m.add_port("ready", IO::Output(1));
    clocks.bit(0).declare_clock().unwrap();
    let io = clocks.bit(1).declare_clock().unwrap();
    let core = m.get_port("clocks").bit(0).as_clock().unwrap();
    m.set_default_clock(&core).unwrap();
    let stream = m.def_intf(
        "stream",
        [
            ("payload".into(), ("data".into(), 15, 8)),
            ("clk".into(), ("clocks".into(), 1, 1)),
        ]
        .into(),
    );
    stream.set_default_clock(&io).unwrap();
    data.slice(11, 10).set_clock(&core).unwrap();
    data.bit(9).set_unclocked().unwrap();
    let result = m.clock_associations().unwrap();
    let ranges: Vec<_> = result
        .for_port(&data)
        .iter()
        .map(|r| {
            (
                r.slice.msb(),
                r.slice.lsb(),
                r.role.clone(),
                r.origin.clone(),
            )
        })
        .collect();
    let inherited = ClockOrigin::Interfaces(vec!["stream".into()]);
    assert_eq!(
        ranges,
        vec![
            (
                7,
                0,
                ClockRole::TimedBy(Clock::from(&core)),
                ClockOrigin::ModuleDefault
            ),
            (
                8,
                8,
                ClockRole::TimedBy(Clock::from(&io)),
                inherited.clone()
            ),
            (9, 9, ClockRole::Unclocked, ClockOrigin::Explicit),
            (
                11,
                10,
                ClockRole::TimedBy(Clock::from(&core)),
                ClockOrigin::Explicit
            ),
            (15, 12, ClockRole::TimedBy(Clock::from(&io)), inherited),
        ]
    );
    assert_eq!(result.for_port(&clocks)[0].role, ClockRole::ClockPin);
    assert_eq!(
        result.for_port(&ready)[0].role,
        ClockRole::TimedBy(Clock::from(&core))
    );
    assert_eq!(result.iter().count(), 7);
    stream.remove("payload");
    assert_eq!(
        m.clock_associations()
            .unwrap()
            .for_port(&data)
            .last()
            .unwrap()
            .origin,
        ClockOrigin::ModuleDefault
    );
}

#[test]
fn rejects_ambiguous_declarations_only_where_they_apply() {
    let m = ModDef::new("pair");
    let a = m.add_port("a", IO::Input(1)).declare_clock().unwrap();
    let b = m.add_port("b", IO::Input(1)).declare_clock().unwrap();
    let word = m.add_port("word", IO::Output(8));
    for (name, clock, msb, lsb) in [("left", &a, 7, 2), ("right", &b, 4, 0)] {
        m.def_intf(name, [("value".into(), ("word".into(), msb, lsb))].into())
            .set_default_clock(clock)
            .unwrap();
    }
    let error = m.clock_associations().err().unwrap();
    assert!(
        ["pair.word[4:2]", "left", "right"]
            .iter()
            .all(|s| error.contains(s)),
        "{error}"
    );
    word.slice(4, 2).set_clock(&b).unwrap();
    let result = m.clock_associations().unwrap();
    assert_eq!(result.for_port(&word).len(), 3);
    let error = word.slice(3, 1).set_clock(&a).unwrap_err();
    assert!(error.contains("pair.word[3:2]"), "{error}");
    assert_eq!(
        m.clock_associations().unwrap().for_port(&word),
        result.for_port(&word)
    );
}

#[test]
fn rejects_conflicting_explicit_roles_at_assignment() {
    let module = ModDef::new("device");
    let clk = module.add_port("clk", IO::Input(1));
    let a = clk.declare_clock().unwrap();
    let b = module.declare_external_clock("other").unwrap();
    let data = module.add_port("data", IO::Input(1));
    data.set_clock(&a).unwrap();
    data.set_clock(&a).unwrap();
    let error = data.set_clock(&b).unwrap_err();
    for detail in ["device.data[0:0]", "clk", "other"] {
        assert!(error.contains(detail), "{error}");
    }
    assert!(data.declare_clock().is_err());
    assert!(data.set_async().is_err());
    assert!(data.set_unclocked().is_err());
    assert!(clk.set_clock(&b).is_err());
    assert_eq!(
        module.clock_associations().unwrap().for_port(&data)[0].role,
        ClockRole::TimedBy((&a).into())
    );
    data.clear_clock_association().unwrap();
    data.set_clock(&b).unwrap();
    assert_eq!(
        module.clock_associations().unwrap().for_port(&data)[0].role,
        ClockRole::TimedBy((&b).into())
    );
}

#[test]
fn expired_clock_pin_keeps_its_identity_and_cannot_be_assigned() {
    let owner = ModDef::new("unit");
    let clock = owner.add_port("clk", IO::Input(2)).bit(1);
    let pin = clock.declare_clock().unwrap();
    let same_pin = clock.as_clock().unwrap();
    let pins = std::collections::HashSet::from([pin.clone()]);
    drop(owner);

    let replacement = ModDef::new("unit");
    let other_pin = replacement
        .add_port("clk", IO::Input(2))
        .bit(1)
        .declare_clock()
        .unwrap();
    let data = replacement.add_port("data", IO::Input(1));
    let error = data.set_clock(&pin).unwrap_err();
    assert!(
        error.contains("dropped module") && error.contains("clk[1"),
        "{error}"
    );
    assert_eq!(pin, same_pin);
    assert!(pins.contains(&same_pin));
    assert_ne!(pin, other_pin);
    assert_eq!(
        replacement.clock_associations().unwrap().for_port(&data)[0].role,
        ClockRole::Unspecified
    );
}

#[test]
fn clock_identity_belongs_to_one_definition() {
    let m = ModDef::new("unit");
    let clk = m.add_port("clk", IO::Input(1));
    let data = m.add_port("data", IO::Input(4));
    assert!(clk.as_clock().is_err());
    assert!(data.declare_clock().is_err());
    let pin = clk.declare_clock().unwrap();
    assert_eq!(clk.declare_clock().unwrap(), pin);
    assert!(data.bit(0).as_clock().is_err());
    let spare = m.add_port("spare", IO::Input(1));
    data.set_clock(&pin).unwrap();
    assert_eq!(
        m.clock_associations().unwrap().for_port(&spare)[0].role,
        ClockRole::Unspecified
    );
    let wrapper = m.wrap(Some("wrapper"), None);
    let wrapped_pin = wrapper.get_port("clk").as_clock().unwrap();
    assert_eq!(
        wrapper
            .clock_associations()
            .unwrap()
            .for_port(&wrapper.get_port("data"))[0]
            .role,
        ClockRole::TimedBy(Clock::from(&wrapped_pin))
    );
    let copy = m.stub("copy");
    let copy_pin = copy.get_port("clk").as_clock().unwrap();
    assert_ne!(pin, copy_pin);
    assert_eq!(
        copy.clock_associations()
            .unwrap()
            .for_port(&copy.get_port("data"))[0]
            .role,
        ClockRole::TimedBy(Clock::from(&copy_pin))
    );
    assert!(m.set_default_clock(&copy_pin).is_err());
    assert!(copy.get_port("data").set_clock(&pin).is_err());
    let instance = m.instantiate(&copy, Some("child"), None);
    assert!(instance.get_port("clk").declare_clock().is_err());
    assert!(instance.get_port("data").set_clock(&pin).is_err());
}

#[test]
fn inventories_and_resolves_local_and_external_domains() {
    let m = ModDef::new("receiver");
    let bus = m.add_port("clk", IO::Input(4));
    let b = bus.bit(3).declare_clock().unwrap();
    let a = bus.bit(0).declare_clock().unwrap();
    assert_eq!(bus.bit(3).declare_clock().unwrap(), b);
    assert!(m.get_external_clock("remote").is_err());
    let remote = m.declare_external_clock("remote").unwrap();
    let spare = m.declare_external_clock("unused").unwrap();
    let input = m.add_port("data", IO::Input(8));
    let ready = m.add_port("ready", IO::Input(1));
    m.set_default_clock(&b).unwrap();
    m.def_intf_from_prefix("rx", "data")
        .set_default_clock(&remote)
        .unwrap();
    ready.set_async().unwrap();
    assert!(input.set_async().is_err());
    assert_eq!(
        m.declare_external_clock("remote").unwrap(),
        m.get_external_clock("remote").unwrap()
    );
    assert_eq!(
        m.clocks(),
        vec![(&a).into(), (&b).into(), (&remote).into(), (&spare).into()]
    );
    let runs = m.clock_associations().unwrap();
    assert_eq!(
        runs.for_port(&input)[0].role,
        ClockRole::TimedBy((&remote).into())
    );
    assert_eq!(runs.for_port(&ready)[0].role, ClockRole::Async);
    let copy = m.stub("copy");
    assert!(copy.set_default_clock(&remote).is_err());
    assert_ne!(copy.get_external_clock("remote").unwrap(), remote);
    assert_eq!(
        copy.clock_associations()
            .unwrap()
            .for_port(&copy.get_port("data"))[0]
            .role,
        ClockRole::TimedBy((&copy.get_external_clock("remote").unwrap()).into())
    );
}

#[test]
fn clear_subtracts_only_the_requested_bits() {
    let module = ModDef::new("ranges");
    let data = module.add_port("data", IO::Input(5));
    let external = module.declare_external_clock("reference").unwrap();
    for start in 0..5 {
        for end in start..5 {
            for first in 0..5 {
                for last in first..5 {
                    data.slice(end, start).set_clock(&external).unwrap();
                    data.slice(last, first).clear_clock_association().unwrap();
                    let resolved = module.clock_associations().unwrap();
                    let actual: Vec<_> = resolved
                        .for_port(&data)
                        .iter()
                        .flat_map(|run| {
                            (run.slice.lsb()..=run.slice.msb()).map(|bit| (bit, run.role.clone()))
                        })
                        .collect();
                    let expected: Vec<_> = (0..5)
                        .map(|bit| {
                            let role =
                                if (start..=end).contains(&bit) && !(first..=last).contains(&bit) {
                                    ClockRole::TimedBy((&external).into())
                                } else {
                                    ClockRole::Unspecified
                                };
                            (bit, role)
                        })
                        .collect();
                    assert_eq!(
                        actual, expected,
                        "assign [{end}:{start}], clear [{last}:{first}]"
                    );
                    data.clear_clock_association().unwrap();
                }
            }
        }
    }
}

#[test]
fn clearing_restores_defaults_and_preserves_clock_sources() {
    let module = ModDef::new("defaults");
    let bus = module.add_port("bus", IO::Input(5));
    let source = bus.bit(4).declare_clock().unwrap();
    let local = module.declare_external_clock("local").unwrap();
    module.set_default_clock(&source).unwrap();
    module
        .def_intf("link", [("data".into(), ("bus".into(), 2, 1))].into())
        .set_default_clock(&local)
        .unwrap();
    bus.bit(0).set_clock(&local).unwrap();
    bus.bit(1).set_async().unwrap();
    bus.bit(2).set_unclocked().unwrap();
    bus.bit(3).set_clock(&local).unwrap();
    let consumer = module.add_port("consumer", IO::Output(1));
    consumer.set_clock(&source).unwrap();
    for _ in 0..2 {
        bus.clear_clock_association().unwrap();
        let snapshot = module.clock_associations().unwrap();
        let runs: Vec<_> = snapshot
            .for_port(&bus)
            .iter()
            .map(|r| {
                (
                    r.slice.lsb(),
                    r.slice.msb(),
                    r.role.clone(),
                    r.origin.clone(),
                )
            })
            .collect();
        assert_eq!(
            runs,
            vec![
                (
                    0,
                    0,
                    ClockRole::TimedBy((&source).into()),
                    ClockOrigin::ModuleDefault
                ),
                (
                    1,
                    2,
                    ClockRole::TimedBy((&local).into()),
                    ClockOrigin::Interfaces(vec!["link".into()])
                ),
                (
                    3,
                    3,
                    ClockRole::TimedBy((&source).into()),
                    ClockOrigin::ModuleDefault
                ),
                (4, 4, ClockRole::ClockPin, ClockOrigin::Explicit),
            ]
        );
        assert_eq!(bus.bit(4).as_clock().unwrap(), source);
        assert_eq!(
            snapshot.for_port(&consumer)[0].role,
            ClockRole::TimedBy((&source).into())
        );
        assert_eq!(module.clocks(), vec![(&source).into(), (&local).into()]);
    }
}

#[test]
fn clear_removes_all_overlapping_assignments_and_rejects_instance_ports() {
    let module = ModDef::new("child");
    let data = module.add_port("data", IO::Input(8));
    let a = module.declare_external_clock("a").unwrap();
    let b = module.declare_external_clock("b").unwrap();
    data.slice(7, 2).set_clock(&b).unwrap();
    data.slice(4, 0).set_clock(&b).unwrap();
    data.slice(4, 2).clear_clock_association().unwrap();
    data.slice(4, 2).set_clock(&a).unwrap();
    let parent = ModDef::new("parent");
    let child = parent.instantiate(&module, Some("child_i"), None);
    assert!(child.get_port("data").clear_clock_association().is_err());
    let resolved = module.clock_associations().unwrap();
    let runs = resolved.for_port(&data);
    assert_eq!(runs.len(), 3);
    for (run, (lsb, msb, clock)) in runs.iter().zip([(0, 1, &b), (2, 4, &a), (5, 7, &b)]) {
        assert_eq!((run.slice.lsb(), run.slice.msb()), (lsb, msb));
        assert_eq!(run.role, ClockRole::TimedBy(clock.into()));
    }
}

#[test]
fn clear_exposes_conflicting_inherited_defaults() {
    let module = ModDef::new("overlap");
    let data = module.add_port("data", IO::Input(4));
    for name in ["left", "right"] {
        let clock = module.declare_external_clock(name).unwrap();
        module
            .def_intf(name, [("value".into(), ("data".into(), 3, 0))].into())
            .set_default_clock(&clock)
            .unwrap();
    }
    data.set_unclocked().unwrap();
    let old = module.clock_associations().unwrap();
    data.bit(2).clear_clock_association().unwrap();
    let error = module.clock_associations().err().unwrap();
    assert!(
        ["overlap.data[2", "left", "right"]
            .iter()
            .all(|s| error.contains(s)),
        "{error}"
    );
    assert_eq!(old.for_port(&data)[0].role, ClockRole::Unclocked);
}
