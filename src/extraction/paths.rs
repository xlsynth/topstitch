// SPDX-License-Identifier: Apache-2.0

use super::trace::{Flow, Intervals, Session, Side, StopAt, opaque};
use super::*;
use crate::{ConvertibleToPortSlice, IO, Port, PortSlice};
use indexmap::IndexSet;
use std::collections::{HashSet, VecDeque};

/// The relationship between the entry and exit pins of a path through an instance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ThroughKind {
    /// A possible crossing. Whether the input affects the output, which bits correspond,
    /// and the latency are all unknown.
    Unknown,
    /// Each output bit carries the corresponding input bit after `cycles` cycles of `clock`.
    Clocked {
        /// Name of the clock port on this instance.
        clock: String,
        /// Number of clock cycles from input to output.
        cycles: usize,
    },
    /// Each output bit follows the corresponding input bit with zero cycles of latency.
    Feedthrough,
}

impl ThroughKind {
    /// Whether a range on the output maps to the same bit indices on the input.
    fn preserves_bits(&self) -> bool {
        matches!(self, Self::Feedthrough | Self::Clocked { .. })
    }
}

/// Per-query settings for downstream paths. Unknown crossings are enabled by default.
#[derive(Clone)]
pub struct PathOptions {
    pub hierarchy: HierarchyPolicy,
    /// Includes unproven input-to-output candidates through an opaque occurrence.
    pub cross_opaque: bool,
    /// Follows modeled register stages, recording the clock and latency on the route.
    pub cross_registers: bool,
}
impl Default for PathOptions {
    fn default() -> Self {
        Self {
            hierarchy: HierarchyPolicy::default(),
            cross_opaque: true,
            cross_registers: true,
        }
    }
}
impl PathOptions {
    /// Chooses which instances to expand, collapse or stop at.
    pub fn hierarchy(mut self, p: HierarchyPolicy) -> Self {
        self.hierarchy = p;
        self
    }

    /// Returns a modeled output or, for a black box, the set of potential output ports.
    fn transfer(&self, def: &ModDef, input: &str) -> Option<(ThroughKind, Option<&'static str>)> {
        if let Some(cycles) = def.bedrock_delay_cycles() {
            if input != "in" || (cycles > 0 && !self.cross_registers) {
                return None;
            }
            let kind = if cycles == 0 {
                ThroughKind::Feedthrough
            } else {
                ThroughKind::Clocked {
                    clock: "clk".into(),
                    cycles,
                }
            };
            return Some((kind, Some("out")));
        }
        self.cross_opaque.then_some((ThroughKind::Unknown, None))
    }
}

type HubKey = (Arc<[String]>, ThroughKind, Option<(usize, usize)>);

/// Pins by which paths enter and leave a single occurrence.
#[derive(Clone, Debug)]
pub struct InstanceStep {
    pub path: Arc<[String]>,
    pub entries: IndexSet<PortSlice>,
    pub exits: IndexSet<PortSlice>,
}

#[derive(Clone, Debug)]
enum Node {
    Start(Option<Box<PortSlice>>),
    Wire(Box<ConnectionTrace>),
    Through(Arc<[String]>, ThroughKind),
}

impl Node {
    /// Pin where the next instance crossing begins, either at the seed or after a wire.
    fn crossing_entry(&self) -> Option<&PortSlice> {
        match self {
            Self::Start(pin) => pin.as_deref(),
            Self::Wire(wire) => wire.peer_endpoint().endpoint.as_pin(),
            Self::Through(..) => None,
        }
    }
}

/// Compact directed path graph. An opaque instance joins N arrivals and M departures
/// with one vertex and N+M edges. Its candidate routes are enumerated only on request.
#[derive(Clone, Debug)]
pub struct PathGraph {
    nodes: Vec<Node>,
    edges: Vec<Vec<usize>>,
    ends: HashSet<usize>,
    targeted: bool,

    // Summaries can split a wire into disjoint ranges. Routes still use its original trace.
    narrowed_wires: HashMap<usize, Vec<ConnectionTrace>>,
    instances: Vec<InstanceStep>,
    collapsed: HashSet<Arc<[String]>>,
}

/// One step on a downstream route. Expanded routes show structural wires and instance
/// crossings. An untargeted route can end with a [`Feedback`](Self::Feedback) marker.
/// [`PathGraph::routes`] can also group steps inside instances marked [`HierarchyAction::Collapse`].
///
/// For example, suppose wrapper `W` contains a two-cycle delay `d`. Both views
/// describe the same route from `A.q` to `B.d`:
///
/// ```text
/// Expanded                              With W collapsed
/// Wire     A.q → W.in → W.d.in          WireHop    A.q → W.in
/// Through  W.d.in → W.d.out (2 cycles)  Collapsed  W.in → W.out
/// Wire     W.d.out → W.out → B.d        WireHop    W.out → B.d
/// ```
///
/// The collapsed step retains the two internal wire hops and the clocked crossing.
#[derive(Clone, Debug)]
pub enum PathStep {
    /// One structural connection. Its [`ConnectionTrace::route`] can visit several
    /// wrapper pins along the same wiring.
    Wire(ConnectionTrace),
    /// An untargeted trace stopped at this pin on detecting a cycle in structural wiring,
    /// hierarchy, or instance crossings. An opaque instance can be revisited on a different input.
    Feedback { at: PortSlice },
    /// The terminal of a split wire whose peer is a non-pin [`Endpoint`], such as a
    /// named wire. `at` is the last pin slice on the route to that terminal.
    Attachment { at: PortSlice, endpoint: Endpoint },
    /// One directly wired pair of adjacent pin slices. Splitting a [`Wire`](Self::Wire)
    /// into hops lets a collapsed view group only the portion inside the instance.
    WireHop { from: PortSlice, to: PortSlice },
    /// Consecutive steps inside `instance`, presented together from `enter` to `leave`.
    /// Inspect `steps` to recover their wiring and any internal crossings in order.
    Collapsed {
        instance: Arc<[String]>,
        enter: PortSlice,
        leave: PortSlice,
        steps: Vec<PathStep>,
    },
    /// A crossing within one instance from `enter` to `leave`. Its [`ThroughKind`]
    /// describes a modeled transfer or marks the crossing as a possible dependency.
    Through {
        enter: PortSlice,
        leave: PortSlice,
        kind: ThroughKind,
    },
}

/// Traces downstream wires and permitted internal crossings from one module to another.
/// Includes feedback that can return to the destination path. Every reported route
/// reaches the destination, directly or after a feedback excursion.
pub fn paths_between(
    from: &ModInst,
    to: &ModInst,
    options: &PathOptions,
) -> Result<PathGraph, String> {
    let (root, from_path) = instance_key(from);
    let (other_root, to_path) = instance_key(to);
    if root != other_root {
        return Err("path endpoints have different design roots".into());
    }
    let inside = to_path.starts_with(&from_path) && to_path.len() > from_path.len();
    let seeds = from
        .get_ports(None)
        .into_iter()
        .filter(|p| p.io().width() > 0)
        .map(|p| (p.to_port_slice(), inside));
    Ok(PathGraph::build(seeds, Some(from), Some(to), options))
}

impl PortSlice {
    /// Discovers downstream paths without requiring a destination. At an opaque
    /// instance input, an enabled internal crossing can be the first path step.
    pub fn trace_paths(&self, options: &PathOptions) -> PathGraph {
        PathGraph::build([(self.clone(), true)], None, None, options)
    }
}
impl Port {
    /// Discovers downstream paths from the whole port.
    pub fn trace_paths(&self, options: &PathOptions) -> PathGraph {
        PathGraph::build(
            self.to_nonempty_port_slice().map(|pin| (pin, true)),
            None,
            None,
            options,
        )
    }
}

impl PathGraph {
    /// Structural wires participating in the query. For [`paths_between`], each wire
    /// covers only lanes that can reach the destination. Disjoint ranges yield separate traces.
    /// This includes feedback from which the destination is reachable. An unknown crossing
    /// keeps the full candidate input slice.
    pub fn connections(&self) -> impl Iterator<Item = &ConnectionTrace> {
        (0..self.nodes.len()).flat_map(|i| self.wire_slices(i))
    }

    /// The wire's participating slices, or an empty slice for a non-wire node.
    fn wire_slices(&self, i: usize) -> &[ConnectionTrace] {
        match &self.nodes[i] {
            Node::Wire(wire) => self
                .narrowed_wires
                .get(&i)
                .map_or(std::slice::from_ref(wire.as_ref()), Vec::as_slice),
            _ => &[],
        }
    }

    /// Each traversed occurrence once, with the entry and exit slices participating in
    /// the same wiring reported by [`Self::connections`].
    pub fn through(&self) -> &[InstanceStep] {
        &self.instances
    }
    /// Enumerates the same routes as [`Self::expanded_routes`], grouping steps
    /// according to the hierarchy's collapse policy.
    pub fn routes(&self) -> impl Iterator<Item = Vec<PathStep>> {
        self.expanded_routes().map(|steps| self.collapse(steps))
    }

    /// Lazily enumerates routes with individual wires and instance crossings.
    /// From a pin, a return to wiring already on this route ends in [`PathStep::Feedback`].
    /// Between modules, the first such return may continue along any non-repeating
    /// path to the destination. This exposes feedback wiring without unrolling a
    /// loop for arbitrarily many cycles.
    pub fn expanded_routes(&self) -> impl Iterator<Item = Vec<PathStep>> {
        let mut stack = vec![(0usize, 0usize)];
        let mut prefix = HashSet::from([0usize]);
        let mut exit = HashSet::new();
        std::iter::from_fn(move || {
            loop {
                let (v, offset) = stack.last_mut()?;
                if *offset == 0 && self.ends.contains(v) {
                    *offset = 1;
                    let mut steps = self.route_steps(&stack);
                    if let Some(PathStep::Wire(wire)) = steps.last()
                        && wire.stop_reason == StopReason::Cycle
                    {
                        let at = wire.route().last().unwrap().clone();
                        steps.push(PathStep::Feedback { at });
                    }
                    return Some(steps);
                }
                if let Some(&child) = self.edges[*v].get(*offset) {
                    *offset += 1;
                    if !exit.is_empty() {
                        if exit.insert(child) {
                            stack.push((child, 0));
                        }
                    } else if prefix.insert(child) {
                        stack.push((child, 0));
                    } else if self.targeted {
                        // Start a fresh simple path at the return. Its exit may reuse
                        // the beginning of the loop before it branches toward the target.
                        exit.insert(child);
                        stack.push((child, 0));
                    } else {
                        return Some(self.feedback_steps(&mut stack, child));
                    }
                } else {
                    if !exit.remove(v) {
                        prefix.remove(v);
                    }
                    stack.pop();
                }
            }
        })
    }

    /// Materializes the path up to the repeated operation, leaving the marker at its entry pin.
    fn feedback_steps(&self, stack: &mut Vec<(usize, usize)>, repeated: usize) -> Vec<PathStep> {
        let (mut steps, at) = match &self.nodes[repeated] {
            Node::Through(..) => {
                let parent = stack.last().unwrap().0;
                (
                    self.route_steps(stack),
                    self.nodes[parent].crossing_entry().unwrap(),
                )
            }
            Node::Wire(wire) => {
                // The preceding crossing needs its output slice for bit-accurate
                // reconstruction. Use the repeated wire as lookahead, then omit it.
                stack.push((repeated, 0));
                let mut steps = self.route_steps(stack);
                stack.pop();
                steps.pop();
                (steps, &wire.route[0])
            }
            Node::Start(_) => unreachable!("the start has no incoming edges"),
        };
        steps.push(PathStep::Feedback { at: at.clone() });
        steps
    }

    /// Propagates a selected output subrange back through bit-preserving transfers.
    /// Bit correspondence ends at an unknown crossing.
    fn route_steps(&self, stack: &[(usize, usize)]) -> Vec<PathStep> {
        let mut reverse = Vec::new();
        let mut keep = None;
        for i in (1..stack.len()).rev() {
            match &self.nodes[stack[i].0] {
                Node::Wire(w) => {
                    let wire = match keep.take() {
                        Some((offset, width))
                            if w.pin_mapping()
                                .is_some_and(|(_, peer)| offset != 0 || width != peer.width()) =>
                        {
                            w.pin_mapping_slice(offset, width)
                        }
                        _ => w.as_ref().clone(),
                    };
                    reverse.push(PathStep::Wire(wire));
                }
                Node::Through(_, kind) => {
                    let full = self.nodes[stack[i - 1].0]
                        .crossing_entry()
                        .expect("a transfer enters through a pin");
                    let Some(PathStep::Wire(next)) = reverse.last() else {
                        unreachable!()
                    };
                    let leave = next
                        .queried_endpoint()
                        .endpoint
                        .as_pin()
                        .expect("a transfer leaves through a pin")
                        .clone();
                    let enter = if kind.preserves_bits() {
                        let offset = leave.lsb - full.lsb;
                        keep = Some((offset, leave.width()));
                        full.slice_with_offset_and_width(offset, leave.width())
                    } else {
                        full.clone()
                    };
                    reverse.push(PathStep::Through {
                        enter,
                        leave,
                        kind: kind.clone(),
                    });
                }
                Node::Start(_) => unreachable!(),
            }
        }
        reverse.reverse();
        reverse
    }

    /// Finds the outermost collapsed instance containing this local attachment.
    fn collapsed_owner(&self, path: &[String]) -> Option<&Arc<[String]>> {
        (2..=path.len()).find_map(|n| self.collapsed.get(&path[..n]))
    }
    fn collapse(&self, steps: Vec<PathStep>) -> Vec<PathStep> {
        if self.collapsed.is_empty() {
            return steps;
        }
        let mut out = Vec::new();
        for step in steps {
            match step {
                PathStep::Through {
                    ref enter,
                    ref leave,
                    ..
                } => {
                    let owner = self.collapsed_owner(&occurrence_path(&enter.port));
                    if let Some(owner) = owner {
                        let (enter, leave) = (enter.clone(), leave.clone());
                        Self::append_collapsed(&mut out, owner, enter, leave, step);
                    } else {
                        out.push(step)
                    }
                }
                PathStep::Wire(w)
                    if w.scopes.iter().enumerate().any(|(i, &n)| {
                        self.collapsed_owner(&occurrence_path(&w.route[i + 1].port)[..n])
                            .is_some()
                    }) =>
                {
                    for (i, &n) in w.scopes.iter().enumerate() {
                        let (a, b) = (&w.route[i], &w.route[i + 1]);
                        let hop = PathStep::WireHop {
                            from: a.clone(),
                            to: b.clone(),
                        };
                        if let Some(owner) = self.collapsed_owner(&occurrence_path(&b.port)[..n]) {
                            Self::append_collapsed(&mut out, owner, a.clone(), b.clone(), hop);
                        } else {
                            out.push(hop)
                        }
                    }
                    if w.peer_endpoint().endpoint.as_pin().is_none() {
                        out.push(PathStep::Attachment {
                            at: w.route.last().unwrap().clone(),
                            endpoint: w.peer_endpoint().endpoint.clone(),
                        })
                    }
                }
                _ => out.push(step),
            }
        }
        out
    }
    /// Joins adjacent steps inside one collapsed occurrence without discarding their semantics.
    fn append_collapsed(
        out: &mut Vec<PathStep>,
        owner: &Arc<[String]>,
        enter: PortSlice,
        leave: PortSlice,
        step: PathStep,
    ) {
        if let Some(PathStep::Collapsed {
            instance,
            leave: last,
            steps,
            ..
        }) = out.last_mut()
            && instance == owner
        {
            steps.push(step);
            *last = leave;
        } else {
            out.push(PathStep::Collapsed {
                instance: owner.clone(),
                enter,
                leave,
                steps: vec![step],
            });
        }
    }

    fn build(
        seeds: impl IntoIterator<Item = (PortSlice, bool)>,
        source: Option<&ModInst>,
        target: Option<&ModInst>,
        options: &PathOptions,
    ) -> Self {
        let mut g = Self {
            nodes: vec![Node::Start(None)],
            edges: vec![vec![]],
            ends: HashSet::new(),
            targeted: target.is_some(),

            narrowed_wires: HashMap::new(),
            instances: Vec::new(),
            collapsed: HashSet::new(),
        };
        let target_key = target.map(instance_key);
        let selected = target_key
            .iter()
            .cloned()
            .chain(source.map(instance_key))
            .collect();
        let mut session = Session::new(StopAt::Selected(selected));
        let mut cache: HashMap<(PortSlice, bool), Vec<usize>> = HashMap::new();
        let mut hubs = HashMap::new();
        let mut todo = VecDeque::new();
        for (pin, inside) in seeds {
            if source.is_none()
                && matches!(pin.port.io(), IO::Input(_) | IO::InOut(_))
                && let Some(inst) = pin.port.get_mod_inst()
            {
                let def = inst.get_mod_def();
                let action = options.hierarchy.action(&inst, &def);
                if opaque(&def) && action != HierarchyAction::Stop {
                    g.nodes[0] = Node::Start(Some(Box::new(pin.clone())));
                    g.add_crossings(0, &pin, options, &mut hubs, &mut todo);
                }
            }
            // A downstream trace can only leave an output on its parent face. Use the
            // same cache key when feedback returns to a wire followed from an output seed.
            let inside = inside && !matches!(pin.port.io(), IO::Output(_));
            todo.push_back((0, pin, inside));
        }
        while let Some((parent, pin, inside)) = todo.pop_front() {
            if let Some(ids) = cache.get(&(pin.clone(), inside)) {
                g.edges[parent].extend_from_slice(ids);
                continue;
            }
            let sides = if inside {
                &[Side::Parent, Side::Body][..]
            } else {
                &[Side::Parent][..]
            };
            let mut wires = Vec::new();
            let mut dedup = HashSet::new();
            for link in session.trace(&pin, Flow::Down, &options.hierarchy, sides) {
                if !dedup.insert(link.connection.clone()) {
                    continue;
                }
                let reached_target = link.stop_reason == StopReason::Selected
                    && link
                        .peer_endpoint()
                        .endpoint
                        .as_pin()
                        .is_some_and(|p| Some(port_owner(&p.port)) == target_key);
                let id = g.push(Node::Wire(Box::new(link)));
                wires.push(id);
                g.edges[parent].push(id);
                if let Node::Wire(link) = &g.nodes[id]
                    && link.stop_reason == StopReason::Opaque
                    && let Some(peer) = link.peer_endpoint().endpoint.as_pin()
                {
                    let peer = peer.clone();
                    g.add_crossings(id, &peer, options, &mut hubs, &mut todo);
                }
                if reached_target || (g.edges[id].is_empty() && target.is_none()) {
                    g.ends.insert(id);
                }
            }
            cache.insert((pin, inside), wires);
        }
        g.collapsed = session.collapsed;
        g.finalize();
        g
    }
    /// Represents all opaque input/output combinations with a shared vertex;
    /// known bit-preserving transfers also distinguish the incoming slice.
    fn add_crossings(
        &mut self,
        wire: usize,
        peer: &PortSlice,
        options: &PathOptions,
        hubs: &mut HashMap<HubKey, usize>,
        todo: &mut VecDeque<(usize, PortSlice, bool)>,
    ) {
        let Some(inst) = peer.port.get_mod_inst() else {
            return;
        };
        let Some(main) = options.transfer(&inst.get_mod_def(), peer.port.name()) else {
            return;
        };
        let path = occurrence_path(&peer.port);
        // The modeled input-to-output latency does not specify intermediate tap layout.
        // A separately connected tap can still participate in an explicitly unknown path.
        let has_tap = options.cross_opaque
            && matches!(main.0, ThroughKind::Clocked { .. })
            && inst.has_port("out_stages")
            && inst
                .get_port("out_stages")
                .get_port_connections()
                .is_some_and(|arcs| {
                    arcs.read().into_iter().any(|a| {
                        matches!(
                            a.other,
                            crate::ConnectedItem::PortSlice(_) | crate::ConnectedItem::Wire(_)
                        )
                    })
                });
        let tap = has_tap.then_some((ThroughKind::Unknown, Some("out_stages")));
        for (kind, output) in std::iter::once(main).chain(tap) {
            let mapped = kind.preserves_bits();
            let key = (
                path.clone(),
                kind.clone(),
                mapped.then_some((peer.msb, peer.lsb)),
            );
            if let Some(&id) = hubs.get(&key) {
                self.edges[wire].push(id);
                continue;
            }
            let outputs = match output {
                Some(name) => vec![inst.get_port(name)],
                None => inst
                    .get_ports(None)
                    .into_iter()
                    .filter(|p| matches!(p.io(),IO::Output(w)|IO::InOut(w) if w>0))
                    .collect(),
            };
            if outputs.is_empty() {
                continue;
            }
            let hub = self.push(Node::Through(path.clone(), kind));
            hubs.insert(key, hub);
            for port in outputs {
                let slice = if mapped {
                    port.slice(peer.msb, peer.lsb)
                } else {
                    port.to_port_slice()
                };
                todo.push_back((hub, slice, false))
            }
            self.edges[wire].push(hub);
        }
    }

    fn push(&mut self, node: Node) -> usize {
        let i = self.nodes.len();
        self.nodes.push(node);
        self.edges.push(Vec::new());
        i
    }
    fn reverse_edges(&self) -> Vec<Vec<usize>> {
        let mut reverse = vec![vec![]; self.nodes.len()];
        for (i, children) in self.edges.iter().enumerate() {
            for &j in children {
                reverse[j].push(i);
            }
        }
        reverse
    }

    /// Drops off-target cones, including their wire snapshots, before returning the graph.
    fn prune(&mut self) {
        let reverse = self.reverse_edges();
        let mut live = vec![false; self.nodes.len()];
        let mut todo = self.ends.iter().copied().collect::<Vec<_>>();
        while let Some(i) = todo.pop() {
            if !std::mem::replace(&mut live[i], true) {
                todo.extend_from_slice(&reverse[i]);
            }
        }
        live[0] = true;
        let mut next = 0;
        let remap: Vec<_> = live
            .iter()
            .map(|keep| {
                if *keep {
                    let id = next;
                    next += 1;
                    Some(id)
                } else {
                    None
                }
            })
            .collect();
        self.nodes = std::mem::take(&mut self.nodes)
            .into_iter()
            .zip(&live)
            .filter_map(|(n, &keep)| keep.then_some(n))
            .collect();
        self.edges = std::mem::take(&mut self.edges)
            .into_iter()
            .zip(&live)
            .filter_map(|(edges, &keep)| {
                keep.then(|| edges.into_iter().filter_map(|j| remap[j]).collect())
            })
            .collect();
        self.ends = self.ends.iter().filter_map(|&j| remap[j]).collect();
    }

    /// Propagates destination lanes backwards through the pruned graph. A wire
    /// tracks offsets from its LSB; a bit-preserving crossing tracks port bit indices.
    fn narrow_to_target(&mut self, reverse: &[Vec<usize>]) {
        if !self
            .nodes
            .iter()
            .any(|node| matches!(node, Node::Through(_, kind) if kind.preserves_bits()))
        {
            return;
        }
        let mut lanes: Vec<Intervals> = (0..self.nodes.len())
            .map(|_| Intervals::default())
            .collect();
        let mut todo = VecDeque::new();
        for (id, node) in self.nodes.iter().enumerate() {
            // An unmodeled dependency can use every input bit whenever any output survives pruning.
            if let Node::Wire(wire) = node
                && (self.ends.contains(&id)
                    || self.edges[id].iter().any(|&next| {
                        matches!(self.nodes[next], Node::Through(_, ThroughKind::Unknown))
                    }))
            {
                todo.push_back((id, 0, wire.route[0].width() - 1));
            }
        }
        while let Some((id, low, high)) = todo.pop_front() {
            for (lo, hi) in lanes[id].insert(low, high) {
                for &prev in &reverse[id] {
                    let (start, end) = match (&self.nodes[id], &self.nodes[prev]) {
                        (Node::Wire(wire), Node::Through(_, kind)) if kind.preserves_bits() => {
                            let base = wire.route[0].lsb;
                            (base + lo, base + hi)
                        }
                        (Node::Through(_, _), Node::Wire(wire)) => {
                            let base = wire
                                .peer_endpoint()
                                .endpoint
                                .as_pin()
                                .expect("a crossing starts at a pin")
                                .lsb;
                            (lo - base, hi - base)
                        }
                        _ => continue,
                    };
                    todo.push_back((prev, start, end));
                }
            }
        }
        for (id, live) in lanes.into_iter().enumerate() {
            if let Node::Wire(wire) = &self.nodes[id]
                && !live.ranges().eq([(0, wire.route[0].width() - 1)])
            {
                let slices = live
                    .ranges()
                    .map(|(lo, hi)| wire.pin_mapping_slice(lo, hi - lo + 1))
                    .collect();
                self.narrowed_wires.insert(id, slices);
            }
        }
    }

    /// Indexes participating terminals per occurrence after destination filtering.
    fn finalize(&mut self) {
        if self.targeted {
            self.prune();
        }
        let reverse = self.reverse_edges();
        if self.targeted {
            self.narrow_to_target(&reverse);
        }
        let mut instances = indexmap::IndexMap::new();
        for (i, n) in self.nodes.iter().enumerate() {
            for link in self.wire_slices(i) {
                for (j, &depth) in link.scopes.iter().enumerate() {
                    let (a, b) = (&link.route[j], &link.route[j + 1]);
                    if depth < 2 {
                        continue;
                    }
                    let a_path = occurrence_path(&a.port);
                    let b_path = occurrence_path(&b.port);
                    let scope = &b_path[..depth];
                    let owner = self.collapsed_owner(scope).map_or(scope, Arc::as_ref);
                    if a_path.as_ref() == owner || b_path.as_ref() == owner {
                        let step =
                            instances
                                .entry(Arc::from(owner))
                                .or_insert_with(|| InstanceStep {
                                    path: Arc::from(owner),
                                    entries: IndexSet::new(),
                                    exits: IndexSet::new(),
                                });
                        if a_path.as_ref() == owner {
                            step.entries.insert(a.clone());
                        }
                        if b_path.as_ref() == owner {
                            step.exits.insert(b.clone());
                        }
                    }
                }
            }
            if let Node::Through(path, _) = n
                && self.collapsed_owner(path).is_none_or(|p| p == path)
            {
                let step = instances
                    .entry(path.clone())
                    .or_insert_with(|| InstanceStep {
                        path: path.clone(),
                        entries: IndexSet::new(),
                        exits: IndexSet::new(),
                    });
                for &j in &reverse[i] {
                    if let Node::Start(Some(pin)) = &self.nodes[j] {
                        step.entries.insert(pin.as_ref().clone());
                    }
                    for wire in self.wire_slices(j) {
                        if let Some(pin) = wire.peer_endpoint().endpoint.as_pin() {
                            step.entries.insert(pin.clone());
                        }
                    }
                }
                for &j in &self.edges[i] {
                    for wire in self.wire_slices(j) {
                        if let Some(pin) = wire.queried_endpoint().endpoint.as_pin() {
                            step.exits.insert(pin.clone());
                        }
                    }
                }
            }
        }
        self.instances = instances.into_values().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_transfer_has_linear_storage_after_target_pruning() {
        const N: usize = 16;
        let top = ModDef::new("Top");
        let make = |name: &str, inputs: usize, outputs: usize| {
            let d = ModDef::new(name);
            for i in 0..inputs {
                d.add_port(format!("d{i}"), IO::Input(1));
            }
            for i in 0..outputs {
                d.add_port(format!("q{i}"), IO::Output(1));
            }
            d.set_usage(crate::Usage::EmitNothingAndStop);
            top.instantiate(&d, None, None)
        };
        let a = make("Source", 0, N);
        let x = make("Box", N, N);
        let b = make("Dest", N, 0);
        let extra = make("Other", N, 0);
        for i in 0..N {
            let q = format!("q{i}");
            let d = format!("d{i}");
            a.get_port(&q).connect(&x.get_port(&d));
            x.get_port(&q).connect(&b.get_port(&d));
            x.get_port(&q).connect(&extra.get_port(&d));
        }
        let g = paths_between(&a, &b, &PathOptions::default()).unwrap();
        assert_eq!(g.nodes.len(), 2 * N + 2); // Start, incoming wires, one hub, outgoing wires.
        assert_eq!(g.edges.iter().map(Vec::len).sum::<usize>(), 3 * N);
        assert_eq!(g.routes().count(), N * N);
    }
}
