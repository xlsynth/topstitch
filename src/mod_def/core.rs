// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use indexmap::IndexMap;
use num_bigint::BigInt;

use crate::mod_def::dtypes::VerilogImport;

pub(crate) use crate::mod_def::{Assignment, InstConnection, Wire};
use crate::{PortSlice, Usage, IO};

/// Data structure representing a module definition.
///
/// Contains the module's name, ports, interfaces, instances, etc. Not intended
/// to be used directly; use `ModDef` instead, which contains a smart pointer to
/// this struct.
pub struct ModDefCore {
    pub(crate) name: String,
    pub(crate) ports: IndexMap<String, IO>,
    pub(crate) interfaces: IndexMap<String, IndexMap<String, (String, usize, usize)>>,
    pub(crate) instances: IndexMap<String, Rc<RefCell<ModDefCore>>>,
    pub(crate) usage: Usage,
    pub(crate) generated_verilog: Option<String>,
    pub(crate) verilog_import: Option<VerilogImport>,
    pub(crate) assignments: Vec<Assignment>,
    pub(crate) unused: Vec<PortSlice>,
    pub(crate) tieoffs: Vec<(PortSlice, BigInt)>,
    pub(crate) whole_port_tieoffs: IndexMap<String, IndexMap<String, BigInt>>,
    pub(crate) whole_port_unused: IndexMap<String, HashSet<String>>,
    pub(crate) inst_connections: IndexMap<String, IndexMap<String, Vec<InstConnection>>>,
    pub(crate) reserved_net_definitions: IndexMap<String, Wire>,
    pub(crate) enum_ports: IndexMap<String, String>,
    pub(crate) adjacency_matrix: HashMap<String, HashSet<String>>,
    pub(crate) ignore_adjacency: HashSet<String>,
}
