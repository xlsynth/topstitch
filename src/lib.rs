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
    BOTTOM_EDGE_INDEX, BoundingBox, CalculatedPlacement, ConvertibleToModDef, Coordinate,
    EAST_EDGE_INDEX, EdgeOrientation, LEFT_EDGE_INDEX, Mat3, ModDef, NORTH_EDGE_INDEX, Orientation,
    Placement, Polygon, RIGHT_EDGE_INDEX, Range, SOUTH_EDGE_INDEX, TOP_EDGE_INDEX, TrackDefinition,
    TrackDefinitions, TrackOrientation, WEST_EDGE_INDEX,
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
    Package, Parameter, extract_packages_from_verilog, extract_packages_from_verilog_file,
    extract_packages_from_verilog_files, extract_packages_with_config,
};

pub use mod_def::{ParserConfig, PhysicalPin, SpreadPinsOptions};
