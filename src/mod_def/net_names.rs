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

/// Checks every component of an SV package-qualified name used in emitted RTL.
pub(crate) fn check_qualified_identifier(
    name: &str,
    role: &str,
    module: &str,
) -> Result<(), String> {
    for component in name.split("::") {
        check_identifier(component, role, module)?;
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

    /// Reserves parameter declaration names before allocating module wires.
    pub(super) fn reserve_declarations(
        &mut self,
        names: &[String],
        module: &str,
    ) -> Result<(), String> {
        for name in names {
            if !self.used.insert(name.clone()) {
                return Err(format!(
                    "Name collision in module {module}: parameter {name} conflicts with a port, instance, or requested wire"
                ));
            }
        }
        Ok(())
    }

    /// Reserves a generated name, returning an alternative if the base is occupied or a keyword.
    pub(super) fn reserve_generated(&mut self, base: &str) -> Option<String> {
        if !xlsynth_vast::is_system_verilog_keyword(base) && self.used.insert(base.to_owned()) {
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
