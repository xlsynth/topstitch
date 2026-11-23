// SPDX-License-Identifier: Apache-2.0

use super::port_slice::PortSliceConnections;
use crate::{IO, Port, PortSlice};

use super::connected_item::{ConnectedItem, Tieoff, Unused};
use super::port_slice::PortSliceConnection;

impl PortSliceConnections {
    /// Returns a ConnectedItem (PortSlice, tieoff, etc.) indicating the source
    /// of the expression connected to this port slice to be used when
    /// emitting Verilog code. For example, if a ModInst output is connected to
    /// a ModInst input, the expression source will be the ModInst output port
    /// slice, unless overridden by a wire name specification. If a ModInst
    /// output is connected to a ModDef output, the expression source will be
    /// the ModDef output port slice, to avoid creating an unnecessary wire.
    pub fn to_expression_source(&self) -> Option<PortSliceConnection> {
        self.resolve_expression_source(&mut Vec::new())
    }

    /// Also collects the constant drivers needed by resolved bidirectional nets.
    pub(crate) fn resolve_expression_source(
        &self,
        inout_tieoffs: &mut Vec<(PortSliceConnection, Tieoff)>,
    ) -> Option<PortSliceConnection> {
        if self.is_empty() {
            return None;
        }

        let this = self[0].this.clone();
        let this_debug_string = this.debug_string();

        let unused_count = self.to_unused_count();
        let tieoffs = self.to_tieoffs();
        let wires = self.to_wires();
        let port_slices = self.to_port_slices();

        ///////////////////////////////
        // handle an "unused" marker //
        ///////////////////////////////

        match unused_count {
            0 => {}
            1 => {
                assert!(
                    tieoffs.is_empty(),
                    "{this_debug_string} is unused, so it cannot also be tied off"
                );
                assert!(
                    wires.is_empty(),
                    "{this_debug_string} is unused, so it cannot also be connected to a wire"
                );
                assert!(
                    port_slices.len() == 1,
                    "{this_debug_string} is unused, so it cannot be connected to other ports or port slices"
                );
                let io = this.port.io();
                assert!(
                    matches!(
                        (&this.port, io),
                        (Port::ModDef { .. }, IO::Input(_))
                            | (Port::ModInst { .. }, IO::Output(_))
                            | (Port::ModDef { .. }, IO::InOut(_))
                            | (Port::ModInst { .. }, IO::InOut(_))
                    ),
                    "{this_debug_string} cannot be marked as unused because it has an incompatible directionality"
                );
                return Some(PortSliceConnection {
                    this,
                    other: Unused::new().into(),
                });
            }
            _ => {
                panic!("{this_debug_string} has been marked as unused multiple times");
            }
        }

        /////////////////////
        // handle a tieoff //
        /////////////////////

        let tieoff = match tieoffs.as_slice() {
            [] => None,
            [value] => {
                assert!(
                    matches!(
                        (&this.port, this.port.io()),
                        (Port::ModDef { .. }, IO::Output(_) | IO::InOut(_))
                            | (Port::ModInst { .. }, IO::Input(_) | IO::InOut(_))
                    ),
                    "{this_debug_string} has the wrong directionality to be tied off"
                );
                if !port_slices
                    .iter()
                    .any(|pin| matches!(pin.port.io(), IO::InOut(_)))
                {
                    assert!(
                        wires.is_empty(),
                        "{this_debug_string} is tied off, so it cannot also be connected to a wire"
                    );
                    return Some(PortSliceConnection {
                        this,
                        other: ConnectedItem::Tieoff(value.clone()),
                    });
                }
                Some(value)
            }
            _ => panic!("{this_debug_string} has been tied off multiple times"),
        };

        ////////////////////////////////////
        // handle port slices connections //
        ////////////////////////////////////

        let mut mod_def_inouts = Vec::new();
        let mut mod_def_inputs = Vec::new();
        let mut mod_def_outputs = Vec::new();
        let mut mod_inst_outputs = Vec::new();
        let mut mod_inst_inouts = Vec::new();
        for port_slice in port_slices {
            match (&port_slice.port, port_slice.port.io()) {
                (Port::ModDef { .. }, IO::Input(_)) => mod_def_inputs.push(port_slice),
                (Port::ModDef { .. }, IO::Output(_)) => mod_def_outputs.push(port_slice),
                (Port::ModDef { .. }, IO::InOut(_)) => mod_def_inouts.push(port_slice),
                (Port::ModInst { .. }, IO::Output(_)) => mod_inst_outputs.push(port_slice),
                (Port::ModInst { .. }, IO::InOut(_)) => mod_inst_inouts.push(port_slice),
                // No action needed in this case, because ModInst inputs never control
                // the net name.
                (Port::ModInst { .. }, IO::Input(_)) => {}
            }
        }

        // collapse ModDef inputs to at most one (or error out)
        if mod_def_inputs.len() > 1 {
            panic!(
                "{this_debug_string} is multiply driven by {}",
                port_slice_list(&mod_def_inputs)
            );
        }
        let mod_def_input = mod_def_inputs.first().cloned();

        // collapse ModInst outputs to at most one (or error out)
        if mod_inst_outputs.len() > 1 {
            panic!(
                "{this_debug_string} is multiply driven by {}",
                port_slice_list(&mod_inst_outputs)
            );
        }
        let mod_inst_output = mod_inst_outputs.first().cloned();

        // make sure we don't have both a ModDef input and a ModInst output
        if let Some(mod_def_input) = mod_def_input.as_ref()
            && let Some(mod_inst_output) = mod_inst_output.as_ref()
        {
            panic!(
                "{this_debug_string} is multiply driven by {}",
                port_slice_list(&[mod_def_input.clone(), mod_inst_output.clone()])
            );
        }

        // collapse ModDef inouts to at most one (or error out)
        if mod_def_inouts.len() > 1 {
            panic!(
                "{this_debug_string} is connected to multiple ModDef InOut ports: {}. This is not allowed because it cannot be expressed in Verilog.",
                port_slice_list(&mod_def_inouts)
            );
        }
        let mod_def_inout = mod_def_inouts.first().cloned();
        let mod_def_inout_is_some = mod_def_inout.is_some();

        // Drivers share the resolved top-level inout. Module outputs can read it,
        // while a module input would introduce a second driver-facing definition pin.
        let mod_def_outputs_or_inouts = if let Some(mod_def_inout) = mod_def_inout {
            assert!(
                mod_def_input.is_none(),
                "Cannot have both ModDef Input and ModDef InOut for {this_debug_string}"
            );
            vec![mod_def_inout]
        } else {
            mod_def_outputs
        };

        // Mechanism for determing the "prevailing" ModInst port slice in a way that
        // will yield the same result for all PortSlices on this net: pick the ModInst
        // Output, or if not present, sort InOuts by the instance name and then
        // the port name on the instance. Note that this will not necessarily yield a
        // "nice" name; it is always possible to override this with a wire
        // connection.
        let mod_inst_output_or_inout = if let Some(mod_inst_output) = mod_inst_output {
            Some(mod_inst_output)
        } else {
            mod_inst_inouts.into_iter().min_by_key(|port_slice| {
                (
                    port_slice.get_inst_name().unwrap_or_default(),
                    port_slice.port.name().to_string(),
                    port_slice.msb,
                    port_slice.lsb,
                )
            })
        };

        assert!(
            mod_def_input.is_some() || mod_def_inout_is_some || mod_inst_output_or_inout.is_some(),
            "No driver found for {this_debug_string}"
        );

        // The "prevailing" port slice is the one that will be used as
        // the expression source when emitting Verilog code, unless there
        // is also a wire connection.

        // The top-level inout, or the first child inout, emits the constant driver once.
        let tieoff_driver = tieoff.filter(|_| {
            if mod_def_inout_is_some {
                mod_def_outputs_or_inouts.first() == Some(&this)
            } else {
                mod_inst_output_or_inout.as_ref() == Some(&this)
            }
        });
        let prevailing_port_slice = if let Some(mod_def_input) = mod_def_input {
            mod_def_input
        } else {
            match mod_def_outputs_or_inouts.len() {
                0 => {
                    // No ModDef ports at all, so use the ModInst Output or InOut port
                    // Note that there can be multiple ModInst InOuts, but they are
                    // already reduced to a single "prevailing" PortSlice by this point.
                    mod_inst_output_or_inout
                        .unwrap_or_else(|| panic!("No driver found for {this_debug_string}"))
                }
                1 => {
                    // One ModDef Output or InOut, and no ModDef Inputs. In this case, the
                    // ModDef port prevails. Anything else connected to it can simply use
                    // the ModDef port name to connect, without an intermediate wire.
                    mod_def_outputs_or_inouts[0].clone()
                }
                _ => mod_inst_output_or_inout
                    .unwrap_or_else(|| panic!("No driver found for {this_debug_string}")),
            }
        };

        //////////////////////////////
        // handle a wire connection //
        //////////////////////////////

        let other = match wires.len() {
            0 => ConnectedItem::PortSlice(prevailing_port_slice),
            1 => {
                let io = prevailing_port_slice.port.io();
                assert!(
                    !matches!(
                        (&prevailing_port_slice.port, io),
                        (Port::ModDef { .. }, IO::Input(_))
                    ),
                    "A wire cannot be attached to {this_debug_string} because it is a ModDef Input"
                );
                // Every driver of a top-level inout must drive that same resolved net.
                // Input pins can read the named wire, which mirrors the inout's value.
                if mod_def_inout_is_some
                    && matches!(
                        (&this.port, this.port.io()),
                        (Port::ModInst { .. }, IO::Output(_) | IO::InOut(_))
                    )
                {
                    ConnectedItem::PortSlice(prevailing_port_slice)
                } else {
                    ConnectedItem::Wire(wires[0].clone())
                }
            }
            _ => {
                panic!("Multiple wires connections found for {this_debug_string}");
            }
        };
        if let Some(value) = tieoff_driver {
            // A named wire on a top-level inout mirrors that port and cannot drive it.
            let net = if mod_def_inout_is_some {
                ConnectedItem::PortSlice(this.clone())
            } else {
                other.clone()
            };
            inout_tieoffs.push((
                PortSliceConnection {
                    this: this.clone(),
                    other: net,
                },
                value.clone(),
            ));
        }
        Some(PortSliceConnection { this, other })
    }
}

pub(crate) fn merge_expression_sources(
    mut sources: Vec<PortSliceConnection>,
) -> Vec<PortSliceConnection> {
    let mut merged = Vec::new();

    while sources.len() > 1 {
        let current = sources.remove(0);
        let next = sources.remove(0);

        if let Some(other_merged) = current.other.try_merge(&next.other) {
            if let Some(this_merged) = current.this.try_merge(&next.this) {
                sources.insert(
                    0,
                    PortSliceConnection {
                        this: this_merged,
                        other: other_merged,
                    },
                );
            } else {
                merged.push(current);
                sources.insert(0, next);
            }
        } else {
            merged.push(current);
            sources.insert(0, next)
        }
    }

    if sources.len() == 1 {
        merged.push(sources.remove(0));
    }

    merged
}

fn port_slice_list(items: &[PortSlice]) -> String {
    comma_list(
        items
            .iter()
            .map(|p| p.debug_string())
            .collect::<Vec<_>>()
            .as_slice(),
    )
}

fn comma_list(items: &[impl AsRef<str>]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].as_ref().to_string(),
        2 => format!("{} and {}", items[0].as_ref(), items[1].as_ref()),
        _ => format!(
            "{}, and {}",
            items[0..items.len() - 1]
                .iter()
                .map(|i| i.as_ref())
                .collect::<Vec<_>>()
                .join(", "),
            items[items.len() - 1].as_ref()
        ),
    }
}
