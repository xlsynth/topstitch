// SPDX-License-Identifier: Apache-2.0

use crate::PortSlice;
use std::collections::HashSet;
use std::ops::Index;

use super::connected_item::ConnectedItem;

/// Describes a connection between a PortSlice and something else (another
/// PortSlice, a tieoff, etc.). Does not convey the directionality of the
/// connection, i.e. `this`` may drive `other`, `other` may drive `this`, or the
/// connection may be bidirectional.
#[derive(Clone, Debug, PartialEq)]
pub struct PortSliceConnection {
    pub(crate) this: PortSlice,
    pub(crate) other: ConnectedItem,
}

/// PortSliceConnection collection. This is used within ModDefCore to track
/// connections at the Port level.
#[derive(Clone, Debug, Default)]
pub struct PortSliceConnections {
    connections: Vec<PortSliceConnection>,
}

impl Index<usize> for PortSliceConnections {
    type Output = PortSliceConnection;

    fn index(&self, index: usize) -> &Self::Output {
        &self.connections[index]
    }
}

impl<'a> IntoIterator for &'a PortSliceConnections {
    type Item = &'a PortSliceConnection;
    type IntoIter = std::slice::Iter<'a, PortSliceConnection>;

    fn into_iter(self) -> Self::IntoIter {
        self.connections.iter()
    }
}

impl PortSliceConnections {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
        }
    }

    /// Adds a new connection between `this` and `other`
    pub fn add(&mut self, this: PortSlice, other: impl Into<ConnectedItem>) {
        self.connections.push(PortSliceConnection {
            this,
            other: other.into(),
        });
    }

    /// Returns a new `PortSliceConnections` that is a single-bit slice of this one.
    pub fn bit(&self, bit: usize) -> PortSliceConnections {
        self.slice(bit, bit)
    }

    /// Returns a new `PortSliceConnections` that is a slice of this one,
    /// meaning connections are clipped to the specified `msb` and `lsb`
    /// range.
    pub fn slice(&self, msb: usize, lsb: usize) -> PortSliceConnections {
        let mut result = PortSliceConnections::new();
        for connection in self {
            // Skip connection if there is no overlap with the requested range.
            if msb < connection.this.lsb || connection.this.msb < lsb {
                continue;
            }

            // Clip "this" side of the slice to the requested range.
            let this_lsb_clipped = lsb.max(connection.this.lsb);
            let this_msb_clipped = msb.min(connection.this.msb);

            let this_slice = connection
                .this
                .port
                .slice(this_msb_clipped, this_lsb_clipped);
            let other_sliced = connection.other.slice_with_offset_and_width(
                this_lsb_clipped - connection.this.lsb,
                this_msb_clipped - this_lsb_clipped + 1,
            );
            result.add(this_slice, other_sliced);
        }

        result
    }

    /// Returns a new `PortSliceConnections` that recursively traces all
    /// `PortSlice` connection arcs. In the result, `this` still refers to the
    /// same port as in `self`, effectively collapsing multi-hop connections in
    /// the connection graph.
    pub fn trace(&self) -> PortSliceConnections {
        let mut result = PortSliceConnections::new();
        for connection in self {
            result.extend(connection.trace());
        }
        result
    }

    /// Returns a new `PortSliceConnections` in which the starting points of the
    /// connection arcs are non-overlapping. This is important for resolving the
    /// expression source for a port slice for Verilog code generation, and may
    /// also be used for physical pin location derivation in the future.
    pub(crate) fn make_non_overlapping(&self) -> Vec<PortSliceConnections> {
        let mut result = Vec::new();

        if self.is_empty() {
            return result;
        }

        // Collect breakpoints
        let mut breakpoints = HashSet::new();
        for connection in self {
            breakpoints.insert(connection.this.msb + 1);
            breakpoints.insert(connection.this.lsb);
        }

        assert!(breakpoints.len() > 1);

        // Sort ascending
        let mut breakpoints: Vec<usize> = breakpoints.into_iter().collect();
        breakpoints.sort();

        for i in 0..breakpoints.len() - 1 {
            let msb = breakpoints[i + 1] - 1;
            let lsb = breakpoints[i];
            result.push(self.slice(msb, lsb));
        }

        result
    }

    /// Returns a vector of port slices that are part of this
    /// `PortSliceConnections` collection and have the specified width.
    pub(crate) fn keep_only_port_slices_with_width(&self, width: usize) -> Vec<PortSlice> {
        self.connections
            .iter()
            .filter_map(|connection| {
                if let ConnectedItem::PortSlice(port_slice) = &connection.other {
                    if port_slice.width() == width {
                        Some(port_slice.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    pub fn extend(&mut self, other: PortSliceConnections) {
        self.connections.extend(other.connections);
    }

    pub fn len(&self) -> usize {
        self.connections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
}

impl PortSliceConnection {
    /// Traces all directly or indirectly connected `PortSlice` reachable from
    /// this connection's `other` slice.
    pub fn trace(&self) -> PortSliceConnections {
        let mut visited = HashSet::new();
        visited.insert(self.this.clone());
        self.trace_helper(self.this.clone(), &mut visited)
    }

    /// Helper function for trace() that keeps track of the position with
    /// respect to the original port through recursive calls.
    fn trace_helper(
        &self,
        origin: PortSlice,
        visited: &mut HashSet<PortSlice>,
    ) -> PortSliceConnections {
        let mut result = PortSliceConnections::new();

        // When recursively tracing port connections, offsets are relative to
        // the original port. This is why "origin" is tracked and updated in
        // each recursive call.
        result.add(origin.clone(), self.other.clone());

        // Recursively trace PortSlice connections
        if let ConnectedItem::PortSlice(port_slice) = &self.other {
            if !visited.insert(port_slice.clone()) {
                panic!(
                    "Cycle detected in the connection graph involving {} and {}",
                    self.this.debug_string(),
                    port_slice.debug_string(),
                );
            }

            let port_connections = match port_slice.port.get_port_connections() {
                Some(port_connections) => port_connections,
                None => return result,
            };
            for next in &port_connections
                .borrow()
                .slice(port_slice.msb, port_slice.lsb)
            {
                if let ConnectedItem::PortSlice(next_other) = &next.other
                    && next_other == &self.this
                {
                    // don't trace backwards
                    continue;
                }

                let offset = next.this.lsb - port_slice.lsb;
                let width = next.this.msb - next.this.lsb + 1;

                let origin = origin.slice_with_offset_and_width(offset, width);

                result.extend(next.trace_helper(origin, visited));
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        IO, ModDef,
        connection::connected_item::{ConnectedItem, Tieoff, Unused, Wire},
        connection::port_slice::PortSliceConnections,
    };

    // Deterministic order for PortSliceConnections for the purpose of test
    // comparisons.
    fn sort_for_test(port_slice_connections: &mut PortSliceConnections) {
        port_slice_connections.connections.sort_by_key(|a| {
            (
                a.this.get_inst_name().unwrap_or_default(),
                a.this.port.name().to_string(),
                a.this.msb,
                a.this.lsb,
                format!("{:?}", a.other),
            )
        });
    }

    #[test]
    fn test_slice() {
        let m = ModDef::new("M");
        let a = m.add_port("a", IO::Output(12));
        let b = m.add_port("b", IO::Input(16));

        a.slice(7, 4).connect(&b.slice(15, 12));
        a.slice(3, 0).connect(&b.slice(7, 4));
        a.slice(11, 8).connect(&b.slice(3, 0));

        let mut overlaps = a.get_port_connections().unwrap().borrow().slice(5, 2);
        sort_for_test(&mut overlaps);

        assert_eq!(overlaps.len(), 2);
        assert_eq!(overlaps[0].this, a.slice(3, 2));
        assert_eq!(overlaps[0].other, b.slice(7, 6));
        assert_eq!(overlaps[1].this, a.slice(5, 4));
        assert_eq!(overlaps[1].other, b.slice(13, 12));

        let empty = a.get_port_connections().unwrap().borrow().slice(13, 12);
        assert_eq!(empty.len(), 0);

        let edge = a.get_port_connections().unwrap().borrow().slice(8, 8);
        assert_eq!(edge.len(), 1);
        assert_eq!(edge[0].this, a.bit(8));
        assert_eq!(edge[0].other, b.bit(0));
    }

    #[test]
    fn test_trace() {
        let a = ModDef::new("A");
        a.add_port("x", IO::InOut(8));
        let top = ModDef::new("Top");
        let y = top.add_port("y", IO::InOut(8));

        let a0 = top.instantiate(&a, Some("a0"), None);
        let a1 = top.instantiate(&a, Some("a1"), None);

        a0.get_port("x").connect(&y);
        y.connect(&a1.get_port("x"));

        let mut traced = a0
            .get_port("x")
            .get_port_connections()
            .unwrap()
            .borrow()
            .slice(5, 2)
            .trace();
        sort_for_test(&mut traced);

        assert_eq!(traced.len(), 2);
        assert_eq!(traced[0].this, a0.get_port("x").slice(5, 2));
        assert_eq!(traced[0].other, a1.get_port("x").slice(5, 2));
        assert_eq!(traced[1].this, a0.get_port("x").slice(5, 2));
        assert_eq!(traced[1].other, y.slice(5, 2));
    }

    #[test]
    fn test_trace_complex() {
        let a = ModDef::new("A");
        a.add_port("o", IO::Output(6));
        let b = ModDef::new("B");
        b.add_port("i", IO::Input(6));
        let c = ModDef::new("C");
        c.add_port("i", IO::Input(6));
        let d = ModDef::new("D");
        d.add_port("i", IO::Input(6));

        let top = ModDef::new("Top");
        let a_i = top.instantiate(&a, Some("A"), None);
        let b_i = top.instantiate(&b, Some("B"), None);
        let c_i = top.instantiate(&c, Some("C"), None);
        let d_i = top.instantiate(&d, Some("D"), None);

        a_i.get_port("o")
            .slice(4, 3)
            .connect(&b_i.get_port("i").slice(3, 2));
        a_i.get_port("o")
            .slice(4, 4)
            .connect(&c_i.get_port("i").slice(5, 5));
        a_i.get_port("o")
            .slice(3, 3)
            .connect(&d_i.get_port("i").slice(0, 0));

        let mut traced = b_i
            .get_port("i")
            .get_port_connections()
            .unwrap()
            .borrow()
            .slice(3, 2)
            .trace();
        sort_for_test(&mut traced);

        assert_eq!(traced.len(), 3);

        assert_eq!(traced[0].this, b_i.get_port("i").slice(2, 2));
        assert_eq!(traced[0].other, d_i.get_port("i").slice(0, 0));

        assert_eq!(traced[1].this, b_i.get_port("i").slice(3, 2));
        assert_eq!(traced[1].other, a_i.get_port("o").slice(4, 3));

        assert_eq!(traced[2].this, b_i.get_port("i").slice(3, 3));
        assert_eq!(traced[2].other, c_i.get_port("i").slice(5, 5));
    }

    #[test]
    fn test_tieoff_clipping_slice() {
        let m = ModDef::new("M");
        let p = m.add_port("p", IO::Input(8));
        p.tieoff(0xaau32);

        let overlaps = p.get_port_connections().unwrap().borrow().slice(6, 1);
        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].this, p.slice(6, 1));
        assert_eq!(
            overlaps[0].other,
            ConnectedItem::Tieoff(Tieoff::new(0x15u32, 6))
        );
    }

    #[test]
    #[should_panic]
    fn test_unused_not_allowed_on_output_to_resolve() {
        let m = ModDef::new("M");
        let q = m.add_port("q", IO::Output(4));
        q.unused();
        let segments = q
            .get_port_connections()
            .unwrap()
            .borrow()
            .make_non_overlapping();
        // resolve should panic for Output with Unused
        let _ = segments[0].to_expression_source();
    }

    #[test]
    fn test_tieoff_as_driver_input() {
        let a = ModDef::new("A");
        a.add_port("i", IO::Input(2));

        let top = ModDef::new("Top");
        let ai = top.instantiate(&a, Some("ai"), None);
        ai.get_port("i").tieoff(0b10u32);

        let segments = ai
            .get_port("i")
            .get_port_connections()
            .unwrap()
            .borrow()
            .make_non_overlapping();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0][0].this, ai.get_port("i").slice(1, 0));
        assert_eq!(
            segments[0].to_expression_source().unwrap().other,
            ConnectedItem::Tieoff(Tieoff::new(0b10u32, 2)),
        );
    }

    #[test]
    fn test_unused_inout_only_alone() {
        let m = ModDef::new("M");
        let y = m.add_port("y", IO::InOut(2));
        y.unused();
        let segments = y
            .get_port_connections()
            .unwrap()
            .borrow()
            .make_non_overlapping();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0][0].this, y.slice(1, 0));
        assert_eq!(
            segments[0].to_expression_source().unwrap().other,
            ConnectedItem::Unused(Unused::new())
        );
    }

    #[test]
    fn test_wire_specify_net_name() {
        let a_mod_def = ModDef::new("A");
        a_mod_def.add_port("a_io", IO::InOut(8));
        let b_mod_def = ModDef::new("B");
        b_mod_def.add_port("b_io", IO::InOut(8));
        let top = ModDef::new("TopModule");
        let a_inst = top.instantiate(&a_mod_def, None, None);
        let b_inst = top.instantiate(&b_mod_def, None, None);
        a_inst.get_port("a_io").connect(&b_inst.get_port("b_io"));
        a_inst.get_port("a_io").specify_net_name("custom");

        let segments = b_inst
            .get_port("b_io")
            .get_port_connections()
            .unwrap()
            .borrow()
            .trace()
            .make_non_overlapping();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0][0].this, b_inst.get_port("b_io").slice(7, 0));
        assert_eq!(
            segments[0].to_expression_source().unwrap().other,
            ConnectedItem::Wire(Wire {
                name: "custom".to_string(),
                width: 8,
                msb: 7,
                lsb: 0
            })
        );
    }

    #[test]
    fn test_tracing_in_detail() {
        let a = ModDef::new("A");
        a.add_port("o", IO::Output(4));
        let b = ModDef::new("B");
        b.add_port("i", IO::Input(3));

        let top = ModDef::new("Top");
        let a0 = top.instantiate(&a, Some("a0"), None);
        let b0 = top.instantiate(&b, Some("b0"), None);
        let b1 = top.instantiate(&b, Some("b1"), None);

        top.add_port("in", IO::Input(1));
        top.add_port("out", IO::Output(2));
        top.get_port("in").connect(&top.get_port("out").bit(1));
        a0.get_port("o").bit(3).connect(&top.get_port("out").bit(0));
        b0.get_port("i").connect(&a0.get_port("o").slice(3, 1));
        b1.get_port("i").connect(&a0.get_port("o").slice(2, 0));

        let expected = vec![
            (
                a0.get_port("o"),
                vec![
                    (
                        (0, 0),
                        vec![b1.get_port("i").bit(0)],
                        a0.get_port("o").bit(0),
                    ),
                    (
                        (2, 1),
                        vec![b0.get_port("i").slice(1, 0), b1.get_port("i").slice(2, 1)],
                        a0.get_port("o").slice(2, 1),
                    ),
                    (
                        (3, 3),
                        vec![b0.get_port("i").bit(2), top.get_port("out").bit(0)],
                        top.get_port("out").bit(0),
                    ),
                ],
            ),
            (
                b0.get_port("i"),
                vec![
                    (
                        (1, 0),
                        vec![a0.get_port("o").slice(2, 1), b1.get_port("i").slice(2, 1)],
                        a0.get_port("o").slice(2, 1),
                    ),
                    (
                        (2, 2),
                        vec![a0.get_port("o").bit(3), top.get_port("out").bit(0)],
                        top.get_port("out").bit(0),
                    ),
                ],
            ),
            (
                b1.get_port("i"),
                vec![
                    (
                        (0, 0),
                        vec![a0.get_port("o").bit(0)],
                        a0.get_port("o").bit(0),
                    ),
                    (
                        (2, 1),
                        vec![a0.get_port("o").slice(2, 1), b0.get_port("i").slice(1, 0)],
                        a0.get_port("o").slice(2, 1),
                    ),
                ],
            ),
            (
                top.get_port("out"),
                vec![
                    (
                        (0, 0),
                        vec![a0.get_port("o").bit(3), b0.get_port("i").bit(2)],
                        top.get_port("out").bit(0),
                    ),
                    (
                        (1, 1),
                        vec![top.get_port("in").bit(0)],
                        top.get_port("in").bit(0),
                    ),
                ],
            ),
        ];

        for (port, segments) in expected {
            let mut non_overlapping = port
                .get_port_connections()
                .unwrap()
                .borrow()
                .trace()
                .make_non_overlapping();
            assert_eq!(non_overlapping.len(), segments.len());
            for (expected, actual_connections) in segments.iter().zip(non_overlapping.iter_mut()) {
                let ((expected_msb, expected_lsb), expected_connections, expected_name_source) =
                    expected;
                assert_eq!(actual_connections.len(), expected_connections.len());
                assert_eq!(
                    actual_connections.to_expression_source().unwrap().other,
                    *expected_name_source
                );
                sort_for_test(actual_connections);
                for (actual_connection, expected_connection) in actual_connections
                    .into_iter()
                    .zip(expected_connections.iter())
                {
                    assert_eq!(
                        actual_connection.this,
                        port.slice(*expected_msb as usize, *expected_lsb as usize)
                    );
                    assert_eq!(actual_connection.other, *expected_connection);
                }
            }
        }
    }
}
