// SPDX-License-Identifier: Apache-2.0

use super::{EmitOptions, Expr, LiteralFormat, NetCollection, VastFile, VastModule, slice_bits};
use crate::{ConnectedItem, ModDefCore, Port, Primitive, ResetKind};
use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet};
use xlsynth_vast::IndexableExpr;

use super::{connected_item_to_expression, port_expression};
use crate::connection::connected_item::Unused;
use crate::mod_def::core::InstanceConnections;
use crate::primitive::DELAY_STAGES_PORT;
use crate::{IO, ModDef, PortSliceConnection};
use std::borrow::Cow;

/// Stem for the parent-local net that carries an anonymous delay's taps.
const DELAY_TAPS_NET: &str = "delay_taps";

/// Materialize long combinational expressions before recursive VAST emission exhausts the stack.
const MAX_INLINE_DEPTH: usize = 32;

impl ModDefCore {
    /// Behavioral primitive occurrences emit logic inside the enclosing module.
    pub(crate) fn inlines_primitive(&self, key: &str) -> bool {
        let child = self.instances[key].read();
        child.primitive.is_some() && !child.primitives.mapped_body
    }

    /// Allocates physical occurrence names and their derived wires in the completed local namespace.
    pub(crate) fn physical_primitive_names(&self) -> HashMap<String, String> {
        let mut physical = HashMap::new();
        if self.primitives.anonymous.is_empty() {
            return physical;
        }
        let mut reserved: HashSet<_> = self
            .ports
            .keys()
            .chain(&self.specified_net_names)
            .cloned()
            .collect();
        for (name, child) in &self.instances {
            if !self.primitives.anonymous.contains(name) {
                reserved.insert(name.clone());
                reserved.extend(child.read().ports.keys().map(|pin| format!("{name}_{pin}")));
            }
        }
        for key in &self.primitives.anonymous {
            if self.inlines_primitive(key) {
                continue;
            }
            let stem = key.replace('@', "_");
            let child = self.instances[key].read();
            let mut name = stem.clone();
            for serial in 1.. {
                if !reserved.contains(&name)
                    && child
                        .ports
                        .keys()
                        .all(|pin| !reserved.contains(&format!("{name}_{pin}")))
                {
                    break;
                }
                name = format!("{stem}_{serial}");
            }
            reserved.insert(name.clone());
            reserved.extend(child.ports.keys().map(|pin| format!("{name}_{pin}")));
            physical.insert(key.clone(), name);
        }
        physical
    }
}

impl NetCollection {
    /// Keeps generated temporaries clear of user-visible identifiers and structural instance wires.
    pub(super) fn reserve_names(&mut self, core: &ModDefCore) {
        self.reserved.extend(core.ports.keys().cloned());
        self.reserved
            .extend(core.specified_net_names.iter().cloned());
        self.physical_names = core.physical_primitive_names();
        for (key, inst) in &core.instances {
            let name = self.physical_names.get(key).unwrap_or(key);
            self.reserved.insert(name.clone());
            self.reserved.extend(
                inst.read()
                    .ports
                    .keys()
                    .map(|port| format!("{name}_{port}")),
            );
        }
    }

    /// Reserves output nets for one anonymous behavioral primitive.
    pub(super) fn reserve_inline_outputs(&mut self, name: &str, def: &ModDefCore) {
        let kind = def.primitive.as_ref().unwrap();
        let wire = self.unique_name(&format!("{}_result", kind.label()));
        self.primitive_wires
            .insert((name.to_owned(), kind.output().into()), wire);
        if def.ports.contains_key(DELAY_STAGES_PORT) {
            let taps = self.unique_name(DELAY_TAPS_NET);
            self.primitive_wires
                .insert((name.to_owned(), DELAY_STAGES_PORT.into()), taps);
        }
    }

    /// Allocates a module-local signal name without using the primitive occurrence ID.
    fn unique_name(&mut self, stem: &str) -> String {
        if self.reserved.insert(stem.to_owned()) {
            return stem.to_owned();
        }
        let next = self.next_suffix.entry(stem.to_owned()).or_insert(1);
        loop {
            let name = format!("{stem}_{next}");
            *next += 1;
            if self.reserved.insert(name.clone()) {
                return name;
            }
        }
    }
}

/// The sole consumer of a whole result can use its expression if both pins share a private net.
/// An inout or another connection on the receiving pin needs a real net for SV resolution.
/// Structural consumers are emitted after all inline operations.
fn inline_consumer(inline_ops: &IndexSet<String>, source: &str, output: &Port) -> Option<String> {
    let connections = output.get_connections();
    let [link] = connections.as_slice() else {
        return None;
    };
    let ConnectedItem::PortSlice(pin) = &link.other else {
        return None;
    };
    let sink = pin.port.inst_name()?;
    let source_index = inline_ops.get_index_of(source)?;
    (matches!(pin.port.io(), crate::IO::Input(_))
        && link.this.width() == output.io().width()
        && pin.width() == pin.port.io().width()
        && pin.port.get_connections().len() == 1
        && inline_ops
            .get_index_of(sink)
            .is_none_or(|sink_index| source_index < sink_index))
    .then(|| sink.to_owned())
}

/// Adds a continuous assignment between two already-sized expressions.
fn assign(file: &mut VastFile, module: VastModule, lhs: &Expr, rhs: &Expr) {
    let statement = file.make_continuous_assignment(lhs, rhs);
    file.add_member_continuous_assignment(module, statement);
}

impl Primitive {
    /// Fills open ranges of the optional delay tap bus for both behavioral and mapped delays.
    /// Only the emitter's copy is changed, so extracting connections sees the original graph.
    pub(super) fn connections_for_emission<'a>(
        &self,
        parent: &ModDef,
        name: &str,
        ports: &IndexMap<String, IO>,
        pins: &'a InstanceConnections,
    ) -> Cow<'a, InstanceConnections> {
        if !matches!(self, Self::Delay { .. }) || !ports.contains_key(DELAY_STAGES_PORT) {
            return Cow::Borrowed(pins);
        }
        let pin = parent.get_instance(name).get_port(DELAY_STAGES_PORT);
        let mut taps = pins
            .get(DELAY_STAGES_PORT)
            .map(|c| c.read().clone())
            .unwrap_or_default();
        let mut ranges: Vec<_> = (&taps)
            .into_iter()
            .map(|link| (link.this.lsb, link.this.msb))
            .collect();
        ranges.sort_unstable();
        let mut next = 0;
        for (lo, hi) in ranges {
            if next < lo {
                taps.add(pin.slice(lo - 1, next), Unused::new());
            }
            next = next.max(hi + 1);
        }
        let width = ports[DELAY_STAGES_PORT].width();
        if next < width {
            taps.add(pin.slice(width - 1, next), Unused::new());
        }
        let mut pins = pins.clone();
        pins.insert(
            DELAY_STAGES_PORT.into(),
            std::sync::Arc::new(parking_lot::RwLock::new(taps)),
        );
        Cow::Owned(pins)
    }

    /// Emits the operations and assignments for a behavioral primitive inside its parent.
    pub(super) fn emit_inline<'a>(
        &self,
        name: &str,
        inline_ops: &IndexSet<String>,
        width: usize,
        pins: impl Iterator<Item = (&'a str, &'a IO, Vec<PortSliceConnection>)>,
        file: &mut VastFile,
        module: &mut VastModule,
        nets: &mut NetCollection,
        options: &EmitOptions,
    ) {
        let input_depth = nets.inline_input_depths.remove(name).unwrap_or_default();
        let depth = match self {
            // These operations produce a reference to separately declared storage or a case result.
            Self::Flop { .. } | Self::Delay { cycles: 1.., .. } => 0,
            Self::Select { arms: 1.., .. } if !self.is_binary_mux() => 0,
            _ => input_depth + 1,
        };
        let mut words = Vec::new();
        let mut pin_bits = Vec::new();
        let mut delay_input_bits = Vec::new();
        for (pin, io, sources) in pins {
            if pin == DELAY_STAGES_PORT
                && sources
                    .iter()
                    .all(|s| matches!(s.other, ConnectedItem::Unused(_)))
            {
                if options.bitblast_assignments {
                    pin_bits.push(Vec::new());
                } else {
                    words.push(None);
                }
                continue;
            }
            let delay_input = matches!(self, Self::Delay { cycles: 1.., .. }) && pin == "in";
            if options.bitblast_assignments
                && (matches!(io, IO::Output(_)) || !self.uses_word_inputs() || delay_input)
            {
                let mut bits = Vec::new();
                for source in sources.iter().rev() {
                    for offset in 0..source.this.width() {
                        let this = source.this.slice_with_offset_and_width(offset, 1);
                        let other = source.other.slice_with_offset_and_width(offset, 1);
                        bits.push(connected_item_to_expression(
                            &this, &other, file, module, nets, options,
                        ));
                    }
                }
                if delay_input {
                    delay_input_bits = bits;
                } else {
                    pin_bits.push(bits);
                    continue;
                }
            }
            if pin == "rst"
                && let [reset] = sources.as_slice()
                && self.async_reset_needs_wire(&reset.other, nets)
            {
                let expr = connected_item_to_expression(
                    &reset.this,
                    &reset.other,
                    file,
                    module,
                    nets,
                    options,
                );
                let scalar = file.make_bit_vector_type(1, false);
                let wire = file
                    .add_wire(*module, &nets.unique_name("async_reset"), &scalar)
                    .to_expr();
                assign(file, *module, &wire, &expr);
                words.push(Some(wire));
            } else if pin == self.output()
                && let [source] = sources.as_slice()
                && matches!(&source.other, ConnectedItem::PortSlice(p) if p == &source.this)
                && depth <= MAX_INLINE_DEPTH
                && let Some(sink) = inline_consumer(inline_ops, name, &source.this.port)
            {
                let sink_depth = nets.inline_input_depths.entry(sink).or_default();
                *sink_depth = (*sink_depth).max(depth);
                words.push(None);
            } else {
                words.push(port_expression(sources, file, module, nets, options));
            }
        }
        if options.bitblast_assignments {
            self.emit_bit_assignments(
                &pin_bits,
                &words.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
                &delay_input_bits,
                file,
                *module,
                nets,
                options,
            );
            return;
        }
        let taps = if matches!(self, Self::Delay { .. }) {
            words.pop().unwrap()
        } else {
            None
        };
        let output = words.pop().unwrap();
        let inputs: Vec<_> = words.into_iter().map(Option::unwrap).collect();
        let (value, stages) =
            self.emit_outputs(width, &inputs, file, *module, nets, taps.is_some());
        if let Some(taps) = taps {
            assign(file, *module, &taps, &stages.unwrap());
        }
        if let Some(output) = output {
            assign(file, *module, &output, &value);
        } else {
            nets.inline_values
                .insert((name.to_owned(), self.output().to_owned()), value);
        }
    }

    /// Synthesizers match an asynchronous reset's event and condition by signal identity.
    /// A literal or an inlined expression needs a wire shared by both references.
    fn async_reset_needs_wire(&self, source: &ConnectedItem, nets: &NetCollection) -> bool {
        let (Self::Flop { reset, .. } | Self::Delay { reset, .. }) = self else {
            return false;
        };
        if !matches!(reset, Some((ResetKind::AsyncHigh | ResetKind::AsyncLow, _))) {
            return false;
        }
        match source {
            ConnectedItem::Tieoff(_) => true,
            ConnectedItem::PortSlice(pin) => pin.port.inst_name().is_some_and(|name| {
                nets.inline_values
                    .contains_key(&(name.to_owned(), pin.port.get_port_name()))
            }),
            _ => false,
        }
    }

    /// Lowers a primitive to logic in the current SV module; inputs are in formal port order.
    fn emit_value(
        &self,
        width: usize,
        inputs: &[Expr],
        file: &mut VastFile,
        module: VastModule,
        nets: &mut NetCollection,
    ) -> Expr {
        match self {
            Self::And => file.make_bitwise_and(&inputs[0], &inputs[1]),
            Self::Or => file.make_bitwise_or(&inputs[0], &inputs[1]),
            Self::Xor => file.make_bitwise_xor(&inputs[0], &inputs[1]),
            Self::Not => file.make_bitwise_not(&inputs[0]),
            Self::ReduceAnd => file.make_and_reduce(&inputs[0]),
            Self::ReduceOr => file.make_or_reduce(&inputs[0]),
            Self::ReduceXor => file.make_xor_reduce(&inputs[0]),
            Self::Concat { .. } => {
                if inputs.len() == 1 {
                    inputs[0]
                } else {
                    file.make_concat(&inputs.iter().collect::<Vec<_>>())
                }
            }
            Self::Select { arms: 0, .. } => inputs[1],
            Self::Select { .. } if self.is_binary_mux() => {
                file.make_ternary(&inputs[0], &inputs[2], &inputs[1])
            }
            Self::Select { .. } => self.emit_case(width, inputs, file, module, nets).to_expr(),
            Self::Delay { cycles: 0, .. } => inputs[0],
            Self::Flop { .. } => self
                .emit_register(width, inputs, file, module, nets)
                .to_expr(),
            Self::Delay { cycles, .. } => {
                let storage = self.emit_register(width, inputs, file, module, nets);
                delay_output(storage, *cycles, file).to_expr()
            }
        }
    }

    /// Emits the main result and, when requested, the complete delay tap bus.
    fn emit_outputs(
        &self,
        width: usize,
        inputs: &[Expr],
        file: &mut VastFile,
        module: VastModule,
        nets: &mut NetCollection,
        taps: bool,
    ) -> (Expr, Option<Expr>) {
        match self {
            Self::Delay { cycles: 0, .. } => (inputs[0], taps.then_some(inputs[0])),
            Self::Delay { cycles, .. } => {
                let storage = self.emit_register(width, inputs, file, module, nets);
                let out = delay_output(storage, *cycles, file).to_expr();
                let stages = taps.then(|| file.make_concat(&[&storage.to_expr(), &inputs[0]]));
                (out, stages)
            }
            _ => (self.emit_value(width, inputs, file, module, nets), None),
        }
    }

    /// A complete one-bit selection shares the ternary behavior of `select(true_arm, false_arm)`.
    fn is_binary_mux(&self) -> bool {
        matches!(
            self,
            Self::Select {
                index_width: 1,
                arms: 2,
                fallback: false
            }
        )
    }

    /// Emits one word-wide lookup. A selector containing X or Z takes the default arm.
    fn emit_case(
        &self,
        width: usize,
        inputs: &[Expr],
        file: &mut VastFile,
        module: VastModule,
        nets: &mut NetCollection,
    ) -> IndexableExpr {
        let Self::Select {
            index_width,
            arms,
            fallback,
        } = *self
        else {
            unreachable!()
        };
        let ty = file.make_bit_vector_type(width as i64, false);
        let value = file
            .add_logic(module, &nets.unique_name("selected"), &ty)
            .unwrap();
        let always = file.add_always_comb(module).unwrap();
        let case = file.block_add_case(file.statement_block(always), &inputs[0]);
        for (index, data) in inputs[1..=arms].iter().enumerate() {
            let label = file
                .make_literal(
                    &format!("bits[{index_width}]:{index}"),
                    &LiteralFormat::UnsignedDecimal,
                )
                .unwrap();
            let item = file.case_add_item(case, &label);
            file.block_add_blocking_assignment(item, &value.to_expr(), data);
        }
        let default = file.case_add_default(case);
        let other = if fallback {
            inputs[arms + 1]
        } else {
            file.make_unsized_zero_literal()
        };
        file.block_add_blocking_assignment(default, &value.to_expr(), &other);
        value.to_indexable_expr()
    }

    /// These operations consume complete words even when output assigns are split.
    fn uses_word_inputs(&self) -> bool {
        match self {
            Self::Select { arms, .. } => *arms != 0 && !self.is_binary_mux(),
            Self::ReduceAnd
            | Self::ReduceOr
            | Self::ReduceXor
            | Self::Flop { .. }
            | Self::Delay { cycles: 1.., .. } => true,
            _ => false,
        }
    }

    /// Emits scalar continuous assignments from LSB-first pin bits, retaining a single register bank.
    fn emit_bit_assignments(
        &self,
        pins: &[Vec<Expr>],
        word_inputs: &[Expr],
        delay_input_bits: &[Expr],
        file: &mut VastFile,
        module: VastModule,
        nets: &mut NetCollection,
        options: &EmitOptions,
    ) {
        let (stage_pins, pins) = if matches!(self, Self::Delay { .. }) {
            let (stages, remaining) = pins.split_last().unwrap();
            (Some(stages), remaining)
        } else {
            (None, pins)
        };
        let (out, inputs) = pins.split_last().unwrap();
        let values: Vec<Expr> = match self {
            Self::Concat { .. } => inputs.iter().rev().flatten().copied().collect(),
            Self::Delay { cycles: 0, .. } => {
                if let Some(stages) = stage_pins {
                    for (lhs, rhs) in stages.iter().zip(&inputs[0]).rev() {
                        assign(file, module, lhs, rhs);
                    }
                }
                inputs[0].clone()
            }
            Self::Delay { cycles, .. } => {
                let storage = self.emit_register(out.len(), word_inputs, file, module, nets);
                if let Some(stages) = stage_pins {
                    for (bit, lhs) in stages.iter().enumerate().rev() {
                        let stage = bit / out.len();
                        let offset = bit % out.len();
                        let rhs = if stage == 0 {
                            delay_input_bits[offset]
                        } else {
                            let word = if *cycles == 1 {
                                storage
                            } else {
                                file.make_index(&storage, (stage - 1) as i64)
                                    .to_indexable_expr()
                            };
                            slice_bits(word, out.len(), offset, offset, file, options)
                        };
                        assign(file, module, lhs, &rhs);
                    }
                }
                let q = delay_output(storage, *cycles, file);
                (0..out.len())
                    .map(|bit| slice_bits(q, out.len(), bit, bit, file, options))
                    .collect()
            }
            Self::Select { arms: 0, .. } => inputs[1].clone(),
            Self::Select { .. } if self.is_binary_mux() => (0..out.len())
                .map(|bit| file.make_ternary(&inputs[0][0], &inputs[2][bit], &inputs[1][bit]))
                .collect(),
            Self::Select { .. } => {
                let value = self.emit_case(out.len(), word_inputs, file, module, nets);
                (0..out.len())
                    .map(|bit| slice_bits(value, out.len(), bit, bit, file, options))
                    .collect()
            }
            Self::Flop { .. } => {
                let q = self.emit_register(out.len(), word_inputs, file, module, nets);
                (0..out.len())
                    .map(|bit| slice_bits(q, out.len(), bit, bit, file, options))
                    .collect()
            }
            Self::ReduceAnd | Self::ReduceOr | Self::ReduceXor => {
                vec![self.emit_value(1, word_inputs, file, module, nets)]
            }
            Self::And | Self::Or | Self::Xor | Self::Not => (0..out.len())
                .map(|bit| {
                    let operands: Vec<_> = inputs.iter().map(|pin| pin[bit]).collect();
                    self.emit_value(1, &operands, file, module, nets)
                })
                .collect(),
        };
        for (lhs, rhs) in out.iter().zip(values).rev() {
            assign(file, module, lhs, &rhs);
        }
    }

    /// Emits one sequential process and returns its indexable output word.
    fn emit_register(
        &self,
        width: usize,
        inputs: &[Expr],
        file: &mut VastFile,
        module: VastModule,
        nets: &mut NetCollection,
    ) -> IndexableExpr {
        let (cycles, falling, reset) = match self {
            Self::Flop { falling, reset } => (1, falling, reset),
            Self::Delay {
                cycles,
                falling,
                reset,
            } => (*cycles, falling, reset),
            _ => unreachable!(),
        };
        let word = file.make_bit_vector_type(width as i64, false);
        let ty = if cycles > 1 {
            file.make_packed_array_type(word, &[cycles as i64])
        } else {
            word
        };
        let stem = if matches!(self, Self::Delay { .. }) {
            "delay_stages"
        } else {
            "flop_q"
        };
        let storage = file
            .add_logic(module, &nets.unique_name(stem), &ty)
            .unwrap();
        let q = storage.to_expr();
        let next = if cycles == 1 {
            inputs[0]
        } else {
            let stages = storage.to_indexable_expr();
            let shifted = if cycles == 2 {
                file.make_index(&stages, 0).to_expr()
            } else {
                file.make_slice(&stages, (cycles - 2) as i64, 0).to_expr()
            };
            file.make_concat(&[&shifted, &inputs[0]])
        };
        let clock = if *falling {
            file.make_neg_edge(&inputs[1])
        } else {
            file.make_pos_edge(&inputs[1])
        };
        let reset_edge = match reset {
            Some((ResetKind::AsyncLow, _)) => Some(file.make_neg_edge(&inputs[2])),
            Some((ResetKind::AsyncHigh, _)) => Some(file.make_pos_edge(&inputs[2])),
            _ => None,
        };
        let mut edges = vec![&clock];
        edges.extend(reset_edge.as_ref());
        let always = file.add_always_ff(module, &edges).unwrap();
        let mut load = file.statement_block(always);
        if let Some((kind, value)) = reset {
            let asserted = if matches!(kind, ResetKind::SyncLow | ResetKind::AsyncLow) {
                file.make_logical_not(&inputs[2])
            } else {
                inputs[2]
            };
            let cond = file.block_add_cond(load, &asserted);
            let clear = file.conditional_then_block(cond);
            let literal = file
                .make_literal(&format!("bits[{width}]:0x{value:x}"), &LiteralFormat::Hex)
                .unwrap();
            let fill = if cycles == 1 {
                literal
            } else {
                file.make_replicated_concat_i64(cycles as i64, &[&literal])
            };
            file.block_add_nonblocking_assignment(clear, &q, &fill);
            load = file.conditional_add_else(cond);
        }
        file.block_add_nonblocking_assignment(load, &q, &next);
        storage.to_indexable_expr()
    }
}

/// Selects the last registered word from a packed delay shift register.
fn delay_output(storage: IndexableExpr, cycles: usize, file: &mut VastFile) -> IndexableExpr {
    if cycles == 1 {
        storage
    } else {
        file.make_index(&storage, (cycles - 1) as i64)
            .to_indexable_expr()
    }
}
