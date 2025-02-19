// SPDX-License-Identifier: Apache-2.0

use crate::{ConvertibleToPortSlice, ModInst, PipelineConfig, Port};

impl Port {
    /// Connects this port to a net with a specific name.
    pub fn connect_to_net(&self, net: &str) {
        self.to_port_slice().connect_to_net(net);
    }

    /// Connects this port to another port or port slice.
    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T) {
        self.connect_generic(other, None);
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
