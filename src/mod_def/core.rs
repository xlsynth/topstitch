// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::mod_def::dtypes::{PhysicalPin, VerilogImport};
use crate::mod_def::tracks::{TrackDefinitions, TrackOccupancies};

use crate::connection::PortSliceConnections;
use crate::{Usage, IO};

type PhysicalPinMap = IndexMap<String, Vec<Option<PhysicalPin>>>;

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
    pub(crate) mod_inst_connections:
        IndexMap<String, IndexMap<String, Rc<RefCell<PortSliceConnections>>>>,
    pub(crate) mod_def_connections: IndexMap<String, Rc<RefCell<PortSliceConnections>>>,
    pub(crate) enum_ports: IndexMap<String, String>,
    pub(crate) adjacency_matrix: HashMap<String, HashSet<String>>,
    pub(crate) ignore_adjacency: HashSet<String>,
    pub(crate) shape: Option<crate::mod_def::dtypes::Polygon>,
    pub(crate) layer: Option<String>,
    pub(crate) inst_placements: IndexMap<String, crate::mod_def::dtypes::Placement>,
    pub(crate) physical_pins: PhysicalPinMap,
    pub(crate) track_definitions: Option<TrackDefinitions>,
    pub(crate) track_occupancies: Option<TrackOccupancies>,
    /// Set of net names explicitly specified via `specify_net_name` within
    /// this module definition. Used to detect duplicate specifications and to
    /// check for name collisions during emission.
    pub(crate) specified_net_names: HashSet<String>,
}

impl ModDefCore {
    pub fn get_physical_pin(&self, port_name: &str, bit: usize) -> PhysicalPin {
        let pins = self.physical_pins.get(port_name).unwrap_or_else(|| {
            panic!(
                "Physical pins for port {}.{} not defined",
                self.name, port_name
            )
        });

        if bit >= pins.len() {
            panic!(
                "Bit {} out of range for port {}.{} (width {})",
                bit,
                self.name,
                port_name,
                pins.len()
            );
        }

        pins[bit]
            .as_ref()
            .unwrap_or_else(|| {
                panic!(
                    "Physical pin for {}.{}[{}] is not placed",
                    self.name, port_name, bit
                )
            })
            .clone()
    }
}
