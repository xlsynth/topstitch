// SPDX-License-Identifier: Apache-2.0

use super::PortSliceConnections;
use crate::{ConnectedItem, IO, Port, PortSlice, PortSliceConnection, Wire};

/// The electrical source of one interval and the endpoints available for naming its net.
/// A bidirectional net can have no single driver.
pub(crate) enum ResolvedNet<'a> {
    /// A constant or an explicit unused marker.
    Terminal(&'a ConnectedItem),
    Ports {
        driver: Option<&'a PortSlice>,
        module_alias: Option<&'a PortSlice>,
        instance_inouts: Vec<&'a PortSlice>,
        wire: Option<&'a Wire>,
    },
}

impl PortSliceConnections {
    /// Resolves, selects and coalesces a whole port's sources in MSB-first order.
    pub(crate) fn resolved_sources(
        &self,
        io: &IO,
        name: &str,
        select: impl Fn(ResolvedNet<'_>) -> Option<ConnectedItem>,
    ) -> Result<Vec<PortSliceConnection>, String> {
        let mut sources: Vec<PortSliceConnection> = Vec::new();
        for chunk in self.resolved_chunks(io, name)? {
            let this = &chunk[0].this;
            let other = select(chunk.resolve_net()?)
                .ok_or_else(|| format!("{} requires a driving value", this.debug_string()))?;
            if let Some(previous) = sources.last_mut()
                && let Some(merged_this) = previous.this.try_merge(this)
                && let Some(merged_other) = previous.other.try_merge(&other)
            {
                previous.this = merged_this;
                previous.other = merged_other;
            } else {
                sources.push(PortSliceConnection {
                    this: this.clone(),
                    other,
                });
            }
        }
        Ok(sources)
    }

    /// Validates one nonempty, traced interval, including the port at which tracing started.
    pub(crate) fn resolve_net(&self) -> Result<ResolvedNet<'_>, String> {
        let this = &self[0].this;
        let name = || this.debug_string();
        let mut terminal = None;
        let mut wire = None;
        for connection in self {
            match &connection.other {
                ConnectedItem::PortSlice(_) => {}
                ConnectedItem::Wire(w) => {
                    if wire.replace(w).is_some() {
                        return Err(format!("Multiple wire connections found for {}", name()));
                    }
                }
                item @ (ConnectedItem::Tieoff(_) | ConnectedItem::Unused(_)) => {
                    if let Some(previous) = terminal.replace(item) {
                        return Err(match (previous, item) {
                            (ConnectedItem::Tieoff(_), ConnectedItem::Tieoff(_)) => {
                                format!("{} has been tied off multiple times", name())
                            }
                            (ConnectedItem::Unused(_), ConnectedItem::Unused(_)) => {
                                format!("{} has been marked as unused multiple times", name())
                            }
                            _ => format!("{} is unused, so it cannot also be tied off", name()),
                        });
                    }
                }
            }
        }

        if let Some(item @ ConnectedItem::Unused(_)) = terminal {
            if self.len() != 1 {
                return Err(format!(
                    "{} is unused, so it cannot also be connected",
                    name()
                ));
            }
            if !this.port.is_driver() && !matches!(this.port.io(), IO::InOut(_)) {
                return Err(format!(
                    "{} cannot be marked as unused because it has an incompatible directionality",
                    name()
                ));
            }
            return Ok(ResolvedNet::Terminal(item));
        }

        let endpoints = std::iter::once(this).chain(self.into_iter().filter_map(|c| {
            if let ConnectedItem::PortSlice(p) = &c.other {
                Some(p)
            } else {
                None
            }
        }));
        if let Some(item) = terminal {
            if wire.is_some() {
                return Err(format!(
                    "{} is tied off, so it cannot also be connected to a wire",
                    name()
                ));
            }
            if let Some(driver) = endpoints.into_iter().find(|p| p.port.is_driver()) {
                if self.len() == 1 && driver == this {
                    return Err(format!(
                        "{} has the wrong directionality to be tied off",
                        name()
                    ));
                }
                return Err(format!(
                    "{} is multiply driven by a tieoff and {}",
                    name(),
                    driver.debug_string()
                ));
            }
            return Ok(ResolvedNet::Terminal(item));
        }

        let mut driver: Option<&PortSlice> = None;
        let mut module_outputs = Vec::new();
        let mut module_inout: Option<&PortSlice> = None;
        let mut instance_inouts = Vec::new();
        for port in endpoints {
            match (&port.port, port.port.io()) {
                (Port::ModDef { .. }, IO::Input(_)) | (Port::ModInst { .. }, IO::Output(_)) => {
                    if let Some(previous) = driver.replace(port) {
                        return Err(format!(
                            "{} is multiply driven by {} and {}",
                            name(),
                            previous.debug_string(),
                            port.debug_string()
                        ));
                    }
                }
                (Port::ModDef { .. }, IO::Output(_)) => module_outputs.push(port),
                (Port::ModDef { .. }, IO::InOut(_)) => {
                    if let Some(previous) = module_inout.replace(port) {
                        return Err(format!(
                            "{} is connected to multiple ModDef InOut ports: {} and {}",
                            name(),
                            previous.debug_string(),
                            port.debug_string()
                        ));
                    }
                }
                (Port::ModInst { .. }, IO::InOut(_)) => instance_inouts.push(port),
                (Port::ModInst { .. }, IO::Input(_)) => {}
            }
        }

        let module_input = driver.is_some_and(|p| matches!(p.port, Port::ModDef { .. }));
        if module_inout.is_some() && module_input {
            return Err(format!(
                "Cannot have both ModDef Input and ModDef InOut for {}",
                name()
            ));
        }
        if module_inout.is_some() && !module_outputs.is_empty() {
            return Err(format!(
                "Cannot have both ModDef Outputs and ModDef InOuts for {}",
                name()
            ));
        }
        if driver.is_none() && module_inout.is_none() && instance_inouts.is_empty() {
            return Err(format!("No driver found for {}", name()));
        }
        if wire.is_some() && module_input {
            return Err(format!(
                "A wire cannot be attached to {} because it is a ModDef Input",
                name()
            ));
        }
        // A sole module output (or inout) can itself be used as the net.
        let module_alias = match module_outputs.as_slice() {
            [only] => Some(*only),
            _ => module_inout,
        };
        Ok(ResolvedNet::Ports {
            driver,
            module_alias,
            instance_inouts,
            wire,
        })
    }
}

impl ResolvedNet<'_> {
    /// Selects a requested net name at a sink, retaining the wire's original bit coordinates.
    #[cfg(feature = "block-ir")]
    pub(crate) fn named_driving_value(self) -> Option<ConnectedItem> {
        match self {
            Self::Ports {
                wire: Some(wire), ..
            } => Some(wire.clone().into()),
            net => net.electrical_value(),
        }
    }

    /// Samples an inout net through one of its bidirectional endpoints, or selects its driver.
    #[cfg(feature = "block-ir")]
    pub(crate) fn electrical_value(self) -> Option<ConnectedItem> {
        match self {
            Self::Ports {
                module_alias,
                instance_inouts,
                driver,
                ..
            } => module_alias
                .filter(|p| matches!(p.port.io(), IO::InOut(_)))
                .or_else(|| instance_inouts.first().copied())
                .or(driver)
                .cloned()
                .map(Into::into),
            Self::Terminal(item) => Some(item.clone()),
        }
    }

    /// Returns the value consumed by directed connections, preserving explicit unused markers.
    #[cfg(feature = "block-ir")]
    pub(crate) fn driving_value(self) -> Option<ConnectedItem> {
        match self {
            Self::Terminal(item) => Some(item.clone()),
            Self::Ports { driver, .. } => driver.cloned().map(Into::into),
        }
    }
}
