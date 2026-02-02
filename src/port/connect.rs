// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToPortSlice, ModInst, PipelineConfig, Port};

impl Port {
    /// Connects this port to a net with a specific name.
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

    pub(crate) fn connect_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        pipeline: Option<PipelineConfig>,
    ) {
        self.to_port_slice().connect_generic(other, pipeline);
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this port to another port or port slice.
    pub fn connect_through<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        through: &[&ModInst],
        prefix: impl AsRef<str>,
    ) {
        self.to_port_slice().connect_through(other, through, prefix);
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this port to another port or port slice, with
    /// optional pipelining for each connection.
    pub fn connect_through_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        through: &[(&ModInst, Option<PipelineConfig>)],
        prefix: impl AsRef<str>,
    ) {
        self.to_port_slice()
            .connect_through_generic(other, through, prefix);
    }
}
