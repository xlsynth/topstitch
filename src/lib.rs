// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use indexmap::IndexMap;
use num_bigint::BigInt;
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
        let mut file = VastFile::new(VastFileType::Verilog);
        self.emit_recursive(&mut emitted_module_names, &mut file);
        file.emit()
    }

    fn emit_recursive(
        &self,
        emitted_module_names: &mut IndexMap<String, Rc<RefCell<ModDefCore>>>,
        file: &mut VastFile,
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
                    .emit_recursive(emitted_module_names, file);
                }
                EmitConfig::Stub => {
                    panic!("Stub mode not implemented yet.");
                }
                EmitConfig::Nothing => {
                    // Do nothing
                }
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
        for (src, dst) in &core.connections {
            let src_expr = match src {
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
            let dst_expr = match dst {
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
                file.make_continuous_assignment(&dst_expr.to_expr(), &src_expr.to_expr());
            module.add_member_continuous_assignment(assignment);
        }

        // // Emit assign statements for tie-offs.
        // for (slice, value) in &core.tieoffs {
        //     let dst_expr = match slice {
        //         PortSlice {
        //             port: Port::ModDef { name, .. },
        //             msb,
        //             lsb,
        //         } => {
        //             file.make_slice(&ports.get(name).unwrap().to_indexable_expr(), *msb as i64, *lsb as i64)
        //         }
        //         PortSlice {
        //             port:
        //             Port::ModInst {
        //                 inst_name,
        //                 port_name,
        //                 ..
        //             },
        //             msb,
        //             lsb,
        //         } => {
        //             let net_name = format!("{}_{}", inst_name, port_name);
        //             file.make_slice(&nets.get(&net_name).unwrap().to_indexable_expr(), *msb as i64, *lsb as i64)
        //         }
        //     };

        //     verilog.push_str(&format!("    assign {} = {};\n", dst_expr, value));
        // }
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
