// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::{Intf, Port, Usage};

mod core;
pub use core::ModDefCore;

mod dtypes;
pub(crate) use dtypes::{Assignment, InstConnection, PortSliceOrWire, Wire};
pub use dtypes::{BoundingBox, Coordinate, Mat3, Orientation, Placement, Polygon};

mod emit;
mod feedthrough;
mod instances;
mod intf;
mod parameterize;
mod placement;
pub use parameterize::ParameterType;
pub use placement::CalculatedPlacement;
mod lefdef;
mod parser;
mod parser_cfg;
pub use parser_cfg::ParserConfig;
mod pins;
mod ports;
mod stub;
mod validate;
mod wrap;
use parser::{parser_param_to_param, parser_port_to_port};
mod abutment;
mod hierarchy;
/// Represents a module definition, like `module <mod_def_name> ... endmodule`
/// in Verilog.
#[derive(Clone)]
pub struct ModDef {
    pub(crate) core: Rc<RefCell<ModDefCore>>,
}

impl ModDef {
    /// Creates a new module definition with the given name.
    pub fn new(name: impl AsRef<str>) -> ModDef {
        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: name.as_ref().to_string(),
                ports: IndexMap::new(),
                enum_ports: IndexMap::new(),
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Default::default(),
                generated_verilog: None,
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                whole_port_tieoffs: IndexMap::new(),
                whole_port_unused: IndexMap::new(),
                verilog_import: None,
                inst_connections: IndexMap::new(),
                reserved_net_definitions: IndexMap::new(),
                adjacency_matrix: HashMap::new(),
                ignore_adjacency: HashSet::new(),
                shape: None,
                layer: None,
                inst_placements: IndexMap::new(),
                physical_pins: IndexMap::new(),
            })),
        }
    }

    fn frozen(&self) -> bool {
        self.core.borrow().generated_verilog.is_some()
            || self.core.borrow().verilog_import.is_some()
    }

    /// Returns the name of this module definition.
    pub fn get_name(&self) -> String {
        self.core.borrow().name.clone()
    }

    /// Configures how this module definition should be used when validating
    /// and/or emitting Verilog.
    pub fn set_usage(&self, usage: Usage) {
        if self.core.borrow().generated_verilog.is_some() {
            assert!(
                usage != Usage::EmitDefinitionAndDescend,
                "Cannot descend into a module defined from Verilog sources."
            );
        }
        self.core.borrow_mut().usage = usage;
    }

    /// Define a rectangular shape at (0, 0) with width and height. This is
    /// shorthand for set_shape with four rectilinear points.
    pub fn set_width_height(&self, width: i64, height: i64) {
        assert!(width > 0 && height > 0, "Width and height must be positive");
        self.set_shape(Polygon::from_width_height(width, height));
    }

    /// Define a rectilinear polygonal outline by its vertices in order
    pub fn set_shape(&self, shape: Polygon) {
        assert!(
            shape.is_rectilinear(),
            "Only rectilinear polygons are supported"
        );
        let mut core = self.core.borrow_mut();
        core.shape = Some(shape);
    }

    /// Define the layer of this module.
    pub fn set_layer(&self, layer: impl AsRef<str>) {
        let mut core = self.core.borrow_mut();
        core.layer = Some(layer.as_ref().to_string());
    }

    /// Returns this module's shape and its layer, if defined.
    pub fn get_shape(&self) -> Option<Polygon> {
        self.core.borrow().shape.clone()
    }

    /// Returns this module's layer, if defined.
    pub fn get_layer(&self) -> Option<String> {
        self.core.borrow().layer.clone()
    }
}

/// Indicates that a type can be converted to a `ModDef`. `ModDef` and
/// `ModInst` both implement this trait, which makes it easier to perform the
/// same operations on both.
pub trait ConvertibleToModDef {
    fn to_mod_def(&self) -> ModDef;
    fn get_port(&self, name: impl AsRef<str>) -> Port;
    fn get_intf(&self, name: impl AsRef<str>) -> Intf;
}

impl ConvertibleToModDef for ModDef {
    fn to_mod_def(&self) -> ModDef {
        self.clone()
    }
    fn get_port(&self, name: impl AsRef<str>) -> Port {
        self.get_port(name)
    }
    fn get_intf(&self, name: impl AsRef<str>) -> Intf {
        self.get_intf(name)
    }
}
