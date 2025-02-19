// SPDX-License-Identifier: Apache-2.0

use crate::{Intf, ModInst, PipelineConfig};

impl Intf {
    /// Connects this interface to another interface. Interfaces are connected
    /// by matching up ports with the same function name and connecting them.
    /// For example, if this interface is {"data": "a_data", "valid": "a_valid"}
    /// and the other interface is {"data": "b_data", "valid": "b_valid"}, then
    /// "a_data" will be connected to "b_data" and "a_valid" will be connected
    /// to "b_valid".
    ///
    /// Unless `allow_mismatch` is `true`, this method will panic if a function
    /// in this interface is not in the other interface. Continuing the previous
    /// example, if this interface also contained function "ready", but the
    /// other interface did not, this method would panic unless `allow_mismatch`
    /// was `true`.
    pub fn connect(&self, other: &Intf, allow_mismatch: bool) {
        self.connect_generic(other, None, allow_mismatch);
    }
    pub fn connect_pipeline(&self, other: &Intf, pipeline: PipelineConfig, allow_mismatch: bool) {
        self.connect_generic(other, Some(pipeline), allow_mismatch);
    }

    pub(crate) fn connect_generic(
        &self,
        other: &Intf,
        pipeline: Option<PipelineConfig>,
        allow_mismatch: bool,
    ) {
        let self_ports = self.get_port_slices();
        let other_ports = other.get_port_slices();

        for (func_name, self_port) in &self_ports {
            if let Some(other_port) = other_ports.get(func_name) {
                self_port.connect_generic(other_port, pipeline.clone());
            } else if !allow_mismatch {
                panic!(
                    "Interfaces {} and {} have mismatched functions and allow_mismatch is false. Example: function '{}' is present in {} but not in {}.",
                    self.debug_string(),
                    other.debug_string(),
                    func_name,
                    self.debug_string(),
                    other.debug_string()
                );
            }
        }

        if !allow_mismatch {
            for (func_name, _) in &other_ports {
                if !self_ports.contains_key(func_name) {
                    panic!(
                        "Interfaces {} and {} have mismatched functions and allow_mismatch is false. Example: function '{}' is present in {} but not in {}",
                        self.debug_string(),
                        other.debug_string(),
                        func_name,
                        other.debug_string(),
                        self.debug_string()
                    );
                }
            }
        }
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this interface to another interface.
    pub fn connect_through(
        &self,
        other: &Intf,
        through: &[&ModInst],
        prefix: impl AsRef<str>,
        allow_mismatch: bool,
    ) {
        let mut through_generic = Vec::new();
        for inst in through {
            through_generic.push((*inst, None));
        }
        self.connect_through_generic(other, &through_generic, prefix, allow_mismatch);
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this interface to another interface, with
    /// optional pipelining for each connection.
    pub fn connect_through_generic(
        &self,
        other: &Intf,
        through: &[(&ModInst, Option<PipelineConfig>)],
        prefix: impl AsRef<str>,
        allow_mismatch: bool,
    ) {
        if through.is_empty() {
            self.connect(other, allow_mismatch);
            return;
        }

        let flipped = format!("{}_flipped_{}", prefix.as_ref(), self.get_intf_name());
        let original = format!("{}_original_{}", prefix.as_ref(), self.get_intf_name());

        for (i, (inst, pipeline)) in through.iter().enumerate() {
            self.feedthrough_generic(
                &inst.get_mod_def(),
                &flipped,
                &original,
                pipeline.as_ref().cloned(),
            );
            if i == 0 {
                self.connect(&inst.get_intf(&flipped), false);
            } else {
                through[i - 1]
                    .0
                    .get_intf(&original)
                    .connect(&inst.get_intf(&flipped), false);
            }

            if i == through.len() - 1 {
                other.connect(&inst.get_intf(&original), allow_mismatch);
            }
        }
    }
}
