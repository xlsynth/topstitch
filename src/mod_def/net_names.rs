// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use super::ModDefCore;

/// Checks the common identifier syntax and reserved words of both RTL backends.
pub(crate) fn check_identifier(name: &str, role: &str, module: &str) -> Result<(), String> {
    // Escaping a SystemVerilog name does not change the identifier it denotes.
    let identifier = name.trim_end().strip_prefix('\\').unwrap_or(name);
    let language = if xlsynth_vast::is_system_verilog_keyword(identifier) {
        "SystemVerilog"
    } else if is_ir_keyword(identifier) {
        "Block IR"
    } else {
        let mut chars = name.chars();
        if chars
            .next()
            .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
            && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        {
            return Ok(());
        }
        return Err(format!(
            "{role} {name:?} in module {module} is not a valid SV/Block IR identifier (expected [A-Za-z_][A-Za-z0-9_]*)"
        ));
    };
    Err(format!(
        "{role} {name:?} in module {module} is a reserved {language} keyword"
    ))
}

/// XLS scanner keywords and boolean literals. The direct SV emitter also builds on stable Rust.
fn is_ir_keyword(name: &str) -> bool {
    matches!(
        name,
        "fn" | "bits"
            | "token"
            | "ret"
            | "package"
            | "proc"
            | "chan"
            | "chan_interface"
            | "reg"
            | "next"
            | "block"
            | "clock"
            | "instantiation"
            | "top"
            | "file_number"
            | "proc_instantiation"
            | "scheduled_proc"
            | "scheduled_fn"
            | "stage"
            | "scheduled_block"
            | "source"
            | "controlled_stage"
            | "active_inputs_valid"
            | "active_outputs_ready"
            | "true"
            | "false"
    )
}

/// SV ports, instances, and explicitly named wires own their names before generated wires.
pub(super) struct NetNames {
    used: HashSet<String>,
}

impl NetNames {
    /// Reserves the fixed names in a module before either emitter visits its connections.
    pub(super) fn new(core: &ModDefCore) -> Result<Self, String> {
        let mut used: HashSet<_> = core
            .ports
            .keys()
            .chain(core.instances.keys())
            .cloned()
            .collect();
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

    /// Reserves a generated name, returning an alternative only when the base is taken.
    pub(super) fn reserve_generated(&mut self, base: &str) -> Option<String> {
        if !is_ir_keyword(base)
            && !xlsynth_vast::is_system_verilog_keyword(base)
            && self.used.insert(base.to_owned())
        {
            return None;
        }
        (1..)
            .map(|i| format!("{base}__{i}"))
            .find(|name| self.used.insert(name.clone()))
    }
}
