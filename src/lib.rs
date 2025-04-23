// SPDX-License-Identifier: Apache-2.0

mod enum_type;
mod inout;

mod pipeline;
pub use pipeline::PipelineConfig;
mod io;
pub use io::IO;

mod port;
pub use port::Port;

mod port_slice;
pub use port_slice::{ConvertibleToPortSlice, PortSlice};

mod mod_def;
use mod_def::ModDefCore;
pub use mod_def::{ConvertibleToModDef, ModDef};

mod usage;
pub use usage::Usage;

mod mod_inst;
pub use mod_inst::ModInst;

mod validate;
use validate::PortKey;

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

pub use mod_def::ParserConfig;
