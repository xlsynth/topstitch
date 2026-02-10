// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use indexmap::IndexMap;
use regex::Regex;

use crate::util::concat_captures;
use crate::{Intf, ModInst, PipelineConfig};

impl Intf {
    /// Signals matching regex `pattern_a` on one interface are connected to
    /// signals matching regex `pattern_b` on the other interface, and vice
    /// versa. For example, suppose that this interface is `{"data_tx":
    /// "a_data_tx", "data_rx": "a_data_rx"}` and the other interface is
    /// `{"data_tx": "b_data_tx", "data_rx": "b_data_rx"}`. One might write
    /// this_intf.crossover(&other_intf, "(.*)_tx", "(.*)_rx") to connect the
    /// `data_tx` function on this interface (mapped to `a_data_tx`) to the
    /// `data_rx` function on the other interface (mapped to `b_data_rx`), and
    /// vice versa.
    pub fn crossover(&self, other: &Intf, pattern_a: impl AsRef<str>, pattern_b: impl AsRef<str>) {
        self.crossover_generic(other, pattern_a, pattern_b, None, None::<&[&str]>);
    }

    /// Places this interface across from another interface using a crossover
    /// matching pattern. Matched functions are found the same way as crossover;
    /// each pair is placed using place_across_from.
    pub fn place_crossover_from(
        &self,
        other: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
    ) {
        let self_port_slices = self.get_port_slices();
        let other_port_slices = other.get_port_slices();

        for (self_func_name, other_func_name) in
            find_crossover_matches(self, other, pattern_a, pattern_b)
        {
            let self_port = &self_port_slices[&self_func_name];
            let other_port = &other_port_slices[&other_func_name];
            self_port.place_across_from(other_port.clone());
        }
    }

    /// Connects this interface to another interface, skipping the specified functions.
    pub fn crossover_except<'a, I, T>(
        &self,
        other: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        skip: Option<I>,
    ) where
        I: IntoIterator<Item = &'a T>,
        T: AsRef<str> + 'a,
    {
        self.crossover_generic(other, pattern_a, pattern_b, None, skip);
    }

    pub fn crossover_pipeline(
        &self,
        other: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) {
        self.crossover_generic(other, pattern_a, pattern_b, Some(pipeline), None::<&[&str]>);
    }

    pub fn crossover_pipeline_except<'a, I, T>(
        &self,
        other: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        pipeline: PipelineConfig,
        skip: Option<I>,
    ) where
        I: IntoIterator<Item = &'a T>,
        T: AsRef<str> + 'a,
    {
        self.crossover_generic(other, pattern_a, pattern_b, Some(pipeline), skip);
    }

    fn crossover_generic<'a, I, T>(
        &self,
        other: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        pipeline: Option<PipelineConfig>,
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

        let x_port_slices = self.get_port_slices();
        let y_port_slices = other.get_port_slices();

        for (x_func_name, y_func_name) in find_crossover_matches(self, other, pattern_a, pattern_b)
        {
            if is_skipped(&x_func_name) || is_skipped(&y_func_name) {
                continue;
            }

            let x_port = &x_port_slices[&x_func_name];
            let y_port = &y_port_slices[&y_func_name];
            x_port.connect_generic(y_port, pipeline.clone());
        }
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this interface to another interface, using a
    /// crossover pattern. For example, one could have "^(.*)_tx$" and
    /// "^(.*)_rx$" as the patterns, and this would connect the "tx" signals
    /// on this interface to the "rx" signals on the other interface.
    pub fn crossover_through(
        &self,
        other: &Intf,
        through: &[&ModInst],
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        flipped_prefix: impl AsRef<str>,
        original_prefix: impl AsRef<str>,
    ) {
        let mut through_generic = Vec::new();
        for inst in through {
            through_generic.push((*inst, None));
        }
        self.crossover_through_generic(
            other,
            &through_generic,
            pattern_a,
            pattern_b,
            flipped_prefix,
            original_prefix,
        );
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this interface to another interface, using a
    /// crossover pattern. For example, one could have "^(.*)_tx$" and
    /// "^(.*)_rx$" as the patterns, and this would connect the "tx" signals
    /// on this interface to the "rx" signals on the other interface.
    /// Optional pipelining is used for each connection.
    pub fn crossover_through_generic(
        &self,
        other: &Intf,
        through: &[(&ModInst, Option<PipelineConfig>)],
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        flipped_prefix: impl AsRef<str>,
        original_prefix: impl AsRef<str>,
    ) {
        if through.is_empty() {
            self.crossover(other, pattern_a, pattern_b);
            return;
        }

        let matches = find_crossover_matches(self, other, pattern_a, pattern_b);
        let x_intf_port_slices = self.get_port_slices();
        let y_intf_port_slices = other.get_port_slices();

        for (x_func_name, y_func_name) in matches {
            let flipped_name = format!("{}_{}", flipped_prefix.as_ref(), y_func_name);
            let original_name = format!("{}_{}", original_prefix.as_ref(), x_func_name);
            for (i, (inst, pipeline)) in through.iter().enumerate() {
                x_intf_port_slices[&x_func_name].feedthrough_generic(
                    &inst.get_mod_def(),
                    &flipped_name,
                    &original_name,
                    pipeline.as_ref().cloned(),
                );

                if i == 0 {
                    x_intf_port_slices[&x_func_name].connect(&inst.get_port(&flipped_name));
                } else {
                    through[i - 1]
                        .0
                        .get_port(&original_name)
                        .connect(&inst.get_port(&flipped_name));
                }

                if i == through.len() - 1 {
                    y_intf_port_slices[&y_func_name].connect(&inst.get_port(&original_name));
                }
            }
        }
    }
}

pub fn find_crossover_matches(
    x: &Intf,
    y: &Intf,
    pattern_a: impl AsRef<str>,
    pattern_b: impl AsRef<str>,
) -> Vec<(String, String)> {
    let mut matches = Vec::new();

    let pattern_a_regex = Regex::new(pattern_a.as_ref()).unwrap();
    let pattern_b_regex = Regex::new(pattern_b.as_ref()).unwrap();

    let mut x_a_matches = IndexMap::new();
    let mut x_b_matches = IndexMap::new();
    let mut y_a_matches = IndexMap::new();
    let mut y_b_matches = IndexMap::new();

    const CONCAT_SEP: &str = "_";

    for (x_func_name, _) in x.get_port_slices() {
        if let Some(captures) = pattern_a_regex.captures(&x_func_name) {
            x_a_matches.insert(concat_captures(&captures, CONCAT_SEP), x_func_name);
        } else if let Some(captures) = pattern_b_regex.captures(&x_func_name) {
            x_b_matches.insert(concat_captures(&captures, CONCAT_SEP), x_func_name);
        }
    }

    for (y_func_name, _) in y.get_port_slices() {
        if let Some(captures) = pattern_a_regex.captures(&y_func_name) {
            y_a_matches.insert(concat_captures(&captures, CONCAT_SEP), y_func_name);
        } else if let Some(captures) = pattern_b_regex.captures(&y_func_name) {
            y_b_matches.insert(concat_captures(&captures, CONCAT_SEP), y_func_name);
        }
    }

    for (key, x_func_name) in x_a_matches {
        if let Some(y_func_name) = y_b_matches.get(&key) {
            matches.push((x_func_name, y_func_name.clone()));
        }
    }

    for (key, x_func_name) in x_b_matches {
        if let Some(y_func_name) = y_a_matches.get(&key) {
            matches.push((x_func_name, y_func_name.clone()));
        }
    }

    matches
}
