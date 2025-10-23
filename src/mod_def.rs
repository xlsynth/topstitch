// SPDX-License-Identifier: Apache-2.0

pub use self::pins::SpreadPinsOptions;

use indexmap::IndexMap;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::{Intf, Port, Usage};

mod core;
pub use core::ModDefCore;

mod dtypes;
pub use dtypes::{
    BoundingBox, Coordinate, Edge, EdgeOrientation, Mat3, Orientation, PhysicalPin, Placement,
    Polygon, Range,
};

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
mod tracks;
pub use tracks::{TrackDefinition, TrackDefinitions, TrackOrientation};
use tracks::{TrackOccupancies, TrackOccupancy};
mod edges;
mod shape;
pub use edges::{
    BOTTOM_EDGE_INDEX, EAST_EDGE_INDEX, LEFT_EDGE_INDEX, NORTH_EDGE_INDEX, RIGHT_EDGE_INDEX,
    SOUTH_EDGE_INDEX, TOP_EDGE_INDEX, WEST_EDGE_INDEX,
};

/// Represents a module definition, like `module <mod_def_name> ... endmodule`
/// in Verilog.
#[derive(Clone)]
pub struct ModDef {
    pub(crate) core: Rc<RefCell<ModDefCore>>,
}

#[macro_export]
macro_rules! for_each_edge_direction {
    ($macro_name:ident) => {
        $macro_name!(west, $crate::mod_def::WEST_EDGE_INDEX);
        $macro_name!(left, $crate::mod_def::LEFT_EDGE_INDEX);
        $macro_name!(north, $crate::mod_def::NORTH_EDGE_INDEX);
        $macro_name!(top, $crate::mod_def::TOP_EDGE_INDEX);
        $macro_name!(east, $crate::mod_def::EAST_EDGE_INDEX);
        $macro_name!(right, $crate::mod_def::RIGHT_EDGE_INDEX);
        $macro_name!(south, $crate::mod_def::SOUTH_EDGE_INDEX);
        $macro_name!(bottom, $crate::mod_def::BOTTOM_EDGE_INDEX);
    };
}

macro_rules! define_keepout_on_named_edge {
    ($edge_name:ident, $const_name:path) => {
        paste::paste! {
            #[doc = concat!(
                "Marks the specified tracks on the ",
                stringify!($edge_name),
                " edge as a keepout region using the provided polygon."
            )]
            pub fn [<define_keepout_on_ $edge_name _edge>](
                &self,
                layer: impl AsRef<str>,
                track_index: usize,
                polygon: &Polygon,
            ) {
                self.define_keepout_on_edge_index($const_name, layer, track_index, polygon);
            }
        }
    };
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
                verilog_import: None,
                mod_inst_connections: IndexMap::new(),
                mod_def_connections: IndexMap::new(),
                adjacency_matrix: HashMap::new(),
                ignore_adjacency: HashSet::new(),
                shape: None,
                layer: None,
                inst_placements: IndexMap::new(),
                physical_pins: IndexMap::new(),
                track_definitions: None,
                track_occupancies: None,
                specified_net_names: HashSet::new(),
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
            "A ModDef shape must be rectilinear."
        );
        assert!(
            shape.is_clockwise(),
            "ModDef shape edges must be defined in a clockwise order."
        );
        assert!(
            shape.starts_with_leftmost_vertical_edge(),
            "ModDef shapes must start with the leftmost vertical edge."
        );
        let mut core = self.core.borrow_mut();
        core.track_occupancies = Some(TrackOccupancies::new(shape.num_edges()));
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

    /// Returns the number of edges (vertices) of the current shape, if set.
    pub fn get_num_edges(&self) -> usize {
        self.core
            .borrow()
            .shape
            .as_ref()
            .map(|s| s.num_edges())
            .unwrap_or(0)
    }

    /// Sets the track definitions for this module.
    pub fn set_track_definitions(&self, track_definitions: TrackDefinitions) {
        let mut core = self.core.borrow_mut();
        let shape = core
            .shape
            .as_ref()
            .expect("Shape must be set before setting track definitions")
            .clone();
        core.track_definitions = Some(track_definitions);

        // For each edge, for each track definition (layer), initialize occupancy
        let track_defs = core.track_definitions.as_ref().unwrap().clone();
        let occupancies = core
            .track_occupancies
            .as_mut()
            .expect("Track occupancies must be initialized before setting track definitions");

        for (edge_index, edge_map) in occupancies.0.iter_mut().enumerate() {
            let edge = shape.get_edge(edge_index);
            for (layer_name, track_def) in track_defs.0.iter() {
                if let Some(range) = edge.get_index_range(track_def) {
                    let length = (range.max.unwrap() - range.min.unwrap() + 1) as usize;
                    edge_map.insert(layer_name.clone(), TrackOccupancy::new(length));
                }
            }
        }
    }

    /// Looks up the [`TrackDefinition`] for `name`, if one has been registered.
    pub fn get_track(&self, name: impl AsRef<str>) -> Option<TrackDefinition> {
        let core_borrowed = self.core.borrow();
        let track_definitions = &core_borrowed.track_definitions;
        track_definitions
            .as_ref()
            .and_then(|t| t.get_track(name.as_ref()).cloned())
    }

    /// Returns the polygon edge at `edge_index`, or `None` if the shape is not
    /// defined or the index is out of bounds.
    pub fn get_edge(&self, edge_index: usize) -> Option<Edge> {
        let core_borrowed = self.core.borrow();
        let shape = &core_borrowed.shape;
        shape.as_ref().map(|s| s.get_edge(edge_index))
    }

    /// Returns the nearest usable track index relative to the start of
    /// `edge_index` for `coordinate`, if the layer and edge are compatible.
    pub fn nearest_relative_track_index(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        coordinate: &Coordinate,
    ) -> Option<usize> {
        let layer = layer.as_ref();
        let shape = self.get_shape()?;
        let track = self.get_track(layer)?;
        let edge = shape.get_edge(edge_index);
        let orientation = edge.orientation()?;
        let track_range = edge.get_index_range(&track)?;
        let min_index = track_range.min?;
        let max_index = track_range.max?;

        let axis_coordinate = match orientation {
            EdgeOrientation::North | EdgeOrientation::South => coordinate.y,
            EdgeOrientation::East | EdgeOrientation::West => coordinate.x,
        };

        let absolute_track_index = track.nearest_track_index(axis_coordinate);
        if (min_index <= absolute_track_index) && (absolute_track_index <= max_index) {
            Some((absolute_track_index - min_index) as usize)
        } else {
            None
        }
    }

    /// Marks the inclusive track index range as occupied by an existing pin.
    pub fn mark_pin_range(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        min_index: i64,
        max_index: i64,
    ) {
        let mut core = self.core.borrow_mut();
        let occupancies = core
            .track_occupancies
            .as_mut()
            .expect("Track occupancies not initialized");
        if let Some(occupancy) = occupancies.get_occupancy_mut(edge_index, layer.as_ref()) {
            occupancy.mark_pin(min_index, max_index);
        }
    }

    /// Marks the inclusive track index range as blocked by a keepout region.
    pub fn mark_keepout_range(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        min_index: i64,
        max_index: i64,
    ) {
        let mut core = self.core.borrow_mut();
        let occupancies = core
            .track_occupancies
            .as_mut()
            .expect("Track occupancies not initialized");
        if let Some(occupancy) = occupancies.get_occupancy_mut(edge_index, layer.as_ref()) {
            occupancy.mark_keepout(min_index, max_index);
        }
    }

    /// Records both the pin and the keepout envelopes for a placed pin in a
    /// single call.
    pub fn mark_pin_and_keepout_ranges(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        pin_min_index: i64,
        pin_max_index: i64,
        keepout_min_index: i64,
        keepout_max_index: i64,
    ) {
        let mut core = self.core.borrow_mut();
        let occupancies = core
            .track_occupancies
            .as_mut()
            .expect("Track occupancies not initialized");
        if let Some(occupancy) = occupancies.get_occupancy_mut(edge_index, layer.as_ref()) {
            occupancy.place_pin_and_keepout(
                pin_min_index,
                pin_max_index,
                keepout_min_index,
                keepout_max_index,
            );
        }
    }

    for_each_edge_direction!(define_keepout_on_named_edge);

    /// Marks the keepout polygon corresponding to `track_index` on
    /// `edge_index`, using the provided shape to derive track coverage.
    pub fn define_keepout_on_edge_index(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
        polygon: &Polygon,
    ) {
        let layer_ref = layer.as_ref();

        let (keepout_min_track, keepout_max_track) =
            self.track_range_for_polygon(layer_ref, track_index, polygon);

        self.mark_keepout_range(edge_index, layer_ref, keepout_min_track, keepout_max_track);
    }

    /// Returns the ordered list of routing layer names with defined track
    /// families.
    pub fn get_layers(&self) -> Vec<String> {
        self.core
            .borrow()
            .track_definitions
            .as_ref()
            .map(|td| td.0.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Fetches a clone of the occupancy bitmap for internal placement checks.
    pub(crate) fn get_occupancy(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
    ) -> Option<TrackOccupancy> {
        self.core
            .borrow()
            .track_occupancies
            .as_ref()
            .and_then(|occupancies| {
                occupancies
                    .get_occupancy(edge_index, layer.as_ref())
                    .cloned()
            })
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
