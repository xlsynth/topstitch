// SPDX-License-Identifier: Apache-2.0

mod pipeline;
pub use pipeline::PipelineConfig;
mod io;
pub use io::IO;

mod port;
pub use port::Port;

mod port_slice;
pub use port_slice::{ConvertibleToPortSlice, PortSlice};

mod connection;
mod mod_def;
use mod_def::ModDefCore;
pub use mod_def::ParameterType;
pub use mod_def::{
    BoundingBox, CalculatedPlacement, ConvertibleToModDef, Coordinate, EdgeOrientation, Mat3,
    ModDef, Orientation, Placement, Polygon, Range, TrackDefinition, TrackDefinitions,
    TrackOrientation, BOTTOM_EDGE_INDEX, EAST_EDGE_INDEX, LEFT_EDGE_INDEX, NORTH_EDGE_INDEX,
    RIGHT_EDGE_INDEX, SOUTH_EDGE_INDEX, TOP_EDGE_INDEX, WEST_EDGE_INDEX,
};
pub mod lefdef;
pub use lefdef::LefDefOptions;

mod usage;
pub use usage::Usage;

mod mod_inst;
pub use mod_inst::ModInst;

mod validate;

mod intf;
pub use intf::Intf;

mod util;

mod funnel;
pub use funnel::Funnel;

mod package;
pub use package::{
    extract_packages_from_verilog, extract_packages_from_verilog_file,
    extract_packages_from_verilog_files, extract_packages_with_config, Package, Parameter,
};

pub use mod_def::{ParserConfig, PhysicalPin, SpreadPinsOptions};
