// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

use super::net_names::{check_identifier, check_qualified_identifier};
use crate::port::PortType;
use crate::{ModDef, Usage};

impl ModDef {
    /// TODO(sherbst) 2025-10-27: Remove this.
    pub fn validate(&self) {}

    /// Checks declarations and instance references down to each external interface.
    pub(super) fn check_emission_names(&self) -> Result<(), String> {
        let mut pending = vec![self.core.clone()];
        let mut visited = HashSet::new();
        while let Some(module) = pending.pop() {
            if !visited.insert(Arc::as_ptr(&module)) {
                continue;
            }
            let core = module.read();
            check_identifier(&core.name, "module name", &core.name)?;
            for (port, decl) in &core.ports {
                check_identifier(port, "port name", &core.name)?;
                if let PortType::EnumInputCast(ty) = &decl.ty {
                    check_qualified_identifier(ty, "port type", &core.name)?;
                }
            }
            for (parameter, binding) in &core.parameters {
                check_identifier(parameter, "parameter name", &core.name)?;
                if let Some(ty) = &binding.sv_type {
                    check_qualified_identifier(ty, "parameter cast type", &core.name)?;
                }
            }
            if core.usage == Usage::EmitDefinitionAndDescend {
                for wire in &core.specified_net_names {
                    check_identifier(wire, "net name", &core.name)?;
                }
                for (name, child) in &core.instances {
                    check_identifier(name, "instance name", &core.name)?;
                    pending.push(child.clone());
                }
            }
        }
        Ok(())
    }
}
