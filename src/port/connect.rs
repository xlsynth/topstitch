// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToPortSlice, ConvertibleToPortSliceVec, ModInst, PipelineConfig, Port};

impl Port {
    /// Connects this port to a net with a specific name.
    ///
    /// The requested name takes precedence over generated wire names and must be distinct
    /// from ports, instances and other explicitly named wires in the containing module.
    /// Panics if the name does not meet the identifier rules of [`crate::ModDef::emit`].
    pub fn specify_net_name(&self, net: &str) {
        self.to_port_slice().specify_net_name(net);
    }

    pub fn set_max_distance(&self, max_distance: Option<i64>) {
        self.to_port_slice().set_max_distance(max_distance);
    }

    /// Connects this port to another port or port slice.
    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T) {
        self.to_port_slice().connect(other);
    }

    pub fn connect_pipeline<T: ConvertibleToPortSlice>(&self, other: &T, pipeline: PipelineConfig) {
        self.connect_generic(other, Some(pipeline));
    }

    /// Jam-connects this port and `other` LSB-first and marks any remainder on
    /// either side with `unused_or_tieoff(0)`.
    pub fn todo_jam_connect<T: ConvertibleToPortSliceVec>(&self, other: &T) {
        self.to_port_slice().todo_jam_connect(other);
    }

    pub(crate) fn connect_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        pipeline: Option<PipelineConfig>,
    ) {
        self.to_port_slice().connect_generic(other, pipeline);
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this port to another port or port slice. Each hop retains this port's packed layout.
    pub fn connect_through<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        through: &[&ModInst],
        prefix: impl AsRef<str>,
    ) {
        let hops: Vec<_> = through.iter().map(|&inst| (inst, None)).collect();
        self.connect_through_generic(other, &hops, prefix);
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this port to another port or port slice, with
    /// optional pipelining for each connection. Each hop retains this port's packed layout.
    pub fn connect_through_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        through: &[(&ModInst, Option<PipelineConfig>)],
        prefix: impl AsRef<str>,
    ) {
        self.to_port_slice().connect_through_with_decl(
            other,
            through,
            prefix,
            self.decl_for_copy(),
        );
    }
}
