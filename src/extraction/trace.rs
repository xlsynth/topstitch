// SPDX-License-Identifier: Apache-2.0

use super::intervals::{Interval, IntervalIndex};
use super::*;
use crate::mod_inst::HierPathElem;
use crate::{ConnectedItem, ConvertibleToPortSlice, IO, Port, PortSlice};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Flow {
    Up,
    Down,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Side {
    Parent,
    Body,
}

/// Which pins and attachments to report in a structural query.
pub(super) enum StopAt {
    /// Direct neighbors and otherwise unbound attachments on the selected subtree boundary.
    GroupBoundary,
    /// Follow structural wires to a terminal or the first selected occurrence.
    Selected(HashSet<InstanceKey>),
}

/// Snapshots the direct attachments on a local port for all occurrences using that port.
fn index_attachments(port: &Port, stop_at: &StopAt) -> IntervalIndex<ConnectedItem> {
    let mut entries: Vec<_> = port
        .get_port_connections()
        .map(|arcs| {
            arcs.read()
                .into_iter()
                .map(|arc| Interval {
                    lsb: arc.this.lsb,
                    msb: arc.this.msb,
                    value: arc.other.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    if matches!(stop_at, StopAt::GroupBoundary)
        && entries
            .iter()
            .any(|entry| matches!(entry.value, ConnectedItem::Wire(_)))
    {
        // A name labels any binding on the same bits. Only otherwise unbound
        // portions of a name are standalone attachments at a group boundary.
        let (names, mut bindings): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|entry| matches!(entry.value, ConnectedItem::Wire(_)));
        let mut bound = Intervals::default();
        for entry in &bindings {
            bound.insert(entry.lsb, entry.msb);
        }
        for name in names {
            for (lsb, msb) in bound.uncovered(name.lsb, name.msb) {
                bindings.push(Interval {
                    lsb,
                    msb,
                    value: name
                        .value
                        .slice_with_offset_and_width(lsb - name.lsb, msb - lsb + 1),
                });
            }
        }
        entries = bindings;
    }
    IntervalIndex::new(entries)
}

#[derive(Clone)]
struct Hop {
    pin: PortSlice,
    origin: usize,
    visible: bool,
    prev: Option<Rc<Hop>>,
    arrived: Option<Side>,
}

#[derive(Clone)]
struct Cursor {
    side: Side,
    origin: usize,
    route: Rc<Hop>,
    hidden: bool,
}
/// Query-local caches never survive a design edit.
pub(super) struct Session {
    stop_at: StopAt,
    adjacency: HashMap<Port, Rc<IntervalIndex<ConnectedItem>>>,
    pub(super) collapsed: HashSet<Arc<[String]>>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new(StopAt::Selected(HashSet::new()))
    }
}

impl Session {
    /// Creates a query whose cached attachments use the same stopping rules as its traces.
    pub(super) fn new(stop_at: StopAt) -> Self {
        Self {
            stop_at,
            adjacency: HashMap::new(),
            collapsed: HashSet::new(),
        }
    }

    /// Traces a pin from each applicable face, preserving the orientation of the seed.
    pub(super) fn trace(
        &mut self,
        pin: &PortSlice,
        flow: Flow,
        policy: &HierarchyPolicy,
        sides: &[Side],
    ) -> Vec<ConnectionTrace> {
        let mut hits = Vec::new();
        for &side in sides {
            if side == Side::Parent && pin.port.get_mod_inst().is_none() {
                continue;
            }
            let origin_role = role(&pin.port, side);
            if !can_depart(origin_role, flow) {
                continue;
            }
            let collapsed = collapsed_scope_depth(pin, side, policy);
            if let Some(depth) = collapsed {
                self.collapsed
                    .insert(Arc::from(&occurrence_path(&pin.port)[..=depth]));
            }
            let route = Rc::new(Hop {
                pin: pin.clone(),
                origin: pin.lsb,
                visible: true,
                prev: None,
                arrived: None,
            });
            if side == Side::Body {
                let def = pin.port.get_mod_def_where_declared();
                let inst = pin.port.get_mod_inst();
                if opaque(&def)
                    || inst
                        .as_ref()
                        .is_some_and(|i| policy.action(i, &def) == HierarchyAction::Stop)
                {
                    // No exterior face is available when the query starts on the definition itself.
                    if inst.is_none() {
                        hits.push(finish(
                            &route,
                            pin.lsb,
                            pin.width(),
                            Endpoint::Pin(route.pin.clone()),
                            origin_role,
                            None,
                            StopReason::Opaque,
                        ));
                    }
                    continue;
                }
            }
            let start = Cursor {
                side,
                origin: pin.lsb,
                route,
                hidden: collapsed.is_some(),
            };
            let mut walk = Walk {
                session: self,
                flow,
                origin_role,
                policy,
                todo: vec![start],
                hits: &mut hits,
            };
            walk.run();
        }
        hits
    }
}

/// Worklist for one direction and boundary face, scoped to one root slice.
struct Walk<'a> {
    session: &'a mut Session,
    flow: Flow,
    origin_role: Role,
    policy: &'a HierarchyPolicy,
    todo: Vec<Cursor>,
    hits: &'a mut Vec<ConnectionTrace>,
}

impl Walk<'_> {
    fn run(&mut self) {
        let mut seen = HashSet::new();
        while let Some(cursor) = self.todo.pop() {
            if seen.insert((cursor.route.pin.clone(), cursor.side, cursor.origin)) {
                self.visit(&cursor);
            } else if cursor.repeats_ancestor() {
                self.hits.push(finish(
                    &cursor.route,
                    cursor.origin,
                    cursor.route.pin.width(),
                    Endpoint::Pin(cursor.route.pin.clone()),
                    self.origin_role,
                    None,
                    StopReason::Cycle,
                ));
            }
        }
    }

    /// Clips each direct attachment to the requested interval and reports uncovered bits.
    fn visit(&mut self, c: &Cursor) {
        let (local, scope) = local_port(&c.route.pin.port, c.side);
        let adjacency = self
            .session
            .adjacency
            .entry(local.clone())
            .or_insert_with(|| Rc::new(index_attachments(&local, &self.session.stop_at)))
            .clone();
        let mut covered = c.route.pin.lsb;
        for arc in adjacency.overlapping(c.route.pin.lsb, c.route.pin.msb) {
            let lo = c.route.pin.lsb.max(arc.lsb);
            let hi = c.route.pin.msb.min(arc.msb);
            if lo > covered {
                self.hits.push(gap(c, covered, lo - 1, self.origin_role))
            }
            covered = covered.max(hi + 1);
            let width = hi - lo + 1;
            let origin = c.origin + (lo - c.route.pin.lsb);
            let item = arc.value.slice_with_offset_and_width(lo - arc.lsb, width);
            match item {
                ConnectedItem::PortSlice(peer) => {
                    let side = local_side(&peer.port);
                    let peer_role = role(&peer.port, side);
                    if can_arrive(peer_role, self.flow) {
                        self.follow(c, qualify(&peer, &scope), side, peer_role, origin)
                    }
                }
                item => {
                    let at = c
                        .route
                        .pin
                        .slice_with_offset_and_width(lo - c.route.pin.lsb, width);
                    let (end, role, stop_reason) = match item {
                        ConnectedItem::Tieoff(t) => (
                            Endpoint::Constant { at, value: t.value },
                            Some(Role::Driver),
                            StopReason::Constant,
                        ),
                        ConnectedItem::Unused(_) => {
                            (Endpoint::Unused(at), None, StopReason::Unused)
                        }
                        ConnectedItem::Wire(w) => (
                            Endpoint::NamedWire {
                                scope: occurrence_path(&c.route.pin.port)[..1 + scope.len()].into(),
                                name: w.name,
                                msb: w.msb,
                                lsb: w.lsb,
                            },
                            None,
                            StopReason::NamedWire,
                        ),
                        _ => unreachable!(),
                    };
                    self.hits.push(finish(
                        &c.route,
                        origin,
                        width,
                        end,
                        self.origin_role,
                        role,
                        stop_reason,
                    ));
                }
            }
        }
        if covered <= c.route.pin.msb {
            self.hits
                .push(gap(c, covered, c.route.pin.msb, self.origin_role));
        }
    }

    /// Reports a terminal, or crosses its module boundary with the occurrence path intact.
    fn follow(&mut self, c: &Cursor, next: PortSlice, side: Side, peer_role: Role, origin: usize) {
        if c.route.arrived == Some(c.side)
            && c.route.prev.as_ref().is_some_and(|prev| {
                prev.pin.port == next.port && next.lsb == prev.pin.lsb + (origin - prev.origin)
            })
        {
            return;
        }
        let enclosing_hidden = if side == Side::Body {
            collapsed_scope_depth(&next, Side::Parent, self.policy).is_some()
        } else {
            c.hidden
        };
        let hop = Rc::new(Hop {
            pin: next.clone(),
            origin,
            visible: !enclosing_hidden,
            prev: Some(c.route.clone()),
            arrived: Some(side),
        });
        let StopAt::Selected(selected) = &self.session.stop_at else {
            self.hits.push(finish(
                &hop,
                origin,
                next.width(),
                Endpoint::Pin(next),
                self.origin_role,
                Some(peer_role),
                StopReason::Boundary,
            ));
            return;
        };
        let inst = next.port.get_mod_inst();
        let action = inst
            .as_ref()
            .map(|i| self.policy.action(i, &i.get_mod_def()));
        if action == Some(HierarchyAction::Collapse) {
            self.session.collapsed.insert(occurrence_path(&next.port));
        }
        let reason = if !selected.is_empty() && selected.contains(&port_owner(&next.port)) {
            Some(StopReason::Selected)
        } else if action == Some(HierarchyAction::Stop) {
            Some(StopReason::Policy)
        } else if let Some(i) = inst {
            let def = i.get_mod_def();
            if side == Side::Parent
                && i.hierarchy
                    .iter()
                    .any(|f| std::ptr::eq(f.mod_def_core.as_ptr(), Arc::as_ptr(&def.core)))
            {
                Some(StopReason::Cycle)
            } else if side == Side::Parent && opaque(&def) {
                Some(StopReason::Opaque)
            } else {
                None
            }
        } else {
            Some(StopReason::Boundary)
        };
        if let Some(stop_reason) = reason {
            self.hits.push(finish(
                &hop,
                origin,
                next.width(),
                Endpoint::Pin(next.clone()),
                self.origin_role,
                Some(peer_role),
                stop_reason,
            ));
        } else {
            let hidden = if side == Side::Parent {
                c.hidden || action == Some(HierarchyAction::Collapse)
            } else {
                enclosing_hidden
            };
            self.todo.push(Cursor {
                side: side.opposite(),
                origin,
                route: hop.clone(),
                hidden,
            });
        }
        // A bidirectional terminal also connects the other arcs on its current face.
        if peer_role == Role::InOut && next != c.route.pin {
            self.todo.push(Cursor {
                side,
                origin,
                route: hop,
                hidden: c.hidden,
            });
        }
    }
}

impl Cursor {
    /// Distinguishes feedback on this route from convergence of two equivalent routes.
    fn repeats_ancestor(&self) -> bool {
        std::iter::successors(self.route.prev.as_deref(), |hop| hop.prev.as_deref()).any(|hop| {
            hop.pin.port == self.route.pin.port
                && hop.origin == self.origin
                && hop.pin.lsb == self.route.pin.lsb
        })
    }
}

impl Side {
    fn opposite(self) -> Self {
        match self {
            Self::Body => Self::Parent,
            Self::Parent => Self::Body,
        }
    }
}

fn local_side(p: &Port) -> Side {
    if matches!(p, Port::ModInst { .. }) {
        Side::Parent
    } else {
        Side::Body
    }
}
fn role(port: &Port, side: Side) -> Role {
    match (port.io(), side) {
        (IO::Input(_), Side::Body) | (IO::Output(_), Side::Parent) => Role::Driver,
        (IO::InOut(_), _) => Role::InOut,
        _ => Role::Receiver,
    }
}
fn can_depart(r: Role, f: Flow) -> bool {
    matches!(
        (r, f),
        (Role::Driver, Flow::Down) | (Role::Receiver, Flow::Up) | (Role::InOut, _)
    )
}
fn can_arrive(r: Role, f: Flow) -> bool {
    can_depart(
        r,
        match f {
            Flow::Up => Flow::Down,
            Flow::Down => Flow::Up,
        },
    )
}
/// Imports and ports-only definitions expose an interface. Native instances or
/// recorded body wiring provide structural evidence independently of emission settings.
pub(super) fn opaque(def: &ModDef) -> bool {
    let core = def.core.read();
    core.verilog_import.is_some()
        || (core.instances.is_empty() && core.mod_def_connections.is_empty())
}
/// Depth of the outermost collapsed instance containing the wiring on this face.
fn collapsed_scope_depth(pin: &PortSlice, side: Side, policy: &HierarchyPolicy) -> Option<usize> {
    if policy.default == HierarchyAction::Expand
        && policy.definitions.is_empty()
        && policy.instances.is_empty()
    {
        return None;
    }
    let mut inst = pin.port.get_mod_inst()?;
    if side == Side::Parent {
        inst.hierarchy.pop();
    }
    let mut outermost = None;
    while !inst.hierarchy.is_empty() {
        if policy.action(&inst, &inst.get_mod_def()) == HierarchyAction::Collapse {
            outermost = Some(inst.hierarchy.len());
        }
        inst.hierarchy.pop();
    }
    outermost
}
/// Looks up the definition-local pin and the occurrence of the containing body.
fn local_port(p: &Port, side: Side) -> (Port, Vec<HierPathElem>) {
    let scope = p.get_mod_inst().map(|i| i.hierarchy).unwrap_or_default();
    match side {
        Side::Body => (p.as_mod_def_port(), scope),
        Side::Parent => (p.relative_to_parent(), scope[..scope.len() - 1].to_vec()),
    }
}
fn qualify(s: &PortSlice, scope: &[HierPathElem]) -> PortSlice {
    let mut frames = scope.to_vec();
    if let Port::ModInst { hierarchy, .. } = &s.port {
        frames.extend_from_slice(hierarchy)
    }
    if frames.is_empty() {
        s.clone()
    } else {
        s.as_mod_inst_port_slice(frames)
    }
}
fn gap(c: &Cursor, lo: usize, hi: usize, origin_role: Role) -> ConnectionTrace {
    let width = hi - lo + 1;
    let pin = c
        .route
        .pin
        .slice_with_offset_and_width(lo - c.route.pin.lsb, width);
    // A known parent wire still reaches the child's input even if its body has no uses.
    let (end, peer_role) = if c.route.prev.is_some() {
        let arrival = c.side.opposite();
        (Endpoint::Pin(pin), Some(role(&c.route.pin.port, arrival)))
    } else {
        (Endpoint::Unconnected(pin), None)
    };
    finish(
        &c.route,
        c.origin + (lo - c.route.pin.lsb),
        width,
        end,
        origin_role,
        peer_role,
        StopReason::Unconnected,
    )
}
fn finish(
    hop: &Hop,
    origin: usize,
    width: usize,
    peer: Endpoint,
    pin_role: Role,
    peer_role: Option<Role>,
    stop_reason: StopReason,
) -> ConnectionTrace {
    let mut route = Vec::new();
    let mut visible = Vec::new();
    let mut scopes = Vec::new();
    let mut node = Some(hop);
    while let Some(n) = node {
        route.push(n.pin.slice_with_offset_and_width(origin - n.origin, width));
        visible.push(n.visible);
        if let Some(side) = n.arrived {
            let depth = match &n.pin.port {
                Port::ModDef { .. } => 0,
                Port::ModInst { hierarchy, .. } => hierarchy.len(),
            };
            scopes.push(depth + usize::from(side == Side::Body));
        }
        node = n.prev.as_deref();
    }
    route.reverse();
    visible.reverse();
    scopes.reverse();
    visible[0] = true;
    *visible.last_mut().unwrap() = true;
    let queried = ConnectionEndpoint {
        endpoint: Endpoint::Pin(route[0].clone()),
        role: Some(pin_role),
    };
    let peer = ConnectionEndpoint {
        endpoint: peer,
        role: peer_role,
    };
    let (endpoints, peer_is_second) = match (pin_role, peer_role) {
        (Role::Receiver, Some(Role::Driver | Role::InOut)) | (Role::InOut, Some(Role::Driver)) => {
            ([peer, queried], false)
        }
        _ => ([queried, peer], true),
    };
    ConnectionTrace {
        connection: SignalConnection { endpoints },
        peer_is_second,
        stop_reason,
        route,
        visible,
        scopes,
    }
}

/// Returns the bitwise union of hierarchical connections touching these slices.
/// Overlapping seeds or queries from opposite ends emit each mapped bit pair once.
pub fn connections_touching<'a, T: ConvertibleToPortSlice + ?Sized + 'a>(
    pins: impl IntoIterator<Item = &'a T>,
) -> Vec<ConnectionTrace> {
    let mut session = Session::default();
    let mut seeds: HashMap<Port, Intervals> = HashMap::new();
    let mut union = ConnectionUnion::default();
    for pin in pins
        .into_iter()
        .filter_map(ConvertibleToPortSlice::to_nonempty_port_slice)
    {
        for (lo, hi) in seeds
            .entry(pin.port.clone())
            .or_default()
            .insert(pin.lsb, pin.msb)
        {
            let seed = pin.port.slice(hi, lo);
            for link in query(
                &mut session,
                &seed,
                &HierarchyPolicy::default(),
                &[Flow::Up, Flow::Down],
            ) {
                union.insert(link);
            }
        }
    }
    union.links
}

/// Accumulates one trace per mapped bit pair and one per distinct non-pin attachment.
#[derive(Default)]
pub(super) struct ConnectionUnion {
    ports: indexmap::IndexSet<Port>,
    mappings: HashMap<(usize, usize, i128), Intervals>,
    attachments: HashSet<SignalConnection>,
    pub(super) links: Vec<ConnectionTrace>,
}

impl ConnectionUnion {
    /// Retains the first route for each bit pair, clipping later routes to new coverage.
    pub(super) fn insert(&mut self, link: ConnectionTrace) {
        let Some((queried, peer)) = link.pin_mapping() else {
            if self.attachments.insert(link.connection.clone()) {
                self.links.push(link)
            }
            return;
        };
        // Canonicalize port order so overlapping queries from either end share coverage.
        let a_id = self.ports.insert_full(queried.port.clone()).0;
        let b_id = self.ports.insert_full(peer.port.clone()).0;
        let (a, b) = if (a_id, queried.lsb) <= (b_id, peer.lsb) {
            (queried, peer)
        } else {
            (peer, queried)
        };
        let key = (
            a_id.min(b_id),
            a_id.max(b_id),
            b.lsb as i128 - a.lsb as i128,
        );
        let uncovered = self.mappings.entry(key).or_default().insert(a.lsb, a.msb);
        if uncovered.as_slice() == [(a.lsb, a.msb)] {
            self.links.push(link);
            return;
        }
        for (lo, hi) in uncovered {
            self.links
                .push(link.pin_mapping_slice(lo - a.lsb, hi - lo + 1));
        }
    }
}

/// Disjoint inclusive intervals, merged on insertion. Lookup is logarithmic plus overlap.
#[derive(Default)]
pub(super) struct Intervals(std::collections::BTreeMap<usize, usize>);
impl Intervals {
    /// The covered inclusive ranges in increasing bit order.
    pub(super) fn ranges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.0.iter().map(|(&lo, &hi)| (lo, hi))
    }

    /// Returns the gaps in coverage within an inclusive range.
    fn uncovered(&self, lo: usize, hi: usize) -> Vec<(usize, usize)> {
        let mut fresh = Vec::new();
        let mut cursor = lo;
        let previous = self.0.range(..lo).next_back();
        for (&start, &end) in previous.into_iter().chain(self.0.range(lo..=hi)) {
            if start > cursor {
                fresh.push((cursor, start - 1));
            }
            if end >= hi {
                return fresh;
            }
            cursor = cursor.max(end + 1);
        }
        fresh.push((cursor, hi));
        fresh
    }

    /// Inserts a range and returns only its previously uncovered pieces.
    pub(super) fn insert(&mut self, lo: usize, hi: usize) -> Vec<(usize, usize)> {
        let fresh = self.uncovered(lo, hi);
        if fresh.is_empty() {
            return fresh;
        }
        let first = self
            .0
            .range(..=lo)
            .next_back()
            .filter(|&(_, end)| *end >= lo.saturating_sub(1))
            .map_or(lo, |(&a, _)| a);
        let mut end = hi;
        while let Some((&a, &b)) = self.0.range(first..=hi.saturating_add(1)).next() {
            end = end.max(b);
            self.0.remove(&a);
        }
        self.0.insert(first, end);
        fresh
    }
}

fn query(
    s: &mut Session,
    seed: &impl ConvertibleToPortSlice,
    p: &HierarchyPolicy,
    flows: &[Flow],
) -> Vec<ConnectionTrace> {
    let Some(pin) = seed.to_nonempty_port_slice() else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    flows
        .iter()
        .flat_map(|&flow| s.trace(&pin, flow, p, &[Side::Parent, Side::Body]))
        .filter(|c| seen.insert(c.connection.clone()))
        .collect()
}
macro_rules! pin_queries {
    ($ty:ty) => {
        impl $ty {
            /// All structural connections from this pin, including fanout and incomplete ranges.
            pub fn trace_connections(&self) -> Vec<ConnectionTrace> {
                self.trace_connections_with(&HierarchyPolicy::default())
            }
            /// Follows wires using query-local hierarchy rules.
            pub fn trace_connections_with(&self, p: &HierarchyPolicy) -> Vec<ConnectionTrace> {
                query(&mut Session::default(), self, p, &[Flow::Up, Flow::Down])
            }
            /// Returns upstream drivers, including possible bidirectional drivers.
            pub fn drivers(&self) -> Vec<ConnectionTrace> {
                self.drivers_with(&HierarchyPolicy::default())
            }
            /// Returns upstream drivers using the supplied hierarchy rules.
            pub fn drivers_with(&self, p: &HierarchyPolicy) -> Vec<ConnectionTrace> {
                query(&mut Session::default(), self, p, &[Flow::Up])
                    .into_iter()
                    .filter(|c| matches!(c.peer_endpoint().role, Some(Role::Driver | Role::InOut)))
                    .collect()
            }
            /// Returns downstream receivers, including bidirectional attachments.
            pub fn receivers(&self) -> Vec<ConnectionTrace> {
                self.receivers_with(&HierarchyPolicy::default())
            }
            /// Returns downstream receivers using the supplied hierarchy rules.
            pub fn receivers_with(&self, p: &HierarchyPolicy) -> Vec<ConnectionTrace> {
                query(&mut Session::default(), self, p, &[Flow::Down])
                    .into_iter()
                    .filter(|c| {
                        matches!(c.peer_endpoint().role, Some(Role::Receiver | Role::InOut))
                    })
                    .collect()
            }
        }
    };
}
pin_queries!(Port);
pin_queries!(PortSlice);

impl crate::Intf {
    /// Traces each declared interface member, retaining its function name.
    pub fn trace_connections(&self) -> indexmap::IndexMap<String, Vec<ConnectionTrace>> {
        self.trace_connections_with(&HierarchyPolicy::default())
    }
    /// Traces the member slices with shared lookups and the supplied stopping rules.
    pub fn trace_connections_with(
        &self,
        policy: &HierarchyPolicy,
    ) -> indexmap::IndexMap<String, Vec<ConnectionTrace>> {
        let mut session = Session::default();
        self.iter()
            .map(|(name, pin)| {
                (
                    name,
                    query(&mut session, &pin, policy, &[Flow::Up, Flow::Down]),
                )
            })
            .collect()
    }
}
