// SPDX-License-Identifier: Apache-2.0

use crate::pipeline::PipelinePreflight;
use std::collections::HashSet;

use crate::{ConvertibleToPortSliceVec, Intf, ModInst, PhysicalPin, PipelineConfig, PortSlice};

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
        self.connect_generic(other, None, allow_mismatch, None::<&[&str]>);
    }

    /// Jam-connects this interface and `other` LSB-first over the ordered
    /// `PortSlice` lists and marks any remainder on either side with
    /// `unused_or_tieoff(0)`.
    pub fn todo_jam_connect<T: ConvertibleToPortSliceVec>(&self, other: &T) {
        let left = self.to_port_slice_vec();
        let right = other.to_port_slice_vec();
        PortSlice::todo_jam_connect_port_slices(&left, &right);
    }

    /// Connects this interface to another interface, skipping the specified functions.
    pub fn connect_except<'a, I, T>(&self, other: &Intf, skip: Option<I>)
    where
        I: IntoIterator<Item = &'a T>,
        T: AsRef<str> + 'a,
    {
        self.connect_generic(other, None, false, skip);
    }

    /// Places this interface across from another interface, matching functions
    pub fn place_across_from(&self, other: &Intf, allow_mismatch: bool) {
        self.place_across_from_generic(other, allow_mismatch, None::<&[&str]>);
    }

    /// Places this interface across from another interface, matching functions
    /// by name, skipping the specified functions.
    pub fn place_across_from_except<'a, I, T>(
        &self,
        other: &Intf,
        allow_mismatch: bool,
        skip: Option<I>,
    ) where
        I: IntoIterator<Item = &'a T>,
        T: AsRef<str> + 'a,
    {
        self.place_across_from_generic(other, allow_mismatch, skip);
    }

    /// Places this interface across from another interface, matching functions
    /// by name. See [`PortSlice::place_across_from`] for placement behavior.
    fn place_across_from_generic<'a, I, T>(
        &self,
        other: &Intf,
        allow_mismatch: bool,
        skip: Option<I>,
    ) where
        I: IntoIterator<Item = &'a T>,
        T: AsRef<str> + 'a,
    {
        let skip_names = skip.map(|i| i.into_iter().map(|i| i.as_ref()).collect::<HashSet<_>>());

        let is_skipped = |func_name: &str| {
            if let Some(names) = skip_names.as_ref() {
                names.contains(func_name)
            } else {
                false
            }
        };

        let self_ports = self.get_port_slices();
        let other_ports = other.get_port_slices();

        for (func_name, self_port) in &self_ports {
            if is_skipped(func_name) {
                continue;
            }

            if let Some(other_port) = other_ports.get(func_name) {
                self_port.place_across_from(other_port.clone());
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
                if is_skipped(func_name) {
                    continue;
                }

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

    /// Places this interface across from ModDef ports it is directly connected
    /// to within the same module.
    pub fn place_across(&self) {
        for (_, port_slice) in self.get_port_slices() {
            port_slice.place_across();
        }
    }

    /// Places this interface based on what has been connected to it.
    pub fn place_abutted(&self) {
        for (_, port_slice) in self.get_port_slices() {
            port_slice.place_abutted();
        }
    }

    /// For each port slice in this interface, trace its connectivity to
    /// determine what existing pin it is connected to, and then place a new
    /// pin for the slice that overlaps the connected pin.
    pub fn place_overlapped(&self, pin: &PhysicalPin) {
        for (_, port_slice) in self.get_port_slices() {
            port_slice.place_overlapped(pin);
        }
    }

    pub fn connect_pipeline(&self, other: &Intf, pipeline: PipelineConfig, allow_mismatch: bool) {
        self.connect_generic(other, Some(pipeline), allow_mismatch, None::<&[&str]>);
    }

    pub fn connect_pipeline_except<'a, I, T>(
        &self,
        other: &Intf,
        pipeline: PipelineConfig,
        skip: Option<I>,
    ) where
        I: IntoIterator<Item = &'a T>,
        T: AsRef<str> + 'a,
    {
        self.connect_generic(other, Some(pipeline), false, skip);
    }

    pub(crate) fn connect_generic<'a, I, T>(
        &self,
        other: &Intf,
        pipeline: Option<PipelineConfig>,
        allow_mismatch: bool,
        skip: Option<I>,
    ) where
        I: IntoIterator<Item = &'a T>,
        T: AsRef<str> + 'a,
    {
        let skip_names = skip.map(|i| i.into_iter().map(|i| i.as_ref()).collect::<HashSet<_>>());

        let is_skipped = |func_name: &str| {
            if let Some(names) = skip_names.as_ref() {
                names.contains(func_name)
            } else {
                false
            }
        };

        let self_ports = self.get_port_slices();
        let other_ports = other.get_port_slices();

        if !allow_mismatch {
            for (source, target) in [(&self_ports, &other_ports), (&other_ports, &self_ports)] {
                for name in source.keys() {
                    assert!(
                        is_skipped(name) || target.contains_key(name),
                        "Interfaces {} and {} have mismatched function {name}",
                        self.debug_string(),
                        other.debug_string()
                    );
                }
            }
        }
        let pairs = self_ports
            .iter()
            .filter(|(name, _)| !is_skipped(name))
            .filter_map(|(name, port)| Some((port, other_ports.get(name)?)));
        if let Some(p) = &pipeline {
            p.preflight_connections(pairs.clone());
        }
        for (source, target) in pairs {
            source.connect_generic(target, pipeline.clone());
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

        ModInst::check_feedthrough_wiring(
            [self.get_mod_def_core(), other.get_mod_def_core()],
            through,
        );
        let flipped = format!("{}_flipped_{}", prefix.as_ref(), self.get_intf_name());
        let original = format!("{}_original_{}", prefix.as_ref(), self.get_intf_name());

        if through.iter().any(|(_, pipeline)| pipeline.is_some()) {
            let ports = self.get_port_slices();
            let other_ports = other.get_port_slices();
            if !allow_mismatch {
                assert!(
                    ports.len() == other_ports.len()
                        && ports.keys().all(|name| other_ports.contains_key(name)),
                    "Interfaces {} and {} have mismatched functions",
                    self.debug_string(),
                    other.debug_string()
                );
            }
            for (name, port) in &ports {
                port.check_route(other_ports.get(name), through);
            }
            let mut plan = PipelinePreflight::default();
            for (inst, pipeline) in through {
                plan.interface(
                    &inst.get_mod_def(),
                    &ports,
                    &flipped,
                    &original,
                    pipeline.as_ref(),
                );
            }
        }
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
