// SPDX-License-Identifier: Apache-2.0

use crate::mod_inst::HierPathElem;
use crate::port::PortDirectionality;
use crate::{ConnectedItem, Mat3, ModDef, PhysicalPin, Port, PortSlice, Usage};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

// A connection or usage change in a shared child can affect any root containing it.
static TRACE_REVISION: AtomicU64 = AtomicU64::new(0);

impl PortSlice {
    /// Traces through the module hierarchy to find out what is connected to
    /// this `PortSlice`, if anything. For example, if `top` instantiates `a`
    /// as `a_inst` and `b` as `b_inst`, and `a_inst.x\[7:0\]` is connected to
    /// `b_inst.y\[7:0\]`, calling this function on `top.a_inst.x\[7:0\]` will
    /// return `Some(top.b_inst.y\[7:0\])`.
    ///
    /// Tracing stops at behavioral logic primitive pins and at modules whose usage stops descent.
    /// Mapped definitions descend to their physical cells. Forwarded outputs, concatenations,
    /// and zero-cycle delays contribute fanout where their bits reach a receiver or a root port.
    /// A slice contained in a single concat part traces through the wiring in either direction.
    ///
    /// This function does not currently support the following situations:
    /// (1) The `PortSlice` fans out to multiple places
    /// (2) Different parts of the `PortSlice` connect to different places
    /// (3) This `PortSlice` is connected to another `PortSlice` using multiple
    ///     `connect()` operations.
    pub fn trace_through_hierarchy(&self) -> Option<PortSlice> {
        self.try_trace_through_hierarchy()
            .unwrap_or_else(|reason| panic!("Failed to trace {} {reason}", self.debug_string()))
    }

    /// Invalidates driver proofs on all threads when any wiring or descent boundary changes.
    pub(crate) fn invalidate_trace_drivers() {
        TRACE_REVISION.fetch_add(1, Ordering::Release);
    }

    /// Walks one connected net, crossing hierarchy only where the implementation exposes its wiring.
    fn try_trace_through_hierarchy(&self) -> Result<Option<PortSlice>, &'static str> {
        let revision = TRACE_REVISION.load(Ordering::Acquire);
        let scope = match &self.port {
            Port::ModDef { .. } => &[][..],
            Port::ModInst { hierarchy, .. } => &hierarchy[..hierarchy.len() - 1],
        };
        let direction = self.get_directionality();
        let mut walk = TraceWalk::new(self, &direction, revision);
        walk.extend(self, scope, None);
        let mut visited = HashSet::new();
        let mut cycle = false;
        let mut endpoint = None;
        let mut partial = false;
        loop {
            if let Some(driver) = walk.cached_driver {
                return Ok(Some(driver));
            }
            let Some(TraceEdge {
                from,
                to,
                mut scope,
            }) = walk.pending.pop()
            else {
                break;
            };
            let path: Vec<_> = scope.iter().map(|frame| frame.inst_name.clone()).collect();
            if !visited.insert((path, from.clone(), to.clone())) {
                cycle = true;
                continue;
            }

            // Pins can join several wires on this side, including an internal pad on an open export.
            walk.extend(&to, &scope, Some(&from));
            let pin = match &to.port {
                Port::ModDef { .. } => {
                    if let Some(frame) = scope.pop() {
                        let outer = to.as_mod_inst_port_slice(vec![frame]);
                        walk.extend(&outer, &scope, None);
                        continue;
                    }
                    to
                }
                Port::ModInst { hierarchy, .. } => {
                    scope.push(hierarchy.last().unwrap().clone());
                    let def = to.get_mod_def_where_declared();
                    if trace_descends(&def) {
                        if scope.iter().any(|frame| {
                            frame.mod_def_core.as_ptr() == std::sync::Arc::as_ptr(&def.core)
                        }) {
                            return Err("due to an infinite loop");
                        }
                        walk.extend(&to.as_mod_def_port_slice(), &scope, None);
                        continue;
                    }
                    to.as_mod_inst_port_slice(scope)
                }
            };
            // A reverse trace from a receiver finds its driver even if that driver has other sinks.
            if pin == *self || !direction.compatible_with(&pin.get_directionality()) {
                continue;
            }
            if endpoint.as_ref() == Some(&pin) {
                continue;
            }
            // A separate live branch on any bit makes a full-width endpoint ambiguous.
            if endpoint.is_some() || (partial && pin.width() == self.width()) {
                return Err("due to fanout");
            }
            if pin.width() == self.width() {
                endpoint = Some(pin);
            } else {
                partial = true;
            }
        }

        if cycle && endpoint.is_none() && !partial {
            return Err("due to an infinite loop");
        }
        if let Some(driver) = &endpoint {
            walk.remember_driver(driver);
        }
        Ok(endpoint)
    }

    /// Returns the Manhattan gap between this ModInst port slice's physical
    /// pin and the physical pin it traces to.
    ///
    /// This currently supports only single-bit ModInst port slices. Wider port
    /// slices may be supported in the future, but for now this panics if called
    /// on a multi-bit slice.
    ///
    /// Returns `None` when the bit is tied off, unconnected, has multi-fanout,
    /// hits another trace error, traces to a top-level port, or either side
    /// does not have a physical pin.
    pub fn get_connection_distance(&self) -> Option<i64> {
        self.get_connected_port_slice_and_distance()
            .map(|(_, distance)| distance)
    }

    /// Returns the ModInst port slice connected to this ModInst port slice,
    /// along with the Manhattan gap between their physical pins.
    ///
    /// This currently supports only single-bit ModInst port slices. Wider port
    /// slices may be supported in the future, but for now this panics if called
    /// on a multi-bit slice.
    ///
    /// Returns `None` when the bit is tied off, unconnected, has multi-fanout,
    /// hits another trace error, traces to a top-level port, or either side
    /// does not have a physical pin.
    pub fn get_connected_port_slice_and_distance(&self) -> Option<(PortSlice, i64)> {
        let Some(self_mod_inst) = self.port.get_mod_inst() else {
            panic!(
                "get_connected_port_slice_and_distance only works on ports (or slices of ports) on module instances"
            );
        };

        let self_transform = self_mod_inst.get_transform();

        // TODO(sherbst) 2026-04-30: Support multi-bit port slices by returning a
        // matching-width connected PortSlice and an aggregate distance, such as
        // the maximum bit distance. Decide how to represent cases without a
        // single connected PortSlice covering the full width; one option is to
        // return the PortSlice associated with the longest-distance bit.
        let self_physical_pin = self.get_local_physical_pin()?;
        self.get_connected_bit_and_distance_with_self_transform(&self_transform, &self_physical_pin)
    }

    /// Crate-internal fast path for callers that already know this slice's
    /// instance transform and physical pin, such as per-bit validation loops.
    /// This avoids recomputing `ModInst::get_transform()` and re-reading the
    /// physical pin map for every bit.
    pub(crate) fn get_connected_bit_and_distance_with_self_transform(
        &self,
        self_transform: &Mat3,
        self_physical_pin: &PhysicalPin,
    ) -> Option<(PortSlice, i64)> {
        self.check_validity();
        assert!(
            matches!(self.port, Port::ModInst { .. }),
            "get_connected_bit_and_distance_with_self_transform requires a port slice on a module instance"
        );

        let other = self.try_trace_through_hierarchy().ok()??;
        let other_mod_inst = other.port.get_mod_inst()?;
        let other_transform = other_mod_inst.get_transform();
        let other_physical_pin = other.get_local_physical_pin()?;

        let distance = physical_pin_distance(
            self_transform,
            self_physical_pin,
            &other_transform,
            &other_physical_pin,
        );

        Some((other, distance))
    }

    fn get_local_physical_pin(&self) -> Option<PhysicalPin> {
        assert!(
            self.width() == 1,
            "physical pin lookup currently only supports single-bit port slices (called on {})",
            self.debug_string()
        );

        self.port
            .get_mod_def_core_where_declared()
            .read()
            .physical_pins
            .get(self.port.name())
            .and_then(|pins| pins.get(self.lsb))
            .and_then(|pin| pin.as_ref())
            .cloned()
    }

    /// Returns `true` if any part of this port slice ultimately traces to a tieoff.
    /// Similar to trace_through_hierarchy above, this does not support different
    /// parts of the `PortSlice` connecting to different places
    pub fn has_tieoff_connection(&self) -> bool {
        // TODO(jowright) 2026-03-25: Make this work with divergence or return a Result
        self.get_port_connections()
            .is_some_and(|connections| !connections.trace().to_tieoffs().is_empty())
    }
}

/// Returns whether this instance exposes its internal wiring to a hierarchy trace.
fn trace_descends(def: &ModDef) -> bool {
    let core = def.core.read();
    core.usage == Usage::EmitDefinitionAndDescend
        && (core.primitives.mapped_body || core.primitive.as_ref().is_none_or(|op| op.is_wiring()))
}

/// A clipped connection and the occurrence of the wiring body that contains it.
struct TraceEdge {
    from: PortSlice,
    to: PortSlice,
    scope: Vec<HierPathElem>,
}

/// Traverses clipped wiring arcs and remembers proven drivers at high-degree junctions.
struct TraceWalk {
    pending: Vec<TraceEdge>,
    revision: u64,
    cache_root: Option<ModDef>,
    junctions: Vec<PortSlice>,
    cached_driver: Option<PortSlice>,
}

impl TraceWalk {
    /// Only receiver bits on visible boundaries can certify every driver on their net.
    fn new(origin: &PortSlice, direction: &PortDirectionality, revision: u64) -> Self {
        let cache_root = if origin.width() == 1 && matches!(direction, PortDirectionality::Receiver)
        {
            match &origin.port {
                Port::ModDef { .. } => Some(origin.get_mod_def()),
                Port::ModInst { hierarchy, .. }
                    if !trace_descends(&origin.get_mod_def_where_declared())
                        && hierarchy.iter().skip(1).all(|frame| {
                            trace_descends(&ModDef {
                                core: frame.mod_def_core.upgrade().unwrap(),
                            })
                        }) =>
                {
                    Some(ModDef {
                        core: hierarchy[0].mod_def_core.upgrade().unwrap(),
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        Self {
            pending: Vec::new(),
            revision,
            cache_root,
            junctions: Vec::new(),
            cached_driver: None,
        }
    }

    /// Queues every clipped connection except the incoming edge. A previously verified junction
    /// can supply the driver before cloning and walking all of its receivers again.
    fn extend(&mut self, pin: &PortSlice, scope: &[HierPathElem], skip: Option<&PortSlice>) {
        let Some(connections) = pin.port.get_port_connections() else {
            return;
        };
        let high_degree = connections.read().len() >= 8;
        if high_degree && let Some(root) = &self.cache_root {
            let junction = if scope.is_empty() {
                pin.clone()
            } else {
                let mut path = scope.to_vec();
                if let Port::ModInst { hierarchy, .. } = &pin.port {
                    path.push(hierarchy.last().unwrap().clone());
                }
                pin.as_mod_inst_port_slice(path)
            };
            let core = root.core.read();
            let cache = core.high_fanout_drivers.read();
            if cache.revision == self.revision
                && let Some(driver) = cache.junctions.get(&junction)
            {
                self.cached_driver = Some(driver.clone());
                return;
            }
            self.junctions.push(junction);
        }
        for link in connections.read().slice(pin.msb, pin.lsb).into_vec() {
            if let ConnectedItem::PortSlice(to) = link.other
                && Some(&to) != skip
            {
                self.pending.push(TraceEdge {
                    from: link.this,
                    to,
                    scope: scope.to_vec(),
                });
            }
        }
    }

    /// Publishes fully qualified driver bits only after the entire visible net had a single driver.
    fn remember_driver(self, driver: &PortSlice) {
        if self.junctions.is_empty() {
            return;
        }
        let root = self.cache_root.unwrap();
        let core = root.core.read();
        let mut cache = core.high_fanout_drivers.write();
        if self.revision != TRACE_REVISION.load(Ordering::Acquire) {
            return;
        }
        if cache.revision != self.revision {
            cache.junctions.clear();
            cache.revision = self.revision;
        }
        for junction in self.junctions {
            cache.junctions.insert(junction, driver.clone());
        }
    }
}

fn physical_pin_distance(
    a_transform: &Mat3,
    a_physical_pin: &PhysicalPin,
    b_transform: &Mat3,
    b_physical_pin: &PhysicalPin,
) -> i64 {
    let a_bbox = a_physical_pin
        .transformed_polygon()
        .apply_transform(a_transform)
        .bbox();
    let b_bbox = b_physical_pin
        .transformed_polygon()
        .apply_transform(b_transform)
        .bbox();

    a_bbox.gap(&b_bbox)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConvertibleToPortSlice, IO, ModDef};

    fn check_trace_bidir(a: &impl ConvertibleToPortSlice, b: &impl ConvertibleToPortSlice) {
        assert_eq!(
            a.to_port_slice().trace_through_hierarchy().unwrap(),
            b.to_port_slice()
        );
        assert_eq!(
            b.to_port_slice().trace_through_hierarchy().unwrap(),
            a.to_port_slice()
        );
    }

    #[test]
    fn high_fanout_driver_tracks_nested_wiring_and_usage_changes() {
        let source = ModDef::new("Source");
        source.add_port("q", IO::Output(1));
        source.set_usage(Usage::EmitStubAndStop);
        let sink = ModDef::new("Sink");
        sink.add_port("d", IO::Input(1));
        sink.set_usage(Usage::EmitStubAndStop);
        let pad = ModDef::new("Pad");
        pad.add_port("p", IO::InOut(1));
        pad.set_usage(Usage::EmitStubAndStop);

        let top = ModDef::new("Top");
        let q = top.instantiate(&source, Some("src"), None).get_port("q");
        let mut receivers = Vec::new();
        for i in 0..16 {
            let d = top
                .instantiate(&sink, Some(&format!("sink_{i}")), None)
                .get_port("d");
            q.connect(&d);
            receivers.push(d.bit(0));
        }
        let branch = ModDef::new("Branch");
        let input = branch.add_port("i", IO::Input(1));
        input.connect(
            &branch
                .instantiate(&sink, Some("inside"), None)
                .get_port("d"),
        );
        let branch_inst = top.instantiate(&branch, Some("branch"), None);
        q.connect(&branch_inst.get_port("i"));

        assert_eq!(receivers[0].trace_through_hierarchy(), Some(q.bit(0)));
        assert_eq!(receivers[1].trace_through_hierarchy(), Some(q.bit(0)));
        std::thread::scope(|threads| {
            threads.spawn(|| {
                input.connect(&branch.instantiate(&pad, Some("pad"), None).get_port("p"))
            });
        });
        assert_eq!(
            receivers[1].try_trace_through_hierarchy(),
            Err("due to fanout")
        );

        branch.set_usage(Usage::EmitStubAndStop);
        assert_eq!(receivers[2].trace_through_hierarchy(), Some(q.bit(0)));
        let inside = branch_inst.get_instance("inside").get_port("d").bit(0);
        assert_eq!(inside.try_trace_through_hierarchy(), Err("due to fanout"));
        std::thread::scope(|threads| {
            threads.spawn(|| branch.set_usage(Usage::EmitDefinitionAndDescend));
        });
        assert_eq!(
            receivers[2].try_trace_through_hierarchy(),
            Err("due to fanout")
        );
    }

    #[test]
    fn high_fanout_forwarded_inputs_distinguish_occurrences_and_roots() {
        let sink = ModDef::new("Sink");
        sink.add_port("d", IO::Input(1));
        sink.set_usage(Usage::EmitStubAndStop);
        let bank = ModDef::new("Bank");
        let bus = bank.add_port("bus", IO::Input(1));
        for i in 0..16 {
            bus.connect(
                &bank
                    .instantiate(&sink, Some(&format!("sink_{i}")), None)
                    .get_port("d"),
            );
        }
        let top = ModDef::new("Top");
        let a = top.add_port("a", IO::Input(1));
        let b = top.add_port("b", IO::Input(1));
        let left = top.instantiate(&bank, Some("left"), None);
        let right = top.instantiate(&bank, Some("right"), None);
        left.get_port("bus").connect(&a);
        right.get_port("bus").connect(&b);

        // A read-only trace also works while a caller holds a read view of the root.
        let _view = top.core.read();
        for i in 0..16 {
            let name = format!("sink_{i}");
            let l = left.get_instance(&name).get_port("d");
            let r = right.get_instance(&name).get_port("d");
            let local = bank.get_instance(&name).get_port("d");
            assert_eq!(l.trace_through_hierarchy(), Some(a.bit(0)));
            assert_eq!(r.trace_through_hierarchy(), Some(b.bit(0)));
            assert_eq!(local.trace_through_hierarchy(), Some(bus.bit(0)));
        }
    }

    #[test]
    fn test_trace_inst_to_inst() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let b = ModDef::new("B");
        b.set_usage(Usage::EmitNothingAndStop);
        b.add_port("y", IO::Input(8));

        let top = ModDef::new("Top");

        let a_inst = top.instantiate(&a, Some("a_inst"), None);
        let b_inst = top.instantiate(&b, Some("b_inst"), None);
        a_inst.get_port("x").connect(&b_inst.get_port("y"));

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &b_inst.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_has_tieoff_connection() {
        let top = ModDef::new("Top");
        let a = top.add_port("a", IO::Output(4));
        let b = top.add_port("b", IO::Input(2));

        a.slice(1, 0).tieoff(0b10u32);
        a.slice(3, 2).connect(&b);

        assert!(a.has_tieoff_connection());
        assert!(a.slice(1, 0).has_tieoff_connection());
        assert!(!a.slice(3, 2).has_tieoff_connection());
        assert!(!b.has_tieoff_connection());
    }

    #[test]
    fn test_trace_inst_to_def() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let top = ModDef::new("Top");

        let a_inst = top.instantiate(&a, Some("a_inst"), None);
        a_inst.get_port("x").export_as("y");

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &top.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_def_to_def() {
        let top = ModDef::new("Top");
        top.feedthrough("x", "y", 8);

        check_trace_bidir(
            &top.get_port("x").slice(3, 0),
            &top.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_inst_up_two_levels() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let inner = ModDef::new("Inner");
        inner
            .instantiate(&a, Some("a_inst"), None)
            .get_port("x")
            .export_as("y");

        let top = ModDef::new("Top");
        top.instantiate(&inner, Some("inner_inst"), None)
            .get_port("y")
            .export_as("z");

        let a_inst = top.get_instance("inner_inst").get_instance("a_inst");

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &top.get_port("z").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_inst_to_inst_through_inner() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);
        let aa = a.wrap(Some("AA"), Some("a_inst"));

        let b = ModDef::new("B");
        b.add_port("y", IO::Input(8));
        b.set_usage(Usage::EmitNothingAndStop);
        let bb = b.wrap(Some("BB"), Some("b_inst"));

        let top = ModDef::new("Top");
        let aa_inst = top.instantiate(&aa, Some("aa_inst"), None);
        let bb_inst = top.instantiate(&bb, Some("bb_inst"), None);
        aa_inst.get_port("x").connect(&bb_inst.get_port("y"));

        let a_inst = top.get_instance("aa_inst").get_instance("a_inst");
        let b_inst = top.get_instance("bb_inst").get_instance("b_inst");

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &b_inst.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_inst_to_inst_feedthrough() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let b = ModDef::new("B");
        b.feedthrough("ft_in", "ft_out", 8);

        let c = ModDef::new("C");
        c.add_port("y", IO::Input(8));
        c.set_usage(Usage::EmitNothingAndStop);

        let top = ModDef::new("Top");
        let a_inst = top.instantiate(&a, Some("a_inst"), None);
        let b_inst = top.instantiate(&b, Some("b_inst"), None);
        let c_inst = top.instantiate(&c, Some("c_inst"), None);
        a_inst.get_port("x").connect(&b_inst.get_port("ft_in"));
        b_inst.get_port("ft_out").connect(&c_inst.get_port("y"));

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &c_inst.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_def_to_def_feedthrough() {
        let top = ModDef::new("top");
        top.add_port("x", IO::Input(8));
        top.add_port("y", IO::Output(8));
        top.set_usage(Usage::EmitNothingAndStop);

        let ft = ModDef::new("ft");
        ft.feedthrough("ft_in", "ft_out", 8);

        let ft_inst = top.instantiate(&ft, Some("ft_inst"), None);
        top.get_port("x").connect(&ft_inst.get_port("ft_in"));
        ft_inst.get_port("ft_out").connect(&top.get_port("y"));

        check_trace_bidir(
            &top.get_port("x").slice(3, 0),
            &top.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_ignores_open_exports_of_an_ordinary_child_output() {
        let leaf = ModDef::new("Leaf");
        leaf.add_port("data", IO::Output(3));
        leaf.set_usage(Usage::EmitStubAndStop);
        let wrapper = leaf.wrap(Some("Wrapper"), Some("leaf"));
        let data = wrapper.get_instance("leaf").get_port("data");
        wrapper.add_port("copy", IO::Output(3)).connect(&data);

        let top = ModDef::new("Top");
        let instance = top.instantiate(&wrapper, None, None);
        let source = instance.get_instance("leaf").get_port("data");
        let data = instance.get_port("data");
        let copy = instance.get_port("copy");
        let out = top.add_port("out", IO::Output(2));
        data.bit(0).unused();
        copy.slice(1, 0).connect(&out);
        data.bit(1)
            .connect(&top.add_port("also_used", IO::Output(1)));
        copy.bit(2).unused();
        data.bit(2).unused();

        check_trace_bidir(&source.bit(0), &out.bit(0));
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source.bit(1).trace_through_hierarchy()
        }))
        .unwrap_err();
        assert!(
            err.downcast_ref::<String>()
                .unwrap()
                .contains("due to fanout")
        );
        assert_eq!(source.bit(2).trace_through_hierarchy(), None);
    }

    #[test]
    #[should_panic(expected = "Failed to trace top.a_inst.x[3:0] due to fanout")]
    fn test_trace_fanout() {
        let a = ModDef::new("a");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let b = ModDef::new("b");
        b.add_port("y", IO::Input(8));
        b.set_usage(Usage::EmitNothingAndStop);

        let c = ModDef::new("c");
        c.add_port("z", IO::Input(8));
        c.set_usage(Usage::EmitNothingAndStop);

        let top = ModDef::new("top");
        let a_inst = top.instantiate(&a, Some("a_inst"), None);
        let b_inst = top.instantiate(&b, Some("b_inst"), None);
        let c_inst = top.instantiate(&c, Some("c_inst"), None);
        a_inst.get_port("x").connect(&b_inst.get_port("y"));
        a_inst.get_port("x").connect(&c_inst.get_port("z"));

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &c_inst.get_port("z").slice(3, 0),
        );
    }

    #[test]
    #[should_panic(expected = "Failed to trace a.x[7:0] due to an infinite loop")]
    fn test_trace_infinite_loop() {
        let a = ModDef::new("a");
        a.add_port("x", IO::Output(8));

        let b = a.wrap(Some("b"), Some("a_inst"));

        let b_inst = a.instantiate(&b, Some("b_inst"), None);
        b_inst.get_port("x").connect(&a.get_port("x"));

        a.get_port("x").trace_through_hierarchy();
    }
}
