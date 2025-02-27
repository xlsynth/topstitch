// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use std::cell::RefCell;
use std::rc::Rc;

use crate::mod_def::dtypes::{Assignment, PortSliceOrWire};

use crate::validate::{DrivenPortBits, DrivingPortBits, PortKey, UnusedError};
use crate::{ModDef, ModDefCore, Port, PortSlice, Usage, IO};

impl ModDef {
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
            ..
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
