// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use super::ModDefCore;

/// Checks the spelling of a user-supplied name that will appear in emitted RTL.
pub(crate) fn check_identifier(name: &str, role: &str, module: &str) -> Result<(), String> {
    let mut bytes = name.bytes();
    if !bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(format!(
            "{role} {name:?} in module {module} must be an unescaped identifier matching [A-Za-z_][A-Za-z0-9_]*"
        ));
    }
    if xlsynth_vast::is_system_verilog_keyword(name) {
        return Err(format!(
            "{role} {name:?} in module {module} is a reserved SystemVerilog keyword"
        ));
    }
    Ok(())
}

/// Checks that a name can appear in both the Block IR package and its generated SystemVerilog.
#[cfg(feature = "block-ir")]
pub(super) fn check_block_ir_identifier(
    name: &str,
    role: &str,
    module: &str,
) -> Result<(), String> {
    check_identifier(name, role, module)?;
    if !xlsynth_pir::ir_builder::is_valid_identifier(name) {
        return Err(format!(
            "{role} {name:?} in module {module} is a reserved Block IR keyword"
        ));
    }
    Ok(())
}

/// SV ports, instances, and explicitly named wires own their names before generated wires.
pub(super) struct NetNames {
    used: HashSet<String>,
}

impl NetNames {
    /// Reserves the fixed names in a module before emitting its connections.
    pub(super) fn new(core: &ModDefCore) -> Result<Self, String> {
        let mut used: HashSet<_> = core.ports.keys().cloned().collect();
        for name in core.instances.keys() {
            if !used.insert(name.clone()) {
                return Err(format!(
                    "Name collision in module {}: instance {name} conflicts with a port.",
                    core.name
                ));
            }
        }
        for name in &core.specified_net_names {
            if !used.insert(name.clone()) {
                return Err(format!(
                    "Net name collision: requested wire name {name} collides with a port or instance in {}",
                    core.name
                ));
            }
        }
        Ok(Self { used })
    }

    /// Reserves a generated name, returning an alternative if the base is occupied or a keyword.
    pub(super) fn reserve_generated(&mut self, base: &str) -> Option<String> {
        self.reserve_generated_name(base, xlsynth_vast::is_system_verilog_keyword)
    }

    /// Also avoids keywords reserved by the IR scanner before generating SystemVerilog.
    #[cfg(feature = "block-ir")]
    pub(super) fn reserve_block_ir_generated(&mut self, base: &str) -> Option<String> {
        self.reserve_generated_name(base, |name| {
            !xlsynth_pir::ir_builder::is_valid_identifier(name)
                || xlsynth_vast::is_system_verilog_keyword(name)
        })
    }

    /// Reserves the base or the first free suffixed name for a generated wire.
    fn reserve_generated_name(
        &mut self,
        base: &str,
        is_keyword: impl FnOnce(&str) -> bool,
    ) -> Option<String> {
        if !is_keyword(base) && self.used.insert(base.to_owned()) {
            return None;
        }
        (1..)
            .map(|i| format!("{base}__{i}"))
            .find(|name| self.used.insert(name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use crate::{EmitOptions, IO, ModDef};

    #[test]
    #[should_panic(expected = "instance occupied conflicts with a port")]
    fn emission_rejects_collisions_that_bypass_construction_checks() {
        let top = ModDef::new("Top");
        top.add_port("occupied", IO::Input(1)).unused();
        let leaf = ModDef::new("Leaf");
        top.core
            .write()
            .instances
            .insert("occupied".to_string(), leaf.core.clone());
        top.emit(EmitOptions::default());
    }
}
