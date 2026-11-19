// SPDX-License-Identifier: Apache-2.0

//! Built-in logic represented by ordinary module instances and port connections.

mod input;
mod mapping;
pub(crate) mod tree;
pub use input::{LogicInput, bits, hex};
pub use mapping::{FlopByWidthFactory, FlopFlavor, FlopPins, GateMap, Mux2Pins, PrimitivePolicy};

use indexmap::IndexSet;
use std::collections::HashMap;
use std::ops::{BitAnd, BitOr, BitXor, Not};
use std::sync::Arc;

use num_bigint::{BigInt, BigUint};

use crate::port::PortDirectionality;
use crate::{
    Clock, ClockPin, ConvertibleToPortSlice, IO, ModDef, ModInst, Port, PortSlice, Tieoff,
};

/// Optional output packing the input and each registered stage, from LSB to MSB.
pub(crate) const DELAY_STAGES_PORT: &str = "out_stages";

/// Operation performed by a built-in module. Reductions map one word to one bit.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum Primitive {
    And,
    Or,
    Xor,
    Not,
    ReduceAnd,
    ReduceOr,
    ReduceXor,
    /// Pure wiring in SV order, most significant part first.
    Concat {
        widths: Vec<usize>,
    },
    /// Advances a word by the given number of active clock edges. Zero cycles is wiring.
    Delay {
        cycles: usize,
        falling: bool,
        reset: Option<(ResetKind, BigUint)>,
    },
    /// Inputs are `sel`, `d0`, `d1`, ... and optionally `fallback`. Output is `y`.
    Select {
        index_width: usize,
        arms: usize,
        fallback: bool,
    },
    /// Inputs are `d`, `clk` and optionally `rst`. Output is `q`.
    Flop {
        falling: bool,
        reset: Option<(ResetKind, BigUint)>,
    },
}

/// Timing and polarity of a flop reset.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ResetKind {
    SyncHigh,
    SyncLow,
    AsyncHigh,
    AsyncLow,
}

#[derive(Default)]
pub(crate) struct PrimitiveInstances {
    pub(crate) mapped_body: bool,
    pub(crate) policy: PrimitivePolicy,
    pub(crate) policy_epoch: usize,
    pub(crate) instantiated_logic: bool,
    mapped_definitions: HashMap<(Primitive, usize), ModDef>,
    definitions: HashMap<(Primitive, usize), ModDef>,
    pub(crate) anonymous: IndexSet<String>,
}

impl ModDef {
    /// Returns the built-in definition used by operator sugar in this parent.
    /// Bitwise gates have input `a`, optional input `b`, and output `y`.
    /// Its internal wiring is fixed. Connect pins on the instantiated primitive, or use
    /// [`ModDef::wrap`] to add surrounding logic in a new definition.
    pub fn primitive_def(&self, kind: Primitive, width: usize) -> ModDef {
        assert!(width > 0, "primitive data width must be positive");
        let mut core = self.core.write();
        core.primitives
            .definitions
            .entry((kind.clone(), width))
            .or_insert_with(|| {
                let def = ModDef::new(kind.definition_name(width));
                for (name, io) in kind.ports(width) {
                    def.add_port(name, io);
                }
                kind.annotate_clock_ports(&def);
                // Give pure wiring a structural path for bit-level connection tracing.
                match &kind {
                    Primitive::Concat { widths } => {
                        let y = def.get_port("y");
                        let mut offset = 0;
                        for (i, part) in widths.iter().enumerate().rev() {
                            y.slice(offset + part - 1, offset)
                                .connect(&def.get_port(format!("d{i}")));
                            offset += part;
                        }
                    }
                    Primitive::Delay { cycles: 0, .. } => {
                        let input = def.get_port("in");
                        def.get_port("out").connect(&input);
                        def.get_port(DELAY_STAGES_PORT).connect(&input);
                    }
                    _ => {}
                }
                def.core.write().primitive = Some(kind);
                def
            })
            .clone()
    }

    /// Returns this definition's built-in operation, if any, for hierarchy and path queries.
    pub fn primitive_kind(&self) -> Option<Primitive> {
        self.core.read().primitive.clone()
    }
}

/// Concatenates nonempty, same-scope ports or slices into one bus, most significant first.
/// A single part covering an entire port returns the original port and preserves its direction.
/// A partial port or multiple parts produce an output that reads the concatenated values.
/// To carry drives both ways between inout slices, connect the corresponding slices directly.
/// Pure wiring in all policies, including `Forbid`. No clock or cell mapping is needed.
pub fn concat<T: ConvertibleToPortSlice>(parts: impl IntoIterator<Item = T>) -> Port {
    let parts: Vec<_> = parts.into_iter().map(|p| p.to_port_slice()).collect();
    let widths: Vec<_> = parts.iter().map(PortSlice::width).collect();
    let width = widths
        .iter()
        .try_fold(0usize, |sum, w| sum.checked_add(*w))
        .expect("concat width overflow");
    assert!(
        width > 0 && !widths.contains(&0),
        "concat needs nonempty parts"
    );
    if let [p] = parts.as_slice()
        && p.width() == p.port.io().width()
    {
        assert!(
            p.get_directionality()
                .compatible_with(&PortDirectionality::Receiver),
            "concat needs a source"
        );
        return p.port.clone();
    }
    let inputs = parts
        .into_iter()
        .enumerate()
        .map(|(i, p)| (format!("d{i}"), p.into()))
        .collect();
    build(Primitive::Concat { widths }, width, inputs)
}

/// Operations on existing ports and slices. Each result is the output port of a primitive.
/// Data operands and resets can read the resolved value of an inout port.
pub trait LogicPort: ConvertibleToPortSlice {
    /// Reduces all bits to one bit with SystemVerilog unary AND (`&x`).
    fn and(&self) -> Port {
        unary(Primitive::ReduceAnd, self.to_port_slice())
    }

    /// Reduces all bits to one bit with SystemVerilog unary OR (`|x`).
    fn or(&self) -> Port {
        unary(Primitive::ReduceOr, self.to_port_slice())
    }

    /// Computes odd parity with SystemVerilog unary XOR (`^x`).
    fn xor(&self) -> Port {
        unary(Primitive::ReduceXor, self.to_port_slice())
    }

    /// Selects `when_true` when this one-bit signal is high. Either data arm may be a literal.
    /// At least one arm must supply a width, e.g. `sel.select(bits(8, 3), 0)`.
    fn select(&self, when_true: impl Into<LogicInput>, when_false: impl Into<LogicInput>) -> Port {
        let sel = self.to_port_slice();
        assert_eq!(sel.width(), 1, "select requires a one-bit selector");
        indexed_select(sel, [when_false.into(), when_true.into()], None)
    }

    /// Selects by unsigned index in iterator order. Requires exactly `2^index_width` arms.
    /// For a multi-bit index containing X or Z, the behavioral case returns zero.
    /// A complete one-bit table uses a ternary. Mapped tables use library mux cells.
    /// Signals and explicitly sized literals supply the width of all integer arms.
    fn select_from<T: Into<LogicInput>>(&self, arms: impl IntoIterator<Item = T>) -> Port {
        indexed_select(self.to_port_slice(), arms, None)
    }

    /// Selects by unsigned index, using `fallback` for every omitted index.
    /// In behavioral RTL the case default also handles X and Z indices.
    /// A mapped mux tree follows the cells' behavior for unknown select bits.
    /// A signal or explicitly sized fallback can supply the width of the table.
    fn select_from_or<T: Into<LogicInput>>(
        &self,
        arms: impl IntoIterator<Item = T>,
        fallback: impl Into<LogicInput>,
    ) -> Port {
        indexed_select(self.to_port_slice(), arms, Some(fallback.into()))
    }

    /// Registers this data on the rising edge of its containing module's default clock.
    fn flop(&self) -> Port {
        register(self.to_port_slice(), FlopOptions::default(), None)
    }

    /// Registers on an explicit clock edge, with a reset, or with a (clock, reset) pair.
    fn flop_with(&self, options: impl Into<FlopOptions>) -> Port {
        register(self.to_port_slice(), options.into(), None)
    }

    /// Delays this word by N rising edges of the physical default clock.
    /// At zero cycles, [`concat()`] returns the original whole port or an output reading a partial slice.
    /// An inout retains its bidirectional connection when the operand covers the whole port.
    fn delay(&self, cycles: usize) -> Port {
        self.delay_with(cycles, FlopOptions::default())
    }

    /// Delays on an explicit clock/reset configuration. Reset applies to every stage.
    /// At zero cycles the result is the same wiring as [`delay(0)`](Self::delay).
    fn delay_with(&self, cycles: usize, options: impl Into<FlopOptions>) -> Port {
        let data = self.to_port_slice();
        if cycles == 0 {
            return concat([data]);
        }
        register(data, options.into(), Some(cycles))
    }

    /// Selects the falling edge when this signal is passed as a flop clock.
    fn negedge(&self) -> ClockEdge {
        ClockEdge {
            signal: self.to_port_slice(),
            falling: true,
        }
    }
}

impl<T: ConvertibleToPortSlice> LogicPort for T {}

/// A concrete flop clock and sampling edge. A bare port selects the rising edge.
#[derive(Clone)]
pub struct ClockEdge {
    signal: PortSlice,
    falling: bool,
}

impl<T: ConvertibleToPortSlice> From<T> for ClockEdge {
    fn from(clock: T) -> Self {
        Self {
            signal: clock.to_port_slice(),
            falling: false,
        }
    }
}

impl ConvertibleToPortSlice for ClockPin {
    fn to_port_slice(&self) -> PortSlice {
        self.slice().clone()
    }
}

/// A reset signal, its timing and polarity, and its unsigned reset value.
#[derive(Clone)]
pub struct Reset {
    signal: PortSlice,
    kind: ResetKind,
    value: BigUint,
}

macro_rules! reset_constructor {
    ($method:ident, $kind:ident, $doc:literal) => {
        #[doc = $doc]
        pub fn $method(
            signal: impl ConvertibleToPortSlice,
            value: impl Into<num_bigint::BigInt>,
        ) -> Self {
            Self {
                signal: signal.to_port_slice(),
                kind: ResetKind::$kind,
                value: value
                    .into()
                    .to_biguint()
                    .expect("reset value must be unsigned"),
            }
        }
    };
}
impl Reset {
    reset_constructor!(
        sync_high,
        SyncHigh,
        "Resets to this value on a clock edge while the signal is high."
    );
    reset_constructor!(
        sync_low,
        SyncLow,
        "Resets to this value on a clock edge while the signal is low."
    );
    reset_constructor!(
        async_high,
        AsyncHigh,
        "Asynchronously resets to this value when the signal goes high."
    );
    reset_constructor!(
        async_low,
        AsyncLow,
        "Asynchronously resets to this value when the signal goes low."
    );
}

/// Optional clock and reset for a flop. Borrow a pair to share its settings between flops.
#[derive(Clone, Default)]
pub struct FlopOptions {
    clock: Option<ClockEdge>,
    reset: Option<Reset>,
}

impl<C: Into<ClockEdge>> From<C> for FlopOptions {
    fn from(clock: C) -> Self {
        Self {
            clock: Some(clock.into()),
            reset: None,
        }
    }
}
impl From<Reset> for FlopOptions {
    fn from(reset: Reset) -> Self {
        Self {
            clock: None,
            reset: Some(reset),
        }
    }
}
impl<C: Into<ClockEdge>> From<(C, Reset)> for FlopOptions {
    fn from((clock, reset): (C, Reset)) -> Self {
        Self {
            clock: Some(clock.into()),
            reset: Some(reset),
        }
    }
}
impl<C: Into<ClockEdge> + Clone> From<&(C, Reset)> for FlopOptions {
    fn from(pair: &(C, Reset)) -> Self {
        pair.clone().into()
    }
}

/// Creates a gate in the containing definition and returns its occurrence-relative output.
fn build(kind: Primitive, width: usize, inputs: Vec<(String, LogicInput)>) -> Port {
    let mut signals = inputs
        .iter()
        .filter_map(|(_, input)| match input {
            LogicInput::Signal(signal) => Some(signal),
            _ => None,
        })
        .peekable();
    let parent = signals
        .peek()
        .expect("a primitive needs a signal for its wiring scope")
        .get_mod_def();
    let mut scope = None;
    // Check every input before modifying the design. Two occurrences of the same definition
    // share a body but cannot be wired to each other through an expression.
    for pin in signals {
        assert!(
            pin.get_directionality()
                .compatible_with(&PortDirectionality::Receiver),
            "{pin:?} is not a source"
        );
        assert!(
            Arc::ptr_eq(&parent.core, &pin.get_mod_def_core()),
            "primitive inputs belong to different module definitions"
        );
        if let Some(inst) = pin.port.get_mod_inst() {
            let path = &inst.hierarchy[..inst.hierarchy.len() - 1];
            if let Some(ref scope) = scope {
                assert_eq!(
                    scope, path,
                    "primitive inputs belong to different occurrences"
                );
            } else {
                scope = Some(path.to_vec());
            }
        }
    }
    let def = parent.primitive_def(kind.clone(), width);
    for (name, input) in &inputs {
        let value = match input {
            LogicInput::Signal(_) => continue,
            LogicInput::Integer(value) => value,
            LogicInput::Bits(tieoff) => &tieoff.value,
        };
        Tieoff::check_value(value, def.get_port(name).io().width());
    }
    let mut inst = parent.instantiate(&def, None, None);
    if let Some(mut path) = scope {
        path.append(&mut inst.hierarchy);
        inst = ModInst { hierarchy: path };
    }
    for (name, input) in inputs {
        let pin = inst.get_port(name);
        match input {
            LogicInput::Signal(src) => pin.connect(&src),
            LogicInput::Integer(value) => pin.tieoff(value),
            LogicInput::Bits(tieoff) => pin.tieoff(tieoff.value),
        }
    }
    inst.get_port(kind.output())
}

/// Connects a unary primitive to the full width of its operand.
fn unary(kind: Primitive, a: PortSlice) -> Port {
    build(kind, a.width(), vec![("a".into(), a.into())])
}

/// Infers the common data width from signals and explicitly sized literals.
fn data_width<'a>(inputs: impl IntoIterator<Item = &'a LogicInput>) -> usize {
    let mut widths = inputs.into_iter().filter_map(LogicInput::width);
    let width = widths
        .next()
        .expect("literal-only data needs an explicit width: use bits(width, value)");
    assert!(
        width > 0 && widths.all(|other| other == width),
        "primitive data widths must match and be positive"
    );
    width
}

/// Makes one binary gate and keeps each operand on its original pin.
fn binary(kind: Primitive, a: LogicInput, b: LogicInput) -> Port {
    let width = data_width([&a, &b]);
    build(kind, width, vec![("a".into(), a), ("b".into(), b)])
}

/// Builds an indexed mux without expanding the unused address space into ports.
fn indexed_select<T: Into<LogicInput>>(
    sel: PortSlice,
    arms: impl IntoIterator<Item = T>,
    fallback: Option<LogicInput>,
) -> Port {
    let index_width = sel.width();
    let mut inputs = vec![("sel".to_string(), sel.into())];
    inputs.extend(
        arms.into_iter()
            .enumerate()
            .map(|(i, arm)| (format!("d{i}"), arm.into())),
    );
    let kind = Primitive::Select {
        index_width,
        arms: inputs.len() - 1,
        fallback: fallback.is_some(),
    };
    if let Some(fallback) = fallback {
        inputs.push(("fallback".to_owned(), fallback));
    }
    assert!(inputs.len() > 1, "select_from requires a complete table");
    let width = data_width(inputs[1..].iter().map(|(_, arm)| arm));
    build(kind, width, inputs)
}

/// Resolves the physical clock and connects D, clock and reset in the same scope.
fn register(data: PortSlice, options: FlopOptions, cycles: Option<usize>) -> Port {
    let parent = data.get_mod_def();
    let clock = options
        .clock
        .unwrap_or_else(|| match parent.default_clock() {
            Some(Clock::Pin(pin)) => pin.into(),
            _ => panic!(
                "{} needs a physical default clock for {}(); use an explicit clock",
                parent.get_name(),
                if cycles.is_some() { "delay" } else { "flop" }
            ),
        });
    assert_eq!(clock.signal.width(), 1, "flop clock must be one bit");
    assert!(
        clock.signal.port.is_driver(),
        "flop clock must be a source with a defined direction"
    );
    let width = data.width();
    let mut inputs = vec![
        (
            (if cycles.is_some() { "in" } else { "d" }).into(),
            data.into(),
        ),
        ("clk".into(), clock.signal.into()),
    ];
    let reset = options.reset.map(|reset| {
        assert_eq!(reset.signal.width(), 1, "flop reset must be one bit");
        inputs.push(("rst".into(), reset.signal.into()));
        (reset.kind, reset.value)
    });
    let falling = clock.falling;
    let kind = match cycles {
        Some(cycles) => Primitive::Delay {
            cycles,
            falling,
            reset,
        },
        None => Primitive::Flop { falling, reset },
    };
    build(kind, width, inputs)
}

impl Primitive {
    pub(crate) fn output(&self) -> &'static str {
        match self {
            Self::Flop { .. } => "q",
            Self::Delay { .. } => "out",
            _ => "y",
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Concat { .. } => "concat",
            Self::Delay { .. } => "delay",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Not => "not",
            Self::ReduceAnd => "reduce_and",
            Self::ReduceOr => "reduce_or",
            Self::ReduceXor => "reduce_xor",
            Self::Select { .. } => "mux",
            Self::Flop { .. } => "flop",
        }
    }

    /// Pure connectivity is permitted by every policy and requires no technology rule.
    pub(crate) fn is_wiring(&self) -> bool {
        matches!(self, Self::Concat { .. } | Self::Delay { cycles: 0, .. })
    }

    /// Produces a deterministic definition name for explicitly instantiated primitives.
    fn definition_name(&self, width: usize) -> String {
        let stem = format!("topstitch_{}_{width}", self.label());
        match self {
            Self::Select {
                index_width,
                arms,
                fallback,
            } => format!(
                "{stem}_{index_width}_{arms}_{}",
                if *fallback { "default" } else { "full" }
            ),
            Self::Concat { widths } => format!(
                "{stem}_{}",
                widths
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join("_")
            ),
            Self::Flop { falling, reset } | Self::Delay { falling, reset, .. } => {
                let stem = if let Self::Delay { cycles, .. } = self {
                    format!("{stem}_{cycles}")
                } else {
                    stem
                };
                let edge = if *falling { "neg" } else { "pos" };
                match reset {
                    Some((kind, val)) => format!("{stem}_{edge}_{kind:?}_{val:x}"),
                    None => format!("{stem}_{edge}"),
                }
            }
            _ => stem,
        }
    }

    /// Declares the generated sequential boundary's clock, data, and reset roles.
    pub(crate) fn annotate_clock_ports(&self, def: &ModDef) {
        let (data, output, reset) = match self {
            Self::Flop { reset, .. } => ("d", "q", reset),
            Self::Delay {
                cycles: 1.., reset, ..
            } => ("in", "out", reset),
            _ => return,
        };
        let clock = def.get_port("clk").declare_clock().unwrap();
        def.get_port(data).set_clock(&clock).unwrap();
        def.get_port(output).set_clock(&clock).unwrap();
        if matches!(self, Self::Delay { .. }) && def.has_port(DELAY_STAGES_PORT) {
            def.get_port(DELAY_STAGES_PORT).set_clock(&clock).unwrap();
        }
        if let Some((kind, _)) = reset {
            let rst = def.get_port("rst");
            match kind {
                ResetKind::AsyncLow | ResetKind::AsyncHigh => rst.set_async().unwrap(),
                ResetKind::SyncLow | ResetKind::SyncHigh => rst.set_clock(&clock).unwrap(),
            }
        }
    }

    /// Lists the formal ports in the order used by both structural and inline emission.
    fn ports(&self, width: usize) -> Vec<(String, IO)> {
        let mut ports = Vec::new();
        match self {
            Self::ReduceAnd | Self::ReduceOr | Self::ReduceXor => {
                return vec![("a".into(), IO::Input(width)), ("y".into(), IO::Output(1))];
            }
            Self::Concat { widths } => {
                assert!(
                    !widths.contains(&0)
                        && widths.iter().try_fold(0usize, |s, w| s.checked_add(*w)) == Some(width),
                    "invalid concat widths"
                );
                ports.extend(
                    widths
                        .iter()
                        .enumerate()
                        .map(|(i, w)| (format!("d{i}"), IO::Input(*w))),
                );
            }
            Self::Delay { cycles: 0, .. } => {
                ports.push(("in".into(), IO::Input(width)));
            }
            Self::Flop { reset, .. } | Self::Delay { reset, .. } => {
                let d = if let Self::Delay { cycles, .. } = self {
                    assert!(
                        width.checked_mul(*cycles).is_some(),
                        "invalid delay dimensions"
                    );
                    "in"
                } else {
                    "d"
                };
                ports.push((d.into(), IO::Input(width)));
                ports.push(("clk".into(), IO::Input(1)));
                if let Some((_, value)) = reset {
                    assert!(
                        value.bits() <= width as u64,
                        "reset value does not fit flop width {width}"
                    );
                    ports.push(("rst".into(), IO::Input(1)));
                }
            }
            Self::Select {
                index_width,
                arms,
                fallback,
            } => {
                assert!(*index_width > 0, "mux selector must have positive width");
                let capacity = if *index_width < usize::BITS as usize {
                    Some(1usize << index_width)
                } else {
                    None
                };
                if *fallback {
                    assert!(
                        capacity.is_none_or(|n| *arms <= n),
                        "too many arms for {index_width}-bit select_from_or"
                    );
                } else {
                    assert_eq!(
                        capacity,
                        Some(*arms),
                        "select_from needs a complete table for a {index_width}-bit index; use select_from_or for a partial table"
                    );
                }
                ports.push(("sel".into(), IO::Input(*index_width)));
                ports.extend((0..*arms).map(|i| (format!("d{i}"), IO::Input(width))));
                if *fallback {
                    ports.push(("fallback".into(), IO::Input(width)));
                }
            }
            _ => {
                ports.push(("a".into(), IO::Input(width)));
                if *self != Self::Not {
                    ports.push(("b".into(), IO::Input(width)));
                }
            }
        }
        ports.push((self.output().into(), IO::Output(width)));
        if let Self::Delay { cycles, .. } = self {
            let stages = cycles
                .checked_add(1)
                .and_then(|n| width.checked_mul(n))
                .expect("delay tap bus width overflow");
            ports.push((DELAY_STAGES_PORT.into(), IO::Output(stages)));
        }
        ports
    }
}

macro_rules! logic_operators {
    ($receiver:ty) => {
        impl Not for $receiver {
            type Output = Port;
            fn not(self) -> Port {
                unary(Primitive::Not, self.to_port_slice())
            }
        }
        binary_operator!($receiver, BitAnd, bitand, And);
        binary_operator!($receiver, BitOr, bitor, Or);
        binary_operator!($receiver, BitXor, bitxor, Xor);
    };
}
macro_rules! binary_operator {
    ($receiver:ty, $trait:ident, $method:ident, $kind:ident) => {
        impl<R: Into<LogicInput>> $trait<R> for $receiver {
            type Output = Port;
            fn $method(self, rhs: R) -> Port {
                binary(Primitive::$kind, self.into(), rhs.into())
            }
        }
        // Rust requires concrete left-hand types when implementing a standard operator for a foreign type.
        literal_left_operator!($receiver, $trait, $method, $kind;
            i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize,
            BigInt, BigUint, Tieoff);
    };
}
macro_rules! literal_left_operator {
    ($signal:ty, $trait:ident, $method:ident, $kind:ident; $($literal:ty),+) => {$(
        impl $trait<$signal> for $literal {
            type Output = Port;
            fn $method(self, rhs: $signal) -> Port {
                binary(Primitive::$kind, self.into(), rhs.into())
            }
        }
        impl $trait<$signal> for &$literal {
            type Output = Port;
            fn $method(self, rhs: $signal) -> Port {
                binary(Primitive::$kind, self.into(), rhs.into())
            }
        }
    )+};
}

logic_operators!(Port);
logic_operators!(&Port);
logic_operators!(PortSlice);
logic_operators!(&PortSlice);
