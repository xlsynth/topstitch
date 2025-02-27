// SPDX-License-Identifier: Apache-2.0

use crate::mod_def::{Assignment, InstConnection, PortSliceOrWire, Wire};
use crate::{ConvertibleToPortSlice, ModDef, ModInst, PipelineConfig, Port, PortSlice, IO};

impl PortSlice {
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
        self.connect_generic(other, None, false);
    }

    /// Connects this port slice to another port or port slice, assuming that
    /// the connection is non-abutted.
    pub fn connect_non_abutted<T: ConvertibleToPortSlice>(&self, other: &T) {
        self.connect_generic(other, None, true);
    }

    pub fn connect_pipeline<T: ConvertibleToPortSlice>(&self, other: &T, pipeline: PipelineConfig) {
        self.connect_generic(other, Some(pipeline), false);
    }

    pub(crate) fn connect_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        pipeline: Option<PipelineConfig>,
        is_non_abutted: bool,
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
                if let Some(inst_name) = &pipeline.inst_name {
                    assert!(
                        !mod_def_core.borrow().instances.contains_key(inst_name),
                        "Cannot use pipeline instance name {}, since that instance name is already used in module definition {}.",
                        inst_name,
                        mod_def_core.borrow().name
                    );
                }
                if !mod_def_core.borrow().ports.contains_key(&pipeline.clk) {
                    ModDef {
                        core: mod_def_core.clone(),
                    }
                    .add_port(pipeline.clk.clone(), IO::Input(1));
                }
            }
            let lhs = (*lhs).clone();
            let rhs = (*rhs).clone();
            mod_def_core.borrow_mut().assignments.push(Assignment {
                lhs,
                rhs,
                pipeline,
                is_non_abutted,
            });
        }
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
}
