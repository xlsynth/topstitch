// SPDX-License-Identifier: Apache-2.0

use crate::mod_inst::HierPathElem;
use crate::pipeline::PipelinePreflight;
use crate::port::PortDirectionality;
use crate::{
    ConvertibleToPortSlice, ConvertibleToPortSliceVec, IO, ModInst, PipelineConfig, Port, PortDecl,
    PortSlice,
};
use parking_lot::RwLock;
use std::sync::Weak;

/// A definition pin can participate in any occurrence of its own definition's wiring.
#[derive(Clone, Copy)]
struct WiringScope<'a> {
    module: &'a Weak<RwLock<crate::ModDefCore>>,
    occurrence: Option<&'a [HierPathElem]>,
}

impl<'a> WiringScope<'a> {
    /// Uses the pin's owning definition and optional enclosing occurrence.
    fn port(port: &'a Port) -> Self {
        match port {
            Port::ModDef { mod_def_core, .. } => Self {
                module: mod_def_core,
                occurrence: None,
            },
            Port::ModInst { hierarchy, .. } => Self::instance(hierarchy),
        }
    }

    /// Returns the scope in which the instance's pins can be wired.
    fn instance(path: &'a [HierPathElem]) -> Self {
        let (instance, occurrence) = path.split_last().unwrap();
        Self {
            module: &instance.mod_def_core,
            occurrence: Some(occurrence),
        }
    }

    /// Definition-relative pins match any occurrence of their owning definition.
    fn matches(self, other: Self) -> bool {
        self.module.ptr_eq(other.module)
            && self
                .occurrence
                .zip(other.occurrence)
                .is_none_or(|(a, b)| a == b)
    }
}

impl PortSlice {
    /// Specifies the net name to be used for this port slice.
    /// Where a slice connects to an inout on the containing module, its drivers share the
    /// inout itself and the named wire carries its resolved value to readers.
    pub fn specify_net_name(&self, net: &str) {
        if let Port::ModInst { .. } = &self.port {
            // Record and enforce uniqueness of explicitly specified net names
            // within the containing ModDef.
            {
                let core_rc = self.get_mod_def_core();
                let mut core = core_rc.write();
                core.assert_wiring_mutable();
                if !core.specified_net_names.insert(net.to_string()) {
                    panic!(
                        "Net \"{}\" has already been manually specified in module {}.",
                        net, core.name
                    );
                };
            }

            let this = self.relative_to_parent();
            let width = this.port.io().width();
            let other = crate::connection::connected_item::Wire {
                name: net.to_string(),
                width,
                msb: self.msb,
                lsb: self.lsb,
            };
            self.port
                .get_port_connections_define_if_missing()
                .write()
                .add(this, other);
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
        self.connect_generic(other, None);
    }

    pub fn connect_pipeline<T: ConvertibleToPortSlice>(&self, other: &T, pipeline: PipelineConfig) {
        self.connect_generic(other, Some(pipeline));
    }

    /// Connects this `PortSlice` to `other`, consuming both sides from LSB to
    /// MSB as an ordered stream of bits. Any unconsumed remainder on either
    /// side is marked with `unused_or_tieoff(0)`.
    pub fn todo_jam_connect<T: ConvertibleToPortSliceVec>(&self, other: &T) {
        let left = self.to_port_slice_vec();
        let right = other.to_port_slice_vec();
        Self::todo_jam_connect_port_slices(&left, &right);
    }

    pub(crate) fn todo_jam_connect_port_slices(left: &[PortSlice], right: &[PortSlice]) {
        for pin in left.iter().chain(right) {
            pin.get_mod_def_core().read().assert_wiring_mutable();
        }
        let mut left_idx = 0usize;
        let mut right_idx = 0usize;
        let mut left_lsb = 0usize;
        let mut right_lsb = 0usize;

        while left_idx < left.len() && right_idx < right.len() {
            let left_width = left[left_idx].width();
            let right_width = right[right_idx].width();

            if left_lsb == left_width {
                left_idx += 1;
                left_lsb = 0;
                continue;
            }

            if right_lsb == right_width {
                right_idx += 1;
                right_lsb = 0;
                continue;
            }

            let chunk_width = usize::min(left_width - left_lsb, right_width - right_lsb);

            left[left_idx]
                .slice_with_offset_and_width(left_lsb, chunk_width)
                .connect(&right[right_idx].slice_with_offset_and_width(right_lsb, chunk_width));

            left_lsb += chunk_width;
            right_lsb += chunk_width;

            if left_lsb == left_width {
                left_idx += 1;
                left_lsb = 0;
            }

            if right_lsb == right_width {
                right_idx += 1;
                right_lsb = 0;
            }
        }

        mark_jam_remainder(left, left_idx, left_lsb);
        mark_jam_remainder(right, right_idx, right_lsb);
    }

    /// Checks the scope, widths and directions before changing either endpoint.
    pub(crate) fn check_connection(&self, other: &PortSlice, pipelined: bool) {
        assert!(
            WiringScope::port(&self.port).matches(WiringScope::port(&other.port)),
            "Cannot connect {} and {} because they are in different module definitions",
            self.debug_string(),
            other.debug_string()
        );
        assert!(
            self.width() == other.width(),
            "Width mismatch when connecting {} and {}",
            self.debug_string(),
            other.debug_string()
        );
        let lhs = self.get_directionality();
        let rhs = other.get_directionality();
        assert!(
            lhs.compatible_with(&rhs),
            "Cannot connect {} and {} because they have incompatible directions.",
            self.debug_string(),
            other.debug_string()
        );
        if pipelined {
            for (pin, direction) in [(self, lhs), (other, rhs)] {
                assert!(
                    !matches!(direction, PortDirectionality::InOut),
                    "Cannot pipeline InOut port {}",
                    pin.debug_string()
                );
            }
        }
    }

    /// Checks the route's siblings and external connections before their feedthrough pins exist.
    pub(crate) fn check_route(
        &self,
        other: Option<&PortSlice>,
        through: &[(&ModInst, Option<PipelineConfig>)],
    ) {
        let first = WiringScope::instance(&through[0].0.hierarchy);
        for (inst, _) in through {
            assert!(
                first.matches(WiringScope::instance(&inst.hierarchy)),
                "Feedthrough instances must be in the same occurrence"
            );
        }
        assert!(
            first.matches(WiringScope::port(&self.port)),
            "Feedthrough instance and endpoint must be in the same occurrence"
        );
        // The flipped instance pin accepts an instance-side signal of the original IO.
        let flipped_direction = match self.port.io() {
            IO::Input(_) => PortDirectionality::Driver,
            IO::Output(_) => PortDirectionality::Receiver,
            IO::InOut(_) => PortDirectionality::InOut,
        };
        assert!(
            self.get_directionality()
                .compatible_with(&flipped_direction),
            "Cannot connect {} to the flipped feedthrough pin because they have incompatible directions.",
            self.debug_string()
        );
        if let Some(other) = other {
            assert!(
                first.matches(WiringScope::port(&other.port)),
                "Feedthrough instance and endpoint must be in the same occurrence"
            );
            self.check_connection(other, false);
        }
    }

    pub(crate) fn connect_generic<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        pipeline: Option<PipelineConfig>,
    ) {
        let other_as_slice = other.to_port_slice();
        self.check_connection(
            &other_as_slice,
            pipeline.as_ref().is_some_and(|p| p.depth > 0),
        );
        let this = self.relative_to_parent();
        let other = other_as_slice.relative_to_parent();

        if let Some(pipeline) = pipeline.filter(|p| p.depth > 0) {
            let mod_def = self.get_mod_def();
            let (driver, receiver) =
                if matches!(self.get_directionality(), PortDirectionality::Driver) {
                    (&this, &other)
                } else {
                    (&other, &this)
                };
            let repeater_instance = mod_def.instantiate_pipeline(&pipeline, self.width());
            let mod_def_clk = mod_def.get_port(&pipeline.clk);
            repeater_instance.get_port("clk").connect(&mod_def_clk);
            repeater_instance.get_port("in").connect(driver);
            repeater_instance.get_port("out").connect(receiver);
        } else {
            this.port
                .get_port_connections_define_if_missing()
                .write()
                .add(this.clone(), other.clone());
            other
                .port
                .get_port_connections_define_if_missing()
                .write()
                .add(other, this);
            Self::invalidate_trace_drivers();
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
        self.connect_through_with_decl(other, through, prefix, self.bits_decl());
    }

    /// Uses the source declaration for both ports created at each hop.
    pub(crate) fn connect_through_with_decl<T: ConvertibleToPortSlice>(
        &self,
        other: &T,
        through: &[(&ModInst, Option<PipelineConfig>)],
        prefix: impl AsRef<str>,
        decl: PortDecl,
    ) {
        if through.is_empty() {
            self.connect(other);
            return;
        }

        ModInst::check_feedthrough_wiring(
            [
                self.get_mod_def_core(),
                other.to_port_slice().get_mod_def_core(),
            ],
            through,
        );
        let flipped = format!("{}_flipped", prefix.as_ref());
        let original = format!("{}_original", prefix.as_ref());

        if through.iter().any(|(_, pipeline)| pipeline.is_some()) {
            self.check_route(Some(&other.to_port_slice()), through);
            let mut plan = PipelinePreflight::default();
            for (inst, pipeline) in through {
                plan.feedthrough(
                    &inst.get_mod_def(),
                    &flipped,
                    &original,
                    self.port.io().with_width(self.width()),
                    pipeline.as_ref(),
                );
            }
        }
        for (i, (inst, pipeline)) in through.iter().enumerate() {
            let (flipped_port, original_port) =
                decl.clone()
                    .feedthrough(*inst, &flipped, &original, pipeline.as_ref().cloned());

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

fn mark_jam_remainder(port_slices: &[PortSlice], mut idx: usize, lsb: usize) {
    if idx >= port_slices.len() {
        return;
    }

    if lsb > 0 {
        let width = port_slices[idx].width();
        port_slices[idx]
            .slice_with_offset_and_width(lsb, width - lsb)
            .unused_or_tieoff(0);
        idx += 1;
    }

    for port_slice in port_slices.iter().skip(idx) {
        port_slice.unused_or_tieoff(0);
    }
}
