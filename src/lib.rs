// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use indexmap::{IndexMap, IndexSet};
use num_bigint::BigInt;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

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

#[derive(Clone)]
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

pub enum EmitConfig {
    Nothing,
    Stub,
    Recurse,
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
        let mut mod_defs_as_strings = Vec::new();
        self.emit_recursive(&mut emitted_module_names, &mut mod_defs_as_strings);
        mod_defs_as_strings.join("\n")
    }

    fn emit_recursive(
        &self,
        emitted_module_names: &mut IndexMap<String, Rc<RefCell<ModDefCore>>>,
        mod_defs_as_strings: &mut Vec<String>,
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

        // Recursively emit instances
        // Verilog generation code is a placeholder - planning to use VAST

        for inst in core.instances.values() {
            let inst_def = inst;
            let inst_def_inner = inst_def.borrow();
            match inst_def_inner.emit_config {
                EmitConfig::Recurse => {
                    ModDef {
                        core: inst_def.clone(),
                    }
                    .emit_recursive(emitted_module_names, mod_defs_as_strings);
                }
                EmitConfig::Stub => {
                    panic!("Stub mode not implemented yet.");
                }
                EmitConfig::Nothing => {
                    // Do nothing
                }
            }
        }

        let mut verilog = String::new();

        // Start the module declaration.
        verilog.push_str(&format!("module {} (\n", core.name));

        let mut port_names: IndexSet<String> = IndexSet::new();

        for (idx, port_name) in core.ports.keys().enumerate() {
            let io = core.ports.get(port_name).unwrap();
            if !port_names.insert(port_name.clone()) {
                panic!("Port name {} is already declared.", port_name);
            }
            match io {
                IO::Input(width) => {
                    if *width == 1 {
                        verilog.push_str(format!("    input {}", port_name).as_str());
                    } else {
                        verilog.push_str(
                            format!("    input [{}:0] {}", width - 1, port_name).as_str(),
                        );
                    }
                }
                IO::Output(width) => {
                    if *width == 1 {
                        verilog.push_str(format!("    output {}", port_name).as_str());
                    } else {
                        verilog.push_str(
                            format!("    output [{}:0] {}", width - 1, port_name).as_str(),
                        );
                    }
                }
            }
            if idx != core.ports.len() - 1 {
                verilog.push(',');
            }
            verilog.push('\n');
        }
        verilog.push_str(");\n\n");

        // List out the wires to be used for internal connections.
        let mut net_names = IndexSet::new();
        for (inst_name, inst) in core.instances.iter() {
            for (port_name, io) in inst.borrow().ports.iter() {
                let net_name = format!("{}_{}", inst_name, port_name);
                if port_names.contains(&net_name) {
                    panic!(
                        "{} is already declared as a port of the module containing this instance.",
                        net_name
                    );
                }
                if !net_names.insert((net_name.clone(), io.width())) {
                    panic!("Wire name {} is already declared in this module", net_name);
                }
            }
        }

        // Emit Verilog wire definitions.
        for (net_name, width) in net_names {
            if !core.ports.contains_key(&net_name) {
                if width == 1 {
                    verilog.push_str(&format!("    wire {};\n", net_name));
                } else {
                    verilog.push_str(&format!("    wire [{}:0] {};\n", width - 1, net_name));
                }
            }
        }
        verilog.push('\n');

        // Instantiate modules.
        for (inst_name, inst) in core.instances.iter() {
            verilog.push_str(&format!("    {} {} (\n", inst.borrow().name, inst_name));
            let mut port_conns = Vec::new();
            for (i, (port_name, _)) in inst.borrow().ports.iter().enumerate() {
                let net_name = format!("{}_{}", inst_name, port_name);
                let sep = if i == inst.borrow().ports.len() - 1 {
                    "\n"
                } else {
                    ",\n"
                };
                port_conns.push(format!("        .{}({}){}", port_name, net_name, sep));
            }
            verilog.push_str(&port_conns.join(""));
            verilog.push_str("    );\n\n");
        }

        // Emit assign statment for connections.
        for (src, dst) in &core.connections {
            let src_expr = match src {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => {
                    if msb == lsb {
                        name.clone()
                    } else {
                        format!("{}[{}:{}]", name, msb, lsb)
                    }
                }
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
                    if msb == lsb {
                        format!("{}_{}", inst_name, port_name)
                    } else {
                        format!("{}_{}[{}:{}]", inst_name, port_name, msb, lsb)
                    }
                }
            };
            let dst_expr = match dst {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => {
                    if msb == lsb {
                        name.clone()
                    } else {
                        format!("{}[{}:{}]", name, msb, lsb)
                    }
                }
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
                    if msb == lsb {
                        format!("{}_{}", inst_name, port_name)
                    } else {
                        format!("{}_{}[{}:{}]", inst_name, port_name, msb, lsb)
                    }
                }
            };
            verilog.push_str(&format!("    assign {} = {};\n", dst_expr, src_expr));
        }

        // Emit assign statements for tie-offs.
        for (slice, value) in &core.tieoffs {
            let dst_expr = match slice {
                PortSlice {
                    port: Port::ModDef { name, .. },
                    msb,
                    lsb,
                } => {
                    if msb == lsb {
                        name.clone()
                    } else {
                        format!("{}[{}:{}]", name, msb, lsb)
                    }
                }
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
                    if msb == lsb {
                        format!("{}_{}", inst_name, port_name)
                    } else {
                        format!("{}_{}[{}:{}]", inst_name, port_name, msb, lsb)
                    }
                }
            };
            verilog.push_str(&format!("    assign {} = {};\n", dst_expr, value));
        }

        // End the module definition.
        verilog.push_str("endmodule\n");

        // Add this module's Verilog code to the running Verilog output.
        mod_defs_as_strings.push(verilog);
    }

    pub fn validate(&self) {
        panic!("Validation not implemented yet.");
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
        let src = self.to_port_slice();
        let dst = other.to_port_slice();

        let mod_def_core = self.get_mod_def_core();
        let mut inner = mod_def_core.borrow_mut();

        inner.connections.push((src, dst));
    }

    pub fn tieoff(&self, value: BigInt) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core
            .borrow_mut()
            .tieoffs
            .push((self.to_port_slice(), value));
    }

    pub fn noconnect(&self) {
        let mod_def_core = self.get_mod_def_core();
        mod_def_core
            .borrow_mut()
            .noconnects
            .push(self.to_port_slice());
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
}
