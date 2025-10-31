// SPDX-License-Identifier: Apache-2.0

use crate::{Port, PortSlice, Usage};
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
        let width = self.width();

        let mut hierarchy = match &self.port {
            Port::ModDef { .. } => vec![],
            Port::ModInst { hierarchy, .. } => hierarchy.clone(),
        };

        let mut current = self.clone();
        let mut visited = HashSet::new();

        for _ in 0..MAX_ITERATIONS {
            if !visited.insert(current.clone()) {
                panic!(
                    "Failed to trace {} due to an infinite loop",
                    current.debug_string()
                );
            }

            let connections = current.get_port_connections()?;

            // TODO(sherbst) 2025-10-30: Merge smaller connections
            let connections_filtered = connections.keep_only_port_slices_with_width(width);

            // TODO(sherbst) 2025-10-30: Trace full fan-out of connections?
            let connection = match connections_filtered.len() {
                0 => return None,
                1 => connections_filtered.first().unwrap(),
                _ => panic!("Failed to trace {} due to fanout", self.debug_string(),),
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

        panic!(
            "Failed to trace {} after {} iterations",
            self.debug_string(),
            MAX_ITERATIONS
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConvertibleToPortSlice, ModDef, IO};

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
