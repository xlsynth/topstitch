// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use indexmap::IndexMap;
use num_bigint::BigInt;
use slang_rs::extract_ports;
use std::cell::RefCell;
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
    fn io(&self) -> IO {
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
}

#[derive(Clone)]
pub struct PortSlice {
    pub port: Port,
    pub msb: usize,
    pub lsb: usize,
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

pub struct ModDefCore {
    pub name: String,
    pub ports: IndexMap<String, IO>,
    pub interfaces: IndexMap<String, IndexMap<String, String>>,
    pub instances: IndexMap<String, Rc<RefCell<ModDefCore>>>,
    pub emit_config: EmitConfig,
    pub implementation: Option<String>,
    pub connections: Vec<(PortSlice, PortSlice)>,
    pub noconnects: Vec<PortSlice>,
    pub tieoffs: Vec<(PortSlice, BigInt)>,
}

#[derive(PartialEq)]
pub enum EmitConfig {
    Nothing,
    Stub,
    Recurse,
    Leaf,
}

impl ModDef {
    pub fn new(name: &str) -> ModDef {
        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: name.to_string(),
                ports: IndexMap::new(),
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                emit_config: EmitConfig::Recurse,
                implementation: None,
                connections: Vec::new(),
                noconnects: Vec::new(),
                tieoffs: Vec::new(),
            })),
        }
    }

    pub fn from_verilog(
        name: &str,
        verilog: &str,
        ignore_unknown_modules: bool,
        emit_config: EmitConfig,
    ) -> ModDef {
        let parser_ports = extract_ports(verilog, ignore_unknown_modules);

        let mut ports = IndexMap::new();
        for parser_port in parser_ports[name].iter() {
            let io = match parser_port.dir {
                slang_rs::PortDir::Input => IO::Input(parser_port.msb - parser_port.lsb + 1),
                slang_rs::PortDir::Output => IO::Output(parser_port.msb - parser_port.lsb + 1),
                _ => panic!("Unsupported port direction: {:?}", parser_port.dir),
            };
            ports.insert(parser_port.name.clone(), io);
        }

        ModDef {
            core: Rc::new(RefCell::new(ModDefCore {
                name: name.to_string(),
                ports,
                interfaces: IndexMap::new(),
                instances: IndexMap::new(),
                emit_config,
                implementation: Some(verilog.to_string()),
                connections: Vec::new(),
                noconnects: Vec::new(),
                tieoffs: Vec::new(),
            })),
        }
    }

    pub fn add_port(&self, name: &str, io: IO) -> Port {
        let mut core = self.core.borrow_mut();
        match core.ports.entry(name.to_string()) {
            Entry::Occupied(_) => panic!("Port {} already exists in module {}", name, core.name),
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
            panic!("Port {} does not exist in module {}", name, inner.name)
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

    pub fn instantiate(&self, moddef: &ModDef, name: &str) -> ModInst {
        let mut inner = self.core.borrow_mut();
        match inner.instances.entry(name.to_string()) {
            Entry::Occupied(_) => {
                panic!("Instance {} already exists in module {}", name, inner.name)
            }
            Entry::Vacant(entry) => {
                let inst = ModInst {
                    name: name.to_string(),
                    mod_def_core: Rc::downgrade(&self.core),
                };
                entry.insert(moddef.core.clone());
                inst
            }
        }
    }

    pub fn emit(&self) -> String {
        // self.validate();
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

        if core.emit_config == EmitConfig::Nothing {
            return;
        } else if core.emit_config == EmitConfig::Leaf {
            leaf_text.push(core.implementation.clone().unwrap());
            return;
        }

        // Recursively emit instances

        if core.emit_config == EmitConfig::Recurse {
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

        if core.emit_config == EmitConfig::Stub {
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
        for (a, b) in &core.connections {
            let a_expr = match a {
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
            let b_expr = match b {
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
            let (lhs, rhs) = match (&a.port, a.port.io(), &b.port, b.port.io()) {
                (Port::ModDef { .. }, IO::Input(_), Port::ModDef { .. }, IO::Output(_)) => {
                    (b_expr.to_expr(), a_expr.to_expr())
                }
                (Port::ModDef { .. }, IO::Output(_), Port::ModDef { .. }, IO::Input(_)) => {
                    (a_expr.to_expr(), b_expr.to_expr())
                }
                (Port::ModInst { .. }, IO::Input(_), Port::ModDef { .. }, IO::Input(_)) => {
                    (a_expr.to_expr(), b_expr.to_expr())
                }
                (Port::ModDef { .. }, IO::Input(_), Port::ModInst { .. }, IO::Input(_)) => {
                    (b_expr.to_expr(), a_expr.to_expr())
                }
                (Port::ModInst { .. }, IO::Output(_), Port::ModDef { .. }, IO::Output(_)) => {
                    (b_expr.to_expr(), a_expr.to_expr())
                }
                (Port::ModDef { .. }, IO::Output(_), Port::ModInst { .. }, IO::Output(_)) => {
                    (a_expr.to_expr(), b_expr.to_expr())
                }
                (Port::ModInst { .. }, IO::Input(_), Port::ModInst { .. }, IO::Output(_)) => {
                    (a_expr.to_expr(), b_expr.to_expr())
                }
                (Port::ModInst { .. }, IO::Output(_), Port::ModInst { .. }, IO::Input(_)) => {
                    (b_expr.to_expr(), a_expr.to_expr())
                }
                _ => panic!(
                    "Invalid connection between ports: {:?} and {:?}",
                    a.port.io(),
                    b.port.io()
                ),
            };
            let assignment = file.make_continuous_assignment(&lhs, &rhs);
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
            let value_expr = file.make_literal(
                &literal_str,
                &xlsynth::ir_value::IrFormatPreference::UnsignedDecimal,
            );
            let assignment =
                file.make_continuous_assignment(&dst_expr.to_expr(), &value_expr.unwrap());
            module.add_member_continuous_assignment(assignment);
        }
    }

    pub fn validate(&self) {
        panic!("Validation not implemented yet.");
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

    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T, _pipeline: usize) {
        self.to_port_slice().connect(other, _pipeline);
    }

    pub fn tieoff(&self, value: BigInt) {
        self.to_port_slice().tieoff(value);
    }

    pub fn noconnect(&self) {
        self.to_port_slice().noconnect();
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

    pub fn export_as(&self, name: &str) {
        let mod_def_core = self.get_mod_def_core();
        let mut mod_def_core_borrowed = mod_def_core.borrow_mut();
        if mod_def_core_borrowed.ports.contains_key(name) {
            panic!(
                "Port {} already exists in module {}",
                name, mod_def_core_borrowed.name
            );
        }
        mod_def_core_borrowed
            .ports
            .insert(name.to_string(), self.io().clone());
        let new_port = Port::ModDef {
            name: name.to_string(),
            mod_def_core: Rc::downgrade(&mod_def_core),
        };
        self.connect(&new_port, 0);
    }

    // wrap()

    // feedthrough()
}

impl PortSlice {
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

    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T, _pipeline: usize) {
        let dst = other.to_port_slice();

        let mod_def_core = self.get_mod_def_core();
        let mut inner = mod_def_core.borrow_mut();

        inner.connections.push(((*self).clone(), dst));
    }

    pub fn tieoff(&self, value: BigInt) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core
            .borrow_mut()
            .tieoffs
            .push(((*self).clone(), value));
    }

    pub fn noconnect(&self) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core.borrow_mut().noconnects.push((*self).clone());
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

    pub fn connect(&self, other: &Intf, pipeline: usize, allow_mismatch: bool) {
        let self_ports = self.get_ports();
        let other_ports = other.get_ports();

        for (func_name, self_port) in self_ports {
            if let Some(other_port) = other_ports.get(&func_name) {
                self_port.connect(other_port, pipeline);
            } else if !allow_mismatch {
                panic!("Interfaces have mismatched functions and allow_mismatch is false");
            }
        }
    }

    pub fn tieoff(&self, value: BigInt) {
        let core = self.get_mod_def_core();
        let binding = core.borrow();
        let mapping = binding.interfaces.get(&self.get_intf_name()).unwrap();
        for (_, port_name) in mapping {
            ModDef { core: core.clone() }
                .get_port(port_name)
                .tieoff(value.clone());
        }
    }

    pub fn noconnect(&self) {
        let core = self.get_mod_def_core();
        let binding = core.borrow();
        let mapping = binding.interfaces.get(&self.get_intf_name()).unwrap();
        for (_, port_name) in mapping {
            ModDef { core: core.clone() }
                .get_port(port_name)
                .noconnect();
        }
    }

    pub fn export_with_prefix(&self, prefix: &str) {
        match self {
            Intf::ModInst {
                intf_name,
                inst_name,
                mod_def_core,
            } => {
                let mod_def = ModDef {
                    core: mod_def_core.upgrade().unwrap(),
                };
                let binding = mod_def.core.borrow();
                let mapping = binding.interfaces.get(intf_name).unwrap();

                let mod_inst = ModDef {
                    core: binding.instances.get(inst_name).unwrap().clone(),
                };

                for (func_name, port_name) in mapping {
                    let mod_def_port_name = format!("{}{}", prefix, func_name);
                    let mod_inst_port = mod_inst.get_port(port_name);
                    let mod_def_port = mod_def.add_port(&mod_def_port_name, mod_inst_port.io());
                    mod_inst_port.connect(&mod_def_port, 0);
                }
            }
            Intf::ModDef { .. } => {
                panic!("export_with_prefix() can only be called on ModInst interfaces");
            }
        }
    }
}
