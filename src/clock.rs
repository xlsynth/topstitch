// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::{Arc, Weak},
};

use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::{ConvertibleToPortSlice, Intf, ModDef, ModDefCore, Port, PortSlice};

/// One declared clock bit on a module definition.
///
/// A pin refers weakly to its module. After the module is dropped, the pin
/// retains its identity, and assigning it to a port on another module returns an error.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClockPin(PortSlice);

impl ClockPin {
    /// Returns the module port bit identified by this handle.
    pub fn slice(&self) -> &PortSlice {
        &self.0
    }
}

/// A named boundary domain whose clock is outside the module.
#[derive(Clone, Debug)]
pub struct ExternalClock {
    owner: Weak<RwLock<ModDefCore>>,
    name: String,
}

impl ExternalClock {
    /// Returns the name under which the module declared this clock.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl PartialEq for ExternalClock {
    fn eq(&self, other: &Self) -> bool {
        self.owner.ptr_eq(&other.owner) && self.name == other.name
    }
}

impl Eq for ExternalClock {}

/// A clock declared on a physical bit or by name on a module boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clock {
    Pin(ClockPin),
    External(ExternalClock),
}

impl From<&ClockPin> for Clock {
    fn from(pin: &ClockPin) -> Self {
        Self::Pin(pin.clone())
    }
}

impl From<&ExternalClock> for Clock {
    fn from(clock: &ExternalClock) -> Self {
        Self::External(clock.clone())
    }
}

impl From<&Clock> for Clock {
    fn from(clock: &Clock) -> Self {
        clock.clone()
    }
}

impl Clock {
    /// Checks ownership before storing a reference on a module boundary.
    fn local_to(&self, module: &ModDef) -> Result<LocalClock, String> {
        let owner = Arc::downgrade(&module.core);
        match self {
            Self::Pin(pin) => {
                let Port::ModDef { mod_def_core, name } = &pin.0.port else {
                    unreachable!()
                };
                if mod_def_core.ptr_eq(&owner) {
                    return Ok(LocalClock::Pin {
                        port: name.clone(),
                        bit: pin.0.lsb,
                    });
                }
            }
            Self::External(clock) if clock.owner.ptr_eq(&owner) => {
                return Ok(LocalClock::External(clock.name.clone()));
            }
            _ => {}
        }
        Err(format!(
            "Clock {self:?} belongs to a different module than {}",
            module.get_name()
        ))
    }
}

/// The timing role of a contiguous range of boundary bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClockRole {
    /// Each bit in this run is a declared clock terminal.
    ClockPin,
    /// Data associated with this local pin or named external domain.
    TimedBy(Clock),
    /// An independent asynchronous scalar signal.
    Async,
    /// Explicitly excluded from timing defaults.
    Unclocked,
    /// No declaration applies to this signal.
    Unspecified,
}

/// The most specific declarations that supplied a resolved role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClockOrigin {
    Explicit,
    /// Names of all agreeing interfaces, in alphabetical order.
    Interfaces(Vec<String>),
    ModuleDefault,
    Unspecified,
}

/// Contiguous port bits with the same timing role and declaration source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockAssociation {
    pub slice: PortSlice,
    pub role: ClockRole,
    pub origin: ClockOrigin,
}

/// Snapshot of the module boundary, in port declaration order and ascending bit order.
pub struct ClockAssociations {
    module: ModDef,
    ports: IndexMap<String, Vec<ClockAssociation>>,
}

impl ClockAssociations {
    /// Returns all runs for this definition port, or an empty slice for a foreign port.
    pub fn for_port(&self, port: &Port) -> &[ClockAssociation] {
        if let Port::ModDef { mod_def_core, name } = port
            && mod_def_core.ptr_eq(&Arc::downgrade(&self.module.core))
            && let Some(runs) = self.ports.get(name)
        {
            return runs;
        }
        &[]
    }

    /// Iterates over every resolved run, including unspecified bits.
    pub fn iter(&self) -> impl Iterator<Item = &ClockAssociation> {
        self.ports.values().flatten()
    }
}

// Store local names and bit indices so a stub can inherit its boundary with fresh handles.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LocalClock {
    Pin { port: String, bit: usize },
    External(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Declaration {
    ClockPin,
    TimedBy(LocalClock),
    Async,
    Unclocked,
}

#[derive(Clone, Default)]
pub(crate) struct ClockDeclarations {
    external: BTreeSet<String>,
    default: Option<LocalClock>,
    interfaces: BTreeMap<String, LocalClock>,
    ports: BTreeMap<String, Vec<(Range<usize>, Declaration)>>,
}

/// A current interface mapping with a declared default clock.
struct InterfaceDefault<'a> {
    range: Range<usize>,
    name: &'a str,
    clock: &'a LocalClock,
}

impl ModDef {
    /// Declares a module-local external domain. Repeating a name returns the same clock.
    pub fn declare_external_clock(&self, name: impl AsRef<str>) -> Result<ExternalClock, String> {
        let name = name.as_ref();
        if name.is_empty() {
            return Err("An external clock needs a name".into());
        }
        self.core.write().clocks.external.insert(name.to_owned());
        Ok(self.external_clock_handle(name))
    }

    /// Looks up an external clock without creating a declaration.
    pub fn get_external_clock(&self, name: &str) -> Result<ExternalClock, String> {
        if self.core.read().clocks.external.contains(name) {
            Ok(self.external_clock_handle(name))
        } else {
            Err(format!(
                "Unknown external clock {name} on {}",
                self.get_name()
            ))
        }
    }

    /// Lists declared pin bits in port and bit order, followed by external clocks in name order.
    /// Includes declarations that no signal uses.
    pub fn clocks(&self) -> Vec<Clock> {
        let core = self.core.read();
        let mut result = Vec::new();
        for name in core.ports.keys() {
            if let Some(declarations) = core.clocks.ports.get(name) {
                let mut bits: Vec<_> = declarations
                    .iter()
                    .filter_map(|(range, role)| {
                        (*role == Declaration::ClockPin).then_some(range.start)
                    })
                    .collect();
                bits.sort_unstable();
                result.extend(bits.into_iter().map(|bit| {
                    self.clock_handle(&LocalClock::Pin {
                        port: name.clone(),
                        bit,
                    })
                }));
            }
        }
        result.extend(
            core.clocks
                .external
                .iter()
                .map(|name| Clock::External(self.external_clock_handle(name))),
        );
        result
    }

    /// Rebinds a stored declaration to the current module, including copied boundaries.
    fn clock_handle(&self, clock: &LocalClock) -> Clock {
        match clock {
            LocalClock::Pin { port, bit } => Clock::Pin(ClockPin(PortSlice {
                port: Port::ModDef {
                    mod_def_core: Arc::downgrade(&self.core),
                    name: port.clone(),
                },
                msb: *bit,
                lsb: *bit,
            })),
            LocalClock::External(name) => Clock::External(self.external_clock_handle(name)),
        }
    }

    /// Binds an external name to this definition without consulting or changing its registry.
    fn external_clock_handle(&self, name: &str) -> ExternalClock {
        ExternalClock {
            owner: Arc::downgrade(&self.core),
            name: name.to_owned(),
        }
    }

    /// Sets the clock for data bits with no explicit assignment or interface default.
    pub fn set_default_clock(&self, clock: impl Into<Clock>) -> Result<(), String> {
        let bit = clock.into().local_to(self)?;
        let mut core = self.core.write();
        if let Some(previous) = &core.clocks.default
            && previous != &bit
        {
            return Err(format!(
                "Conflicting default clocks on {}: {previous:?} and {bit:?}",
                core.name
            ));
        }
        core.clocks.default = Some(bit);
        Ok(())
    }

    /// Resolves the current interface mappings and explicit bit ranges without changing them.
    /// Equally specific declarations must agree on every bit where they take effect.
    pub fn clock_associations(&self) -> Result<ClockAssociations, String> {
        let core = self.core.read();
        let port = |name: &str| Port::ModDef {
            mod_def_core: Arc::downgrade(&self.core),
            name: name.to_owned(),
        };
        let mut by_port: BTreeMap<&str, Vec<InterfaceDefault<'_>>> = BTreeMap::new();
        for (intf, bit) in &core.clocks.interfaces {
            for (name, msb, lsb) in core.interfaces[intf].values() {
                if !core
                    .ports
                    .get(name)
                    .is_some_and(|io| lsb <= msb && *msb < io.width())
                {
                    return Err(format!(
                        "Invalid clocked interface mapping {}.{intf} to {name}[{msb}:{lsb}]",
                        core.name
                    ));
                }
                by_port.entry(name).or_default().push(InterfaceDefault {
                    range: *lsb..*msb + 1,
                    name: intf,
                    clock: bit,
                });
            }
        }
        let mut ports = IndexMap::new();
        for (name, io) in &core.ports {
            let explicit = core
                .clocks
                .ports
                .get(name)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let interfaces = by_port
                .get(name.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            // A role can change only at a declaration boundary, even on very wide buses.
            let mut bounds = vec![0, io.width()];
            for range in explicit
                .iter()
                .map(|item| &item.0)
                .chain(interfaces.iter().map(|item| &item.range))
            {
                bounds.extend([range.start, range.end]);
            }
            bounds.sort_unstable();
            bounds.dedup();
            let mut runs: Vec<ClockAssociation> = Vec::new();
            for pair in bounds.windows(2) {
                let (lsb, msb) = (pair[0], pair[1] - 1);
                let (role, origin) = resolve_role(
                    lsb,
                    explicit,
                    interfaces,
                    core.clocks.default.as_ref(),
                    |clock| self.clock_handle(clock),
                )
                .map_err(|error| format!("{}.{name}[{msb}:{lsb}]: {error}", core.name))?;
                if let Some(last) = runs.last_mut()
                    && last.role == role
                    && last.origin == origin
                {
                    last.slice.msb = msb;
                } else {
                    runs.push(ClockAssociation {
                        slice: PortSlice {
                            port: port(name),
                            msb,
                            lsb,
                        },
                        role,
                        origin,
                    });
                }
            }
            ports.insert(name.clone(), runs);
        }
        Ok(ClockAssociations {
            module: self.clone(),
            ports,
        })
    }
}

/// Selects the most specific declarations covering a bit and rejects ties that disagree.
fn resolve_role(
    bit: usize,
    explicit: &[(Range<usize>, Declaration)],
    interfaces: &[InterfaceDefault<'_>],
    default: Option<&LocalClock>,
    pin: impl Fn(&LocalClock) -> Clock,
) -> Result<(ClockRole, ClockOrigin), String> {
    if let Some((_, declaration)) = explicit.iter().find(|(range, _)| range.contains(&bit)) {
        let role = match declaration {
            Declaration::ClockPin => ClockRole::ClockPin,
            Declaration::TimedBy(clock) => ClockRole::TimedBy(pin(clock)),
            Declaration::Async => ClockRole::Async,
            Declaration::Unclocked => ClockRole::Unclocked,
        };
        return Ok((role, ClockOrigin::Explicit));
    }
    let mut defaults = interfaces.iter().filter(|entry| entry.range.contains(&bit));
    if let Some(first) = defaults.next() {
        let mut names = vec![first.name.to_owned()];
        for other in defaults {
            if first.clock != other.clock {
                return Err(format!(
                    "interfaces {} and {} specify different clocks ({:?} and {:?})",
                    first.name, other.name, first.clock, other.clock
                ));
            }
            if names.last().unwrap() != other.name {
                names.push(other.name.to_owned());
            }
        }
        return Ok((
            ClockRole::TimedBy(pin(first.clock)),
            ClockOrigin::Interfaces(names),
        ));
    }
    Ok(match default {
        Some(clock) => (ClockRole::TimedBy(pin(clock)), ClockOrigin::ModuleDefault),
        None => (ClockRole::Unspecified, ClockOrigin::Unspecified),
    })
}

impl Intf {
    /// Supplies a clock for member bits without an explicit declaration.
    pub fn set_default_clock(&self, clock: impl Into<Clock>) -> Result<(), String> {
        let Intf::ModDef { name, .. } = self else {
            return Err("Clock defaults require an interface on a module definition".into());
        };
        let module = ModDef {
            core: self.get_mod_def_core(),
        };
        let bit = clock.into().local_to(&module)?;
        let mut core = module.core.write();
        if let Some(previous) = core.clocks.interfaces.get(name)
            && previous != &bit
        {
            return Err(format!(
                "Conflicting default clocks on {}.{name}: {previous:?} and {bit:?}",
                core.name
            ));
        }
        core.clocks.interfaces.insert(name.clone(), bit);
        Ok(())
    }
}

impl PortSlice {
    /// Marks this one-bit slice as a clock and returns its handle. Repeating this is idempotent.
    pub fn declare_clock(&self) -> Result<ClockPin, String> {
        let pin = self.clock_pin()?;
        self.assign_clock_role(Declaration::ClockPin)?;
        Ok(pin)
    }

    /// Retrieves the handle for a previously declared clock bit.
    pub fn as_clock(&self) -> Result<ClockPin, String> {
        let pin = self.clock_pin()?;
        let module = self.get_mod_def();
        let core = module.core.read();
        if core
            .clocks
            .ports
            .get(self.port.name())
            .is_some_and(|declarations| {
                declarations.iter().any(|(range, decl)| {
                    range.contains(&self.lsb) && *decl == Declaration::ClockPin
                })
            })
        {
            Ok(pin)
        } else {
            Err(format!(
                "{}.{}[{}] is not a declared clock",
                core.name,
                self.port.name(),
                self.lsb
            ))
        }
    }

    /// Times this data slice against a clock declared on the same module.
    /// Returns an error if any bit already has a different explicit role.
    /// To replace an explicit timing assignment, clear it first.
    pub fn set_clock(&self, clock: impl Into<Clock>) -> Result<(), String> {
        let bit = clock.into().local_to(&self.get_mod_def())?;
        self.assign_clock_role(Declaration::TimedBy(bit))
    }

    /// Marks one independently asynchronous bit, overriding any inherited clock.
    pub fn set_async(&self) -> Result<(), String> {
        if self.width() != 1 {
            return Err(format!(
                "An independent asynchronous signal must be one bit: {self:?}"
            ));
        }
        self.assign_clock_role(Declaration::Async)
    }

    /// Excludes this slice from interface and module clock defaults.
    pub fn set_unclocked(&self) -> Result<(), String> {
        self.assign_clock_role(Declaration::Unclocked)
    }

    /// Clears explicit timing, async, and unclocked assignments on these bits.
    /// Data bits then inherit their interface or module default, or become unspecified.
    /// Clock-pin declarations survive. Repeated clears are harmless.
    pub fn clear_clock_association(&self) -> Result<(), String> {
        let Port::ModDef { name, .. } = &self.port else {
            return Err(format!(
                "Clock associations require a module-definition port: {self:?}"
            ));
        };
        let module = self.get_mod_def();
        let mut core = module.core.write();
        let Some(declarations) = core.clocks.ports.get_mut(name) else {
            return Ok(());
        };
        let cleared = self.lsb..self.msb + 1;
        for (range, role) in std::mem::take(declarations) {
            if role == Declaration::ClockPin
                || range.end <= cleared.start
                || cleared.end <= range.start
            {
                declarations.push((range, role));
                continue;
            }
            if range.start < cleared.start {
                declarations.push((range.start..cleared.start, role.clone()));
            }
            if cleared.end < range.end {
                declarations.push((cleared.end..range.end, role));
            }
        }
        if declarations.is_empty() {
            core.clocks.ports.remove(name);
        }
        Ok(())
    }

    /// Checks the shape and owner shared by clock declaration and lookup.
    fn clock_pin(&self) -> Result<ClockPin, String> {
        if self.width() != 1 || !matches!(self.port, Port::ModDef { .. }) {
            return Err(format!(
                "A clock must be a single module-definition port bit: {self:?}"
            ));
        }
        Ok(ClockPin(self.clone()))
    }

    /// Records the assignment only if it agrees with every explicit role it overlaps.
    fn assign_clock_role(&self, decl: Declaration) -> Result<(), String> {
        let Port::ModDef { name: port, .. } = &self.port else {
            return Err(format!(
                "Clock associations require a module-definition port: {self:?}"
            ));
        };
        let module = self.get_mod_def();
        let ModDefCore { name, clocks, .. } = &mut *module.core.write();
        let declarations = clocks.ports.entry(port.clone()).or_default();
        for (range, previous) in declarations.iter() {
            let lsb = self.lsb.max(range.start);
            let msb = self.msb.min(range.end - 1);
            if lsb > msb {
                continue;
            }
            if previous != &decl {
                return Err(format!(
                    "Conflicting clock assignments on {name}.{port}[{msb}:{lsb}]: {previous:?} and {decl:?}"
                ));
            }
            if lsb == self.lsb && msb == self.msb {
                return Ok(());
            }
        }
        declarations.push((self.lsb..self.msb + 1, decl));
        Ok(())
    }
}

impl Port {
    /// Declares a scalar clock port. For a clock bus, declare individual bits.
    pub fn declare_clock(&self) -> Result<ClockPin, String> {
        self.to_port_slice().declare_clock()
    }

    /// Retrieves the handle for a previously declared scalar clock.
    pub fn as_clock(&self) -> Result<ClockPin, String> {
        self.to_port_slice().as_clock()
    }

    /// Associates all data bits of this port with the given clock.
    /// A conflicting explicit role on any bit rejects the entire assignment.
    pub fn set_clock(&self, clock: impl Into<Clock>) -> Result<(), String> {
        self.to_port_slice().set_clock(clock)
    }

    /// Clears explicit data assignments across the port, preserving declared clock pins.
    pub fn clear_clock_association(&self) -> Result<(), String> {
        self.to_port_slice().clear_clock_association()
    }

    /// Marks a scalar port as independently asynchronous.
    pub fn set_async(&self) -> Result<(), String> {
        self.to_port_slice().set_async()
    }

    /// Excludes all bits of this port from interface and module clock defaults.
    pub fn set_unclocked(&self) -> Result<(), String> {
        self.to_port_slice().set_unclocked()
    }
}
