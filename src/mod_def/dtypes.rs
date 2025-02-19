// SPDX-License-Identifier: Apache-2.0

use crate::{PipelineConfig, PortSlice};

pub(crate) struct VerilogImport {
    pub(crate) sources: Vec<String>,
    pub(crate) incdirs: Vec<String>,
    pub(crate) defines: Vec<(String, String)>,
    pub(crate) skip_unsupported: bool,
    pub(crate) ignore_unknown_modules: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Assignment {
    pub lhs: PortSlice,
    pub rhs: PortSlice,
    pub pipeline: Option<PipelineConfig>,
}

#[derive(Clone)]
pub(crate) struct InstConnection {
    pub(crate) inst_port_slice: PortSlice,
    pub(crate) connected_to: PortSliceOrWire,
}

#[derive(Clone)]
pub(crate) struct Wire {
    pub(crate) name: String,
    pub(crate) width: usize,
}

#[derive(Clone)]
pub(crate) enum PortSliceOrWire {
    PortSlice(PortSlice),
    Wire(Wire),
}
