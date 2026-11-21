// SPDX-License-Identifier: Apache-2.0

//! Read-only, occurrence-relative queries on assembled TopStitch wiring.
//!
//! A pin query follows structural wires. A group query stops at selected pins and
//! compares their owners. A path query additionally describes steps through
//! instances. Each result records the wiring found by that query. Its port slices
//! refer to the original design, which must remain alive while using them.
//! Finish editing the design before starting a query for a consistent result.
//!
//! ```
//! use topstitch::*;
//!
//! let top = ModDef::new("Top");
//! let producer = ModDef::new("Producer");
//! producer.add_port("data", IO::Output(8));
//! producer.set_usage(Usage::EmitNothingAndStop);
//! let consumer = ModDef::new("Consumer");
//! consumer.add_port("data", IO::Input(4));
//! consumer.set_usage(Usage::EmitNothingAndStop);
//! let tx = top.instantiate(&producer, Some("tx"), None);
//! let rx = top.instantiate(&consumer, Some("rx"), None);
//! let tap = top.instantiate(&consumer, Some("tap"), None);
//! let high = tx.get_port("data").slice(7, 4);
//! high.connect(&rx.get_port("data"));
//! high.connect(&tap.get_port("data"));
//! tx.get_port("data").slice(3, 0).unused();
//!
//! let loads = tx.get_port("data").bit(5).receivers();
//! assert_eq!(loads.len(), 2);
//! for trace in &loads {
//!     let peer = &trace.peer_endpoint().endpoint;
//!     assert_eq!(peer.as_pin().map(PortSlice::lsb), Some(1));
//! }
//!
//! // The two queries see the same wiring from opposite ends.
//! let load = rx.get_port("data").bit(1);
//! let upstream = load.drivers();
//! assert!(loads.iter().any(|trace| trace.connection == upstream[0].connection));
//!
//! let group = [&tx, &rx];
//! let between = connections_between(&[&group, &tap], ConnectionView::Signals)?;
//! assert_eq!(between.len(), 1);
//! // The group boundary includes the connection to the tap and the unused low bits of tx.data.
//! assert_eq!(boundary_connections(&group)?.len(), 2);
//! # Ok::<(), String>(())
//! ```
//!
//! Use [`paths_between`] or [`crate::PortSlice::trace_paths`] to include modeled
//! dependencies and candidate crossings through opaque instances. A structural
//! wire query stops at their pins. Imported SV bodies are opaque until their
//! internal bindings are available. Native ports-only definitions also describe opaque
//! interfaces. Recorded internal wiring and child instances are traversable under any emission usage.

mod groups;
mod intervals;
mod paths;
mod trace;

pub use groups::{
    Connection, ConnectionView, InterfaceConnection, InterfaceName, ModuleGroup, ModuleRef,
    boundary_connections, connections_between,
};
pub use paths::{InstanceStep, PathGraph, PathOptions, PathStep, ThroughKind, paths_between};
pub use trace::connections_touching;

use crate::{ModDef, ModInst, Port, PortSlice};
use num_bigint::BigInt;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
type InstanceKey = (usize, Vec<String>);

/// Identifies an occurrence independently of module names or path separators.
fn instance_key(inst: &ModInst) -> InstanceKey {
    (
        inst.hierarchy[0].mod_def_core.as_ptr() as usize,
        inst.hierarchy
            .iter()
            .map(|frame| frame.inst_name.clone())
            .collect(),
    )
}

fn port_owner(port: &crate::Port) -> InstanceKey {
    match port {
        crate::Port::ModDef { mod_def_core, .. } => (mod_def_core.as_ptr() as usize, vec![]),
        crate::Port::ModInst { .. } => instance_key(&port.get_mod_inst().unwrap()),
    }
}

/// Names an occurrence for results that label a module or a wire scope.
fn occurrence_path(port: &Port) -> Arc<[String]> {
    match port {
        Port::ModDef { .. } => vec![port.get_mod_def_where_declared().get_name()].into(),
        Port::ModInst { hierarchy, .. } => {
            let root = hierarchy[0]
                .mod_def_core
                .upgrade()
                .expect("design root dropped");
            std::iter::once(root.read().name.clone())
                .chain(hierarchy.iter().map(|frame| frame.inst_name.clone()))
                .collect()
        }
    }
}

/// A terminal encountered while following structural wiring.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Endpoint {
    Pin(PortSlice),
    Constant {
        at: PortSlice,
        value: BigInt,
    },
    NamedWire {
        scope: Arc<[String]>,
        name: String,
        msb: usize,
        lsb: usize,
    },
    Unused(PortSlice),
    Unconnected(PortSlice),
}

impl Endpoint {
    /// Borrows the mapped port slice if this terminal is a pin.
    /// Only [`Endpoint::Pin`] yields a slice, even when another variant records its attachment site.
    pub fn as_pin(&self) -> Option<&PortSlice> {
        match self {
            Self::Pin(pin) => Some(pin),
            _ => None,
        }
    }
}

/// Role of an endpoint as seen from the module containing the wiring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Supplies a value, as a module input, a child output, or a constant does.
    Driver,
    /// Receives a value, as a module output or a child input does.
    Receiver,
    /// Can drive or receive, so both directional queries include it.
    InOut,
}

/// Why a structural trace ends at this endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// The trace reached a root port or the first external pin in a group boundary query.
    Boundary,
    /// The trace reached a module whose internal wiring is unavailable.
    Opaque,
    /// The query's hierarchy policy specifies [`HierarchyAction::Stop`] at this instance.
    Policy,
    /// The trace reached a port on a module explicitly selected by the query.
    Selected,
    /// A constant tieoff supplies the value of this slice.
    Constant,
    /// The slice has an explicit named-wire attachment.
    NamedWire,
    /// The design explicitly marks these bits unused.
    Unused,
    /// The trace found no further wiring for these bits on this side of the port.
    Unconnected,
    /// Continuing would revisit wiring on this route or descend into recursive hierarchy.
    Cycle,
}

/// One terminal and the role observed there during extraction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionEndpoint {
    /// The pin, constant, or other attachment at this end.
    pub endpoint: Endpoint,
    /// Present when a driver, receiver, or inout was identified at this terminal.
    pub role: Option<Role>,
}

/// One exact bit mapping. Equality and hashing use the unordered pair of terminals,
/// including their bit ranges. Roles describe the observation and do not affect identity.
#[derive(Clone, Debug)]
pub struct SignalConnection {
    /// A known driver precedes its receiver. Two inouts can appear in either order.
    pub endpoints: [ConnectionEndpoint; 2],
}

impl PartialEq for SignalConnection {
    fn eq(&self, other: &Self) -> bool {
        let [a, b] = &self.endpoints;
        let [c, d] = &other.endpoints;
        (a.endpoint == c.endpoint && b.endpoint == d.endpoint)
            || (a.endpoint == d.endpoint && b.endpoint == c.endpoint)
    }
}

impl Eq for SignalConnection {}

impl Hash for SignalConnection {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Endpoints support hashing but have no natural total order.
        let mut hashes = self.endpoints.each_ref().map(|end| {
            let mut hasher = DefaultHasher::new();
            end.endpoint.hash(&mut hasher);
            hasher.finish()
        });
        hashes.sort_unstable();
        hashes.hash(state);
    }
}

/// A connection as encountered by one query. The route runs from the queried endpoint
/// toward the peer, independently of the hardware direction. Use `connection` for deduplication.
#[derive(Clone, Debug)]
pub struct ConnectionTrace {
    /// The wiring identified by this trace, comparable across queries.
    pub connection: SignalConnection,
    /// Why tracing stopped at the peer.
    pub stop_reason: StopReason,
    // Whether the peer is the second endpoint in hardware presentation order.
    peer_is_second: bool,
    route: Vec<PortSlice>,
    visible: Vec<bool>,
    // For each wire hop, depth of the module body containing that attachment.
    scopes: Vec<usize>,
}

impl ConnectionTrace {
    /// The endpoint where this trace starts, clipped to the bits participating in the mapping.
    pub fn queried_endpoint(&self) -> &ConnectionEndpoint {
        &self.connection.endpoints[usize::from(!self.peer_is_second)]
    }

    /// The endpoint reached by following the route from the queried endpoint.
    pub fn peer_endpoint(&self) -> &ConnectionEndpoint {
        &self.connection.endpoints[usize::from(self.peer_is_second)]
    }

    /// All wrapper pins on this mapping, clipped to the exact bits at each hop.
    pub fn route(&self) -> &[PortSlice] {
        &self.route
    }

    /// The route with pins beneath collapsed occurrences hidden.
    pub fn visible_route(&self) -> impl Iterator<Item = &PortSlice> {
        self.route
            .iter()
            .zip(&self.visible)
            .filter_map(|(pin, visible)| visible.then_some(pin))
    }

    /// Returns the queried and peer slices of a pin-to-pin mapping, in trace order.
    fn pin_mapping(&self) -> Option<(&PortSlice, &PortSlice)> {
        Some((
            self.queried_endpoint().endpoint.as_pin()?,
            self.peer_endpoint().endpoint.as_pin()?,
        ))
    }

    /// Clips a pin-to-pin mapping and every intermediate hop by the same offset.
    fn pin_mapping_slice(&self, offset: usize, width: usize) -> Self {
        let endpoints = self.connection.endpoints.each_ref().map(|end| {
            let Endpoint::Pin(pin) = &end.endpoint else {
                unreachable!("only pin-to-pin mappings can be subdivided")
            };
            ConnectionEndpoint {
                endpoint: Endpoint::Pin(pin.slice_with_offset_and_width(offset, width)),
                role: end.role,
            }
        });
        Self {
            connection: SignalConnection { endpoints },
            stop_reason: self.stop_reason,
            peer_is_second: self.peer_is_second,
            route: self
                .route
                .iter()
                .map(|p| p.slice_with_offset_and_width(offset, width))
                .collect(),
            visible: self.visible.clone(),
            scopes: self.scopes.clone(),
        }
    }
}

/// How a query traverses an instance and presents its internal wiring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HierarchyAction {
    /// Follow available internal wiring and show intermediate pins by default.
    #[default]
    Expand,
    /// Follow the same wiring as [`Expand`](Self::Expand), hiding intermediate pins
    /// inside the instance in [`ConnectionTrace::visible_route`]. The full sequence
    /// remains available through [`ConnectionTrace::route`].
    Collapse,
    /// End the trace at the instance pin it reaches. When this rule ends a trace,
    /// its stop reason is [`StopReason::Policy`].
    Stop,
}

/// Per-query hierarchy rules. Instance rules take precedence over definition rules.
/// The policy holds the definitions it identifies for the policy's lifetime.
#[derive(Clone, Default)]
pub struct HierarchyPolicy {
    default: HierarchyAction,
    definitions: HashMap<usize, (ModDef, HierarchyAction)>,
    instances: HashMap<InstanceKey, HierarchyAction>,
    roots: Vec<ModDef>,
}

impl HierarchyPolicy {
    /// Shows the internals of every available structural body.
    pub fn expand_all() -> Self {
        Self::default()
    }
    /// Follows the same structural wiring and hides intermediate occurrences by default.
    pub fn collapse_all() -> Self {
        Self {
            default: HierarchyAction::Collapse,
            ..Self::default()
        }
    }
    /// Applies a rule to all occurrences of a particular definition.
    pub fn for_definition(mut self, def: &ModDef, action: HierarchyAction) -> Self {
        self.definitions
            .insert(Arc::as_ptr(&def.core) as usize, (def.clone(), action));
        self
    }
    /// Overrides the rule for one occurrence reached from this design root.
    pub fn for_instance(mut self, inst: &ModInst, action: HierarchyAction) -> Self {
        self.roots.push(ModDef {
            core: inst.hierarchy[0]
                .mod_def_core
                .upgrade()
                .expect("design root dropped"),
        });
        self.instances.insert(instance_key(inst), action);
        self
    }
    fn action(&self, inst: &ModInst, def: &ModDef) -> HierarchyAction {
        if !self.instances.is_empty()
            && let Some(action) = self.instances.get(&instance_key(inst))
        {
            return *action;
        }
        self.definitions
            .get(&(Arc::as_ptr(&def.core) as usize))
            .map_or(self.default, |(_, action)| *action)
    }
}
