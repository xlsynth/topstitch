// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use regex::Regex;

use crate::util::concat_captures;
use crate::{ConvertibleToPortSlice, Intf, PortSlice, IO};

pub struct Funnel {
    a_in: PortSlice,
    a_out: PortSlice,
    b_in: PortSlice,
    b_out: PortSlice,
    a_in_offset: usize,
    a_out_offset: usize,
}

impl Funnel {
    pub fn new(
        a: (impl ConvertibleToPortSlice, impl ConvertibleToPortSlice),
        b: (impl ConvertibleToPortSlice, impl ConvertibleToPortSlice),
    ) -> Self {
        let a0 = a.0.to_port_slice();
        let a1 = a.1.to_port_slice();

        let (a_in, a_out) = match (a0.port.io(), a1.port.io()) {
            (IO::Input(_), IO::Output(_)) => (a0, a1),
            (IO::Output(_), IO::Input(_)) => (a1, a0),
            (IO::Input(_), IO::Input(_)) => panic!(
                "Funnel error: Side A cannot have both ports as inputs ({} and {})",
                a0.debug_string(),
                a1.debug_string()
            ),
            (IO::Output(_), IO::Output(_)) => panic!(
                "Funnel error: Side A cannot have both ports as outputs ({} and {})",
                a0.debug_string(),
                a1.debug_string()
            ),
            (IO::InOut(_), _) => panic!(
                "Funnel error: Side A cannot have inout ports ({})",
                a0.debug_string()
            ),
            (_, IO::InOut(_)) => panic!(
                "Funnel error: Side A cannot have inout ports ({})",
                a1.debug_string()
            ),
        };

        let b0 = b.0.to_port_slice();
        let b1 = b.1.to_port_slice();

        let (b_in, b_out) = match (b0.port.io(), b1.port.io()) {
            (IO::Input(_), IO::Output(_)) => (b0, b1),
            (IO::Output(_), IO::Input(_)) => (b1, b0),
            (IO::Input(_), IO::Input(_)) => panic!(
                "Funnel error: Side B cannot have both ports as inputs ({}, {})",
                b0.debug_string(),
                b1.debug_string()
            ),
            (IO::Output(_), IO::Output(_)) => panic!(
                "Funnel error: Side B cannot have both ports as outputs ({}, {})",
                b0.debug_string(),
                b1.debug_string()
            ),
            (IO::InOut(_), _) => panic!(
                "Funnel error: Side B cannot have inout ports ({})",
                b0.debug_string()
            ),
            (_, IO::InOut(_)) => panic!(
                "Funnel error: Side B cannot have inout ports ({})",
                b1.debug_string()
            ),
        };

        assert!(
            a_in.width() == b_out.width(),
            "Funnel error: Side A input and side B output must have the same width ({}, {})",
            a_in.debug_string(),
            b_out.debug_string()
        );
        assert!(
            a_out.width() == b_in.width(),
            "Funnel error: Side A output and side B input must have the same width ({}, {})",
            a_out.debug_string(),
            b_in.debug_string()
        );

        Self {
            a_in,
            a_out,
            b_in,
            b_out,
            a_in_offset: 0,
            a_out_offset: 0,
        }
    }

    pub fn connect(&mut self, a: &impl ConvertibleToPortSlice, b: &impl ConvertibleToPortSlice) {
        let a = a.to_port_slice();
        let b = b.to_port_slice();

        assert!(
            a.width() == b.width(),
            "Funnel error: a and b must have the same width ({}, {})",
            a.debug_string(),
            b.debug_string()
        );

        if a.port.is_driver() {
            if b.port.is_driver() {
                panic!(
                    "Funnel error: Cannot connect two outputs together ({}, {})",
                    a.debug_string(),
                    b.debug_string()
                );
            } else {
                assert!(
                    self.a_in_offset + a.width() <= self.a_in.width(),
                    "Funnel error: out of capacity when trying to connect {} -> {} via {} -> {}. Would need {} extra bit(s).",
                    a.debug_string(),
                    b.debug_string(),
                    self.a_in.debug_string(),
                    self.b_out.debug_string(),
                    self.a_in_offset + a.width() - self.a_in.width()
                );
                self.a_in
                    .slice_with_offset_and_width(self.a_in_offset, a.width())
                    .connect(&a);
                self.b_out
                    .slice_with_offset_and_width(self.a_in_offset, b.width())
                    .connect(&b);
                self.a_in_offset += a.width();
            }
        } else if b.port.is_driver() {
            assert!(
                self.a_out_offset + a.width() <= self.a_out.width(),
                "Funnel error: out of capacity when trying to connect {} -> {} via {} -> {}. Would need {} extra bit(s).",
                b.debug_string(),
                a.debug_string(),
                self.b_in.debug_string(),
                self.a_out.debug_string(),
                self.a_out_offset + a.width() - self.a_out.width()
            );
            self.a_out
                .slice_with_offset_and_width(self.a_out_offset, a.width())
                .connect(&a);
            self.b_in
                .slice_with_offset_and_width(self.a_out_offset, b.width())
                .connect(&b);
            self.a_out_offset += a.width();
        } else {
            panic!(
                "Funnel error: Cannot connect two inputs together ({}, {})",
                a.debug_string(),
                b.debug_string()
            );
        }
    }

    pub fn connect_intf(&mut self, a: &Intf, b: &Intf, allow_mismatch: bool) {
        let a_ports = a.get_port_slices();
        let b_ports = b.get_port_slices();

        for (a_func_name, a_port) in &a_ports {
            if let Some(b_port) = b_ports.get(a_func_name) {
                self.connect(a_port, b_port);
            } else if !allow_mismatch {
                panic!("Funnel error: interfaces {} and {} have mismatched functions and allow_mismatch is false. Example: function '{}' is present in {} but not in {}",
                    a.debug_string(),
                    b.debug_string(),
                    a_func_name,
                    a.debug_string(),
                    b.debug_string()
                );
            }
        }

        if !allow_mismatch {
            for (func_name, _) in &b_ports {
                if !a_ports.contains_key(func_name) {
                    panic!(
                        "Interfaces {} and {} have mismatched functions and allow_mismatch is false. Example: function '{}' is present in {} but not in {}",
                        a.debug_string(),
                        b.debug_string(),
                        func_name,
                        b.debug_string(),
                        a.debug_string()
                    );
                }
            }
        }
    }

    pub fn crossover_intf(
        &mut self,
        x: &Intf,
        y: &Intf,
        pattern_a: impl AsRef<str>,
        pattern_b: impl AsRef<str>,
    ) {
        let pattern_a_regex = Regex::new(pattern_a.as_ref()).unwrap();
        let pattern_b_regex = Regex::new(pattern_b.as_ref()).unwrap();

        let mut x_a_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut x_b_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut y_a_matches: IndexMap<String, PortSlice> = IndexMap::new();
        let mut y_b_matches: IndexMap<String, PortSlice> = IndexMap::new();

        const CONCAT_SEP: &str = "_";

        for (x_func_name, x_port_slice) in x.get_port_slices() {
            if let Some(captures) = pattern_a_regex.captures(&x_func_name) {
                x_a_matches.insert(concat_captures(&captures, CONCAT_SEP), x_port_slice);
            } else if let Some(captures) = pattern_b_regex.captures(&x_func_name) {
                x_b_matches.insert(concat_captures(&captures, CONCAT_SEP), x_port_slice);
            }
        }

        for (y_func_name, y_port_slice) in y.get_port_slices() {
            if let Some(captures) = pattern_a_regex.captures(&y_func_name) {
                y_a_matches.insert(concat_captures(&captures, CONCAT_SEP), y_port_slice);
            } else if let Some(captures) = pattern_b_regex.captures(&y_func_name) {
                y_b_matches.insert(concat_captures(&captures, CONCAT_SEP), y_port_slice);
            }
        }

        for (x_func_name, x_port_slice) in x_a_matches {
            if let Some(y_port_slice) = y_b_matches.get(&x_func_name) {
                self.connect(&x_port_slice, y_port_slice);
            }
        }

        for (x_func_name, x_port_slice) in x_b_matches {
            if let Some(y_port_slice) = y_a_matches.get(&x_func_name) {
                self.connect(&x_port_slice, y_port_slice);
            }
        }
    }

    pub fn tieoff_remaining(&mut self) {
        if let Some((a_in_slice, b_out_slice)) = self.a2b_yield_remaining() {
            a_in_slice.tieoff(0);
            b_out_slice.unused();
        }

        if let Some((b_in_slice, a_out_slice)) = self.b2a_yield_remaining() {
            b_in_slice.tieoff(0);
            a_out_slice.unused();
        }
    }

    /// Returns port slices for the remaining bits in the a -> b channel.
    /// The first slice is the "a" input port slice, and the second slice is the
    /// "b" output port slice. If there are no remaining bits, returns None.
    pub fn a2b_yield_remaining(&mut self) -> Option<(PortSlice, PortSlice)> {
        if self.a_in_offset < self.a_in.width() {
            let a_in_slice = self.a_in.slice_with_offset_and_width(
                self.a_in_offset,
                self.a_in.width() - self.a_in_offset,
            );
            let b_out_slice = self.b_out.slice_with_offset_and_width(
                self.a_in_offset,
                self.b_out.width() - self.a_in_offset,
            );
            self.a_in_offset = self.a_in.width();
            Some((a_in_slice, b_out_slice))
        } else {
            None
        }
    }

    /// Returns port slices for the remaining bits in the b -> a channel.
    /// The first slice is the "b" input port slice, and the second slice is the
    /// "a" output port slice. If there are no remaining bits, returns None.
    pub fn b2a_yield_remaining(&mut self) -> Option<(PortSlice, PortSlice)> {
        if self.a_out_offset < self.a_out.width() {
            let b_in_slice = self.b_in.slice_with_offset_and_width(
                self.a_out_offset,
                self.b_in.width() - self.a_out_offset,
            );
            let a_out_slice = self.a_out.slice_with_offset_and_width(
                self.a_out_offset,
                self.a_out.width() - self.a_out_offset,
            );
            self.a_out_offset = self.a_out.width();
            Some((b_in_slice, a_out_slice))
        } else {
            None
        }
    }

    /// Asserts that the a -> b channel is full.
    pub fn assert_a2b_full(&self) {
        assert!(
            self.a_in_offset == self.a_in.width(),
            "Funnel error: a -> b channel is not full ({} bits remaining)",
            self.a_in.width() - self.a_in_offset
        );
    }

    /// Asserts that the b -> a channel is full.
    pub fn assert_b2a_full(&self) {
        assert!(
            self.a_out_offset == self.a_out.width(),
            "Funnel error: b -> a channel is not full ({} bits remaining)",
            self.a_out.width() - self.a_out_offset
        );
    }

    pub fn assert_full(&self) {
        self.assert_a2b_full();
        self.assert_b2a_full();
    }
}
