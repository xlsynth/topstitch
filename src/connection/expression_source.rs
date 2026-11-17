// SPDX-License-Identifier: Apache-2.0

use super::PortSliceConnections;
use super::resolve::ResolvedNet;
use crate::{ConnectedItem, Port, PortSliceConnection};

impl PortSliceConnections {
    /// Returns the SV expression for a traced interval, including a net-name override if present.
    pub fn to_expression_source(&self) -> Option<PortSliceConnection> {
        if self.is_empty() {
            return None;
        }
        let other = self
            .resolve_net()
            .unwrap_or_else(|e| panic!("{e}"))
            .sv_expression()?;
        Some(PortSliceConnection {
            this: self[0].this.clone(),
            other,
        })
    }
}

impl ResolvedNet<'_> {
    /// Prefers an explicit wire or module port over an intermediate instance wire.
    pub(crate) fn sv_expression(self) -> Option<ConnectedItem> {
        match self {
            Self::Terminal(item) => Some(item.clone()),
            Self::Ports {
                driver,
                module_alias,
                instance_inouts,
                wire,
            } => {
                if let Some(wire) = wire {
                    return Some(wire.clone().into());
                }
                let module_input = driver.filter(|p| matches!(p.port, Port::ModDef { .. }));
                // With only instance inouts, all endpoints must choose the same wire.
                module_input
                    .or(module_alias)
                    .or(driver)
                    .or_else(|| {
                        instance_inouts
                            .into_iter()
                            .min_by_key(|p| (p.get_inst_name(), p.port.name(), p.msb, p.lsb))
                    })
                    .cloned()
                    .map(Into::into)
            }
        }
    }
}
