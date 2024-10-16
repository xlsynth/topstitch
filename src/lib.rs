// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use indexmap::IndexMap;
use itertools::Itertools;
use num_bigint::{BigInt, BigUint};
use regex::Regex;
use slang_rs::{self, extract_ports, str2tmpfile, SlangConfig};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::Path;
use std::rc::{Rc, Weak};
use xlsynth::vast::{Expr, LogicRef, VastFile, VastFileType};

/// Represents the direction (`Input` or `Output`) and bit width of a port.
#[derive(Clone, Debug)]
pub enum IO {
    Input(usize),
    Output(usize),
}

impl IO {
    /// Returns the width of the port in bits.
    pub fn width(&self) -> usize {
        match self {
            IO::Input(width) => *width,
            IO::Output(width) => *width,
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
}

/// Represents a slice of a port, which may be on a module definition or on a module instance. A slice is a defined as a contiguous range of bits from `msb` down to `lsb`, inclusive. A slice can be a single bit on the port (`msb` equal to `lsb`), the entire port, or any range in between.
#[derive(Clone)]
pub struct PortSlice {
    port: Port,
    msb: usize,
    lsb: usize,
}

impl PortSlice {
    /// Divides a port slice into `n` parts of equal bit width, return a vector of `n` port slices. For example, if a port is 8 bits wide and `n` is 2, the port will be divided into 2 slices of 4 bits each: `port[3:0]` and `port[7:4]`. This method panics if the port width is not divisible by `n`.
    pub fn subdivide(&self, n: usize) -> Vec<Self> {
        let width = self.msb - self.lsb + 1;
        if width % n != 0 {
            panic!(
                "Cannot subdivide a port slice of width {} into {} equal parts.",
                width, n
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

    fn export_as(&self, name: &str) -> Port {
        let io = match self.port.io() {
            IO::Input(_) => IO::Input(self.width()),
            IO::Output(_) => IO::Output(self.width()),
        };

        let mod_def_core = self.port.get_mod_def_core();
        let mod_def = ModDef {
            core: mod_def_core.clone(),
        };

        let port = mod_def.add_port(name, io);
        port.connect(self);
        port
    }
}

/// Indicates that a type can be converted to a `PortSlice`. `Port` and `PortSlice` both implement this trait, which makes it easier to perform the same operations on both.
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

/// Represents a module definition, like `module <mod_def_name> ... endmodule` in Verilog.
#[derive(Clone)]
pub struct ModDef {
    core: Rc<RefCell<ModDefCore>>,
}

/// Represents an instance of a module definition, like `<mod_def_name> <mod_inst_name> ( ... );` in Verilog.
#[derive(Clone)]
pub struct ModInst {
    name: String,
    mod_def_core: Weak<RefCell<ModDefCore>>,
}

struct VerilogImport {
    sources: Vec<String>,
    skip_unsupported: bool,
    ignore_unknown_modules: bool,
}

/// Data structure representing a module definition. Contains the module's name, ports, interfaces, instances, etc. Not intended to be used directly; use `ModDef` instead, which contains a smart pointer to this struct.
pub struct ModDefCore {
    name: String,
    ports: IndexMap<String, IO>,
    interfaces: IndexMap<String, IndexMap<String, (String, usize, usize)>>,
    instances: IndexMap<String, Rc<RefCell<ModDefCore>>>,
    usage: Usage,
    generated_verilog: Option<String>,
    verilog_import: Option<VerilogImport>,
    assignments: Vec<(PortSlice, PortSlice)>,
    unused: Vec<PortSlice>,
    tieoffs: Vec<(PortSlice, BigInt)>,
}

/// Represents how a module definition should be used when validating and/or emitting Verilog.
#[derive(PartialEq, Default, Clone)]
pub enum Usage {
    /// When validating, validate the module definition and descend into its instances. When emitting Verilog, emit its definition and descend into its instances.
    #[default]
    EmitDefinitionAndDescend,

    /// When validating, do not validate the module definition and do not descend into its instances. When emitting Verilog, do not emit its definition and do not descend into its instances.
    EmitNothingAndStop,

    /// When validating, do not validate the module definition and do not descend into its instances. When emitting Verilog, emit a stub (interface only) and do not descend into its instances.
    EmitStubAndStop,

    /// When validating, do not validate the module definition and do not descend into its instances. When emitting Verilog, emit its definition but do not descend into its instances.
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
}

impl ModDef {
    /// Creates a new module definition with the given name.
    pub fn new(name: &str) -> ModDef {
        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: name.to_string(),
                ports: IndexMap::new(),
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Default::default(),
                generated_verilog: None,
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                verilog_import: None,
            })),
        }
    }

    fn frozen(&self) -> bool {
        self.core.borrow().generated_verilog.is_some()
            || self.core.borrow().verilog_import.is_some()
    }

    /// Creates a new module definition from a Verilog file. The `name` parameter is the name of the module to extract from the Verilog file, and `verilog` is the path to the Verilog file. If `ignore_unknown_modules` is `true`, do not panic if the Verilog file instantiates modules whose definitions cannot be found. This is often useful because only the interface of module `name` needs to be extracted; its contents do not need to be interpreted. If `skip_unsupported` is `true`, do not panic if the interface of module `name` contains unsupported features; simply skip these ports. This is occasionally useful when prototyping.
    pub fn from_verilog_file(
        name: &str,
        verilog: &Path,
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        Self::from_verilog_files(name, &[verilog], ignore_unknown_modules, skip_unsupported)
    }

    /// Creates a new module definition from a list of Verilog files. The `name` parameter is the name of the module to extract from the Verilog sources, and `verilog` is an array of paths of Verilog sources. If `ignore_unknown_modules` is `true`, do not panic if the Verilog file instantiates modules whose definitions cannot be found. This is often useful because only the interface of module `name` needs to be extracted; its contents do not need to be interpreted. If `skip_unsupported` is `true`, do not panic if the interface of module `name` contains unsupported features; simply skip these ports. This is occasionally useful when prototyping.
    pub fn from_verilog_files(
        name: &str,
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

    /// Creates a new module definition from Verilog source code. The `name` parameter is the name of the module to extract from the Verilog code, and `verilog` is a string containing Verilog code. If `ignore_unknown_modules` is `true`, do not panic if the Verilog file instantiates modules whose definitions cannot be found. This is often useful because only the interface of module `name` needs to be extracted; its contents do not need to be interpreted. If `skip_unsupported` is `true`, do not panic if the interface of module `name` contains unsupported features; simply skip these ports. This is occasionally useful when prototyping.
    pub fn from_verilog(
        name: &str,
        verilog: &str,
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        let verilog = str2tmpfile(verilog).unwrap();

        let cfg = SlangConfig {
            sources: &[verilog.path().to_str().unwrap()],
            ignore_unknown_modules,
            ..Default::default()
        };

        Self::from_verilog_using_slang(name, &cfg, skip_unsupported)
    }

    /// Creates a new module definition from Verilog sources. The `name` parameter is the name of the module to extract from Verilog code, and `cfg` is a `SlangConfig` struct specifying source files, include directories, etc. If `skip_unsupported` is `true`, do not panic if the interface of module `name` contains unsupported features; simply skip these ports. This is occasionally useful when prototyping.
    pub fn from_verilog_using_slang(name: &str, cfg: &SlangConfig, skip_unsupported: bool) -> Self {
        let parser_ports = extract_ports(cfg, skip_unsupported);

        let mut ports = IndexMap::new();
        for parser_port in parser_ports[name].iter() {
            match parser_port_to_port(parser_port) {
                Ok((name, io)) => {
                    ports.insert(name, io);
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
                name: name.to_string(),
                ports,
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Usage::EmitNothingAndStop,
                generated_verilog: None,
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                verilog_import: Some(VerilogImport {
                    sources: cfg.sources.iter().map(|s| s.to_string()).collect(),
                    skip_unsupported,
                    ignore_unknown_modules: cfg.ignore_unknown_modules,
                }),
            })),
        }
    }

    /// Adds a port to the module definition with the given name. The direction and width are specfied via the `io` parameter.
    pub fn add_port(&self, name: &str, io: IO) -> Port {
        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.borrow().name
            );
        }

        let mut core = self.core.borrow_mut();
        match core.ports.entry(name.to_string()) {
            Entry::Occupied(_) => {
                panic!("Port '{}' already exists in module '{}'.", name, core.name)
            }
            Entry::Vacant(entry) => {
                entry.insert(io);
                Port::ModDef {
                    name: name.to_string(),
                    mod_def_core: Rc::downgrade(&self.core),
                }
            }
        }
    }

    /// Returns the port on this module definition with the given name; panics if a port with that name does not exist.
    pub fn get_port(&self, name: &str) -> Port {
        let inner = self.core.borrow();
        if inner.ports.contains_key(name) {
            Port::ModDef {
                name: name.to_string(),
                mod_def_core: Rc::downgrade(&self.core),
            }
        } else {
            panic!("Port '{}' does not exist in module '{}'.", name, inner.name)
        }
    }

    /// Returns a slice of the port on this module definition with the given name, from `msb` down to `lsb`, inclusive; panics if a port with that name does not exist.
    pub fn get_port_slice(&self, name: &str, msb: usize, lsb: usize) -> PortSlice {
        self.get_port(name).slice(msb, lsb)
    }

    /// Returns a vector of all ports on this module definition with the given prefix. If `prefix` is `None`, returns all ports.
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

    /// Returns the module instance within this module definition with the given name; panics if an instance with that name does not exist.
    pub fn get_instance(&self, name: &str) -> ModInst {
        let inner = self.core.borrow();
        if inner.instances.contains_key(name) {
            ModInst {
                name: name.to_string(),
                mod_def_core: Rc::downgrade(&self.core),
            }
        } else {
            panic!("Instance {} does not exist in module {}", name, inner.name)
        }
    }

    /// Configures how this module definition should be used when validating and/or emitting Verilog.
    pub fn set_usage(&self, usage: Usage) {
        if self.core.borrow().generated_verilog.is_some() {
            assert!(
                usage != Usage::EmitDefinitionAndDescend,
                "Cannot descend into a module defined from Verilog sources."
            );
        }
        self.core.borrow_mut().usage = usage;
    }

    /// Instantiate a module, using the provided instance name. `autoconnect` is an optional list of port names to automatically connect between the parent module and the instantiated module. For example, if the parent module has a port named `clk` and the instantiated module has a port named `clk`, passing `Some(&["clk"])` will automatically connect the two ports. It's OK if some or all of the `autoconnect` names do not exist in the parent module and/or instantiated module; TopStitch will not panic in this case.
    pub fn instantiate(
        &self,
        moddef: &ModDef,
        name: Option<&str>,
        autoconnect: Option<&[&str]>,
    ) -> ModInst {
        let name_default = format!("{}_i", moddef.core.borrow().name);
        let name = name.unwrap_or(name_default.as_str());

        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.borrow().name
            );
        }

        {
            let mut inner = self.core.borrow_mut();
            if inner.instances.contains_key(name) {
                panic!(
                    "An instance named '{}' already exists in module '{}'.",
                    name, inner.name
                );
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

    /// Create one or more instances of a module, using the provided dimensions. For example, if `dimensions` is `&[3]`, TopStitch will create a 1D array of 3 instances, called `<mod_def_name>_i_0`, `<mod_def_name>_i_1`, `<mod_def_name>_i_2`. If `dimensions` is `&[2, 3]`, TopStitch will create a `2x3` array of instances, called `<mod_def_name>_i_0_0`, `<mod_def_name>_i_0_1`, `<mod_def_name>_i_0_2`, `<mod_def_name>_i_1_0`, etc. If provided, the optional `prefix` argument sets the prefix used in naming instances to something other than `<mod_def_name>_i_`. `autoconnect` has the same meaning as in `instantiate()`: if provided, it is a list of port names to automatically connect between the parent module and the instantiated module. For example, if the parent module has a port named `clk` and the instantiated module has a port named `clk`, passing `Some(&["clk"])` will automatically connect the two ports.
    pub fn instantiate_array(
        &self,
        moddef: &ModDef,
        dimensions: &[usize],
        prefix: Option<&str>,
        autoconnect: Option<&[&str]>,
    ) -> Vec<ModInst> {
        if dimensions.is_empty() {
            panic!("Dimensions array cannot be empty.");
        }
        if dimensions.iter().any(|&d| d == 0) {
            panic!("Dimension sizes must be greater than zero.");
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

    /// Writes Verilog code for this module definition to the given file path. If `validate` is `true`, validate the module definition before emitting Verilog.
    pub fn emit_to_file(&self, path: &Path, validate: bool) {
        let err_msg = format!("emitting ModDef to file at path: {:?}", path);
        std::fs::write(path, self.emit(validate)).expect(&err_msg);
    }

    /// Returns Verilog code for this module definition as a string. If `validate` is `true`, validate the module definition before emitting Verilog.
    pub fn emit(&self, validate: bool) -> String {
        if validate {
            self.validate();
        }
        let mut emitted_module_names = IndexMap::new();
        let mut file = VastFile::new(VastFileType::SystemVerilog);
        let mut leaf_text = Vec::new();
        self.emit_recursive(&mut emitted_module_names, &mut file, &mut leaf_text);
        leaf_text.push(file.emit());
        leaf_text.join("\n")
    }

    fn emit_recursive(
        &self,
        emitted_module_names: &mut IndexMap<String, Rc<RefCell<ModDefCore>>>,
        file: &mut VastFile,
        leaf_text: &mut Vec<String>,
    ) {
        let core = self.core.borrow();

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
                ModDef { core: inst.clone() }.emit_recursive(emitted_module_names, file, leaf_text);
            }
        }

        // Start the module declaration.

        let mut module = file.add_module(&core.name);

        let mut ports: IndexMap<String, LogicRef> = IndexMap::new();

        for port_name in core.ports.keys() {
            let io = core.ports.get(port_name).unwrap();
            if ports.contains_key(port_name) {
                panic!("Port name {} is already declared.", port_name);
            }
            let logic_ref =
                match io {
                    IO::Input(width) => module
                        .add_input(port_name, &file.make_bit_vector_type(*width as i64, false)),
                    IO::Output(width) => module
                        .add_output(port_name, &file.make_bit_vector_type(*width as i64, false)),
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
                let net_name = format!("{}_{}", inst_name, port_name);
                if ports.contains_key(&net_name) {
                    panic!(
                        "{} is already declared as a port of the module containing this instance.",
                        net_name
                    );
                }
                let data_type = file.make_bit_vector_type(io.width() as i64, false);
                if nets
                    .insert(net_name.clone(), module.add_wire(&net_name, &data_type))
                    .is_some()
                {
                    panic!("Wire name {} is already declared in this module", net_name);
                }
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

            for (port_name, _) in inst.borrow().ports.iter() {
                let net_name = format!("{}_{}", inst_name, port_name);
                connection_port_names.push(port_name.clone());
                connection_expressions.push(nets.get(&net_name).unwrap().to_expr());
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
                &connection_expressions.iter().collect::<Vec<_>>(),
            );
            module.add_member_instantiation(instantiation);
        }

        // Emit assign statements for connections.
        for (lhs, rhs) in &core.assignments {
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
            let assignment =
                file.make_continuous_assignment(&lhs_slice.to_expr(), &rhs_slice.to_expr());
            module.add_member_continuous_assignment(assignment);
        }

        // Emit assign statements for tieoffs.
        for (dst, value) in &core.tieoffs {
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

    /// Defines an interface with the given name. `mapping` is a map from function names to tuples of `(port_name, msb, lsb)`. For example, if `mapping` is `{"data": ("a_data", 3, 0), "valid": ("a_valid", 1, 1)}`, this defines an interface with two functions, `data` and `valid`, where the `data` function is provided by the port slice `a_data[3:0]` and the `valid` function is provided by the port slice `[1:1]`.
    pub fn def_intf(&self, name: &str, mapping: IndexMap<String, (String, usize, usize)>) -> Intf {
        let mut core = self.core.borrow_mut();
        if core.interfaces.contains_key(name) {
            panic!(
                "Interface '{}' already exists in module '{}'",
                name, core.name
            );
        }
        core.interfaces.insert(name.to_string(), mapping);
        Intf::ModDef {
            name: name.to_string(),
            mod_def_core: Rc::downgrade(&self.core),
        }
    }

    /// Defines an interface with the given name, where the function names are derived from the port names by stripping a common prefix. For example, if the module has ports `a_data`, `a_valid`, `b_data`, and `b_valid`, calling `def_intf_from_prefix("a_intf", "a_")` will define an interface with functions `data` and `valid`, where `data` is provided by the full port `a_data` and `valid` is provided by the full port `a_valid`.
    pub fn def_intf_from_prefix(&self, name: &str, prefix: &str) -> Intf {
        let mut mapping = IndexMap::new();
        {
            let core = self.core.borrow();
            for port_name in core.ports.keys() {
                if port_name.starts_with(prefix) {
                    let func_name = port_name.strip_prefix(prefix).unwrap().to_string();
                    let port = self.get_port(port_name);
                    mapping.insert(func_name, (port_name.clone(), port.io().width() - 1, 0));
                }
            }
        }
        self.def_intf(name, mapping)
    }

    /// Returns the interface with the given name; panics if an interface with that name does not exist.
    pub fn get_intf(&self, name: &str) -> Intf {
        let core = self.core.borrow();
        if core.interfaces.contains_key(name) {
            Intf::ModDef {
                name: name.to_string(),
                mod_def_core: Rc::downgrade(&self.core),
            }
        } else {
            panic!(
                "Interface '{}' does not exist in module '{}'",
                name, core.name
            );
        }
    }

    /// Punches a feedthrough through this module definition with the given input and output names and width. This will create two new ports on the module definition, `input_name[width-1:0]` and `output_name[width-1:0]`, and connect them together.
    pub fn feedthrough(&self, input_name: &str, output_name: &str, width: usize) {
        let input_port = self.add_port(input_name, IO::Input(width));
        let output_port = self.add_port(output_name, IO::Output(width));
        input_port.connect(&output_port);
    }

    /// Instantiates this module definition within a new module definition, and returns the new module definition. The new module definition has all of the same ports as the original module, which are connected directly to ports with the same names on the instance of the original module.
    pub fn wrap(&self, def_name: Option<&str>, inst_name: Option<&str>) -> ModDef {
        let original_name = &self.core.borrow().name;
        let def_name_default = format!("{}_wrapper", original_name);
        let def_name = def_name.unwrap_or(&def_name_default);

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

        // For each port in the original module, add a corresponding port to the wrapper and connect them.
        for (port_name, io) in self.core.borrow().ports.iter() {
            let wrapper_port = wrapper.add_port(port_name, io.clone());
            let inst_port = inst.get_port(port_name);
            wrapper_port.connect(&inst_port);
        }

        wrapper
    }

    /// Returns a new module definition that is a variant of this module definition, where the given parameters have been overridden from their default values. For example, if the module definition has a parameter `WIDTH` with a default value of `32`, calling `parameterize(&[("WIDTH", 64)])` will return a new module definition with the same ports and instances, but with the parameter `WIDTH` set to `64`. This is implemented by creating a wrapper module that instantiates the original module with the given parameters. The name of the wrapper module defaults to `<original_mod_def_name>_<param_name_0>_<param_value_0>_<param_name_1>_<param_value_1>_...`; this can be overridden via the optional `def_name` argument. The instance name of the original module within the wrapper is `<original_mod_def_name>_i`; this can be overridden via the optional `inst_name` argument.
    pub fn parameterize(
        &self,
        parameters: &[(&str, i32)],
        def_name: Option<&str>,
        inst_name: Option<&str>,
    ) -> ModDef {
        let core = self.core.borrow();

        if core.verilog_import.is_none() {
            panic!("Can only parameterize a module defined in external Verilog sources.");
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

        let cfg = SlangConfig {
            sources: sources.as_slice(),
            parameters: &parameters_with_string_values
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            ignore_unknown_modules: core.verilog_import.as_ref().unwrap().ignore_unknown_modules,
            ..Default::default()
        };

        let parser_ports: HashMap<String, Vec<slang_rs::Port>> = extract_ports(&cfg, true);

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
                    };
                    connection_port_names.push(name.clone());
                    connection_expressions.push(logic_expr.to_expr());
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
            // TODO(sherbst) 09/24/2024: support parameter values other than 32-bit integers.
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
                &connection_expressions.iter().collect::<Vec<_>>(),
            ),
        );

        let verilog = file.emit();

        let mut ports = IndexMap::new();
        for parser_port in parser_ports[&core.name].iter() {
            match parser_port_to_port(parser_port) {
                Ok((name, io)) => {
                    ports.insert(name, io);
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

        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: def_name.to_string(),
                ports,
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                usage: Usage::EmitDefinitionAndStop,
                generated_verilog: Some(verilog.to_string()),
                assignments: Vec::new(),
                unused: Vec::new(),
                tieoffs: Vec::new(),
                verilog_import: None,
            })),
        }
    }

    /// Validates this module hierarchically; panics if any errors are found. Validation primarily consists of checking that all inputs are driven exactly once, and all outputs are used at least once, unless specifically marked as unused. Validation behavior is controlled via the usage setting. If this module has the usage `EmitDefinitionAndDescend`, validation descends into each of those module definitions before validating the module. If this module definition has a usage other than `EmitDefinitionAndDescend`, it is not validated, and the modules it instantiates are not validated.
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

        let mut driven_bits: HashMap<PortKey, DrivenPortBits> = HashMap::new();
        let mut driving_bits: HashMap<PortKey, DrivingPortBits> = HashMap::new();

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
                IO::Input(_) => {
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
                    IO::Output(_) => {
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

        for (lhs_slice, rhs_slice) in &self.core.borrow().assignments {
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
        }

        // driven bits should be all driven

        for (key, driven) in &driven_bits {
            if !driven.all_driven() {
                panic!("{} is not fully driven.", key.debug_string());
            }
        }

        // driving bits should be all driving or unused

        for (key, driving) in &driving_bits {
            if !driving.all_driving_or_unused() {
                panic!("{} is not fully used. If some or all of this port is unused, mark those bits as unused.", key.debug_string());
            }
        }
    }

    fn can_be_driven(slice: &PortSlice) -> bool {
        matches!(
            (&slice.port, slice.port.io(),),
            (Port::ModDef { .. }, IO::Output(_),) | (Port::ModInst { .. }, IO::Input(_))
        )
    }

    fn can_drive(slice: &PortSlice) -> bool {
        matches!(
            (&slice.port, slice.port.io(),),
            (Port::ModDef { .. }, IO::Input(_),) | (Port::ModInst { .. }, IO::Output(_))
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

    /// Connects this port to another port or port slice.
    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T) {
        self.to_port_slice().connect(other);
    }

    /// Ties off this port to the given constant value, specified as a `BigInt` or type that can be converted to a `BigInt`.
    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        self.to_port_slice().tieoff(value);
    }

    /// Marks this port as unused, meaning that if it is a module instance output or module definition input, validation will not fail if the port drives nothing. In fact, validation will fail if the port drives anything.
    pub fn unused(&self) {
        self.to_port_slice().unused();
    }

    /// Returns a slice of this port from `msb` down to `lsb`, inclusive.
    pub fn slice(&self, msb: usize, lsb: usize) -> PortSlice {
        if msb >= self.io().width() || lsb > msb {
            panic!("Invalid slice of port {}", self.get_port_name());
        }
        PortSlice {
            port: self.clone(),
            msb,
            lsb,
        }
    }

    /// Splits this port into `n` equal slices, returning a vector of port slices. For example, if this port is 8-bit wide and `n` is 4, this will return a vector of 4 port slices, each 2 bits wide: `[1:0]`, `[3:2]`, `[5:4]`, and `[7:6]`.
    pub fn subdivide(&self, n: usize) -> Vec<PortSlice> {
        self.to_port_slice().subdivide(n)
    }

    /// Create a new port called `name` on the parent module and connect it to this port.
    ///
    /// The exact behavior depends on whether this is a port on a module definition or a module instance. If this is a port on a module definition, a new port is created on the same module definition, with the same width, but opposite direction. For example, suppose that this is a port `a` on a module definition that is an 8-bit input; calling `export_as("y")` will create an 8-bit output on the same module definition called `y`.
    ///
    /// If, on the other hand, this is a port on a module instance, a new port will be created on the module definition containing the instance, with the same width and direction. For example, if this is an 8-bit input port `x` on a module instance, calling `export_as("y")` will create a new 8-bit input port `y` on the module definition that contains the instance.
    pub fn export_as(&self, name: &str) {
        let io = self.io().clone();
        let mod_def_core = self.get_mod_def_core();
        if mod_def_core.borrow().ports.contains_key(name) {
            panic!(
                "Port {} already exists in module {}",
                name,
                mod_def_core.borrow().name
            );
        }
        mod_def_core.borrow_mut().ports.insert(name.to_string(), io);
        let new_port = Port::ModDef {
            name: name.to_string(),
            mod_def_core: Rc::downgrade(&mod_def_core),
        };
        self.connect(&new_port);
    }
}

impl PortSlice {
    fn debug_string(&self) -> String {
        match &self.port {
            Port::ModDef { name, .. } => format!(
                "{}.{}[{}:{}]",
                self.get_mod_def_core().borrow().name,
                name,
                self.msb,
                self.lsb
            ),
            Port::ModInst {
                inst_name,
                port_name,
                ..
            } => format!(
                "{}.{}.{}[{}:{}]",
                self.get_mod_def_core().borrow().name,
                inst_name,
                port_name,
                self.msb,
                self.lsb
            ),
        }
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

    /// Connects this port slice to another port or port slice. Performs some upfront checks to make sure that the connection is valid in terms of width and directionality. Panics if any of these checks fail.
    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T) {
        let other_as_slice = other.to_port_slice();

        let mod_def_core = self.get_mod_def_core();

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
                "Invalid connection between ports: {} and {}",
                self.debug_string(),
                other_as_slice.debug_string()
            ),
        };

        let lhs = (*lhs).clone();
        let rhs = (*rhs).clone();
        mod_def_core.borrow_mut().assignments.push((lhs, rhs));
    }

    /// Ties off this port slice to the given constant value, specified as a `BigInt` or type that can be converted to a `BigInt`.
    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core
            .borrow_mut()
            .tieoffs
            .push(((*self).clone(), value.into()));
    }

    /// Marks this port slice as unused, meaning that if it is an module instance output or module definition input, validation will not fail if the slice drives nothing. In fact, validation will fail if the slice drives anything.
    pub fn unused(&self) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core.borrow_mut().unused.push((*self).clone());
    }

    fn check_validity(&self) {
        if self.msb >= self.port.io().width() {
            panic!(
                "{} is invalid: msb must be less than the width of the port.",
                self.debug_string()
            );
        } else if self.lsb > self.msb {
            panic!(
                "{} is invalid: lsb must be less than or equal to msb.",
                self.debug_string()
            );
        }
    }
}

impl ModInst {
    /// Returns the port on this instance with the given name. Panics if no such port exists.
    pub fn get_port(&self, name: &str) -> Port {
        ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .get_port(name)
        .assign_to_inst(self)
    }

    /// Returns a slice of the port on this instance with the given name, from `msb` down to `lsb`, inclusive. Panics if no such port exists.
    pub fn get_port_slice(&self, name: &str, msb: usize, lsb: usize) -> PortSlice {
        self.get_port(name).slice(msb, lsb)
    }

    /// Returns a vector of ports on this instance with the given prefix, or all ports if `prefix` is `None`.
    pub fn get_ports(&self, prefix: Option<&str>) -> Vec<Port> {
        let result = ModDef {
            core: self.mod_def_core.upgrade().unwrap(),
        }
        .get_ports(prefix);
        result
            .into_iter()
            .map(|port| port.assign_to_inst(self))
            .collect()
    }

    /// Returns the interface on this instance with the given name. Panics if no such interface exists.
    pub fn get_intf(&self, name: &str) -> Intf {
        let mod_def_core = self.mod_def_core.upgrade().unwrap();
        let instances = &mod_def_core.borrow().instances;
        let inst_core = instances.get(&self.name).unwrap().clone();

        let inst_core_borrowed = inst_core.borrow();

        if inst_core_borrowed.interfaces.contains_key(name) {
            Intf::ModInst {
                intf_name: name.to_string(),
                inst_name: self.name.clone(),
                mod_def_core: self.mod_def_core.clone(),
            }
        } else {
            panic!(
                "Interface '{}' does not exist in instance '{}'",
                name, self.name
            );
        }
    }
}

/// Represents an interface on a module definition or module instance. Interfaces are used to connect modules together by function name.
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

    /// Connects this interface to another interface. Interfaces are connected by matching up ports with the same function name and connecting them. For example, if this interface is {"data": "a_data", "valid": "a_valid"} and the other interface is {"data": "b_data", "valid": "b_valid"}, then "a_data" will be connected to "b_data" and "a_valid" will be connected to "b_valid".
    ///
    /// Unless `allow_mismatch` is `true`, this method will panic if a function in this interface is not in the other interface. Continuing the previous example, if this interface also contained function "ready", but the other interface did not, this method would panic unless `allow_mismatch` was `true`.
    pub fn connect(&self, other: &Intf, allow_mismatch: bool) {
        let self_ports = self.get_port_slices();
        let other_ports = other.get_port_slices();

        for (func_name, self_port) in self_ports {
            if let Some(other_port) = other_ports.get(&func_name) {
                self_port.connect(other_port);
            } else if !allow_mismatch {
                panic!("Interfaces have mismatched functions and allow_mismatch is false");
            }
        }
    }

    /// Signals matching regex `pattern_a` on one interface are connected to signals matching regex `pattern_b` on the other interface, and vice versa. For example, suppose that this interface is `{"data_tx": "a_data_tx", "data_rx": "a_data_rx"}` and the other interface is `{"data_tx": "b_data_tx", "data_rx": "b_data_rx"}`. One might write this_intf.crossover(&other_intf, "(.*)_tx", "(.*)_rx") to connect the `data_tx` function on this interface (mapped to `a_data_tx`) to the `data_rx` function on the other interface (mapped to `b_data_rx`), and vice versa.
    pub fn crossover(&self, other: &Intf, pattern_a: &str, pattern_b: &str) {
        let pattern_a_regex = Regex::new(pattern_a).unwrap();
        let pattern_b_regex = Regex::new(pattern_b).unwrap();

        let mut self_a_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut self_b_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut other_a_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut other_b_matches: IndexMap<String, PortSlice> = IndexMap::new();

        for (func_name, port_slice) in self.get_port_slices() {
            if let Some(captures) = pattern_a_regex.captures(&func_name) {
                let func_name = captures.get(1).unwrap().as_str().to_string();
                self_a_matches.insert(func_name, port_slice);
            } else if let Some(captures) = pattern_b_regex.captures(&func_name) {
                let func_name = captures.get(1).unwrap().as_str().to_string();
                self_b_matches.insert(func_name, port_slice);
            }
        }

        for (func_name, port_slice) in other.get_port_slices() {
            if let Some(captures) = pattern_a_regex.captures(&func_name) {
                let func_name = captures.get(1).unwrap().as_str().to_string();
                other_a_matches.insert(func_name, port_slice);
            } else if let Some(captures) = pattern_b_regex.captures(&func_name) {
                let func_name = captures.get(1).unwrap().as_str().to_string();
                other_b_matches.insert(func_name, port_slice);
            }
        }

        for (func_name, self_a_port) in self_a_matches {
            if let Some(other_b_port) = other_b_matches.get(&func_name) {
                self_a_port.connect(other_b_port);
            }
        }

        for (func_name, self_b_port) in self_b_matches {
            if let Some(other_a_port) = other_a_matches.get(&func_name) {
                self_b_port.connect(other_a_port);
            }
        }
    }

    /// Ties off driven signals on this interface to the given constant value. A "driven signal" is an input of a module instance or an output of a module definition; it's a signal that would appear on the left hand side of a Verilog `assign` statement.
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

    /// Marks unused driving signals on this interface. A "driving signal" is an output of a module instance or an input of a module definition; it's a signal that would appear on the right hand side of a Verilog `assign` statement.
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

    /// Creates a new interface on the parent module and connects it to this interface. The new interface will have the same functions as this interface; signal names are formed by concatenating the given prefix and the function name. For example, if this interface is `{"data": "a_data", "valid": "a_valid"}` and the prefix is "b_", the new interface will be `{"data": "b_data", "valid": "b_valid"}`. The `name` argument specifies the name of the new interface, which is used to retrieve the interface with `get_intf`.
    pub fn export_with_prefix(&self, name: &str, prefix: &str) {
        match self {
            Intf::ModInst { .. } => {
                let mut mapping = IndexMap::new();
                for (func_name, port_slice) in self.get_port_slices() {
                    let mod_def_port_name = format!("{}{}", prefix, func_name);
                    port_slice.export_as(&mod_def_port_name);
                    mapping.insert(func_name, (mod_def_port_name, port_slice.width() - 1, 0));
                }
                ModDef {
                    core: self.get_mod_def_core(),
                }
                .def_intf(name, mapping);
            }
            Intf::ModDef { .. } => {
                panic!("export_with_prefix() can only be called on ModInst interfaces");
            }
        }
    }

    /// Divides each signal in this interface into `n` equal slices, returning a vector of interfaces. For example, if this interface is `{"data": "a_data[31:0]", "valid": "a_valid[3:0]"}` and `n` is 4, this will return a vector of 4 interfaces, each with signals `{"data": "a_data[7:0]", "valid": "a_valid[0:0]"}`, `{"data": "a_data[15:8]", "valid": "a_valid[1:1]"}`, and so on. The names of the new interfaces are formed by appending "_0", "_1", "_2", and so on to the name of this interface; these names can be used to retrieve specific slices of the interface with `get_intf`.
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
                _ => panic!("Subdividing ModInst interfaces is not supported."),
            };
            result.push(intf);
        }

        result
    }
}

fn parser_port_to_port(parser_port: &slang_rs::Port) -> Result<(String, IO), String> {
    let size = parser_port.ty.width().unwrap();

    let io = match parser_port.dir {
        slang_rs::PortDir::Input => IO::Input(size),
        slang_rs::PortDir::Output => IO::Output(size),
        _ => panic!("Unsupported port direction: {:?}", parser_port.dir),
    };
    Ok((parser_port.name.clone(), io))
}
