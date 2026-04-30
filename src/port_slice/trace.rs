// SPDX-License-Identifier: Apache-2.0

use crate::{Mat3, PhysicalPin, Port, PortSlice, Usage};
use std::collections::HashSet;

const MAX_ITERATIONS: usize = 1000;

impl PortSlice {
    /// Traces through the module hierarchy to find out what is connected to
    /// this `PortSlice``, if anything. For example, if `top` instantiates `a`
    /// as `a_inst` and `b` as `b_inst`, and `a_inst.x\[7:0\]` is connected to
    /// `b_inst.y\[7:0\]`, calling this function on `top.a_inst.x\[7:0\]` will
    /// return `Some(top.b_inst.y\[7:0\])`.
    ///
    /// Tracing stops at modules that have not been marked as
    /// `Usage::EmitDefinitionAndDescend`.
    ///
    /// This function does not currently support the following situations:
    /// (1) The `PortSlice`` fans out to multiple places
    /// (2) Different parts of the `PortSlice` connect to different places
    /// (3) This `PortSlice` is connected to another `PortSlice` using multiple
    ///     `connect()` operations.
    pub fn trace_through_hierarchy(&self) -> Option<PortSlice> {
        self.trace_through_hierarchy_impl(false)
    }

    fn trace_through_hierarchy_impl(&self, return_none_on_error: bool) -> Option<PortSlice> {
        let width = self.width();

        let mut hierarchy = match &self.port {
            Port::ModDef { .. } => vec![],
            Port::ModInst { hierarchy, .. } => hierarchy.clone(),
        };

        let mut current = self.clone();
        let mut visited = HashSet::new();

        for _ in 0..MAX_ITERATIONS {
            if !visited.insert(current.clone()) {
                if return_none_on_error {
                    return None;
                } else {
                    panic!(
                        "Failed to trace {} due to an infinite loop",
                        current.debug_string()
                    );
                }
            }

            let connections = current.get_port_connections()?;

            // TODO(sherbst) 2025-10-30: Merge smaller connections
            let connections_filtered = connections.keep_only_port_slices_with_width(width);

            // TODO(sherbst) 2025-10-30: Trace full fan-out of connections?
            let connection = match connections_filtered.len() {
                0 => return None,
                1 => connections_filtered.first().unwrap(),
                _ => {
                    if return_none_on_error {
                        return None;
                    } else {
                        panic!("Failed to trace {} due to fanout", self.debug_string());
                    }
                }
            };

            if matches!(current.port, Port::ModInst { .. }) {
                hierarchy.pop();
            }

            current = match &connection.port {
                Port::ModDef { .. } => {
                    if hierarchy.is_empty() {
                        return Some(connection.clone());
                    } else {
                        connection.as_mod_inst_port_slice(hierarchy.clone())
                    }
                }
                Port::ModInst {
                    hierarchy: connection_hierarchy,
                    ..
                } => {
                    let last = connection_hierarchy.last().unwrap();
                    hierarchy.push(last.clone());

                    let usage = connection.get_mod_def_where_declared().get_usage();
                    match usage {
                        Usage::EmitDefinitionAndDescend => connection.as_mod_def_port_slice(),
                        _ => {
                            // TODO(sherbst) 2025-10-30: refine mechanism for finding stopping
                            // points?
                            return Some(connection.as_mod_inst_port_slice(hierarchy.clone()));
                        }
                    }
                }
            };
        }

        if return_none_on_error {
            None
        } else {
            panic!(
                "Failed to trace {} after {} iterations",
                self.debug_string(),
                MAX_ITERATIONS
            );
        }
    }

    /// Returns the Manhattan gap between this ModInst port slice's physical
    /// pin and the physical pin it traces to.
    ///
    /// This currently supports only single-bit ModInst port slices. Wider port
    /// slices may be supported in the future, but for now this panics if called
    /// on a multi-bit slice.
    ///
    /// Returns `None` when the bit is tied off, unconnected, has multi-fanout,
    /// hits another trace error, traces to a top-level port, or either side
    /// does not have a physical pin.
    pub fn get_connection_distance(&self) -> Option<i64> {
        self.get_connected_port_slice_and_distance()
            .map(|(_, distance)| distance)
    }

    /// Returns the ModInst port slice connected to this ModInst port slice,
    /// along with the Manhattan gap between their physical pins.
    ///
    /// This currently supports only single-bit ModInst port slices. Wider port
    /// slices may be supported in the future, but for now this panics if called
    /// on a multi-bit slice.
    ///
    /// Returns `None` when the bit is tied off, unconnected, has multi-fanout,
    /// hits another trace error, traces to a top-level port, or either side
    /// does not have a physical pin.
    pub fn get_connected_port_slice_and_distance(&self) -> Option<(PortSlice, i64)> {
        let Some(self_mod_inst) = self.port.get_mod_inst() else {
            panic!(
                "get_connected_port_slice_and_distance only works on ports (or slices of ports) on module instances"
            );
        };

        let self_transform = self_mod_inst.get_transform();

        // TODO(sherbst) 2026-04-30: Support multi-bit port slices by returning a
        // matching-width connected PortSlice and an aggregate distance, such as
        // the maximum bit distance. Decide how to represent cases without a
        // single connected PortSlice covering the full width; one option is to
        // return the PortSlice associated with the longest-distance bit.
        let self_physical_pin = self.get_local_physical_pin()?;
        self.get_connected_bit_and_distance_with_self_transform(&self_transform, &self_physical_pin)
    }

    /// Crate-internal fast path for callers that already know this slice's
    /// instance transform and physical pin, such as per-bit validation loops.
    /// This avoids recomputing `ModInst::get_transform()` and re-reading the
    /// physical pin map for every bit.
    pub(crate) fn get_connected_bit_and_distance_with_self_transform(
        &self,
        self_transform: &Mat3,
        self_physical_pin: &PhysicalPin,
    ) -> Option<(PortSlice, i64)> {
        self.check_validity();
        assert!(
            matches!(self.port, Port::ModInst { .. }),
            "get_connected_bit_and_distance_with_self_transform requires a port slice on a module instance"
        );

        let other = self.trace_through_hierarchy_impl(true)?;
        let other_mod_inst = other.port.get_mod_inst()?;
        let other_transform = other_mod_inst.get_transform();
        let other_physical_pin = other.get_local_physical_pin()?;

        let distance = physical_pin_distance(
            self_transform,
            self_physical_pin,
            &other_transform,
            &other_physical_pin,
        );

        Some((other, distance))
    }

    fn get_local_physical_pin(&self) -> Option<PhysicalPin> {
        assert!(
            self.width() == 1,
            "physical pin lookup currently only supports single-bit port slices (called on {})",
            self.debug_string()
        );

        self.port
            .get_mod_def_core_where_declared()
            .read()
            .physical_pins
            .get(self.port.name())
            .and_then(|pins| pins.get(self.lsb))
            .and_then(|pin| pin.as_ref())
            .cloned()
    }

    /// Returns `true` if any part of this port slice ultimately traces to a tieoff.
    /// Similar to trace_through_hierarchy above, this does not support different
    /// parts of the `PortSlice` connecting to different places
    pub fn has_tieoff_connection(&self) -> bool {
        // TODO(jowright) 2026-03-25: Make this work with divergence or return a Result
        self.get_port_connections()
            .is_some_and(|connections| !connections.trace().to_tieoffs().is_empty())
    }
}

fn physical_pin_distance(
    a_transform: &Mat3,
    a_physical_pin: &PhysicalPin,
    b_transform: &Mat3,
    b_physical_pin: &PhysicalPin,
) -> i64 {
    let a_bbox = a_physical_pin
        .transformed_polygon()
        .apply_transform(a_transform)
        .bbox();
    let b_bbox = b_physical_pin
        .transformed_polygon()
        .apply_transform(b_transform)
        .bbox();

    a_bbox.gap(&b_bbox)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConvertibleToPortSlice, IO, ModDef};

    fn check_trace_bidir(a: &impl ConvertibleToPortSlice, b: &impl ConvertibleToPortSlice) {
        assert_eq!(
            a.to_port_slice().trace_through_hierarchy().unwrap(),
            b.to_port_slice()
        );
        assert_eq!(
            b.to_port_slice().trace_through_hierarchy().unwrap(),
            a.to_port_slice()
        );
    }

    #[test]
    fn test_trace_inst_to_inst() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let b = ModDef::new("B");
        b.set_usage(Usage::EmitNothingAndStop);
        b.add_port("y", IO::Input(8));

        let top = ModDef::new("Top");

        let a_inst = top.instantiate(&a, Some("a_inst"), None);
        let b_inst = top.instantiate(&b, Some("b_inst"), None);
        a_inst.get_port("x").connect(&b_inst.get_port("y"));

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &b_inst.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_has_tieoff_connection() {
        let top = ModDef::new("Top");
        let a = top.add_port("a", IO::Output(4));
        let b = top.add_port("b", IO::Input(2));

        a.slice(1, 0).tieoff(0b10u32);
        a.slice(3, 2).connect(&b);

        assert!(a.has_tieoff_connection());
        assert!(a.slice(1, 0).has_tieoff_connection());
        assert!(!a.slice(3, 2).has_tieoff_connection());
        assert!(!b.has_tieoff_connection());
    }

    #[test]
    fn test_trace_inst_to_def() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let top = ModDef::new("Top");

        let a_inst = top.instantiate(&a, Some("a_inst"), None);
        a_inst.get_port("x").export_as("y");

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &top.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_def_to_def() {
        let top = ModDef::new("Top");
        top.feedthrough("x", "y", 8);

        check_trace_bidir(
            &top.get_port("x").slice(3, 0),
            &top.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_inst_up_two_levels() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let inner = ModDef::new("Inner");
        inner
            .instantiate(&a, Some("a_inst"), None)
            .get_port("x")
            .export_as("y");

        let top = ModDef::new("Top");
        top.instantiate(&inner, Some("inner_inst"), None)
            .get_port("y")
            .export_as("z");

        let a_inst = top.get_instance("inner_inst").get_instance("a_inst");

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &top.get_port("z").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_inst_to_inst_through_inner() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);
        let aa = a.wrap(Some("AA"), Some("a_inst"));

        let b = ModDef::new("B");
        b.add_port("y", IO::Input(8));
        b.set_usage(Usage::EmitNothingAndStop);
        let bb = b.wrap(Some("BB"), Some("b_inst"));

        let top = ModDef::new("Top");
        let aa_inst = top.instantiate(&aa, Some("aa_inst"), None);
        let bb_inst = top.instantiate(&bb, Some("bb_inst"), None);
        aa_inst.get_port("x").connect(&bb_inst.get_port("y"));

        let a_inst = top.get_instance("aa_inst").get_instance("a_inst");
        let b_inst = top.get_instance("bb_inst").get_instance("b_inst");

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &b_inst.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_inst_to_inst_feedthrough() {
        let a = ModDef::new("A");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let b = ModDef::new("B");
        b.feedthrough("ft_in", "ft_out", 8);

        let c = ModDef::new("C");
        c.add_port("y", IO::Input(8));
        c.set_usage(Usage::EmitNothingAndStop);

        let top = ModDef::new("Top");
        let a_inst = top.instantiate(&a, Some("a_inst"), None);
        let b_inst = top.instantiate(&b, Some("b_inst"), None);
        let c_inst = top.instantiate(&c, Some("c_inst"), None);
        a_inst.get_port("x").connect(&b_inst.get_port("ft_in"));
        b_inst.get_port("ft_out").connect(&c_inst.get_port("y"));

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &c_inst.get_port("y").slice(3, 0),
        );
    }

    #[test]
    fn test_trace_def_to_def_feedthrough() {
        let top = ModDef::new("top");
        top.add_port("x", IO::Input(8));
        top.add_port("y", IO::Output(8));
        top.set_usage(Usage::EmitNothingAndStop);

        let ft = ModDef::new("ft");
        ft.feedthrough("ft_in", "ft_out", 8);

        let ft_inst = top.instantiate(&ft, Some("ft_inst"), None);
        top.get_port("x").connect(&ft_inst.get_port("ft_in"));
        ft_inst.get_port("ft_out").connect(&top.get_port("y"));

        check_trace_bidir(
            &top.get_port("x").slice(3, 0),
            &top.get_port("y").slice(3, 0),
        );
    }

    #[test]
    #[should_panic(expected = "Failed to trace top.a_inst.x[3:0] due to fanout")]
    fn test_trace_fanout() {
        let a = ModDef::new("a");
        a.add_port("x", IO::Output(8));
        a.set_usage(Usage::EmitNothingAndStop);

        let b = ModDef::new("b");
        b.add_port("y", IO::Input(8));
        b.set_usage(Usage::EmitNothingAndStop);

        let c = ModDef::new("c");
        c.add_port("z", IO::Input(8));
        c.set_usage(Usage::EmitNothingAndStop);

        let top = ModDef::new("top");
        let a_inst = top.instantiate(&a, Some("a_inst"), None);
        let b_inst = top.instantiate(&b, Some("b_inst"), None);
        let c_inst = top.instantiate(&c, Some("c_inst"), None);
        a_inst.get_port("x").connect(&b_inst.get_port("y"));
        a_inst.get_port("x").connect(&c_inst.get_port("z"));

        check_trace_bidir(
            &a_inst.get_port("x").slice(3, 0),
            &c_inst.get_port("z").slice(3, 0),
        );
    }

    #[test]
    #[should_panic(expected = "Failed to trace a.x[7:0] due to an infinite loop")]
    fn test_trace_infinite_loop() {
        let a = ModDef::new("a");
        a.add_port("x", IO::Output(8));

        let b = a.wrap(Some("b"), Some("a_inst"));

        let b_inst = a.instantiate(&b, Some("b_inst"), None);
        b_inst.get_port("x").connect(&a.get_port("x"));

        a.get_port("x").trace_through_hierarchy();
    }
}
