// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use indexmap::IndexMap;
use itertools::Itertools;
use num_bigint::{BigInt, BigUint};
use slang_rs::{self, extract_ports, str2tmpfile, SlangConfig};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::Path;
use std::rc::{Rc, Weak};
use xlsynth::vast::{Expr, LogicRef, VastFile, VastFileType};

#[derive(Clone, Debug)]
pub enum IO {
    Input(usize),
    Output(usize),
}

impl IO {
    pub fn width(&self) -> usize {
        match self {
            IO::Input(width) => *width,
            IO::Output(width) => *width,
        }
    }
}

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

#[derive(Clone)]
pub struct PortSlice {
    pub port: Port,
    pub msb: usize,
    pub lsb: usize,
}

impl PortSlice {
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
}

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

#[derive(Clone)]
pub struct ModInst {
    pub name: String,
    pub mod_def_core: Weak<RefCell<ModDefCore>>,
}

#[derive(Clone)]
pub struct ModDef {
    pub core: Rc<RefCell<ModDefCore>>,
}

pub struct VerilogImport {
    pub sources: Vec<String>,
    pub skip_unsupported: bool,
    pub ignore_unknown_modules: bool,
}

pub struct ModDefCore {
    pub name: String,
    pub ports: IndexMap<String, IO>,
    pub interfaces: IndexMap<String, IndexMap<String, String>>,
    pub instances: IndexMap<String, Rc<RefCell<ModDefCore>>>,
    pub usage: Usage,
    pub generated_verilog: Option<String>,
    pub verilog_import: Option<VerilogImport>,
    pub assignments: Vec<(PortSlice, PortSlice)>,
    pub unused: Vec<PortSlice>,
    pub tieoffs: Vec<(PortSlice, BigInt)>,
}

#[derive(PartialEq, Default, Clone)]
pub enum Usage {
    #[default]
    EmitDefinitionAndDescend,
    EmitNothingAndStop,
    EmitStubAndStop,
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
    pub fn debug_string(&self) -> String {
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

    pub fn from_verilog_file(
        name: &str,
        verilog: &Path,
        ignore_unknown_modules: bool,
        skip_unsupported: bool,
    ) -> Self {
        Self::from_verilog_files(name, &[verilog], ignore_unknown_modules, skip_unsupported)
    }

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

    pub fn set_usage(&self, usage: Usage) {
        if self.core.borrow().generated_verilog.is_some() {
            assert!(
                usage != Usage::EmitDefinitionAndDescend,
                "Cannot descend into a module defined from Verilog sources."
            );
        }
        self.core.borrow_mut().usage = usage;
    }

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

    pub fn emit_to_file(&self, path: &Path, validate: bool) {
        std::fs::write(path, self.emit(validate)).unwrap();
    }

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

    pub fn def_intf(&self, name: &str, mapping: IndexMap<String, String>) -> Intf {
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

    pub fn def_intf_from_prefix(&self, name: &str, prefix: &str) -> Intf {
        let mut mapping = IndexMap::new();
        {
            let core = self.core.borrow();
            for port_name in core.ports.keys() {
                if port_name.starts_with(prefix) {
                    let func_name = port_name.strip_prefix(prefix).unwrap().to_string();
                    mapping.insert(func_name, port_name.clone());
                }
            }
        }
        self.def_intf(name, mapping)
    }

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

    pub fn feedthrough(&self, input_name: &str, output_name: &str, width: usize) {
        let input_port = self.add_port(input_name, IO::Input(width));
        let output_port = self.add_port(output_name, IO::Output(width));
        input_port.connect(&output_port);
    }

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

    pub fn validate(&self) {
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
    pub fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            Port::ModDef { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
            Port::ModInst { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
        }
    }

    pub fn get_port_name(&self) -> String {
        match self {
            Port::ModDef { name, .. } => name.clone(),
            Port::ModInst { port_name, .. } => port_name.clone(),
        }
    }

    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T) {
        self.to_port_slice().connect(other);
    }

    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        self.to_port_slice().tieoff(value);
    }

    pub fn unused(&self) {
        self.to_port_slice().unused();
    }

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

    pub fn subdivide(&self, n: usize) -> Vec<PortSlice> {
        self.to_port_slice().subdivide(n)
    }

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

    pub fn debug_string_bit(&self, idx: usize) -> String {
        match &self {
            Port::ModDef { name, .. } => format!(
                "{}.{}[{}]",
                self.get_mod_def_core().borrow().name,
                name,
                idx
            ),
            Port::ModInst {
                inst_name,
                port_name,
                ..
            } => format!(
                "{}.{}.{}[{}]",
                self.get_mod_def_core().borrow().name,
                inst_name,
                port_name,
                idx
            ),
        }
    }
}

impl PortSlice {
    pub fn debug_string(&self) -> String {
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

    pub fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
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

    pub fn tieoff<T: Into<BigInt>>(&self, value: T) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core
            .borrow_mut()
            .tieoffs
            .push(((*self).clone(), value.into()));
    }

    pub fn unused(&self) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core.borrow_mut().unused.push((*self).clone());
    }

    pub fn check_validity(&self) {
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
    pub fn get_port(&self, name: &str) -> Port {
        ModDef {
            core: self.mod_def_core.upgrade().unwrap().borrow().instances[&self.name].clone(),
        }
        .get_port(name)
        .assign_to_inst(self)
    }

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
    pub fn get_mod_def_core(&self) -> Rc<RefCell<ModDefCore>> {
        match self {
            Intf::ModDef { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
            Intf::ModInst { mod_def_core, .. } => mod_def_core.upgrade().unwrap(),
        }
    }

    pub fn get_intf_name(&self) -> String {
        match self {
            Intf::ModDef { name, .. } => name.clone(),
            Intf::ModInst { intf_name, .. } => intf_name.clone(),
        }
    }

    pub fn get_ports(&self) -> IndexMap<String, Port> {
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
                    .map(|(func_name, port_name)| (func_name.clone(), mod_def.get_port(port_name)))
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
                    .map(|(func_name, port_name)| (func_name.clone(), inst.get_port(port_name)))
                    .collect()
            }
        }
    }

    pub fn connect(&self, other: &Intf, allow_mismatch: bool) {
        let self_ports = self.get_ports();
        let other_ports = other.get_ports();

        for (func_name, self_port) in self_ports {
            if let Some(other_port) = other_ports.get(&func_name) {
                self_port.connect(other_port);
            } else if !allow_mismatch {
                panic!("Interfaces have mismatched functions and allow_mismatch is false");
            }
        }
    }

    pub fn crossover(&self, other: &Intf) {
        let self_ports = self.get_ports();
        assert_eq!(
            self_ports.len(),
            2,
            "Interface must have exactly two functions."
        );

        let other_ports = other.get_ports();
        assert_eq!(
            other_ports.len(),
            2,
            "Other interface must have exactly two functions."
        );

        let mut self_keys: Vec<_> = self_ports.keys().collect();
        self_keys.sort();

        let mut other_keys: Vec<_> = other_ports.keys().collect();
        other_keys.sort();

        if self_keys != other_keys {
            panic!("Interface functions must be the same.");
        }

        self_ports
            .get(self_keys[0].as_str())
            .unwrap()
            .connect(other_ports.get(other_keys[1].as_str()).unwrap());
        self_ports
            .get(self_keys[1].as_str())
            .unwrap()
            .connect(other_ports.get(other_keys[0].as_str()).unwrap());
    }

    pub fn tieoff<T: Into<BigInt> + Clone>(&self, value: T) {
        for (_, port) in self.get_ports() {
            match port {
                Port::ModDef { .. } => {
                    if let IO::Output(_) = port.io() {
                        port.tieoff(value.clone());
                    }
                }
                Port::ModInst { .. } => {
                    if let IO::Input(_) = port.io() {
                        port.tieoff(value.clone());
                    }
                }
            }
        }
    }

    pub fn unused(&self) {
        for (_, port) in self.get_ports() {
            match port {
                Port::ModDef { .. } => {
                    if let IO::Input(_) = port.io() {
                        port.unused();
                    }
                }
                Port::ModInst { .. } => {
                    if let IO::Output(_) = port.io() {
                        port.unused();
                    }
                }
            }
        }
    }

    pub fn export_with_prefix(&self, name: &str, prefix: &str) {
        match self {
            Intf::ModInst { .. } => {
                let mut mapping = IndexMap::new();
                for (func_name, port) in self.get_ports() {
                    let mod_def_port_name = format!("{}{}", prefix, func_name);
                    port.export_as(&mod_def_port_name);
                    mapping.insert(func_name, mod_def_port_name);
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
