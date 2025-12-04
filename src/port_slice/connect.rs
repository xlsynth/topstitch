// SPDX-License-Identifier: Apache-2.0

use crate::connection::port_slice::Abutment;
use crate::port::PortDirectionality;
use crate::{ConvertibleToPortSlice, IO, ModInst, PipelineConfig, Port, PortSlice};
use std::rc::Rc;

impl PortSlice {
    /// Specifies the net name to be used for this port slice.
    pub fn specify_net_name(&self, net: &str) {
        if let Port::ModInst { .. } = &self.port {
            // Record and enforce uniqueness of explicitly specified net names
            // within the containing ModDef.
            {
                let core_rc = self.get_mod_def_core();
                let mut core = core_rc.borrow_mut();
                if !core.specified_net_names.insert(net.to_string()) {
                    panic!(
                        "Net \"{}\" has already been manually specified in module {}.",
                        net, core.name
                    );
                };
            }

            let this = self.to_port_slice();
            let width = this.port.io().width();
            let other = crate::connection::connected_item::Wire {
                name: net.to_string(),
                width,
                msb: self.msb,
                lsb: self.lsb,
            };
            self.port
                .get_port_connections_define_if_missing()
                .borrow_mut()
                .add(this, other, Abutment::NA);
        } else {
            panic!(
                "{} only works on ports (or slices of ports) on module instances",
                stringify!(specify_net_name)
            );
        }
    }

    /// Connects this port slice to another port or port slice. Performs some
    /// upfront checks to make sure that the connection is valid in terms of
    /// width and directionality. Panics if any of these checks fail.
    pub fn connect<T: ConvertibleToPortSlice>(&self, other: &T) {
        self.connect_generic(other, None, Abutment::Abutted);
    }

    /// Connects this port slice to another port or port slice, assuming that
    /// the connection is non-abutted.
    pub fn connect_non_abutted<T: ConvertibleToPortSlice>(&self, other: &T) {
        self.connect_generic(other, None, Abutment::NonAbutted);
    }

    pub fn connect_pipeline<T: ConvertibleToPortSlice>(&self, other: &T, pipeline: PipelineConfig) {
        self.connect_generic(other, Some(pipeline), Abutment::NA);
    }

    pub(crate) fn connect_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        pipeline: Option<PipelineConfig>,
        abutment: Abutment,
    ) {
        let other_as_slice = other.to_port_slice();

        if !Rc::ptr_eq(&self.get_mod_def_core(), &other_as_slice.get_mod_def_core()) {
            panic!(
                "Cannot connect {} and {} because they are in different module definitions",
                self.debug_string(),
                other_as_slice.debug_string()
            );
        }

        if self.width() != other_as_slice.width() {
            panic!(
                "Width mismatch when connecting {} and {}",
                self.debug_string(),
                other_as_slice.debug_string()
            );
        }

        let self_directionality = self.get_directionality();
        let other_directionality = other_as_slice.get_directionality();

        let generate_incompatibility_mesage = || {
            format!(
                "Cannot connect {} and {} because they have incompatible directions.",
                self.debug_string(),
                other_as_slice.debug_string()
            )
        };

        if !self_directionality.compatible_with(&other_directionality) {
            panic!("{}", generate_incompatibility_mesage());
        }

        if let Some(pipeline) = pipeline {
            let repeater_mod_def = pipeline.to_mod_def(self.width());

            let mod_def = self.get_mod_def();
            let repeater_inst_name = mod_def.resolve_pipeline_instance_name(&pipeline);

            // Figure out which port is the driver and which is the receiver, otherwise
            // panic. Note that InOut ports cannot be pipelined.
            let (driver, receiver) = match (self_directionality, other_directionality) {
                (PortDirectionality::Driver, PortDirectionality::Receiver) => {
                    (self, &other_as_slice)
                }
                (PortDirectionality::Receiver, PortDirectionality::Driver) => {
                    (&other_as_slice, self)
                }
                (PortDirectionality::InOut, _) => {
                    panic!("Cannot pipeline InOut port {}", self.debug_string())
                }
                (_, PortDirectionality::InOut) => panic!(
                    "Cannot pipeline InOut port {}",
                    other_as_slice.debug_string()
                ),
                _ => {
                    // This should be unreachable due to the previous compatible_with() check, but
                    // we keep descriptive error message to make debugging easier if it happens.
                    panic!("{}", generate_incompatibility_mesage());
                }
            };

            let repeater_instance =
                mod_def.instantiate(&repeater_mod_def, Some(&repeater_inst_name), None);
            let mod_def_clk = if mod_def.has_port(&pipeline.clk) {
                mod_def.get_port(&pipeline.clk)
            } else {
                mod_def.add_port(&pipeline.clk, IO::Input(1))
            };
            repeater_instance.get_port("clk").connect(&mod_def_clk);
            repeater_instance.get_port("in").connect(driver);
            repeater_instance.get_port("out").connect(receiver);
            repeater_instance.get_port("out_stages").unused();
        } else {
            self.port
                .get_port_connections_define_if_missing()
                .borrow_mut()
                .add(self.clone(), other_as_slice.clone(), abutment.clone());
            other_as_slice
                .port
                .get_port_connections_define_if_missing()
                .borrow_mut()
                .add(other_as_slice.clone(), self.clone(), abutment);
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
