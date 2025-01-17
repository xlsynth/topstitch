// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use indexmap::IndexMap;
use itertools::Itertools;
use num_bigint::{BigInt, BigUint};
use regex::Regex;
use slang_rs::{self, extract_ports, str2tmpfile, SlangConfig};
use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::Hash;
use std::path::Path;
use std::rc::{Rc, Weak};
use xlsynth::vast::{Expr, LogicRef, VastFile, VastFileType};

mod enum_type;
mod inout;
mod pipeline;

use pipeline::add_pipeline;
use pipeline::PipelineDetails;

/// Represents the direction (`Input` or `Output`) and bit width of a port.
#[derive(Clone, Debug)]
pub enum IO {
    Input(usize),
    Output(usize),
    InOut(usize),
}

impl IO {
    /// Returns the width of the port in bits.
    pub fn width(&self) -> usize {
        match self {
            IO::Input(width) => *width,
            IO::Output(width) => *width,
            IO::InOut(width) => *width,
        }
    }

    /// Returns a new IO enum with the same width but the opposite direction.
    pub fn flip(&self) -> IO {
        match self {
            IO::Input(width) => IO::Output(*width),
            IO::Output(width) => IO::Input(*width),
            IO::InOut(width) => IO::InOut(*width),
        }
    }

    /// Returns a new IO enum with the same direction but a different width.
    pub fn with_width(&self, width: usize) -> IO {
        match self {
            IO::Input(_) => IO::Input(width),
            IO::Output(_) => IO::Output(width),
            IO::InOut(_) => IO::InOut(width),
        }
    }

    fn variant_name(&self) -> &str {
        match self {
            IO::Input(_) => "Input",
            IO::Output(_) => "Output",
            IO::InOut(_) => "InOut",
        }
    }
}

/// Represents a port on a module definition or a module instance.
#[derive(Clone, Debug)]
pub enum Port {
    ModDef {
        mod_def_core: Weak<RefCell<ModDefCore>>,
        name: String,
    },
    ModInst {
        mod_def_core: Weak<RefCell<ModDefCore>>,
        inst_name: String,
        port_name: String,
    },
}

impl Port {
    /// Returns the name this port has in its (parent) module definition.
    pub fn name(&self) -> &str {
        match self {
            Port::ModDef { name, .. } => name,
            Port::ModInst { port_name, .. } => port_name,
        }
    }

    fn variant_name(&self) -> &str {
        match self {
            Port::ModDef { .. } => "ModDef",
            Port::ModInst { .. } => "ModInst",
        }
    }

    /// Returns the IO enum associated with this Port.
    pub fn io(&self) -> IO {
        match self {
            Port::ModDef { mod_def_core, name } => {
                mod_def_core.upgrade().unwrap().borrow().ports[name].clone()
            }
            Port::ModInst {
                mod_def_core,
                inst_name,
                port_name,
            } => mod_def_core.upgrade().unwrap().borrow().instances[inst_name]
                .borrow()
                .ports[port_name]
                .clone(),
        }
    }

    fn assign_to_inst(&self, inst: &ModInst) -> Port {
        match self {
            Port::ModDef { name, .. } => Port::ModInst {
                mod_def_core: inst.mod_def_core.clone(),
                inst_name: inst.name.clone(),
                port_name: name.clone(),
            },
            _ => panic!("Already assigned to an instance."),
        }
    }

    fn to_port_key(&self) -> PortKey {
        match self {
            Port::ModDef { name, .. } => PortKey::ModDefPort {
                mod_def_name: self.get_mod_def_core().borrow().name.clone(),
                port_name: name.clone(),
            },
            Port::ModInst {
                inst_name,
                port_name,
                ..
            } => PortKey::ModInstPort {
                mod_def_name: self.get_mod_def_core().borrow().name.clone(),
                inst_name: inst_name.clone(),
                port_name: port_name.clone(),
            },
        }
    }

    fn is_driver(&self) -> bool {
        match self {
            Port::ModDef { .. } => matches!(self.io(), IO::Input(_)),
            Port::ModInst { .. } => matches!(self.io(), IO::Output(_)),
        }
    }
}

/// Represents a slice of a port, which may be on a module definition or on a
/// module instance.
///
/// A slice is a defined as a contiguous range of bits from `msb` down to `lsb`,
/// inclusive. A slice can be a single bit on the port (`msb` equal to `lsb`),
/// the entire port, or any range in between.
#[derive(Clone, Debug)]
pub struct PortSlice {
    port: Port,
    msb: usize,
    lsb: usize,
}

impl PortSlice {
    /// Divides a port slice into `n` parts of equal bit width, return a vector
    /// of `n` port slices. For example, if a port is 8 bits wide and `n` is 2,
    /// the port will be divided into 2 slices of 4 bits each: `port[3:0]` and
    /// `port[7:4]`. This method panics if the port width is not divisible by
    /// `n`.
    pub fn subdivide(&self, n: usize) -> Vec<Self> {
        let width = self.msb - self.lsb + 1;
        if width % n != 0 {
            panic!(
                "Cannot subdivide {} into {} equal parts.",
                self.debug_string(),
                n
            );
        }
        (0..n)
            .map(move |i| {
                let sub_width = width / n;
                PortSlice {
                    port: self.port.clone(),
                    msb: ((i + 1) * sub_width) - 1 + self.lsb,
                    lsb: (i * sub_width) + self.lsb,
                }
            })
            .collect()
    }

    fn width(&self) -> usize {
        self.msb - self.lsb + 1
    }

    /// Create a new port called `name` on the parent module and connects it to
    /// this port slice.
    ///
    /// The exact behavior depends on whether this is a port slice on a module
    /// definition or a module instance. If this is a port slice on a module
    /// definition, a new port is created on the same module definition, with
    /// the same width, but opposite direction. For example, suppose that this
    /// is a port slice `a` on a module definition that is an 8-bit input;
    /// calling `export_as("y")` will create an 8-bit output on the same
    /// module definition called `y`.
    ///
    /// If, on the other hand, this is a port slice on a module instance, a new
    /// port will be created on the module definition containing the
    /// instance, with the same width and direction. For example, if this is
    /// an 8-bit input port `x` on a module instance, calling
    /// `export_as("y")` will create a new 8-bit input port `y` on the
    /// module definition that contains the instance.
    pub fn export_as(&self, name: impl AsRef<str>) -> Port {
        let io = match self.port {
            Port::ModDef { .. } => self.port.io().with_width(self.width()).flip(),
            Port::ModInst { .. } => self.port.io().with_width(self.width()),
        };

        let core = self.get_mod_def_core();
        let moddef = ModDef { core };

        let new_port = moddef.add_port(name, io);
        self.connect(&new_port);

        new_port
    }

    /// Same as export_as(), but the new port is created with the same name as
    /// the port being exported. As a result, this method can only be used with
    /// ports on module instances. The method will panic if called on a port
    /// slice on a module definition.
    pub fn export(&self) -> Port {
        let name = match &self.port {
            Port::ModDef { .. } => panic!(
                "Use export_as() to export {}, specifying the new name of the exported port.",
                self.debug_string()
            ),
            Port::ModInst { port_name, .. } => port_name.clone(),
        };
        self.export_as(&name)
    }

    fn slice_relative(&self, offset: usize, width: usize) -> Self {
        assert!(offset + width <= self.width());

        PortSlice {
            port: self.port.clone(),
            msb: self.lsb + offset + width - 1,
            lsb: self.lsb + offset,
        }
    }
}

/// Indicates that a type can be converted to a `PortSlice`. `Port` and
/// `PortSlice` both implement this trait, which makes it easier to perform the
/// same operations on both.
pub trait ConvertibleToPortSlice {
    fn to_port_slice(&self) -> PortSlice;
}

impl ConvertibleToPortSlice for Port {
    fn to_port_slice(&self) -> PortSlice {
        PortSlice {
            port: self.clone(),
            msb: self.io().width() - 1,
            lsb: 0,
        }
    }
}

impl ConvertibleToPortSlice for PortSlice {
    fn to_port_slice(&self) -> PortSlice {
        self.clone()
    }
}

/// Represents a module definition, like `module <mod_def_name> ... endmodule`
/// in Verilog.
#[derive(Clone)]
pub struct ModDef {
    core: Rc<RefCell<ModDefCore>>,
}

/// Represents an instance of a module definition, like `<mod_def_name>
/// <mod_inst_name> ( ... );` in Verilog.
#[derive(Clone)]
pub struct ModInst {
    name: String,
    mod_def_core: Weak<RefCell<ModDefCore>>,
}

struct VerilogImport {
    sources: Vec<String>,
    incdirs: Vec<String>,
    skip_unsupported: bool,
    ignore_unknown_modules: bool,
}

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub clk: String,
    pub depth: usize,
}

#[derive(Debug, Clone)]
struct Assignment {
    pub lhs: PortSlice,
    pub rhs: PortSlice,
    pub pipeline: Option<PipelineConfig>,
}

/// Data structure representing a module definition.
///
/// Contains the module's name, ports, interfaces, instances, etc. Not intended
/// to be used directly; use `ModDef` instead, which contains a smart pointer to
/// this struct.
pub struct ModDefCore {
    name: String,
    ports: IndexMap<String, IO>,
    interfaces: IndexMap<String, IndexMap<String, (String, usize, usize)>>,
    instances: IndexMap<String, Rc<RefCell<ModDefCore>>>,
    usage: Usage,
    generated_verilog: Option<String>,
    verilog_import: Option<VerilogImport>,
    assignments: Vec<Assignment>,
    unused: Vec<PortSlice>,
    tieoffs: Vec<(PortSlice, BigInt)>,
    whole_port_tieoffs: IndexMap<String, IndexMap<String, BigInt>>,
    inst_connections: IndexMap<String, IndexMap<String, Vec<InstConnection>>>,
    reserved_net_definitions: IndexMap<String, Wire>,
    enum_ports: IndexMap<String, String>,
}

#[derive(Clone)]
struct InstConnection {
    inst_port_slice: PortSlice,
    connected_to: PortSliceOrWire,
}

#[derive(Clone)]
struct Wire {
    name: String,
    width: usize,
}

#[derive(Clone)]
enum PortSliceOrWire {
    PortSlice(PortSlice),
    Wire(Wire),
}

/// Represents how a module definition should be used when validating and/or
/// emitting Verilog.
#[derive(PartialEq, Default, Clone)]
pub enum Usage {
    /// When validating, validate the module definition and descend into its
    /// instances. When emitting Verilog, emit its definition and descend into
    /// its instances.
    #[default]
    EmitDefinitionAndDescend,

    /// When validating, do not validate the module definition and do not
    /// descend into its instances. When emitting Verilog, do not emit its
    /// definition and do not descend into its instances.
    EmitNothingAndStop,

    /// When validating, do not validate the module definition and do not
    /// descend into its instances. When emitting Verilog, emit a stub
    /// (interface only) and do not descend into its instances.
    EmitStubAndStop,

    /// When validating, do not validate the module definition and do not
    /// descend into its instances. When emitting Verilog, emit its definition
    /// but do not descend into its instances.
    EmitDefinitionAndStop,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PortKey {
    ModDefPort {
        mod_def_name: String,
        port_name: String,
    },
    ModInstPort {
        mod_def_name: String,
        inst_name: String,
        port_name: String,
    },
}

impl PortKey {
    fn debug_string(&self) -> String {
        match &self {
            PortKey::ModDefPort {
                mod_def_name,
                port_name,
            } => format!("{}.{}", mod_def_name, port_name),
            PortKey::ModInstPort {
                mod_def_name,
                inst_name,
                port_name,
            } => format!("{}.{}.{}", mod_def_name, inst_name, port_name),
        }
    }

    fn variant_name(&self) -> &'static str {
        match self {
            PortKey::ModDefPort { .. } => "ModDef",
            PortKey::ModInstPort { .. } => "ModInst",
        }
    }

    fn retrieve_port_io(&self, mod_def_core: &ModDefCore) -> IO {
        match self {
            PortKey::ModDefPort { port_name, .. } => mod_def_core.ports[port_name].clone(),
            PortKey::ModInstPort {
                inst_name,
                port_name,
                ..
            } => mod_def_core.instances[inst_name].borrow().ports[port_name].clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DrivenPortBits {
    driven: BigUint,
    width: usize,
}

impl DrivenPortBits {
    fn new(width: usize) -> Self {
        DrivenPortBits {
            driven: BigUint::from(0u32),
            width,
        }
    }

    fn driven(&mut self, msb: usize, lsb: usize) -> Result<(), DrivenError> {
        let mut mask = (BigUint::from(1u32) << (msb - lsb + 1)) - BigUint::from(1u32);

        // make sure this is not already driven
        if (self.driven.clone() >> lsb) & mask.clone() != BigUint::from(0u32) {
            return Err(DrivenError::AlreadyDriven);
        };

        // mark the bits as driven
        mask <<= lsb;
        self.driven |= mask;

        Ok(())
    }

    fn all_driven(&self) -> bool {
        self.driven == (BigUint::from(1u32) << self.width) - BigUint::from(1u32)
    }

    fn example_problematic_bits(&self) -> Option<String> {
        example_problematic_bits(&self.driven, self.width)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DrivingPortBits {
    driving: BigUint,
    unused: BigUint,
    width: usize,
}

enum DrivenError {
    AlreadyDriven,
}

enum DrivingError {
    AlreadyMarkedUnused,
}

enum UnusedError {
    AlreadyMarkedUnused,
    AlreadyUsed,
}

impl DrivingPortBits {
    fn new(width: usize) -> Self {
        DrivingPortBits {
            driving: BigUint::from(0u32),
            unused: BigUint::from(0u32),
            width,
        }
    }

    fn driving(&mut self, msb: usize, lsb: usize) -> Result<(), DrivingError> {
        let mut mask = (BigUint::from(1u32) << (msb - lsb + 1)) - BigUint::from(1u32);

        // make sure nothing in this range is marked as unused
        if (self.unused.clone() >> lsb) & mask.clone() != BigUint::from(0u32) {
            return Err(DrivingError::AlreadyMarkedUnused);
        };

        // mark the bits as driving
        mask <<= lsb;
        self.driving |= mask;

        Ok(())
    }

    fn unused(&mut self, msb: usize, lsb: usize) -> Result<(), UnusedError> {
        let mut mask = (BigUint::from(1u32) << (msb - lsb + 1)) - BigUint::from(1u32);

        // make sure nothing in this range is marked as unused
        if (self.unused.clone() >> lsb) & mask.clone() != BigUint::from(0u32) {
            return Err(UnusedError::AlreadyMarkedUnused);
        };

        // make sure nothing in this range is marked as driving
        if (self.driving.clone() >> lsb) & mask.clone() != BigUint::from(0u32) {
            return Err(UnusedError::AlreadyUsed);
        };

        // mark the bits as unused
        mask <<= lsb;
        self.unused |= mask;

        Ok(())
    }

    fn all_driving_or_unused(&self) -> bool {
        (self.driving.clone() | self.unused.clone())
            == (BigUint::from(1u32) << self.width) - BigUint::from(1u32)
    }

    fn example_problematic_bits(&self) -> Option<String> {
        example_problematic_bits(&(self.driving.clone() | self.unused.clone()), self.width)
    }
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
                verilog_import: None,
                inst_connections: IndexMap::new(),
                reserved_net_definitions: IndexMap::new(),
            })),
        }
    }

    /// Returns a new module definition with the given name, using the same
    /// ports and interfaces as the original module. The new module has no
    /// instantiations or internal connections.
    pub fn stub(&self, name: impl AsRef<str>) -> ModDef {
        let core = self.core.borrow();
        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: name.as_ref().to_string(),
                ports: core.ports.clone(),
                // TODO(sherbst): 12/08/2024 should enum_ports be copied when stubbing?
                // The implication is that modules that instantiate this stub will
                // use casting to connect to enum input ports, even though they appear
                // as flat buses in the stub.
                enum_ports: core.enum_ports.clone(),
                interfaces: core.interfaces.clone(),
                instances: IndexMap::new(),
                usage: Default::default(),
                generated_verilog: None,
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                whole_port_tieoffs: IndexMap::new(),
                verilog_import: None,
                inst_connections: IndexMap::new(),
                reserved_net_definitions: IndexMap::new(),
            })),
        }
    }

    fn frozen(&self) -> bool {
        self.core.borrow().generated_verilog.is_some()
            || self.core.borrow().verilog_import.is_some()
    }

    /// Creates a new module definition from a Verilog file. The `name`
    /// parameter is the name of the module to extract from the Verilog file,
    /// and `verilog` is the path to the Verilog file. If
    /// `ignore_unknown_modules` is `true`, do not panic if the Verilog file
    /// instantiates modules whose definitions cannot be found. This is often
    /// useful because only the interface of module `name` needs to be
    /// extracted; its contents do not need to be interpreted. If
    /// `skip_unsupported` is `true`, do not panic if the interface of module
    /// `name` contains unsupported features; simply skip these ports. This is
    /// occasionally useful when prototyping.
    pub fn from_verilog_file(
        name: impl AsRef<str>,
        verilog: &Path,
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        Self::from_verilog_files(name, &[verilog], ignore_unknown_modules, skip_unsupported)
    }

    /// Creates a new module definition from a list of Verilog files. The `name`
    /// parameter is the name of the module to extract from the Verilog sources,
    /// and `verilog` is an array of paths of Verilog sources. If
    /// `ignore_unknown_modules` is `true`, do not panic if the Verilog file
    /// instantiates modules whose definitions cannot be found. This is often
    /// useful because only the interface of module `name` needs to be
    /// extracted; its contents do not need to be interpreted. If
    /// `skip_unsupported` is `true`, do not panic if the interface of module
    /// `name` contains unsupported features; simply skip these ports. This is
    /// occasionally useful when prototyping.
    pub fn from_verilog_files(
        name: impl AsRef<str>,
        verilog: &[&Path],
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        let cfg = SlangConfig {
            sources: &verilog
                .iter()
                .map(|path| path.to_str().unwrap())
                .collect::<Vec<_>>(),
            ignore_unknown_modules,
            ..Default::default()
        };

        Self::from_verilog_using_slang(name, &cfg, skip_unsupported)
    }

    /// Creates a new module definition from Verilog source code. The `name`
    /// parameter is the name of the module to extract from the Verilog code,
    /// and `verilog` is a string containing Verilog code. If
    /// `ignore_unknown_modules` is `true`, do not panic if the Verilog file
    /// instantiates modules whose definitions cannot be found. This is often
    /// useful because only the interface of module `name` needs to be
    /// extracted; its contents do not need to be interpreted. If
    /// `skip_unsupported` is `true`, do not panic if the interface of module
    /// `name` contains unsupported features; simply skip these ports. This is
    /// occasionally useful when prototyping.
    pub fn from_verilog(
        name: impl AsRef<str>,
        verilog: impl AsRef<str>,
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        let verilog = str2tmpfile(verilog.as_ref()).unwrap();

        let cfg = SlangConfig {
            sources: &[verilog.path().to_str().unwrap()],
            ignore_unknown_modules,
            ..Default::default()
        };

        Self::from_verilog_using_slang(name, &cfg, skip_unsupported)
    }

    /// Creates a new module definition from Verilog sources. The `name`
    /// parameter is the name of the module to extract from Verilog code, and
    /// `cfg` is a `SlangConfig` struct specifying source files, include
    /// directories, etc. If `skip_unsupported` is `true`, do not panic if the
    /// interface of module `name` contains unsupported features; simply skip
    /// these ports. This is occasionally useful when prototyping.
    pub fn from_verilog_using_slang(
        name: impl AsRef<str>,
        cfg: &SlangConfig,
        skip_unsupported: bool,
    ) -> Self {
        let parser_ports = extract_ports(cfg, skip_unsupported);

        let selected = parser_ports.get(name.as_ref()).unwrap_or_else(|| {
            panic!(
                "Module definition '{}' not found in Verilog sources.",
                name.as_ref()
            )
        });

        Self::mod_def_from_parser_ports(name.as_ref(), selected, cfg, skip_unsupported)
    }

    pub fn all_from_verilog_using_slang(cfg: &SlangConfig, skip_unsupported: bool) -> Vec<Self> {
        let parser_ports = extract_ports(cfg, skip_unsupported);
        parser_ports
            .keys()
            .map(|name| {
                Self::mod_def_from_parser_ports(name, &parser_ports[name], cfg, skip_unsupported)
            })
            .collect()
    }

    fn mod_def_from_parser_ports(
        mod_def_name: &str,
        parser_ports: &[slang_rs::Port],
        cfg: &SlangConfig,
        skip_unsupported: bool,
    ) -> ModDef {
        let mut ports = IndexMap::new();
        let mut enum_ports = IndexMap::new();
        for parser_port in parser_ports {
            match parser_port_to_port(parser_port) {
                Ok((name, io)) => {
                    ports.insert(name.clone(), io.clone());
                    // Enum input ports that are not a packed array require special handling
                    // They need to have casting to be valid Verilog.
                    if let slang_rs::Type::Enum {
                        name: enum_name,
                        packed_dimensions,
                        unpacked_dimensions,
                        ..
                    } = &parser_port.ty
                    {
                        if packed_dimensions.is_empty() && unpacked_dimensions.is_empty() {
                            if let IO::Input(_) = io {
                                enum_ports.insert(name.clone(), enum_name.clone());
                            }
                        }
                    }
                }
                Err(e) => {
                    if !skip_unsupported {
                        panic!("{e}");
                    } else {
                        continue;
                    }
                }
            }
        }

        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: mod_def_name.to_string(),
                ports,
                enum_ports,
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Usage::EmitNothingAndStop,
                generated_verilog: None,
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                whole_port_tieoffs: IndexMap::new(),
                verilog_import: Some(VerilogImport {
                    sources: cfg.sources.iter().map(|s| s.to_string()).collect(),
                    incdirs: cfg.incdirs.iter().map(|s| s.to_string()).collect(),
                    skip_unsupported,
                    ignore_unknown_modules: cfg.ignore_unknown_modules,
                }),
                inst_connections: IndexMap::new(),
                reserved_net_definitions: IndexMap::new(),
            })),
        }
    }

    /// Adds a port to the module definition with the given name. The direction
    /// and width are specfied via the `io` parameter.
    pub fn add_port(&self, name: impl AsRef<str>, io: IO) -> Port {
        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.borrow().name
            );
        }

        let mut core = self.core.borrow_mut();
        match core.ports.entry(name.as_ref().to_string()) {
            Entry::Occupied(_) => {
                panic!("Port {}.{} already exists.", core.name, name.as_ref(),)
            }
            Entry::Vacant(entry) => {
                entry.insert(io);
                Port::ModDef {
                    name: name.as_ref().to_string(),
                    mod_def_core: Rc::downgrade(&self.core),
                }
            }
        }
    }

    /// Returns `true` if this module definition has a port with the given name.
    pub fn has_port(&self, name: impl AsRef<str>) -> bool {
        self.core.borrow().ports.contains_key(name.as_ref())
    }

    /// Returns `true` if this module definition has an interface with the given
    /// name.
    pub fn has_interface(&self, name: impl AsRef<str>) -> bool {
        self.core.borrow().interfaces.contains_key(name.as_ref())
    }

    /// Returns the port on this module definition with the given name; panics
    /// if a port with that name does not exist.
    pub fn get_port(&self, name: impl AsRef<str>) -> Port {
        let inner = self.core.borrow();
        if inner.ports.contains_key(name.as_ref()) {
            Port::ModDef {
                name: name.as_ref().to_string(),
                mod_def_core: Rc::downgrade(&self.core),
            }
        } else {
            panic!("Port {}.{} does not exist", inner.name, name.as_ref())
        }
    }

    /// Returns a slice of the port on this module definition with the given
    /// name, from `msb` down to `lsb`, inclusive; panics if a port with that
    /// name does not exist.
    pub fn get_port_slice(&self, name: impl AsRef<str>, msb: usize, lsb: usize) -> PortSlice {
        self.get_port(name).slice(msb, lsb)
    }

    /// Returns a vector of all ports on this module definition with the given
    /// prefix. If `prefix` is `None`, returns all ports.
    pub fn get_ports(&self, prefix: Option<&str>) -> Vec<Port> {
        let inner = self.core.borrow();
        let mut result = Vec::new();
        for name in inner.ports.keys() {
            if prefix.map_or(true, |pfx| name.starts_with(pfx)) {
                result.push(Port::ModDef {
                    name: name.clone(),
                    mod_def_core: Rc::downgrade(&self.core),
                });
            }
        }
        result
    }

    /// Walk through all instances within this module definition, marking those
    /// whose names match the given regex with the usage
    /// `Usage::EmitStubAndStop`. Repeat recursively for all instances whose
    /// names do not match this regex.
    pub fn stub_recursive(&self, regex: impl AsRef<str>) {
        let regex_compiled = Regex::new(regex.as_ref()).unwrap();
        let mut visited = HashSet::new();
        self.stub_recursive_helper(&regex_compiled, &mut visited);
    }

    fn stub_recursive_helper(&self, regex: &Regex, visited: &mut HashSet<String>) {
        for inst in self.get_instances() {
            let mod_def = inst.get_mod_def();
            let mod_def_name = mod_def.get_name();
            if regex.is_match(mod_def_name.as_str()) {
                mod_def.set_usage(Usage::EmitStubAndStop);
            } else if !visited.contains(&mod_def_name) {
                visited.insert(mod_def_name);
                mod_def.stub_recursive_helper(regex, visited);
            }
        }
    }

    /// Returns the name of this module definition.
    pub fn get_name(&self) -> String {
        self.core.borrow().name.clone()
    }

    /// Returns a vector of all module instances within this module definition.
    pub fn get_instances(&self) -> Vec<ModInst> {
        self.core
            .borrow()
            .instances
            .keys()
            .map(|name| ModInst {
                name: name.clone(),
                mod_def_core: Rc::downgrade(&self.core),
            })
            .collect()
    }

    /// Returns the module instance within this module definition with the given
    /// name; panics if an instance with that name does not exist.
    pub fn get_instance(&self, name: impl AsRef<str>) -> ModInst {
        let inner = self.core.borrow();
        if inner.instances.contains_key(name.as_ref()) {
            ModInst {
                name: name.as_ref().to_string(),
                mod_def_core: Rc::downgrade(&self.core),
            }
        } else {
            panic!("Instance {}.{} does not exist", inner.name, name.as_ref())
        }
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

    /// Instantiate a module, using the provided instance name. `autoconnect` is
    /// an optional list of port names to automatically connect between the
    /// parent module and the instantiated module. This feature does not make
    /// any connections between module instances.
    ///
    /// As an example, suppose that the parent module has a port named `clk` and
    /// the instantiated module has a port named `clk`. Passing
    /// `autoconnect=Some(&["clk"])` will automatically connect the two ports.
    /// It will not automatically connect the `clk` port on this module
    /// instance to the `clk` port on any other module instances.
    ///
    /// It's OK if some or all of the `autoconnect` names do not exist in
    /// the parent module and/or instantiated module; TopStitch will not panic
    /// in this case.
    pub fn instantiate(
        &self,
        moddef: &ModDef,
        name: Option<&str>,
        autoconnect: Option<&[&str]>,
    ) -> ModInst {
        let name_default;
        let name = if let Some(name) = name {
            name
        } else {
            name_default = format!("{}_i", moddef.core.borrow().name);
            name_default.as_str()
        };

        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.borrow().name
            );
        }

        {
            let mut inner = self.core.borrow_mut();
            if inner.instances.contains_key(name) {
                panic!("Instance {}.{} already exists", inner.name, name);
            }
            inner
                .instances
                .insert(name.to_string(), moddef.core.clone());
        }

        // Create the ModInst
        let inst = ModInst {
            name: name.to_string(),
            mod_def_core: Rc::downgrade(&self.core),
        };

        // autoconnect logic
        if let Some(port_names) = autoconnect {
            for &port_name in port_names {
                // Check if the instantiated module has this port
                if let Some(io) = moddef.core.borrow().ports.get(port_name) {
                    {
                        let mut inner = self.core.borrow_mut();
                        if !inner.ports.contains_key(port_name) {
                            inner.ports.insert(port_name.to_string(), io.clone());
                        }
                    }

                    // Connect the instance port to the parent module port
                    let parent_port = self.get_port(port_name);
                    let instance_port = inst.get_port(port_name);
                    parent_port.connect(&instance_port)
                }
            }
        }

        inst
    }

    /// Create one or more instances of a module, using the provided dimensions.
    /// For example, if `dimensions` is `&[3]`, TopStitch will create a 1D array
    /// of 3 instances, called `<mod_def_name>_i_0`, `<mod_def_name>_i_1`,
    /// `<mod_def_name>_i_2`. If `dimensions` is `&[2, 3]`, TopStitch will
    /// create a `2x3` array of instances, called `<mod_def_name>_i_0_0`,
    /// `<mod_def_name>_i_0_1`, `<mod_def_name>_i_0_2`, `<mod_def_name>_i_1_0`,
    /// etc. If provided, the optional `prefix` argument sets the prefix used in
    /// naming instances to something other than `<mod_def_name>_i_`.
    /// `autoconnect` has the same meaning as in `instantiate()`: if provided,
    /// it is a list of port names to automatically connect between the parent
    /// module and the instantiated module. For example, if the parent module
    /// has a port named `clk` and the instantiated module has a port named
    /// `clk`, passing `Some(&["clk"])` will automatically connect the two
    /// ports.
    pub fn instantiate_array(
        &self,
        moddef: &ModDef,
        dimensions: &[usize],
        prefix: Option<&str>,
        autoconnect: Option<&[&str]>,
    ) -> Vec<ModInst> {
        if dimensions.is_empty() {
            panic!(
                "Array instantiation of {} in {}: dimensions array cannot be empty.",
                moddef.get_name(),
                self.get_name()
            );
        }
        if dimensions.iter().any(|&d| d == 0) {
            panic!(
                "Array instantiation of {} in {}: dimension sizes must be greater than zero.",
                moddef.get_name(),
                self.get_name()
            );
        }

        // Create a vector of ranges based on dimensions
        let ranges: Vec<std::ops::Range<usize>> = dimensions.iter().map(|&d| 0..d).collect();

        // Generate all combinations of indices
        let index_combinations = ranges.into_iter().multi_cartesian_product();

        let mut instances = Vec::new();

        for indices in index_combinations {
            // Build instance name
            let indices_str = indices
                .iter()
                .map(|&i| i.to_string())
                .collect::<Vec<String>>()
                .join("_");

            let instance_name = match prefix {
                Some(pfx) => {
                    if indices_str.is_empty() {
                        pfx.to_string()
                    } else {
                        format!("{}_{}", pfx, indices_str)
                    }
                }
                None => {
                    let moddef_name = &moddef.core.borrow().name;
                    if indices_str.is_empty() {
                        format!("{}_i", moddef_name)
                    } else {
                        format!("{}_i_{}", moddef_name, indices_str)
                    }
                }
            };

            // Instantiate the moddef
            let inst = self.instantiate(moddef, Some(&instance_name), autoconnect);
            instances.push(inst);
        }

        instances
    }

    /// Writes Verilog code for this module definition to the given file path.
    /// If `validate` is `true`, validate the module definition before emitting
    /// Verilog.
    pub fn emit_to_file(&self, path: &Path, validate: bool) {
        let err_msg = format!("emitting ModDef to file at path: {:?}", path);
        std::fs::write(path, self.emit(validate)).expect(&err_msg);
    }

    /// Returns Verilog code for this module definition as a string. If
    /// `validate` is `true`, validate the module definition before emitting
    /// Verilog.
    pub fn emit(&self, validate: bool) -> String {
        if validate {
            self.validate();
        }
        let mut emitted_module_names = IndexMap::new();
        let mut file = VastFile::new(VastFileType::SystemVerilog);
        let mut leaf_text = Vec::new();
        let mut enum_remapping = IndexMap::new();
        self.emit_recursive(
            &mut emitted_module_names,
            &mut file,
            &mut leaf_text,
            &mut enum_remapping,
        );
        leaf_text.push(file.emit());
        let result = leaf_text.join("\n");
        let result = inout::rename_inout(result);
        enum_type::remap_enum_types(result, &enum_remapping)
    }

    fn emit_recursive(
        &self,
        emitted_module_names: &mut IndexMap<String, Rc<RefCell<ModDefCore>>>,
        file: &mut VastFile,
        leaf_text: &mut Vec<String>,
        enum_remapping: &mut IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
    ) {
        let core = self.core.borrow();
        let mut pipeline_counter = 0usize..;

        match emitted_module_names.entry(core.name.clone()) {
            Entry::Occupied(entry) => {
                let existing_moddef = entry.get();
                if !Rc::ptr_eq(existing_moddef, &self.core) {
                    panic!("Two distinct modules with the same name: {}", core.name);
                } else {
                    return;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(self.core.clone());
            }
        }

        if core.usage == Usage::EmitNothingAndStop {
            return;
        } else if core.usage == Usage::EmitDefinitionAndStop {
            leaf_text.push(core.generated_verilog.clone().unwrap());
            return;
        }

        // Recursively emit instances

        if core.usage == Usage::EmitDefinitionAndDescend {
            for inst in core.instances.values() {
                ModDef { core: inst.clone() }.emit_recursive(
                    emitted_module_names,
                    file,
                    leaf_text,
                    enum_remapping,
                );
            }
        }

        // Start the module declaration.

        let mut module = file.add_module(&core.name);

        let mut ports: IndexMap<String, LogicRef> = IndexMap::new();

        for port_name in core.ports.keys() {
            let io = core.ports.get(port_name).unwrap();
            if ports.contains_key(port_name) {
                panic!("Port {}.{} is already declared", core.name, port_name);
            }
            let logic_ref =
                match io {
                    IO::Input(width) => module
                        .add_input(port_name, &file.make_bit_vector_type(*width as i64, false)),
                    IO::Output(width) => module
                        .add_output(port_name, &file.make_bit_vector_type(*width as i64, false)),
                    // TODO(sherbst) 11/18/24: Replace with VAST API call
                    IO::InOut(width) => module.add_input(
                        &format!("{}{}", port_name, inout::INOUT_MARKER),
                        &file.make_bit_vector_type(*width as i64, false),
                    ),
                };
            ports.insert(port_name.clone(), logic_ref);
        }

        if core.usage == Usage::EmitStubAndStop {
            return;
        }

        // List out the wires to be used for internal connections.
        let mut nets: IndexMap<String, LogicRef> = IndexMap::new();
        for (inst_name, inst) in core.instances.iter() {
            for (port_name, io) in inst.borrow().ports.iter() {
                if self
                    .core
                    .borrow()
                    .whole_port_tieoffs
                    .contains_key(inst_name)
                    && self.core.borrow().whole_port_tieoffs[inst_name].contains_key(port_name)
                {
                    // skip whole port tieoffs; they are handled in the instantiation
                    continue;
                }
                if core.inst_connections.contains_key(inst_name)
                    && core
                        .inst_connections
                        .get(inst_name)
                        .unwrap()
                        .contains_key(port_name)
                {
                    // Don't create a wire for a port that is directly connected to a module
                    // definition port
                    continue;
                }
                let net_name = format!("{}_{}", inst_name, port_name);
                if ports.contains_key(&net_name) {
                    panic!("Generated net name for instance port {}.{} collides with a port name on module definition {}: \
both are called {}. Altering the instance name will likely fix this problem. connect_to_net() could also be used to \
specify an alternate net name for this instance port, although that may be more labor-intensive since all connectivity \
on that net will need to be updated.", 
                        inst_name, port_name, core.name, net_name
                    );
                }
                let data_type = file.make_bit_vector_type(io.width() as i64, false);
                if nets
                    .insert(net_name.clone(), module.add_wire(&net_name, &data_type))
                    .is_some()
                {
                    panic!("Generated net name for instance port {}.{} collides with another generated net name within \
module definition {}: both are called {}. Altering the instance name will likely fix this problem. connect_to_net() could \
also be used to specify an alternate net name for this instance port, although that may be more labor-intensive since all \
connectivity on that net will need to be updated.",
                        inst_name, port_name, core.name, net_name);
                }

                if inst.borrow().enum_ports.contains_key(port_name) {
                    enum_remapping
                        .entry(core.name.clone())
                        .or_default()
                        .entry(inst_name.clone())
                        .or_default()
                        .insert(
                            port_name.clone(),
                            inst.borrow().enum_ports.get(port_name).unwrap().clone(),
                        );
                }
            }
        }

        // Create wires for reserved net definitions.
        for wire in core.reserved_net_definitions.values() {
            if nets
                .insert(
                    wire.name.clone(),
                    module.add_wire(
                        &wire.name,
                        &file.make_bit_vector_type(wire.width as i64, false),
                    ),
                )
                .is_some()
            {
                panic!("connect_to_net()-specified net name {} already exists in module definition {}. \
This is likely due to a collision with a generated net name, which has the form {{instance name}}_{{port name}}. \
Two possible solutions: 1) change the instance name corresponding to the generated net name, or 2) provide an \
alternate net name to connect_to_net().",
                    wire.name, core.name
                );
            }
        }

        // Instantiate modules.
        for (inst_name, inst) in core.instances.iter() {
            let module_name = &inst.borrow().name;
            let instance_name = inst_name;
            let parameter_port_names: Vec<&str> = Vec::new();
            let parameter_expressions: Vec<&Expr> = Vec::new();
            let mut connection_port_names = Vec::new();
            let mut connection_expressions = Vec::new();

            for (port_name, io) in inst.borrow().ports.iter() {
                connection_port_names.push(port_name.clone());

                if core.inst_connections.contains_key(inst_name)
                    && core
                        .inst_connections
                        .get(inst_name)
                        .unwrap()
                        .contains_key(port_name)
                {
                    let mut port_slices = core
                        .inst_connections
                        .get(inst_name)
                        .unwrap()
                        .get(port_name)
                        .unwrap()
                        .clone();
                    port_slices.sort_by(|a, b| b.inst_port_slice.msb.cmp(&a.inst_port_slice.msb));

                    let mut concat_entries = Vec::new();
                    let mut msb_expected: i64 = (io.width() as i64) - 1;

                    for port_slice in port_slices {
                        // create a filler if needed
                        if port_slice.inst_port_slice.msb as i64 > msb_expected {
                            panic!(
                                "Instance port slice index {} is out of bounds for instance port {}.{} in module {}, \
since the width of that port is {}. Check the slice indices for this instance port.", 
                                port_slice.inst_port_slice.msb, inst_name, port_name, core.name, io.width()
                            );
                        }

                        if (port_slice.inst_port_slice.msb as i64) < msb_expected {
                            let filler_msb = msb_expected;
                            let filler_lsb = (port_slice.inst_port_slice.msb as i64) + 1;
                            let net_name = format!(
                                "UNUSED_{}_{}_{}_{}",
                                inst_name, port_name, filler_msb, filler_lsb
                            );
                            let data_type =
                                file.make_bit_vector_type(filler_msb - filler_lsb + 1, false);
                            let wire = module.add_wire(&net_name, &data_type);
                            concat_entries.push(wire.to_expr());
                            if nets.insert(net_name.clone(), wire).is_some() {
                                panic!("Generated net name {} for instance port {}.{} already exists in module definition \
{}. If possible, changing the instance name will likely resolve this issue.", net_name, inst_name, port_name, core.name);
                            }
                        }

                        msb_expected = (port_slice.inst_port_slice.lsb as i64) - 1;

                        match &port_slice.connected_to {
                            PortSliceOrWire::PortSlice(port_slice) => concat_entries.push(
                                file.make_slice(
                                    &ports
                                        .get(&port_slice.port.get_port_name())
                                        .unwrap()
                                        .to_indexable_expr(),
                                    port_slice.msb as i64,
                                    port_slice.lsb as i64,
                                )
                                .to_expr(),
                            ),
                            PortSliceOrWire::Wire(wire) => {
                                concat_entries.push(nets.get(&wire.name).unwrap().to_expr());
                            }
                        }
                    }

                    if msb_expected > -1 {
                        let filler_msb = msb_expected;
                        let filler_lsb = 0;
                        let net_name = format!(
                            "UNUSED_{}_{}_{}_{}",
                            inst_name, port_name, filler_msb, filler_lsb
                        );
                        let data_type =
                            file.make_bit_vector_type(filler_msb - filler_lsb + 1, false);
                        let wire = module.add_wire(&net_name, &data_type);
                        concat_entries.push(wire.to_expr());
                        if nets.insert(net_name.clone(), wire).is_some() {
                            panic!("Generated net name {} for instance port {}.{} already exists in module definition \
{}. If possible, changing the instance name will likely resolve this issue.", net_name, inst_name, port_name, core.name);
                        }
                    }

                    if concat_entries.len() == 1 {
                        connection_expressions.push(Some(concat_entries.remove(0)));
                    } else {
                        let slice_references: Vec<&Expr> = concat_entries.iter().collect();
                        connection_expressions.push(Some(file.make_concat(&slice_references)));
                    }
                } else if self
                    .core
                    .borrow()
                    .whole_port_tieoffs
                    .contains_key(inst_name)
                    && self.core.borrow().whole_port_tieoffs[inst_name].contains_key(port_name)
                {
                    let value = self.core.borrow().whole_port_tieoffs[inst_name][port_name].clone();
                    let literal_str = format!("bits[{}]:{}", io.width(), value);
                    let value_expr = file
                        .make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex)
                        .unwrap();
                    connection_expressions.push(Some(value_expr));
                } else {
                    let net_name = format!("{}_{}", inst_name, port_name);
                    connection_expressions.push(Some(nets.get(&net_name).unwrap().to_expr()));
                }
            }

            let instantiation = file.make_instantiation(
                module_name,
                instance_name,
                &parameter_port_names,
                &parameter_expressions,
                &connection_port_names
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>(),
                &connection_expressions
                    .iter()
                    .map(|o| o.as_ref())
                    .collect::<Vec<_>>(),
            );
            module.add_member_instantiation(instantiation);
        }

        // Emit assign statements for connections.
        for Assignment { lhs, rhs, pipeline } in &core.assignments {
            let lhs_slice = match lhs {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => file.make_slice(
                    &ports.get(name).unwrap().to_indexable_expr(),
                    *msb as i64,
                    *lsb as i64,
                ),
                PortSlice {
                    port:
                        Port::ModInst {
                            inst_name,
                            port_name,
                            ..
                        },
                    msb,
                    lsb,
                } => {
                    let net_name = format!("{}_{}", inst_name, port_name);
                    file.make_slice(
                        &nets.get(&net_name).unwrap().to_indexable_expr(),
                        *msb as i64,
                        *lsb as i64,
                    )
                }
            };
            let rhs_slice = match rhs {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => file.make_slice(
                    &ports.get(name).unwrap().to_indexable_expr(),
                    *msb as i64,
                    *lsb as i64,
                ),
                PortSlice {
                    port:
                        Port::ModInst {
                            inst_name,
                            port_name,
                            ..
                        },
                    msb,
                    lsb,
                } => {
                    let net_name = format!("{}_{}", inst_name, port_name);
                    file.make_slice(
                        &nets.get(&net_name).unwrap().to_indexable_expr(),
                        *msb as i64,
                        *lsb as i64,
                    )
                }
            };
            match pipeline {
                None => {
                    let assignment =
                        file.make_continuous_assignment(&lhs_slice.to_expr(), &rhs_slice.to_expr());
                    module.add_member_continuous_assignment(assignment);
                }
                Some(pipeline) => {
                    // Find a unique name for the pipeline instance
                    let pipeline_inst_name = loop {
                        let name = format!("pipeline_conn_{}", pipeline_counter.next().unwrap());
                        if !core.instances.contains_key(&name) {
                            break name;
                        }
                    };
                    let pipeline_details = PipelineDetails {
                        file,
                        module: &mut module,
                        inst_name: &pipeline_inst_name,
                        clk: &ports
                            .get(&pipeline.clk)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Pipeline clock {} is not defined as a port of module {}.",
                                    pipeline.clk, core.name
                                )
                            })
                            .to_expr(),
                        width: lhs.width(),
                        depth: pipeline.depth,
                        pipe_in: &rhs_slice.to_expr(),
                        pipe_out: &lhs_slice.to_expr(),
                    };
                    add_pipeline(pipeline_details);
                }
            };
        }

        // Emit assign statements for tieoffs.
        for (dst, value) in &core.tieoffs {
            if let Port::ModInst { .. } = &dst.port {
                if dst.port.io().width() == dst.width() {
                    // skip whole port tieoffs; they are handled in the instantiation
                    continue;
                }
            }
            let (dst_expr, width) = match dst {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => (
                    file.make_slice(
                        &ports.get(name).unwrap().to_indexable_expr(),
                        *msb as i64,
                        *lsb as i64,
                    ),
                    msb - lsb + 1,
                ),
                PortSlice {
                    port:
                        Port::ModInst {
                            inst_name,
                            port_name,
                            ..
                        },
                    msb,
                    lsb,
                } => {
                    let net_name = format!("{}_{}", inst_name, port_name);
                    (
                        file.make_slice(
                            &nets.get(&net_name).unwrap().to_indexable_expr(),
                            *msb as i64,
                            *lsb as i64,
                        ),
                        msb - lsb + 1,
                    )
                }
            };
            let literal_str = format!("bits[{}]:{}", width, value);
            let value_expr =
                file.make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex);
            let assignment =
                file.make_continuous_assignment(&dst_expr.to_expr(), &value_expr.unwrap());
            module.add_member_continuous_assignment(assignment);
        }
    }

    /// Defines an interface with the given name. `mapping` is a map from
    /// function names to tuples of `(port_name, msb, lsb)`. For example, if
    /// `mapping` is `{"data": ("a_data", 3, 0), "valid": ("a_valid", 1, 1)}`,
    /// this defines an interface with two functions, `data` and `valid`, where
    /// the `data` function is provided by the port slice `a_data[3:0]` and the
    /// `valid` function is provided by the port slice `[1:1]`.
    pub fn def_intf(
        &self,
        name: impl AsRef<str>,
        mapping: IndexMap<String, (String, usize, usize)>,
    ) -> Intf {
        let mut core = self.core.borrow_mut();
        if core.interfaces.contains_key(name.as_ref()) {
            panic!(
                "Interface {} already exists in module {}",
                name.as_ref(),
                core.name
            );
        }
        core.interfaces.insert(name.as_ref().to_string(), mapping);
        Intf::ModDef {
            name: name.as_ref().to_string(),
            mod_def_core: Rc::downgrade(&self.core),
        }
    }

    /// Defines an interface with the given name, where the function names are
    /// derived from the port names by stripping a common prefix. For example,
    /// if the module has ports `a_data`, `a_valid`, `b_data`, and `b_valid`,
    /// calling `def_intf_from_prefix("a_intf", "a_")` will define an interface
    /// with functions `data` and `valid`, where `data` is provided by the full
    /// port `a_data` and `valid` is provided by the full port `a_valid`.
    pub fn def_intf_from_prefix(&self, name: impl AsRef<str>, prefix: impl AsRef<str>) -> Intf {
        self.def_intf_from_prefixes(name, &[prefix.as_ref()], true)
    }

    /// Defines an interface with the given name, where the function names are
    /// derived from the port names by stripping the prefix `<name>_`. For
    /// example, if the module has ports `a_data`, `a_valid`, `b_data`, and
    /// `b_valid`, calling `def_intf_from_prefix("a")` will define an
    /// interface with functions `data` and `valid`, where `data` is provided by
    /// the full port `a_data` and `valid` is provided by the full port
    /// `a_valid`.
    pub fn def_intf_from_name_underscore(&self, name: impl AsRef<str>) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.def_intf_from_prefix(name, prefix)
    }

    /// Defines an interface with the given name, where the signals to be
    /// included are identified by those that start with one of the provided
    /// prefixies. Function names are either the signal names themselves (if
    /// `strip_prefix` is `false`) or by stripping the prefix (if `strip_prefix`
    /// is true). For example, if the module has ports `a_data`, `a_valid`,
    /// `b_data`, and `b_valid`, calling `def_intf_from_prefixes("intf", &["a_",
    /// "b_"], false)` will define an interface with functions `a_data`,
    /// `a_valid`, `b_data`, and `b_valid`, where each function is provided by
    /// the corresponding port.
    pub fn def_intf_from_prefixes(
        &self,
        name: impl AsRef<str>,
        prefixes: &[&str],
        strip_prefix: bool,
    ) -> Intf {
        let mut mapping = IndexMap::new();
        {
            let core = self.core.borrow();
            for port_name in core.ports.keys() {
                for prefix in prefixes {
                    if port_name.starts_with(prefix) {
                        let func_name = if strip_prefix {
                            port_name.strip_prefix(prefix).unwrap().to_string()
                        } else {
                            port_name.clone()
                        };
                        let port = self.get_port(port_name);
                        mapping.insert(func_name, (port_name.clone(), port.io().width() - 1, 0));
                        break;
                    }
                }
            }
        }

        assert!(
            !mapping.is_empty(),
            "Empty interface definition for {}.{}",
            self.get_name(),
            name.as_ref()
        );

        self.def_intf(name, mapping)
    }

    pub fn def_intf_from_regex(
        &self,
        name: impl AsRef<str>,
        search: impl AsRef<str>,
        replace: impl AsRef<str>,
    ) -> Intf {
        self.def_intf_from_regexes(name, &[(search.as_ref(), replace.as_ref())])
    }

    pub fn def_intf_from_regexes(&self, name: impl AsRef<str>, regexes: &[(&str, &str)]) -> Intf {
        let mut mapping = IndexMap::new();
        let regexes = regexes
            .iter()
            .map(|(search, replace)| {
                (
                    Regex::new(search).expect("Failed to compile regex"),
                    replace,
                )
            })
            .collect::<Vec<_>>();
        {
            let core = self.core.borrow();
            for port_name in core.ports.keys() {
                for (regex, replace) in &regexes {
                    if regex.is_match(port_name) {
                        let func_name = regex.replace(port_name, **replace).to_string();
                        let port = self.get_port(port_name);
                        mapping.insert(func_name, (port_name.clone(), port.io().width() - 1, 0));
                        break;
                    }
                }
            }
        }

        assert!(
            !mapping.is_empty(),
            "Empty interface definition for {}.{}",
            self.get_name(),
            name.as_ref()
        );

        self.def_intf(name, mapping)
    }

    /// Returns the interface with the given name; panics if an interface with
    /// that name does not exist.
    pub fn get_intf(&self, name: impl AsRef<str>) -> Intf {
        let core = self.core.borrow();
        if core.interfaces.contains_key(name.as_ref()) {
            Intf::ModDef {
                name: name.as_ref().to_string(),
                mod_def_core: Rc::downgrade(&self.core),
            }
        } else {
            panic!(
                "Interface '{}' does not exist in module '{}'",
                name.as_ref(),
                core.name
            );
        }
    }

    /// Punches a feedthrough through this module definition with the given
    /// input and output names and width. This will create two new ports on the
    /// module definition, `input_name[width-1:0]` and `output_name[width-1:0]`,
    /// and connect them together.
    pub fn feedthrough(
        &self,
        input_name: impl AsRef<str>,
        output_name: impl AsRef<str>,
        width: usize,
    ) {
        self.feedthrough_generic(input_name, output_name, width, None);
    }

    pub fn feedthrough_pipeline(
        &self,
        input_name: impl AsRef<str>,
        output_name: impl AsRef<str>,
        width: usize,
        pipeline: PipelineConfig,
    ) {
        self.feedthrough_generic(input_name, output_name, width, Some(pipeline));
    }

    fn feedthrough_generic(
        &self,
        input_name: impl AsRef<str>,
        output_name: impl AsRef<str>,
        width: usize,
        pipeline: Option<PipelineConfig>,
    ) {
        let input_port = self.add_port(input_name, IO::Input(width));
        let output_port = self.add_port(output_name, IO::Output(width));
        input_port.connect_generic(&output_port, pipeline);
    }

    /// Instantiates this module definition within a new module definition, and
    /// returns the new module definition. The new module definition has all of
    /// the same ports as the original module, which are connected directly to
    /// ports with the same names on the instance of the original module.
    pub fn wrap(&self, def_name: Option<&str>, inst_name: Option<&str>) -> ModDef {
        let original_name = &self.core.borrow().name;

        let def_name_default;
        let def_name = if let Some(name) = def_name {
            name
        } else {
            def_name_default = format!("{}_wrapper", original_name);
            def_name_default.as_str()
        };

        let wrapper = ModDef::new(def_name);

        let inst = wrapper.instantiate(self, inst_name, None);

        // Copy interface definitions.
        {
            let original_core = self.core.borrow();
            let mut wrapper_core = wrapper.core.borrow_mut();

            // Copy interface definitions
            for (intf_name, mapping) in &original_core.interfaces {
                wrapper_core
                    .interfaces
                    .insert(intf_name.clone(), mapping.clone());
            }
        }

        // For each port in the original module, add a corresponding port to the wrapper
        // and connect them.
        for (port_name, io) in self.core.borrow().ports.iter() {
            let wrapper_port = wrapper.add_port(port_name, io.clone());
            let inst_port = inst.get_port(port_name);
            wrapper_port.connect(&inst_port);
        }

        wrapper
    }

    /// Returns a new module definition that is a variant of this module
    /// definition, where the given parameters have been overridden from their
    /// default values. For example, if the module definition has a parameter
    /// `WIDTH` with a default value of `32`, calling `parameterize(&[("WIDTH",
    /// 64)])` will return a new module definition with the same ports and
    /// instances, but with the parameter `WIDTH` set to `64`. This is
    /// implemented by creating a wrapper module that instantiates the original
    /// module with the given parameters. The name of the wrapper module
    /// defaults to
    /// `<original_mod_def_name>_<param_name_0>_<param_value_0>_<param_name_1>_<param_value_1>_.
    /// ..`; this can be overridden via the optional `def_name` argument. The
    /// instance name of the original module within the wrapper is
    /// `<original_mod_def_name>_i`; this can be overridden via the optional
    /// `inst_name` argument.
    pub fn parameterize(
        &self,
        parameters: &[(&str, i32)],
        def_name: Option<&str>,
        inst_name: Option<&str>,
    ) -> ModDef {
        let core = self.core.borrow();

        if core.verilog_import.is_none() {
            panic!("Error parameterizing {}: can only parameterize a module defined in external Verilog sources.", core.name);
        }

        // Determine the name of the definition if not provided.
        let original_name = &self.core.borrow().name;
        let mut def_name_default = original_name.clone();
        for (param_name, param_value) in parameters {
            def_name_default.push_str(&format!("_{}_{}", param_name, param_value));
        }
        let def_name = def_name.unwrap_or(&def_name_default);

        // Determine the name of the instance inside the wrapper if not provided.
        let inst_name_default = format!("{}_i", original_name);
        let inst_name = inst_name.unwrap_or(&inst_name_default);

        // Determine the I/O for the module.
        let parameters_with_string_values = parameters
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect::<Vec<(String, String)>>();

        let sources: Vec<&str> = core
            .verilog_import
            .as_ref()
            .unwrap()
            .sources
            .iter()
            .map(|s| s.as_str())
            .collect();

        let incdirs: Vec<&str> = core
            .verilog_import
            .as_ref()
            .unwrap()
            .incdirs
            .iter()
            .map(|s| s.as_str())
            .collect();

        let cfg = SlangConfig {
            sources: sources.as_slice(),
            incdirs: incdirs.as_slice(),
            parameters: &parameters_with_string_values
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            ignore_unknown_modules: core.verilog_import.as_ref().unwrap().ignore_unknown_modules,
            ..Default::default()
        };

        let parser_ports = extract_ports(&cfg, true);

        // Generate a wrapper that sets the parameters to the given values.
        let mut file = VastFile::new(VastFileType::Verilog);

        let mut wrapped_module = file.add_module(def_name);
        let mut connection_port_names = Vec::new();
        let mut connection_logic_refs = Vec::new();
        let mut connection_expressions = Vec::new();
        for parser_port in parser_ports[&core.name].iter() {
            match parser_port_to_port(parser_port) {
                Ok((name, io)) => {
                    let logic_expr = match io {
                        IO::Input(width) => wrapped_module.add_input(
                            name.as_str(),
                            &file.make_bit_vector_type(width as i64, false),
                        ),
                        IO::Output(width) => wrapped_module.add_output(
                            name.as_str(),
                            &file.make_bit_vector_type(width as i64, false),
                        ),
                        // TODO(sherbst) 11/18/24: Replace with VAST API call
                        IO::InOut(width) => wrapped_module.add_input(
                            &format!("{}{}", name, inout::INOUT_MARKER),
                            &file.make_bit_vector_type(width as i64, false),
                        ),
                    };
                    connection_port_names.push(name.clone());
                    connection_expressions.push(Some(logic_expr.to_expr()));
                    connection_logic_refs.push(logic_expr);
                }
                Err(e) => {
                    if !core.verilog_import.as_ref().unwrap().skip_unsupported {
                        panic!("{e}");
                    } else {
                        continue;
                    }
                }
            }
        }

        let mut parameter_port_names = Vec::new();
        let mut parameter_port_expressions = Vec::new();

        for (name, value) in parameters {
            parameter_port_names.push(name);
            // TODO(sherbst) 09/24/2024: support parameter values other than 32-bit
            // integers.
            let literal_str = format!("bits[{}]:{}", 32, value);
            let expr = file
                .make_literal(&literal_str, &xlsynth::ir_value::IrFormatPreference::Hex)
                .unwrap();
            parameter_port_expressions.push(expr);
        }

        wrapped_module.add_member_instantiation(
            file.make_instantiation(
                core.name.as_str(),
                inst_name,
                &parameter_port_names
                    .iter()
                    .map(|&&s| s)
                    .collect::<Vec<&str>>(),
                &parameter_port_expressions.iter().collect::<Vec<_>>(),
                &connection_port_names
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>(),
                &connection_expressions
                    .iter()
                    .map(|o| o.as_ref())
                    .collect::<Vec<_>>(),
            ),
        );

        let verilog = file.emit();

        let mut ports = IndexMap::new();
        let mut enum_remapping: IndexMap<String, IndexMap<String, IndexMap<String, String>>> =
            IndexMap::new();
        for parser_port in parser_ports[&core.name].iter() {
            match parser_port_to_port(parser_port) {
                Ok((name, io)) => {
                    ports.insert(name.clone(), io.clone());
                    // Enum input ports that are not a packed array require special handling
                    // They need to have casting to be valid Verilog.
                    if let slang_rs::Type::Enum {
                        name: enum_name,
                        packed_dimensions,
                        unpacked_dimensions,
                        ..
                    } = &parser_port.ty
                    {
                        if packed_dimensions.is_empty() && unpacked_dimensions.is_empty() {
                            if let IO::Input(_) = io {
                                enum_remapping
                                    .entry(def_name.to_string())
                                    .or_default()
                                    .entry(inst_name.to_string())
                                    .or_default()
                                    .insert(name.clone(), enum_name.clone());
                            }
                        }
                    }
                }
                Err(e) => {
                    if !core.verilog_import.as_ref().unwrap().skip_unsupported {
                        panic!("{e}");
                    } else {
                        continue;
                    }
                }
            }
        }

        let verilog = enum_type::remap_enum_types(verilog, &enum_remapping);

        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: def_name.to_string(),
                ports,
                enum_ports: IndexMap::new(),
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Usage::EmitDefinitionAndStop,
                generated_verilog: Some(verilog.to_string()),
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                whole_port_tieoffs: IndexMap::new(),
                verilog_import: None,
                inst_connections: IndexMap::new(),
                reserved_net_definitions: IndexMap::new(),
            })),
        }
    }

    /// Validates this module hierarchically; panics if any errors are found.
    /// Validation primarily consists of checking that all inputs are driven
    /// exactly once, and all outputs are used at least once, unless
    /// specifically marked as unused. Validation behavior is controlled via the
    /// usage setting. If this module has the usage `EmitDefinitionAndDescend`,
    /// validation descends into each of those module definitions before
    /// validating the module. If this module definition has a usage other than
    /// `EmitDefinitionAndDescend`, it is not validated, and the modules it
    /// instantiates are not validated.
    pub fn validate(&self) {
        // TODO(sherbst) 10/16/2024: do not validate the same module twice

        if self.core.borrow().usage != Usage::EmitDefinitionAndDescend {
            return;
        }

        // First, recursively validate submodules
        for instance in self.core.borrow().instances.values() {
            ModDef {
                core: instance.clone(),
            }
            .validate();
        }

        let mut driven_bits: IndexMap<PortKey, DrivenPortBits> = IndexMap::new();
        let mut driving_bits: IndexMap<PortKey, DrivingPortBits> = IndexMap::new();

        // Initialize ModDef outputs
        let mod_def_core = self.core.borrow();

        for (port_name, io) in &mod_def_core.ports {
            let width = io.width();
            match io {
                IO::Output(_) => {
                    driven_bits.insert(
                        PortKey::ModDefPort {
                            mod_def_name: mod_def_core.name.clone(),
                            port_name: port_name.clone(),
                        },
                        DrivenPortBits::new(width),
                    );
                }
                IO::Input(_) | IO::InOut(_) => {
                    driving_bits.insert(
                        PortKey::ModDefPort {
                            mod_def_name: mod_def_core.name.clone(),
                            port_name: port_name.clone(),
                        },
                        DrivingPortBits::new(width),
                    );
                }
            }
        }

        // Initialize ModInst ports
        for (inst_name, inst_core) in &mod_def_core.instances {
            let inst_ports = &inst_core.borrow().ports;
            for (port_name, io) in inst_ports {
                let width = io.width();
                match io {
                    IO::Input(_) => {
                        driven_bits.insert(
                            PortKey::ModInstPort {
                                mod_def_name: mod_def_core.name.clone(),
                                inst_name: inst_name.clone(),
                                port_name: port_name.clone(),
                            },
                            DrivenPortBits::new(width),
                        );
                    }
                    IO::Output(_) | IO::InOut(_) => {
                        driving_bits.insert(
                            PortKey::ModInstPort {
                                mod_def_name: mod_def_core.name.clone(),
                                inst_name: inst_name.clone(),
                                port_name: port_name.clone(),
                            },
                            DrivingPortBits::new(width),
                        );
                    }
                }
            }
        }

        // Process unused

        for unused_slice in &self.core.borrow().unused {
            // check msb/lsb range
            unused_slice.check_validity();

            // check directionality
            if !Self::can_drive(unused_slice) {
                panic!(
                    "Cannot mark {} as unused because it is not a driver.",
                    unused_slice.debug_string()
                );
            }

            // check context
            if !Self::is_in_mod_def_core(unused_slice, &self.core) {
                panic!(
                    "Unused slice {} is not in module {}",
                    unused_slice.debug_string(),
                    self.core.borrow().name
                );
            }

            let key = unused_slice.port.to_port_key();

            let result = driving_bits
                .get_mut(&key)
                .unwrap()
                .unused(unused_slice.msb, unused_slice.lsb);

            match result {
                Err(UnusedError::AlreadyMarkedUnused) => {
                    panic!(
                        "{} is marked as unused multiple times.",
                        unused_slice.debug_string()
                    );
                }
                Err(UnusedError::AlreadyUsed) => {
                    panic!(
                        "{} is marked as unused, but is used somewhere.",
                        unused_slice.debug_string()
                    );
                }
                Ok(()) => {}
            }
        }

        // Process tieoffs

        for (tieoff_slice, _) in &self.core.borrow().tieoffs {
            // check msb/lsb range
            tieoff_slice.check_validity();

            // check directionality
            if !Self::can_be_driven(tieoff_slice) {
                panic!(
                    "Cannot tie off {} because it cannot be driven.",
                    tieoff_slice.debug_string()
                );
            }

            // check context
            if !Self::is_in_mod_def_core(tieoff_slice, &self.core) {
                panic!(
                    "Tieoff slice {} is not in module {}",
                    tieoff_slice.debug_string(),
                    self.core.borrow().name
                );
            }

            let key = tieoff_slice.port.to_port_key();

            let result = driven_bits
                .get_mut(&key)
                .unwrap()
                .driven(tieoff_slice.msb, tieoff_slice.lsb);

            if result.is_err() {
                panic!("{} is multiply driven.", tieoff_slice.debug_string());
            }
        }

        // Process assignments

        for Assignment {
            lhs: lhs_slice,
            rhs: rhs_slice,
            pipeline,
        } in &self.core.borrow().assignments
        {
            for slice in [&lhs_slice, &rhs_slice] {
                // check msb/lsb range
                slice.check_validity();

                // check context
                if !Self::is_in_mod_def_core(slice, &self.core) {
                    panic!(
                        "Slice {} is not in module {}",
                        slice.debug_string(),
                        self.core.borrow().name
                    );
                }
            }

            // check directionality

            if !Self::can_be_driven(lhs_slice) {
                panic!("{} cannot be driven.", lhs_slice.debug_string());
            }

            if !Self::can_drive(rhs_slice) {
                panic!("{} cannot drive.", rhs_slice.debug_string());
            }

            // check that widths match
            let lhs_width = lhs_slice.msb - lhs_slice.lsb + 1;
            let rhs_width = rhs_slice.msb - rhs_slice.lsb + 1;
            if lhs_width != rhs_width {
                panic!(
                    "Width mismatch in connection between {} and {}",
                    lhs_slice.debug_string(),
                    rhs_slice.debug_string()
                );
            }

            let lhs_key = lhs_slice.port.to_port_key();
            let rhs_key = rhs_slice.port.to_port_key();

            let result = driven_bits
                .get_mut(&lhs_key)
                .unwrap()
                .driven(lhs_slice.msb, lhs_slice.lsb);
            if result.is_err() {
                panic!("{} is multiply driven.", lhs_slice.debug_string());
            }

            let result = driving_bits
                .get_mut(&rhs_key)
                .unwrap()
                .driving(rhs_slice.msb, rhs_slice.lsb);
            if result.is_err() {
                panic!(
                    "{} is marked as unused, but is used somewhere.",
                    rhs_slice.debug_string()
                );
            }

            if let Some(pipeline) = &pipeline {
                let clk_key = PortKey::ModDefPort {
                    mod_def_name: mod_def_core.name.clone(),
                    port_name: pipeline.clk.clone(),
                };
                let result = driving_bits.get_mut(&clk_key).unwrap().driving(0, 0);
                if result.is_err() {
                    panic!(
                        "Pipeline clock {}.{} is marked as unused.",
                        mod_def_core.name, pipeline.clk
                    );
                }
            }
        }

        // process instance connections

        for inst_connections in mod_def_core.inst_connections.values() {
            for connections in inst_connections.values() {
                for inst_connection in connections {
                    let inst_slice = &inst_connection.inst_port_slice;
                    inst_slice.check_validity();

                    // check context
                    if !Self::is_in_mod_def_core(inst_slice, &self.core) {
                        panic!(
                            "Slice {} is not in module {}",
                            inst_slice.debug_string(),
                            self.core.borrow().name
                        );
                    }

                    // check that widths match
                    let inst_slice_width = inst_slice.msb - inst_slice.lsb + 1;
                    let connected_to_width = match &inst_connection.connected_to {
                        PortSliceOrWire::PortSlice(other_slice) => {
                            other_slice.msb - other_slice.lsb + 1
                        }
                        PortSliceOrWire::Wire(wire) => wire.width,
                    };

                    if inst_slice_width != connected_to_width {
                        panic!(
                            "Width mismatch in connection to {}",
                            inst_slice.debug_string(),
                        );
                    }

                    let inst_slice_key = inst_slice.port.to_port_key();

                    match inst_slice.port.io() {
                        IO::Input(_) => {
                            let result = driven_bits
                                .get_mut(&inst_slice_key)
                                .unwrap()
                                .driven(inst_slice.msb, inst_slice.lsb);
                            if result.is_err() {
                                panic!("{} is multiply driven.", inst_slice.debug_string());
                            }
                        }
                        IO::Output(_) | IO::InOut(_) => {
                            let result = driving_bits
                                .get_mut(&inst_slice_key)
                                .unwrap()
                                .driving(inst_slice.msb, inst_slice.lsb);
                            if result.is_err() {
                                panic!(
                                    "{} is marked as unused, but is used somewhere.",
                                    inst_slice.debug_string()
                                );
                            }
                        }
                    }

                    if let PortSliceOrWire::PortSlice(other_slice) = &inst_connection.connected_to {
                        let other_slice_key = other_slice.port.to_port_key();
                        match other_slice.port.io() {
                            IO::Output(_) => {
                                let result = driven_bits
                                    .get_mut(&other_slice_key)
                                    .unwrap()
                                    .driven(other_slice.msb, other_slice.lsb);
                                if result.is_err() {
                                    panic!("{} is multiply driven.", other_slice.debug_string());
                                }
                            }
                            IO::Input(_) | IO::InOut(_) => {
                                let result = driving_bits
                                    .get_mut(&other_slice_key)
                                    .unwrap()
                                    .driving(other_slice.msb, other_slice.lsb);
                                if result.is_err() {
                                    panic!(
                                        "{} is marked as unused, but is used somewhere.",
                                        other_slice.debug_string()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // driven bits should be all driven

        for (key, driven) in &driven_bits {
            if !driven.all_driven() {
                panic!(
                    "{}{} ({} {}) is undriven.",
                    key.debug_string(),
                    driven.example_problematic_bits().unwrap(),
                    key.variant_name(),
                    key.retrieve_port_io(&self.core.borrow()).variant_name()
                );
            }
        }

        // driving bits should be all driving or unused

        for (key, driving) in &driving_bits {
            if !driving.all_driving_or_unused() {
                panic!(
                    "{}{} ({} {}) is unused. If this is intentional, mark with unused().",
                    key.debug_string(),
                    driving.example_problematic_bits().unwrap(),
                    key.variant_name(),
                    key.retrieve_port_io(&self.core.borrow()).variant_name()
                );
            }
        }
    }

    fn can_be_driven(slice: &PortSlice) -> bool {
        matches!(
            (&slice.port, slice.port.io(),),
            (Port::ModDef { .. }, IO::Output(_),)
                | (Port::ModInst { .. }, IO::Input(_))
                | (_, IO::InOut(_))
        )
    }

    fn can_drive(slice: &PortSlice) -> bool {
        matches!(
            (&slice.port, slice.port.io(),),
            (Port::ModDef { .. }, IO::Input(_),)
                | (Port::ModInst { .. }, IO::Output(_))
                | (_, IO::InOut(_))
        )
    }

    fn is_in_mod_def_core(slice: &PortSlice, mod_def_core: &Rc<RefCell<ModDefCore>>) -> bool {
        Rc::ptr_eq(&slice.port.get_mod_def_core(), mod_def_core)
    }
}

impl Port {
    fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            Port::ModDef { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
            Port::ModInst { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
        }
    }

    fn get_port_name(&self) -> String {
        match self {
            Port::ModDef { name, .. } => name.clone(),
            Port::ModInst { port_name, .. } => port_name.clone(),
        }
    }

    fn debug_string(&self) -> String {
        match self {
            Port::ModDef { name, mod_def_core } => {
                format!("{}.{}", mod_def_core.upgrade().unwrap().borrow().name, name)
            }
            Port::ModInst {
                inst_name,
                port_name,
                mod_def_core,
            } => format!(
                "{}.{}.{}",
                mod_def_core.upgrade().unwrap().borrow().name,
                inst_name,
                port_name
            ),
        }
    }

    fn debug_string_with_width(&self) -> String {
        format!("{}[{}:{}]", self.debug_string(), self.io().width() - 1, 0)
    }

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

    fn connect_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        pipeline: Option<PipelineConfig>,
    ) {
        self.to_port_slice().connect_generic(other, pipeline);
    }

    /// Punches a feedthrough in the provided module definition for this port.
    pub fn feedthrough(
        &self,
        moddef: &ModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Port, Port) {
        self.to_port_slice().feedthrough(moddef, flipped, original)
    }

    /// Punches a feedthrough in the provided module definition for this port,
    /// with a pipeline.
    pub fn feedthrough_pipeline(
        &self,
        moddef: &ModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) -> (Port, Port) {
        self.to_port_slice()
            .feedthrough_pipeline(moddef, flipped, original, pipeline)
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

    /// Ties off this port to the given constant value, specified as a `BigInt`
    /// or type that can be converted to a `BigInt`.
    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        self.to_port_slice().tieoff(value);
    }

    /// Marks this port as unused, meaning that if it is a module instance
    /// output or module definition input, validation will not fail if the port
    /// drives nothing. In fact, validation will fail if the port drives
    /// anything.
    pub fn unused(&self) {
        self.to_port_slice().unused();
    }

    /// Returns a slice of this port from `msb` down to `lsb`, inclusive.
    pub fn slice(&self, msb: usize, lsb: usize) -> PortSlice {
        if msb >= self.io().width() || lsb > msb {
            panic!(
                "Invalid slice [{}:{}] of port {}",
                msb,
                lsb,
                self.debug_string_with_width()
            );
        }
        PortSlice {
            port: self.clone(),
            msb,
            lsb,
        }
    }

    /// Returns a single-bit slice of this port at the specified index.
    pub fn bit(&self, index: usize) -> PortSlice {
        self.slice(index, index)
    }

    /// Splits this port into `n` equal slices, returning a vector of port
    /// slices. For example, if this port is 8-bit wide and `n` is 4, this will
    /// return a vector of 4 port slices, each 2 bits wide: `[1:0]`, `[3:2]`,
    /// `[5:4]`, and `[7:6]`.
    pub fn subdivide(&self, n: usize) -> Vec<PortSlice> {
        self.to_port_slice().subdivide(n)
    }

    /// Create a new port called `name` on the parent module and connects it to
    /// this port.
    ///
    /// The exact behavior depends on whether this is a port on a module
    /// definition or a module instance. If this is a port on a module
    /// definition, a new port is created on the same module definition, with
    /// the same width, but opposite direction. For example, suppose that this
    /// is a port `a` on a module definition that is an 8-bit input; calling
    /// `export_as("y")` will create an 8-bit output on the same module
    /// definition called `y`.
    ///
    /// If, on the other hand, this is a port on a module instance, a new port
    /// will be created on the module definition containing the instance, with
    /// the same width and direction. For example, if this is an 8-bit input
    /// port `x` on a module instance, calling `export_as("y")` will create a
    /// new 8-bit input port `y` on the module definition that contains the
    /// instance.
    pub fn export_as(&self, name: impl AsRef<str>) -> Port {
        self.to_port_slice().export_as(name)
    }

    /// Same as export_as(), but the new port is created with the same name as
    /// the port being exported. As a result, this method can only be used with
    /// ports on module instances. The method will panic if called on a port on
    /// a module definition.
    pub fn export(&self) -> Port {
        self.to_port_slice().export()
    }
}

impl PortSlice {
    fn debug_string(&self) -> String {
        format!("{}[{}:{}]", self.port.debug_string(), self.msb, self.lsb)
    }

    fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            PortSlice {
                port: Port::ModDef { mod_def_core, .. },
                ..
            } => mod_def_core.upgrade().unwrap(),
            PortSlice {
                port: Port::ModInst { mod_def_core, .. },
                ..
            } => mod_def_core.upgrade().unwrap(),
        }
    }

    /// Connects a port slice to a net with a specific name.
    pub fn connect_to_net(&self, net: &str) {
        if let Port::ModInst {
            inst_name,
            port_name,
            mod_def_core,
        } = &self.port
        {
            let wire = Wire {
                name: net.to_string(),
                width: self.width(),
            };

            // make sure that the net hasn't already been defined in an inconsistent way,
            // then (if it's OK) add it to the reserved net definitions
            let mod_def_core_unwrapped = mod_def_core.upgrade().unwrap();
            let existing_wire = {
                let mut core_borrowed = mod_def_core_unwrapped.borrow_mut();
                core_borrowed
                    .reserved_net_definitions
                    .entry(net.to_string())
                    .or_insert(wire.clone())
                    .clone()
            };

            if existing_wire.width != self.width() {
                panic!(
                    "Net width mismatch for {}.{}: existing width {}, new width {}",
                    mod_def_core_unwrapped.borrow().name,
                    net,
                    existing_wire.width,
                    self.width()
                );
            }

            mod_def_core_unwrapped
                .borrow_mut()
                .inst_connections
                .entry(inst_name.clone())
                .or_default()
                .entry(port_name.clone())
                .or_default()
                .push(InstConnection {
                    inst_port_slice: self.to_port_slice(),
                    connected_to: PortSliceOrWire::Wire(wire),
                });
        } else {
            panic!("connect_to_net() only work on ports (or slices of ports) on module instances");
        }
    }

    /// Connects this port slice to another port or port slice. Performs some
    /// upfront checks to make sure that the connection is valid in terms of
    /// width and directionality. Panics if any of these checks fail.
    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T) {
        self.connect_generic(other, None);
    }

    pub fn connect_pipeline<T: ConvertibleToPortSlice>(&self, other: &T, pipeline: PipelineConfig) {
        self.connect_generic(other, Some(pipeline));
    }

    fn connect_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        pipeline: Option<PipelineConfig>,
    ) {
        let other_as_slice = other.to_port_slice();

        let mod_def_core = self.get_mod_def_core();

        if let (IO::InOut(_), _) | (_, IO::InOut(_)) = (self.port.io(), other_as_slice.port.io()) {
            assert!(pipeline.is_none(), "Cannot pipeline inout ports");
            let mut mod_def_core_borrowed = mod_def_core.borrow_mut();
            match (&self.port, &other_as_slice.port) {
                (Port::ModDef { .. }, Port::ModDef { .. }) => {
                    panic!(
                        "Cannot short inout ports on a module definition: {} and {}",
                        self.debug_string(),
                        other_as_slice.debug_string()
                    );
                }
                (
                    Port::ModDef { .. },
                    Port::ModInst {
                        mod_def_core: _,
                        inst_name,
                        port_name,
                    },
                ) => {
                    mod_def_core_borrowed
                        .inst_connections
                        .entry(inst_name.clone())
                        .or_default()
                        .entry(port_name.clone())
                        .or_default()
                        .push(InstConnection {
                            inst_port_slice: other_as_slice.clone(),
                            connected_to: PortSliceOrWire::PortSlice((*self).clone()),
                        });
                }
                (
                    Port::ModInst {
                        mod_def_core: _,
                        inst_name,
                        port_name,
                    },
                    Port::ModDef { .. },
                ) => {
                    mod_def_core_borrowed
                        .inst_connections
                        .entry(inst_name.clone())
                        .or_default()
                        .entry(port_name.clone())
                        .or_default()
                        .push(InstConnection {
                            inst_port_slice: (*self).clone(),
                            connected_to: PortSliceOrWire::PortSlice(other_as_slice.clone()),
                        });
                }
                (
                    Port::ModInst {
                        inst_name: self_inst_name,
                        port_name: self_port_name,
                        ..
                    },
                    Port::ModInst {
                        inst_name: other_inst_name,
                        port_name: other_port_name,
                        ..
                    },
                ) => {
                    // wire definition
                    let wire_name = format!(
                        "{}_{}_{}_{}_{}_{}_{}_{}",
                        self_inst_name,
                        self_port_name,
                        self.msb,
                        self.lsb,
                        other_inst_name,
                        other_port_name,
                        other_as_slice.msb,
                        other_as_slice.lsb
                    );
                    let wire = Wire {
                        name: wire_name.clone(),
                        width: self.width(),
                    };
                    mod_def_core_borrowed
                        .reserved_net_definitions
                        .insert(wire_name, wire.clone());

                    // self inst connection
                    mod_def_core_borrowed
                        .inst_connections
                        .entry(self_inst_name.clone())
                        .or_default()
                        .entry(self_port_name.clone())
                        .or_default()
                        .push(InstConnection {
                            inst_port_slice: (*self).clone(),
                            connected_to: PortSliceOrWire::Wire(wire.clone()),
                        });

                    // other inst connection
                    mod_def_core_borrowed
                        .inst_connections
                        .entry(other_inst_name.clone())
                        .or_default()
                        .entry(other_port_name.clone())
                        .or_default()
                        .push(InstConnection {
                            inst_port_slice: other_as_slice.clone(),
                            connected_to: PortSliceOrWire::Wire(wire.clone()),
                        });
                }
            }
        } else {
            let (lhs, rhs) = match (
                &self.port,
                self.port.io(),
                &other_as_slice.port,
                other_as_slice.port.io(),
            ) {
                (Port::ModDef { .. }, IO::Output(_), Port::ModDef { .. }, IO::Input(_)) => {
                    (self, &other_as_slice)
                }
                (Port::ModDef { .. }, IO::Input(_), Port::ModDef { .. }, IO::Output(_)) => {
                    (&other_as_slice, self)
                }
                (Port::ModInst { .. }, IO::Input(_), Port::ModDef { .. }, IO::Input(_)) => {
                    (self, &other_as_slice)
                }
                (Port::ModDef { .. }, IO::Input(_), Port::ModInst { .. }, IO::Input(_)) => {
                    (&other_as_slice, self)
                }
                (Port::ModDef { .. }, IO::Output(_), Port::ModInst { .. }, IO::Output(_)) => {
                    (self, &other_as_slice)
                }
                (Port::ModInst { .. }, IO::Output(_), Port::ModDef { .. }, IO::Output(_)) => {
                    (&other_as_slice, self)
                }
                (Port::ModInst { .. }, IO::Input(_), Port::ModInst { .. }, IO::Output(_)) => {
                    (self, &other_as_slice)
                }
                (Port::ModInst { .. }, IO::Output(_), Port::ModInst { .. }, IO::Input(_)) => {
                    (&other_as_slice, self)
                }
                _ => panic!(
                    "Invalid connection between ports: {} ({} {}) and {} ({} {})",
                    self.debug_string(),
                    self.port.variant_name(),
                    self.port.io().variant_name(),
                    other_as_slice.debug_string(),
                    other_as_slice.port.variant_name(),
                    other_as_slice.port.io().variant_name()
                ),
            };

            if let Some(pipeline) = &pipeline {
                if !mod_def_core.borrow().ports.contains_key(&pipeline.clk) {
                    ModDef {
                        core: mod_def_core.clone(),
                    }
                    .add_port(pipeline.clk.clone(), IO::Input(1));
                }
            }
            let lhs = (*lhs).clone();
            let rhs = (*rhs).clone();
            mod_def_core
                .borrow_mut()
                .assignments
                .push(Assignment { lhs, rhs, pipeline });
        }
    }

    /// Punches a feedthrough in the provided module definition for this port
    /// slice.
    pub fn feedthrough(
        &self,
        moddef: &ModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Port, Port) {
        self.feedthrough_generic(moddef, flipped, original, None)
    }

    /// Punches a feedthrough in the provided module definition for this port
    /// slice, with a pipeline.
    pub fn feedthrough_pipeline(
        &self,
        moddef: &ModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) -> (Port, Port) {
        self.feedthrough_generic(moddef, flipped, original, Some(pipeline))
    }

    fn feedthrough_generic(
        &self,
        moddef: &ModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: Option<PipelineConfig>,
    ) -> (Port, Port) {
        let flipped_port = moddef.add_port(flipped, self.port.io().with_width(self.width()).flip());
        let original_port = moddef.add_port(original, self.port.io().with_width(self.width()));
        flipped_port.connect_generic(&original_port, pipeline.clone());
        (flipped_port, original_port)
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this port slice to another port or port slice.
    pub fn connect_through<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        through: &[&ModInst],
        prefix: impl AsRef<str>,
    ) {
        let mut through_generic = Vec::new();
        for inst in through {
            through_generic.push((*inst, None));
        }
        self.connect_through_generic(other, &through_generic, prefix);
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this port slice to another port or port slice,
    /// with optional pipelining for each connection.
    pub fn connect_through_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        through: &[(&ModInst, Option<PipelineConfig>)],
        prefix: impl AsRef<str>,
    ) {
        if through.is_empty() {
            self.connect(other);
            return;
        }

        let flipped = format!("{}_flipped", prefix.as_ref());
        let original = format!("{}_original", prefix.as_ref());

        for (i, (inst, pipeline)) in through.iter().enumerate() {
            let (flipped_port, original_port) = self.feedthrough_generic(
                &inst.get_mod_def(),
                &flipped,
                &original,
                pipeline.as_ref().cloned(),
            );

            // These are ModDef ports, so we need to assign them to the specific
            // instance in order to wire them up.
            let flipped_port = flipped_port.assign_to_inst(inst);
            let original_port = original_port.assign_to_inst(inst);

            if i == 0 {
                self.connect(&flipped_port);
            } else {
                through[i - 1].0.get_port(&original).connect(&flipped_port);
            }

            if i == through.len() - 1 {
                other.to_port_slice().connect(&original_port);
            }
        }
    }

    /// Ties off this port slice to the given constant value, specified as a
    /// `BigInt` or type that can be converted to a `BigInt`.
    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        let mod_def_core = self.get_mod_def_core();

        let big_int_value = value.into();

        mod_def_core
            .borrow_mut()
            .tieoffs
            .push(((*self).clone(), big_int_value.clone()));

        if let Port::ModInst {
            inst_name,
            port_name,
            ..
        } = &self.port
        {
            if self.port.io().width() == self.width() {
                // whole port tieoff
                mod_def_core
                    .borrow_mut()
                    .whole_port_tieoffs
                    .entry(inst_name.clone())
                    .or_default()
                    .insert(port_name.clone(), big_int_value);
            }
        }
    }

    /// Marks this port slice as unused, meaning that if it is an module
    /// instance output or module definition input, validation will not fail if
    /// the slice drives nothing. In fact, validation will fail if the slice
    /// drives anything.
    pub fn unused(&self) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core.borrow_mut().unused.push((*self).clone());
    }

    fn check_validity(&self) {
        if self.msb >= self.port.io().width() {
            panic!(
                "Port slice {} is invalid: msb must be less than the width of the port.",
                self.debug_string()
            );
        } else if self.lsb > self.msb {
            panic!(
                "Port slice {} is invalid: lsb must be less than or equal to msb.",
                self.debug_string()
            );
        }
    }
}

impl ModInst {
    /// Returns `true` if this module instance has an interface with the given
    /// name.
    pub fn has_interface(&self, name: impl AsRef<str>) -> bool {
        ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .has_interface(name)
    }

    /// Returns `true` if this module instance has a port with the given name.
    pub fn has_port(&self, name: impl AsRef<str>) -> bool {
        ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .has_port(name)
    }

    /// Returns the port on this instance with the given name. Panics if no such
    /// port exists.
    pub fn get_port(&self, name: impl AsRef<str>) -> Port {
        ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .get_port(name)
        .assign_to_inst(self)
    }

    /// Returns a slice of the port on this instance with the given name, from
    /// `msb` down to `lsb`, inclusive. Panics if no such port exists.
    pub fn get_port_slice(&self, name: impl AsRef<str>, msb: usize, lsb: usize) -> PortSlice {
        self.get_port(name).slice(msb, lsb)
    }

    /// Returns a vector of ports on this instance with the given prefix, or all
    /// ports if `prefix` is `None`.
    pub fn get_ports(&self, prefix: Option<&str>) -> Vec<Port> {
        let result = ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .get_ports(prefix);
        result
            .into_iter()
            .map(|port| port.assign_to_inst(self))
            .collect()
    }

    /// Returns the interface on this instance with the given name. Panics if no
    /// such interface exists.
    pub fn get_intf(&self, name: impl AsRef<str>) -> Intf {
        let mod_def_core = self.mod_def_core.upgrade().unwrap();
        let instances = &mod_def_core.borrow().instances;

        let inst_core = match instances.get(&self.name) {
            Some(inst_core) => inst_core.clone(),
            None => panic!(
                "Interface '{}' does not exist on module definition '{}'",
                name.as_ref(),
                mod_def_core.borrow().name
            ),
        };

        let inst_core_borrowed = inst_core.borrow();

        if inst_core_borrowed.interfaces.contains_key(name.as_ref()) {
            Intf::ModInst {
                intf_name: name.as_ref().to_string(),
                inst_name: self.name.clone(),
                mod_def_core: self.mod_def_core.clone(),
            }
        } else {
            panic!(
                "Interface '{}' does not exist in instance '{}'",
                name.as_ref(),
                self.debug_string()
            );
        }
    }

    /// Returns the ModDef that this is an instance of.
    pub fn get_mod_def(&self) -> ModDef {
        ModDef {
            core: self
                .mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .instances
                .get(&self.name)
                .unwrap_or_else(|| panic!("Instance named {} not found", self.name))
                .clone(),
        }
    }

    fn debug_string(&self) -> String {
        format!(
            "{}.{}",
            self.mod_def_core.upgrade().unwrap().borrow().name,
            self.name
        )
    }
}

/// Represents an interface on a module definition or module instance.
/// Interfaces are used to connect modules together by function name.
pub enum Intf {
    ModDef {
        name: String,
        mod_def_core: Weak<RefCell<ModDefCore>>,
    },
    ModInst {
        intf_name: String,
        inst_name: String,
        mod_def_core: Weak<RefCell<ModDefCore>>,
    },
}

impl std::fmt::Debug for Intf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mod_def_core = self.get_mod_def_core();
        let core = mod_def_core.borrow();
        match self {
            Intf::ModDef { name, .. } => {
                writeln!(f, "Interface Mapping:")?;
                for (func_name, (port_name, msb, lsb)) in core.interfaces.get(name).unwrap() {
                    writeln!(
                        f,
                        "{}: (port_name: {}, msb: {}, lsb: {})",
                        func_name, port_name, msb, lsb
                    )?;
                }
            }
            Intf::ModInst {
                inst_name,
                intf_name,
                ..
            } => {
                let inst_core = core.instances.get(inst_name).unwrap();
                let inst_binding = inst_core.borrow();
                writeln!(f, "Interface Mapping:")?;
                for (func_name, (port_name, msb, lsb)) in
                    inst_binding.interfaces.get(intf_name).unwrap()
                {
                    writeln!(
                        f,
                        "{}: (port_name: {}, msb: {}, lsb: {})",
                        func_name, port_name, msb, lsb
                    )?;
                }
            }
        };

        Ok(())
    }
}

impl Intf {
    fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            Intf::ModDef { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
            Intf::ModInst { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
        }
    }

    fn get_port_slices(&self) -> IndexMap<String, PortSlice> {
        match self {
            Intf::ModDef {
                mod_def_core, name, ..
            } => {
                let core = mod_def_core.upgrade().unwrap();
                let binding = core.borrow();
                let mod_def = ModDef { core: core.clone() };
                let mapping = binding.interfaces.get(name).unwrap();
                mapping
                    .iter()
                    .map(|(func_name, (port_name, msb, lsb))| {
                        (
                            func_name.clone(),
                            mod_def.get_port_slice(port_name, *msb, *lsb),
                        )
                    })
                    .collect()
            }
            Intf::ModInst {
                inst_name,
                intf_name,
                mod_def_core,
                ..
            } => {
                let core = mod_def_core.upgrade().unwrap();
                let binding = core.borrow();
                let mod_def = ModDef { core: core.clone() };
                let inst = mod_def.get_instance(inst_name);
                let inst_core = binding.instances.get(inst_name).unwrap();
                let inst_binding = inst_core.borrow();
                let inst_mapping = inst_binding.interfaces.get(intf_name).unwrap();
                inst_mapping
                    .iter()
                    .map(|(func_name, (port_name, msb, lsb))| {
                        (
                            func_name.clone(),
                            inst.get_port_slice(port_name, *msb, *lsb),
                        )
                    })
                    .collect()
            }
        }
    }

    fn get_intf_name(&self) -> String {
        match self {
            Intf::ModDef { name, .. } => name.clone(),
            Intf::ModInst { intf_name, .. } => intf_name.clone(),
        }
    }

    fn debug_string(&self) -> String {
        match self {
            Intf::ModDef { name, .. } => {
                format!("{}.{}", self.get_mod_def_core().borrow().name, name)
            }
            Intf::ModInst {
                inst_name,
                intf_name,
                ..
            } => format!(
                "{}.{}.{}",
                self.get_mod_def_core().borrow().name,
                inst_name,
                intf_name
            ),
        }
    }

    /// Connects this interface to another interface. Interfaces are connected
    /// by matching up ports with the same function name and connecting them.
    /// For example, if this interface is {"data": "a_data", "valid": "a_valid"}
    /// and the other interface is {"data": "b_data", "valid": "b_valid"}, then
    /// "a_data" will be connected to "b_data" and "a_valid" will be connected
    /// to "b_valid".
    ///
    /// Unless `allow_mismatch` is `true`, this method will panic if a function
    /// in this interface is not in the other interface. Continuing the previous
    /// example, if this interface also contained function "ready", but the
    /// other interface did not, this method would panic unless `allow_mismatch`
    /// was `true`.
    pub fn connect(&self, other: &Intf, allow_mismatch: bool) {
        self.connect_generic(other, None, allow_mismatch);
    }
    pub fn connect_pipeline(&self, other: &Intf, pipeline: PipelineConfig, allow_mismatch: bool) {
        self.connect_generic(other, Some(pipeline), allow_mismatch);
    }

    fn connect_generic(
        &self,
        other: &Intf,
        pipeline: Option<PipelineConfig>,
        allow_mismatch: bool,
    ) {
        let self_ports = self.get_port_slices();
        let other_ports = other.get_port_slices();

        for (func_name, self_port) in &self_ports {
            if let Some(other_port) = other_ports.get(func_name) {
                self_port.connect_generic(other_port, pipeline.clone());
            } else if !allow_mismatch {
                panic!(
                    "Interfaces {} and {} have mismatched functions and allow_mismatch is false. Example: function '{}' is present in {} but not in {}.",
                    self.debug_string(),
                    other.debug_string(),
                    func_name,
                    self.debug_string(),
                    other.debug_string()
                );
            }
        }

        if !allow_mismatch {
            for (func_name, _) in &other_ports {
                if !self_ports.contains_key(func_name) {
                    panic!(
                        "Interfaces {} and {} have mismatched functions and allow_mismatch is false. Example: function '{}' is present in {} but not in {}",
                        self.debug_string(),
                        other.debug_string(),
                        func_name,
                        other.debug_string(),
                        self.debug_string()
                    );
                }
            }
        }
    }

    /// Signals matching regex `pattern_a` on one interface are connected to
    /// signals matching regex `pattern_b` on the other interface, and vice
    /// versa. For example, suppose that this interface is `{"data_tx":
    /// "a_data_tx", "data_rx": "a_data_rx"}` and the other interface is
    /// `{"data_tx": "b_data_tx", "data_rx": "b_data_rx"}`. One might write
    /// this_intf.crossover(&other_intf, "(.*)_tx", "(.*)_rx") to connect the
    /// `data_tx` function on this interface (mapped to `a_data_tx`) to the
    /// `data_rx` function on the other interface (mapped to `b_data_rx`), and
    /// vice versa.
    pub fn crossover(&self, other: &Intf, pattern_a: impl AsRef<str>, pattern_b: impl AsRef<str>) {
        self.crossover_generic(other, pattern_a, pattern_b, None);
    }

    pub fn crossover_pipeline(
        &self,
        other: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) {
        self.crossover_generic(other, pattern_a, pattern_b, Some(pipeline));
    }

    fn crossover_generic(
        &self,
        other: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        pipeline: Option<PipelineConfig>,
    ) {
        let x_port_slices = self.get_port_slices();
        let y_port_slices = other.get_port_slices();

        for (x_func_name, y_func_name) in find_crossover_matches(self, other, pattern_a, pattern_b)
        {
            x_port_slices[&x_func_name]
                .connect_generic(&y_port_slices[&y_func_name], pipeline.clone());
        }
    }

    /// Ties off driven signals on this interface to the given constant value. A
    /// "driven signal" is an input of a module instance or an output of a
    /// module definition; it's a signal that would appear on the left hand side
    /// of a Verilog `assign` statement.
    pub fn tieoff<T: Into<BigInt> + Clone>(&self, value: T) {
        for (_, port_slice) in self.get_port_slices() {
            match port_slice {
                PortSlice {
                    port: Port::ModDef { .. },
                    ..
                } => {
                    if let IO::Output(_) = port_slice.port.io() {
                        port_slice.tieoff(value.clone());
                    }
                }
                PortSlice {
                    port: Port::ModInst { .. },
                    ..
                } => {
                    if let IO::Input(_) = port_slice.port.io() {
                        port_slice.tieoff(value.clone());
                    }
                }
            }
        }
    }

    /// Marks unused driving signals on this interface. A "driving signal" is an
    /// output of a module instance or an input of a module definition; it's a
    /// signal that would appear on the right hand side of a Verilog `assign`
    /// statement.
    pub fn unused(&self) {
        for (_, port_slice) in self.get_port_slices() {
            match port_slice {
                PortSlice {
                    port: Port::ModDef { .. },
                    ..
                } => {
                    if let IO::Input(_) = port_slice.port.io() {
                        port_slice.unused();
                    }
                }
                PortSlice {
                    port: Port::ModInst { .. },
                    ..
                } => {
                    if let IO::Output(_) = port_slice.port.io() {
                        port_slice.unused();
                    }
                }
            }
        }
    }

    pub fn unused_and_tieoff<T: Into<BigInt> + Clone>(&self, value: T) {
        self.unused();
        self.tieoff(value);
    }

    /// Creates a new interface on the parent module and connects it to this
    /// interface. The new interface will have the same functions as this
    /// interface; signal names are formed by concatenating the given prefix and
    /// the function name. For example, if this interface is `{"data": "a_data",
    /// "valid": "a_valid"}` and the prefix is "b_", the new interface will be
    /// `{"data": "b_data", "valid": "b_valid"}`. The `name` argument specifies
    /// the name of the new interface, which is used to retrieve the interface
    /// with `get_intf`.
    pub fn export_with_prefix(&self, name: impl AsRef<str>, prefix: impl AsRef<str>) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let mod_def_port_name = format!("{}{}", prefix.as_ref(), func_name);
            port_slice.export_as(&mod_def_port_name);
            mapping.insert(func_name, (mod_def_port_name, port_slice.width() - 1, 0));
        }
        ModDef {
            core: self.get_mod_def_core(),
        }
        .def_intf(name, mapping)
    }

    /// Export an interface using the given name, with a signal prefix of the
    /// name followed by an underscore. For example, if a block has an interface
    /// called "a" with signals "a_data" and "a_valid", calling
    /// export_with_name_underscore("b") will create a new interface called "b"
    /// with signals "b_data" and "b_valid".
    pub fn export_with_name_underscore(&self, name: impl AsRef<str>) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.export_with_prefix(name, prefix)
    }

    /// Exports an interface from a module instance to the parent module
    /// definition, returning a new interface. The new interface has the same
    /// name as the original interface, as well as the same signal names and
    /// signal functions. For example, calling this method on an interface on an
    /// intance called "a" with signals "a_data" and "a_valid" will create a new
    /// interface called "a" on the parent module definition with signals
    /// "a_data" and "a_valid".
    pub fn export(&self) -> Intf {
        if matches!(self, Intf::ModDef { .. }) {
            panic!("Cannot export() {}; must use export_with_prefix() or export_with_name_underscore() instead.", self.debug_string());
        }

        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let exported_port = port_slice.export();
            mapping.insert(
                func_name,
                (exported_port.get_port_name(), port_slice.width() - 1, 0),
            );
        }
        ModDef {
            core: self.get_mod_def_core(),
        }
        .def_intf(self.get_intf_name(), mapping)
    }

    pub fn flip_to(&self, mod_def: &ModDef) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let port = mod_def.add_port(port_slice.port.name(), port_slice.port.io().flip());
            mapping.insert(func_name, (port.get_port_name(), port_slice.width() - 1, 0));
        }
        mod_def.def_intf(self.get_intf_name(), mapping)
    }

    pub fn copy_to(&self, mod_def: &ModDef) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let port = mod_def.add_port(port_slice.port.name(), port_slice.port.io());
            mapping.insert(func_name, (port.get_port_name(), port_slice.width() - 1, 0));
        }
        mod_def.def_intf(self.get_intf_name(), mapping)
    }

    pub fn copy_to_with_prefix(
        &self,
        mod_def: &ModDef,
        name: impl AsRef<str>,
        prefix: impl AsRef<str>,
    ) -> Intf {
        let mut mapping = IndexMap::new();
        for (func_name, port_slice) in self.get_port_slices() {
            let port_name = format!("{}{}", prefix.as_ref(), func_name);
            mod_def.add_port(&port_name, port_slice.port.io());
            mapping.insert(func_name, (port_name, port_slice.width() - 1, 0));
        }
        mod_def.def_intf(name, mapping)
    }

    pub fn copy_to_with_name_underscore(&self, mod_def: &ModDef, name: impl AsRef<str>) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.copy_to_with_prefix(mod_def, name, prefix)
    }

    pub fn feedthrough(
        &self,
        moddef: &ModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
    ) -> (Intf, Intf) {
        self.feedthrough_generic(moddef, flipped, original, None)
    }

    pub fn feedthrough_pipeline(
        &self,
        moddef: &ModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: PipelineConfig,
    ) -> (Intf, Intf) {
        self.feedthrough_generic(moddef, flipped, original, Some(pipeline))
    }

    fn feedthrough_generic(
        &self,
        moddef: &ModDef,
        flipped: impl AsRef<str>,
        original: impl AsRef<str>,
        pipeline: Option<PipelineConfig>,
    ) -> (Intf, Intf) {
        let mut flipped_mapping = IndexMap::new();
        let mut original_mapping = IndexMap::new();

        for (func_name, port_slice) in self.get_port_slices() {
            let flipped_func = format!("{}_{}", flipped.as_ref(), func_name);
            let original_func = format!("{}_{}", original.as_ref(), func_name);

            let (flipped_port, original_port) = port_slice.feedthrough_generic(
                moddef,
                flipped_func,
                original_func,
                pipeline.clone(),
            );

            flipped_mapping.insert(
                func_name.clone(),
                (flipped_port.get_port_name(), port_slice.width() - 1, 0),
            );
            original_mapping.insert(
                func_name.clone(),
                (original_port.get_port_name(), port_slice.width() - 1, 0),
            );
        }

        let flipped_intf = moddef.def_intf(flipped, flipped_mapping);
        let original_intf = moddef.def_intf(original, original_mapping);

        (flipped_intf, original_intf)
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this interface to another interface.
    pub fn connect_through(
        &self,
        other: &Intf,
        through: &[&ModInst],
        prefix: impl AsRef<str>,
        allow_mismatch: bool,
    ) {
        let mut through_generic = Vec::new();
        for inst in through {
            through_generic.push((*inst, None));
        }
        self.connect_through_generic(other, &through_generic, prefix, allow_mismatch);
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this interface to another interface, with
    /// optional pipelining for each connection.
    pub fn connect_through_generic(
        &self,
        other: &Intf,
        through: &[(&ModInst, Option<PipelineConfig>)],
        prefix: impl AsRef<str>,
        allow_mismatch: bool,
    ) {
        if through.is_empty() {
            self.connect(other, allow_mismatch);
            return;
        }

        let flipped = format!("{}_flipped_{}", prefix.as_ref(), self.get_intf_name());
        let original = format!("{}_original_{}", prefix.as_ref(), self.get_intf_name());

        for (i, (inst, pipeline)) in through.iter().enumerate() {
            self.feedthrough_generic(
                &inst.get_mod_def(),
                &flipped,
                &original,
                pipeline.as_ref().cloned(),
            );
            if i == 0 {
                self.connect(&inst.get_intf(&flipped), false);
            } else {
                through[i - 1]
                    .0
                    .get_intf(&original)
                    .connect(&inst.get_intf(&flipped), false);
            }

            if i == through.len() - 1 {
                other.connect(&inst.get_intf(&original), allow_mismatch);
            }
        }
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this interface to another interface, using a
    /// crossover pattern. For example, one could have "^(.*)_tx$" and
    /// "^(.*)_rx$" as the patterns, and this would connect the "tx" signals
    /// on this interface to the "rx" signals on the other interface.
    pub fn crossover_through(
        &self,
        other: &Intf,
        through: &[&ModInst],
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        flipped_prefix: impl AsRef<str>,
        original_prefix: impl AsRef<str>,
    ) {
        let mut through_generic = Vec::new();
        for inst in through {
            through_generic.push((*inst, None));
        }
        self.crossover_through_generic(
            other,
            &through_generic,
            pattern_a,
            pattern_b,
            flipped_prefix,
            original_prefix,
        );
    }

    /// Punches a sequence of feedthroughs through the specified module
    /// instances to connect this interface to another interface, using a
    /// crossover pattern. For example, one could have "^(.*)_tx$" and
    /// "^(.*)_rx$" as the patterns, and this would connect the "tx" signals
    /// on this interface to the "rx" signals on the other interface.
    /// Optional pipelining is used for each connection.
    pub fn crossover_through_generic(
        &self,
        other: &Intf,
        through: &[(&ModInst, Option<PipelineConfig>)],
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
        flipped_prefix: impl AsRef<str>,
        original_prefix: impl AsRef<str>,
    ) {
        if through.is_empty() {
            self.crossover(other, pattern_a, pattern_b);
            return;
        }

        let matches = find_crossover_matches(self, other, pattern_a, pattern_b);
        let x_intf_port_slices = self.get_port_slices();
        let y_intf_port_slices = other.get_port_slices();

        for (x_func_name, y_func_name) in matches {
            let flipped_name = format!("{}_{}", flipped_prefix.as_ref(), y_func_name);
            let original_name = format!("{}_{}", original_prefix.as_ref(), x_func_name);
            for (i, (inst, pipeline)) in through.iter().enumerate() {
                x_intf_port_slices[&x_func_name].feedthrough_generic(
                    &inst.get_mod_def(),
                    &flipped_name,
                    &original_name,
                    pipeline.as_ref().cloned(),
                );

                if i == 0 {
                    x_intf_port_slices[&x_func_name].connect(&inst.get_port(&flipped_name));
                } else {
                    through[i - 1]
                        .0
                        .get_port(&original_name)
                        .connect(&inst.get_port(&flipped_name));
                }

                if i == through.len() - 1 {
                    y_intf_port_slices[&y_func_name].connect(&inst.get_port(&original_name));
                }
            }
        }
    }

    /// Divides each signal in this interface into `n` equal slices, returning a
    /// vector of interfaces. For example, if this interface is `{"data":
    /// "a_data[31:0]", "valid": "a_valid[3:0]"}` and `n` is 4, this will return
    /// a vector of 4 interfaces, each with signals `{"data": "a_data[7:0]",
    /// "valid": "a_valid[0:0]"}`, `{"data": "a_data[15:8]", "valid":
    /// "a_valid[1:1]"}`, and so on. The names of the new interfaces are formed
    /// by appending "_0", "_1", "_2", and so on to the name of this interface;
    /// these names can be used to retrieve specific slices of the interface
    /// with `get_intf`.
    pub fn subdivide(&self, n: usize) -> Vec<Intf> {
        let mut result = Vec::new();

        let mut mappings: Vec<IndexMap<String, (String, usize, usize)>> = Vec::with_capacity(n);
        for _ in 0..n {
            mappings.push(IndexMap::new());
        }

        for (func_name, port_slice) in self.get_port_slices() {
            let slices = port_slice.subdivide(n);
            for (i, slice) in slices.into_iter().enumerate() {
                let port_name = port_slice.port.get_port_name();
                mappings[i].insert(func_name.clone(), (port_name.clone(), slice.msb, slice.lsb));
            }
        }

        for i in 0..n {
            let intf = match self {
                Intf::ModDef { name, .. } => {
                    let name = format!("{}_{}", name, i);
                    ModDef {
                        core: self.get_mod_def_core(),
                    }
                    .def_intf(&name, mappings.remove(0))
                }
                _ => panic!(
                    "Error subdividing {}: subdividing ModInst interfaces is not supported.",
                    self.debug_string()
                ),
            };
            result.push(intf);
        }

        result
    }
}

pub struct Funnel {
    a_in: PortSlice,
    a_out: PortSlice,
    b_in: PortSlice,
    b_out: PortSlice,
    a_in_offset: usize,
    a_out_offset: usize,
}

impl Funnel {
    pub fn new(
        a: (impl ConvertibleToPortSlice, impl ConvertibleToPortSlice),
        b: (impl ConvertibleToPortSlice, impl ConvertibleToPortSlice),
    ) -> Self {
        let a0 = a.0.to_port_slice();
        let a1 = a.1.to_port_slice();

        let (a_in, a_out) = match (a0.port.io(), a1.port.io()) {
            (IO::Input(_), IO::Output(_)) => (a0, a1),
            (IO::Output(_), IO::Input(_)) => (a1, a0),
            (IO::Input(_), IO::Input(_)) => panic!(
                "Funnel error: Side A cannot have both ports as inputs ({} and {})",
                a0.debug_string(),
                a1.debug_string()
            ),
            (IO::Output(_), IO::Output(_)) => panic!(
                "Funnel error: Side A cannot have both ports as outputs ({} and {})",
                a0.debug_string(),
                a1.debug_string()
            ),
            (IO::InOut(_), _) => panic!(
                "Funnel error: Side A cannot have inout ports ({})",
                a0.debug_string()
            ),
            (_, IO::InOut(_)) => panic!(
                "Funnel error: Side A cannot have inout ports ({})",
                a1.debug_string()
            ),
        };

        let b0 = b.0.to_port_slice();
        let b1 = b.1.to_port_slice();

        let (b_in, b_out) = match (b0.port.io(), b1.port.io()) {
            (IO::Input(_), IO::Output(_)) => (b0, b1),
            (IO::Output(_), IO::Input(_)) => (b1, b0),
            (IO::Input(_), IO::Input(_)) => panic!(
                "Funnel error: Side B cannot have both ports as inputs ({}, {})",
                b0.debug_string(),
                b1.debug_string()
            ),
            (IO::Output(_), IO::Output(_)) => panic!(
                "Funnel error: Side B cannot have both ports as outputs ({}, {})",
                b0.debug_string(),
                b1.debug_string()
            ),
            (IO::InOut(_), _) => panic!(
                "Funnel error: Side B cannot have inout ports ({})",
                b0.debug_string()
            ),
            (_, IO::InOut(_)) => panic!(
                "Funnel error: Side B cannot have inout ports ({})",
                b1.debug_string()
            ),
        };

        assert!(
            a_in.width() == b_out.width(),
            "Funnel error: Side A input and side B output must have the same width ({}, {})",
            a_in.debug_string(),
            b_out.debug_string()
        );
        assert!(
            a_out.width() == b_in.width(),
            "Funnel error: Side A output and side B input must have the same width ({}, {})",
            a_out.debug_string(),
            b_in.debug_string()
        );

        Self {
            a_in,
            a_out,
            b_in,
            b_out,
            a_in_offset: 0,
            a_out_offset: 0,
        }
    }

    pub fn connect(&mut self, a: &impl ConvertibleToPortSlice, b: &impl ConvertibleToPortSlice) {
        let a = a.to_port_slice();
        let b = b.to_port_slice();

        assert!(
            a.width() == b.width(),
            "Funnel error: a and b must have the same width ({}, {})",
            a.debug_string(),
            b.debug_string()
        );

        if a.port.is_driver() {
            if b.port.is_driver() {
                panic!(
                    "Funnel error: Cannot connect two outputs together ({}, {})",
                    a.debug_string(),
                    b.debug_string()
                );
            } else {
                assert!(
                    self.a_in_offset + a.width() <= self.a_in.width(),
                    "Funnel out of capacity."
                );
                self.a_in
                    .slice_relative(self.a_in_offset, a.width())
                    .connect(&a);
                self.b_out
                    .slice_relative(self.a_in_offset, b.width())
                    .connect(&b);
                self.a_in_offset += a.width();
            }
        } else if b.port.is_driver() {
            assert!(
                self.a_out_offset + a.width() <= self.a_out.width(),
                "Funnel out of capacity."
            );
            self.a_out
                .slice_relative(self.a_out_offset, a.width())
                .connect(&a);
            self.b_in
                .slice_relative(self.a_out_offset, b.width())
                .connect(&b);
            self.a_out_offset += a.width();
        } else {
            panic!(
                "Funnel error: Cannot connect two inputs together ({}, {})",
                a.debug_string(),
                b.debug_string()
            );
        }
    }

    pub fn connect_intf(&mut self, a: &Intf, b: &Intf, allow_mismatch: bool) {
        let a_ports = a.get_port_slices();
        let b_ports = b.get_port_slices();

        for (a_func_name, a_port) in &a_ports {
            if let Some(b_port) = b_ports.get(a_func_name) {
                self.connect(a_port, b_port);
            } else if !allow_mismatch {
                panic!("Funnel error: interfaces {} and {} have mismatched functions and allow_mismatch is false. Example: function '{}' is present in {} but not in {}",
                    a.debug_string(),
                    b.debug_string(),
                    a_func_name,
                    a.debug_string(),
                    b.debug_string()
                );
            }
        }

        if !allow_mismatch {
            for (func_name, _) in &b_ports {
                if !a_ports.contains_key(func_name) {
                    panic!(
                        "Interfaces {} and {} have mismatched functions and allow_mismatch is false. Example: function '{}' is present in {} but not in {}",
                        a.debug_string(),
                        b.debug_string(),
                        func_name,
                        b.debug_string(),
                        a.debug_string()
                    );
                }
            }
        }
    }

    pub fn crossover_intf(
        &mut self,
        x: &Intf,
        y: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
    ) {
        let pattern_a_regex = Regex::new(pattern_a.as_ref()).unwrap();
        let pattern_b_regex = Regex::new(pattern_b.as_ref()).unwrap();

        let mut x_a_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut x_b_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut y_a_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut y_b_matches: IndexMap<String, PortSlice> = IndexMap::new();

        const CONCAT_SEP: &str = "_";

        for (x_func_name, x_port_slice) in x.get_port_slices() {
            if let Some(captures) = pattern_a_regex.captures(&x_func_name) {
                x_a_matches.insert(concat_captures(&captures, CONCAT_SEP), x_port_slice);
            } else if let Some(captures) = pattern_b_regex.captures(&x_func_name) {
                x_b_matches.insert(concat_captures(&captures, CONCAT_SEP), x_port_slice);
            }
        }

        for (y_func_name, y_port_slice) in y.get_port_slices() {
            if let Some(captures) = pattern_a_regex.captures(&y_func_name) {
                y_a_matches.insert(concat_captures(&captures, CONCAT_SEP), y_port_slice);
            } else if let Some(captures) = pattern_b_regex.captures(&y_func_name) {
                y_b_matches.insert(concat_captures(&captures, CONCAT_SEP), y_port_slice);
            }
        }

        for (x_func_name, x_port_slice) in x_a_matches {
            if let Some(y_port_slice) = y_b_matches.get(&x_func_name) {
                self.connect(&x_port_slice, y_port_slice);
            }
        }

        for (x_func_name, x_port_slice) in x_b_matches {
            if let Some(y_port_slice) = y_a_matches.get(&x_func_name) {
                self.connect(&x_port_slice, y_port_slice);
            }
        }
    }

    pub fn done(&mut self) {
        if self.a_in_offset != self.a_in.width() {
            self.a_in
                .slice_relative(self.a_in_offset, self.a_in.width() - self.a_in_offset)
                .tieoff(0);
            self.b_out
                .slice_relative(self.a_in_offset, self.b_out.width() - self.a_in_offset)
                .unused();
            self.a_in_offset = self.a_in.width();
        }

        if self.a_out_offset != self.a_out.width() {
            self.a_out
                .slice_relative(self.a_out_offset, self.a_out.width() - self.a_out_offset)
                .unused();
            self.b_in
                .slice_relative(self.a_out_offset, self.b_in.width() - self.a_out_offset)
                .tieoff(0);
            self.a_out_offset = self.a_out.width();
        }
    }
}

fn parser_port_to_port(parser_port: &slang_rs::Port) -> Result<(String, IO), String> {
    let size = parser_port.ty.width().unwrap();
    let port_name = parser_port.name.clone();

    match parser_port.dir {
        slang_rs::PortDir::Input => Ok((port_name, IO::Input(size))),
        slang_rs::PortDir::Output => Ok((port_name, IO::Output(size))),
        slang_rs::PortDir::InOut => Ok((port_name, IO::InOut(size))),
    }
}

fn concat_captures(captures: &regex::Captures, sep: &str) -> String {
    captures
        .iter()
        .skip(1)
        .filter_map(|m| m.map(|m| m.as_str().to_string()))
        .collect::<Vec<String>>()
        .join(sep)
}

fn example_problematic_bits(value: &BigUint, width: usize) -> Option<String> {
    let mut lsb = None;
    let mut msb = None;
    let mut found_problem = false;
    for i in 0..width {
        if (value.clone() >> i) & BigUint::from(1usize) == BigUint::from(0usize) {
            if found_problem {
                msb = Some(i);
            } else {
                lsb = Some(i);
                found_problem = true;
            }
        } else if found_problem {
            break;
        }
    }
    if found_problem {
        if msb.is_none() {
            msb = Some(width - 1);
        }
        if (msb.unwrap() - lsb.unwrap() + 1) == width {
            Some("".to_string())
        } else if lsb == msb {
            Some(format!("[{}]", lsb.unwrap()))
        } else {
            Some(format!("[{}:{}]", msb.unwrap(), lsb.unwrap()))
        }
    } else {
        None
    }
}

fn find_crossover_matches(
    x: &Intf,
    y: &Intf,
    pattern_a: impl AsRef<str>,
    pattern_b: impl AsRef<str>,
) -> Vec<(String, String)> {
    let mut matches = Vec::new();

    let pattern_a_regex = Regex::new(pattern_a.as_ref()).unwrap();
    let pattern_b_regex = Regex::new(pattern_b.as_ref()).unwrap();

    let mut x_a_matches = IndexMap::new();
    let mut x_b_matches = IndexMap::new();
    let mut y_a_matches = IndexMap::new();
    let mut y_b_matches = IndexMap::new();

    const CONCAT_SEP: &str = "_";

    for (x_func_name, _) in x.get_port_slices() {
        if let Some(captures) = pattern_a_regex.captures(&x_func_name) {
            x_a_matches.insert(concat_captures(&captures, CONCAT_SEP), x_func_name);
        } else if let Some(captures) = pattern_b_regex.captures(&x_func_name) {
            x_b_matches.insert(concat_captures(&captures, CONCAT_SEP), x_func_name);
        }
    }

    for (y_func_name, _) in y.get_port_slices() {
        if let Some(captures) = pattern_a_regex.captures(&y_func_name) {
            y_a_matches.insert(concat_captures(&captures, CONCAT_SEP), y_func_name);
        } else if let Some(captures) = pattern_b_regex.captures(&y_func_name) {
            y_b_matches.insert(concat_captures(&captures, CONCAT_SEP), y_func_name);
        }
    }

    for (key, x_func_name) in x_a_matches {
        if let Some(y_func_name) = y_b_matches.get(&key) {
            matches.push((x_func_name, y_func_name.clone()));
        }
    }

    for (key, x_func_name) in x_b_matches {
        if let Some(y_func_name) = y_a_matches.get(&key) {
            matches.push((x_func_name, y_func_name.clone()));
        }
    }

    matches
}
