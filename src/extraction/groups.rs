// SPDX-License-Identifier: Apache-2.0

use super::intervals::{Interval, IntervalIndex};
use super::trace::{ConnectionUnion, Flow, Session, Side, StopAt};
use super::*;
use crate::{ConvertibleToPortSlice, ModInst};
use indexmap::{IndexMap, IndexSet};

use std::collections::{HashMap, HashSet};

/// An occurrence, or the definition serving as this query's design root.
#[derive(Clone)]
pub enum ModuleRef {
    Root(ModDef),
    Instance(ModInst),
}
impl ModuleRef {
    fn key(&self) -> InstanceKey {
        match self {
            Self::Root(d) => (Arc::as_ptr(&d.core) as usize, vec![]),
            Self::Instance(i) => instance_key(i),
        }
    }
    fn ports(&self) -> Vec<crate::Port> {
        match self {
            Self::Root(d) => d.get_ports(None),
            Self::Instance(i) => i.get_ports(None),
        }
    }
    fn interfaces(&self) -> Vec<crate::Intf> {
        match self {
            Self::Root(d) => d.get_intfs(None),
            Self::Instance(i) => i.get_intfs(None),
        }
    }
}
/// One side of an extraction: a module handle, or a collection of handles.
pub trait ModuleGroup {
    /// Visits every selected occurrence in this group.
    fn visit(&self, f: &mut dyn FnMut(ModuleRef));
}
impl ModuleGroup for ModInst {
    fn visit(&self, f: &mut dyn FnMut(ModuleRef)) {
        f(ModuleRef::Instance(self.clone()))
    }
}
impl ModuleGroup for ModDef {
    fn visit(&self, f: &mut dyn FnMut(ModuleRef)) {
        f(ModuleRef::Root(self.clone()))
    }
}
impl<T: ModuleGroup + ?Sized> ModuleGroup for &T {
    fn visit(&self, f: &mut dyn FnMut(ModuleRef)) {
        (**self).visit(f)
    }
}
impl<T: ModuleGroup> ModuleGroup for [T] {
    fn visit(&self, f: &mut dyn FnMut(ModuleRef)) {
        for m in self {
            m.visit(f)
        }
    }
}
impl<T: ModuleGroup, const N: usize> ModuleGroup for [T; N] {
    fn visit(&self, f: &mut dyn FnMut(ModuleRef)) {
        self.as_slice().visit(f)
    }
}
impl<T: ModuleGroup> ModuleGroup for Vec<T> {
    fn visit(&self, f: &mut dyn FnMut(ModuleRef)) {
        self.as_slice().visit(f)
    }
}

/// Selects how to represent the same wiring in the query result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionView {
    /// Returns every mapped slice individually.
    Signals,
    /// Reports every complete interface pair with matching function names and bit mappings.
    /// Pairs can share wires, including when several interface names alias the same pins.
    /// Wires outside all complete pairs are returned as individual signals.
    InterfacesAndSignals,
}

/// An interface occurrence in the snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceName {
    pub path: Arc<[String]>,
    pub name: String,
}

/// Two declared interface occurrences and their verified, direction-preserving mappings.
#[derive(Clone, Debug)]
pub struct InterfaceConnection {
    interfaces: [InterfaceName; 2],
    members: Vec<(String, ConnectionTrace)>,
}
impl InterfaceConnection {
    /// The two interface occurrences covered by this bundle.
    pub fn interfaces(&self) -> &[InterfaceName; 2] {
        &self.interfaces
    }
    /// Function names and their wires, including reverse-going signals. A wire can
    /// also be a member of another pair. Use [`SignalConnection`] for wire identity.
    pub fn member_connections(&self) -> &[(String, ConnectionTrace)] {
        &self.members
    }
}

/// A single mapped slice or a complete pair of declared interfaces.
#[derive(Clone, Debug)]
pub enum Connection {
    Signal(ConnectionTrace),
    Interface(InterfaceConnection),
}

/// All wiring between distinct supplied groups of occurrences, in either direction.
/// The result excludes peers outside the selection. In a nested selection, ports
/// of selected wrappers are observation points alongside ports of selected children.
/// If several routes map the same endpoint bits, the signal view includes each bit pair
/// once. Each returned trace shows one route for its mapped slice.
pub fn connections_between(
    groups: &[&dyn ModuleGroup],
    view: ConnectionView,
) -> Result<Vec<Connection>, String> {
    let selection = Selection::new(groups)?;
    let signals = selection.extract(false);
    Ok(bundle(&selection, signals, view))
}

/// All wiring that enters or leaves the union of the selected subtrees.
/// Each crossing ends at the pin or attachment directly wired to a selected subtree
/// in its parent. A wire between two selected subtrees stays inside the group, while
/// a loop through an unselected instance contributes a crossing at each attachment.
/// The result includes constants, unused slices and incomplete bindings.
/// A specified net name is a standalone attachment only on bits without another binding.
/// Pin-level traces retain the name on connected bits as well. The design root has no modeled exterior.
pub fn boundary_connections(group: &dyn ModuleGroup) -> Result<Vec<ConnectionTrace>, String> {
    Ok(Selection::new(&[group])?.extract(true))
}

struct Selection {
    instances: Vec<(ModuleRef, usize, InstanceKey)>,
    owners: HashMap<InstanceKey, usize>,
    ancestors: HashSet<InstanceKey>,
}
impl Selection {
    fn new(groups: &[&dyn ModuleGroup]) -> Result<Self, String> {
        let mut instances = Vec::new();
        let mut owners = HashMap::new();
        let mut ancestors = HashSet::new();
        let mut root = None;
        for (group_id, group) in groups.iter().enumerate() {
            let mut error = None;
            group.visit(&mut |inst| {
                let key = inst.key();
                if root.replace(key.0).is_some_and(|r| r != key.0) {
                    error = Some("selected instances have different design roots".to_owned());
                    return;
                }
                if let Some(old) = owners.insert(key.clone(), group_id) {
                    if old != group_id {
                        error = Some(format!("{} belongs to multiple groups", key.1.join(".")))
                    }
                    return;
                }
                for n in 0..key.1.len() {
                    ancestors.insert((key.0, key.1[..n].to_vec()));
                }
                instances.push((inst, group_id, key))
            });
            if let Some(error) = error {
                return Err(error);
            }
        }
        Ok(Self {
            instances,
            owners,
            ancestors,
        })
    }
    fn extract(&self, boundary: bool) -> Vec<ConnectionTrace> {
        let mut union = ConnectionUnion::default();
        let mut session = Session::new(if boundary {
            StopAt::GroupBoundary
        } else {
            StopAt::Selected(self.owners.keys().cloned().collect())
        });
        for (inst, group, key) in &self.instances {
            if boundary
                && (0..key.1.len()).any(|n| self.owners.contains_key(&(key.0, key.1[..n].to_vec())))
            {
                continue;
            }
            let sides = if !boundary && self.ancestors.contains(key) {
                &[Side::Parent, Side::Body][..]
            } else {
                &[Side::Parent][..]
            };
            for port in inst.ports() {
                if port.io().width() == 0 {
                    continue;
                }
                let slice = port.to_port_slice();
                for flow in [Flow::Down, Flow::Up] {
                    for link in session.trace(&slice, flow, &HierarchyPolicy::default(), sides) {
                        let keep = if boundary {
                            link.peer_endpoint()
                                .endpoint
                                .as_pin()
                                .is_none_or(|pin| !self.owners.contains_key(&port_owner(&pin.port)))
                        } else {
                            link.stop_reason == StopReason::Selected
                                && link
                                    .peer_endpoint()
                                    .endpoint
                                    .as_pin()
                                    .and_then(|p| self.owners.get(&port_owner(&p.port)))
                                    .is_some_and(|g| g != group)
                        };
                        if keep {
                            union.insert(link)
                        }
                    }
                }
            }
        }
        union.links
    }
}

struct Decl {
    name: InterfaceName,
    // Named members index the distinct physical slices in this query.
    members: IndexMap<String, usize>,
}

/// Splits wires at interface boundaries and retains every complete interface pair.
fn bundle(
    selection: &Selection,
    signals: Vec<ConnectionTrace>,
    view: ConnectionView,
) -> Vec<Connection> {
    let mut pins = IndexSet::new();
    let mut decls = Vec::new();
    for (inst, _, _) in &selection.instances {
        for intf in inst.interfaces() {
            let members: IndexMap<_, _> = intf
                .iter()
                .map(|(name, pin)| (name, pins.insert_full(pin).0))
                .collect();
            if let Some(&first) = members.values().next() {
                decls.push(Decl {
                    name: InterfaceName {
                        path: occurrence_path(&pins[first].port),
                        name: intf.name().to_owned(),
                    },
                    members,
                });
            }
        }
    }
    if pins.is_empty() {
        return signals.into_iter().map(Connection::Signal).collect();
    }
    let mut by_port: HashMap<Port, Vec<Interval<usize>>> = HashMap::new();
    for (id, pin) in pins.iter().enumerate() {
        by_port.entry(pin.port.clone()).or_default().push(Interval {
            lsb: pin.lsb,
            msb: pin.msb,
            value: id,
        });
    }
    let ranges: HashMap<_, _> = by_port
        .into_iter()
        .map(|(port, entries)| (port, IntervalIndex::new(entries)))
        .collect();
    let mut pieces = Vec::new();
    for wire in signals {
        if let Some((start, end)) = wire.pin_mapping()
            && let (Some(left), Some(right)) = (ranges.get(&start.port), ranges.get(&end.port))
        {
            let mut cuts = vec![0, start.width()];
            for (pin, index) in [(start, left), (end, right)] {
                for range in index.overlapping(pin.lsb, pin.msb) {
                    cuts.push(range.lsb.max(pin.lsb) - pin.lsb);
                    cuts.push(range.msb.min(pin.msb) - pin.lsb + 1);
                }
            }
            cuts.sort_unstable();
            cuts.dedup();
            if cuts.len() > 2 {
                pieces.extend(
                    cuts.windows(2)
                        .map(|r| wire.pin_mapping_slice(r[0], r[1] - r[0])),
                );
                continue;
            }
        }
        pieces.push(wire);
    }
    if view == ConnectionView::Signals {
        return pieces.into_iter().map(Connection::Signal).collect();
    }
    let wiring = MemberWiring::new(&pieces, &pins, &ranges);
    wiring.group(&decls, pieces)
}

/// Complete bit-for-bit wiring between physical member slices, shared by all their aliases.
struct MemberWiring {
    peers: Vec<Vec<usize>>,
    // Keyed by ascending pin IDs. Pieces are kept in order of offset within the member.
    wires: IndexMap<(usize, usize), Vec<(usize, usize)>>,
}

impl MemberWiring {
    /// Indexes each connected pair of full slices once, independent of interface names.
    fn new(
        pieces: &[ConnectionTrace],
        pins: &IndexSet<PortSlice>,
        ranges: &HashMap<Port, IntervalIndex<usize>>,
    ) -> Self {
        let mut wires: IndexMap<_, Vec<_>> = IndexMap::new();
        for (id, piece) in pieces.iter().enumerate() {
            let Some((start, end)) = piece.pin_mapping() else {
                continue;
            };
            let Some(left) = ranges.get(&start.port) else {
                continue;
            };
            let end_width = end.port.io().width();
            for span in left.overlapping(start.lsb, start.lsb) {
                // An equal-width member at the same relative bit offset has exactly one
                // possible range on the peer. Look it up directly, including overlapping declarations.
                let offset = start.lsb - span.lsb;
                let Some(lo) = end.lsb.checked_sub(offset) else {
                    continue;
                };
                if span.width() > end_width - lo {
                    continue;
                }
                let peer = end.port.slice(lo + (span.width() - 1), lo);
                if let Some(other) = pins.get_index_of(&peer) {
                    let key = (span.value.min(other), span.value.max(other));
                    wires.entry(key).or_default().push((offset, id));
                }
            }
        }
        let mut peers = vec![Vec::new(); pins.len()];
        wires.retain(|&(a, b), pieces_for_pair| {
            pieces_for_pair.sort_unstable();
            let mut covered = 0;
            for &(offset, id) in pieces_for_pair.iter() {
                if offset != covered {
                    return false;
                }
                covered += pieces[id].route()[0].width();
            }
            if covered != pins[a].width() {
                return false;
            }
            peers[a].push(b);
            peers[b].push(a);
            true
        });
        Self { peers, wires }
    }

    /// Seeds each declaration's search with its member having the fewest compatible
    /// peer declarations, then verifies every named member of the candidate pair.
    fn group(&self, decls: &[Decl], pieces: Vec<ConnectionTrace>) -> Vec<Connection> {
        let mut owners: HashMap<_, Vec<usize>> = HashMap::new();
        for (id, decl) in decls.iter().enumerate() {
            for (name, &pin) in &decl.members {
                owners.entry((pin, name.as_str())).or_default().push(id);
            }
        }
        let peer_counts: HashMap<_, usize> = owners
            .keys()
            .map(|&(pin, name)| {
                let count = self.peers[pin]
                    .iter()
                    .filter_map(|p| owners.get(&(*p, name)))
                    .map(Vec::len)
                    .sum();
                ((pin, name), count)
            })
            .collect();
        let mut used = vec![false; pieces.len()];
        let mut results = Vec::new();
        for (a, decl) in decls.iter().enumerate() {
            let (name, &pin) = decl
                .members
                .iter()
                .min_by_key(|(name, pin)| peer_counts[&(**pin, name.as_str())])
                .unwrap();
            for b in self.peers[pin]
                .iter()
                .filter_map(|p| owners.get(&(*p, name.as_str())))
                .flatten()
                .copied()
            {
                if b <= a || decl.members.len() != decls[b].members.len() {
                    continue;
                }
                let matched: Option<Vec<_>> = decl
                    .members
                    .iter()
                    .map(|(name, &p)| {
                        let &q = decls[b].members.get(name)?;
                        let wires = self.wires.get(&(p.min(q), p.max(q)))?;
                        Some((name, wires))
                    })
                    .collect();
                if let Some(matched) = matched {
                    let mut members = Vec::new();
                    for (name, wires) in matched {
                        for &(_, id) in wires {
                            used[id] = true;
                            members.push((name.clone(), pieces[id].clone()));
                        }
                    }
                    results.push(Connection::Interface(InterfaceConnection {
                        interfaces: [decl.name.clone(), decls[b].name.clone()],
                        members,
                    }));
                }
            }
        }
        results.extend(
            pieces
                .into_iter()
                .zip(used)
                .filter_map(|(p, used)| (!used).then_some(Connection::Signal(p))),
        );
        results
    }
}
