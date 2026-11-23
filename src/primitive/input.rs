// SPDX-License-Identifier: Apache-2.0

use num_bigint::{BigInt, BigUint};

use crate::{ConvertibleToPortSlice, PortSlice, Tieoff};

/// A primitive data input. Integers take their hardware width from a signal or `bits()`.
/// For a mixed mux table, convert entries into a common `[LogicInput; N]` with `.into()`.
/// An integer on the left of an operator may need a Rust type suffix for method chaining.
///
/// ```
/// use topstitch::{bits, hex, IO, LogicPort, ModDef};
///
/// let top = ModDef::new("Masks");
/// let bus = top.add_port("bus", IO::Input(500));
/// let sel = top.add_port("sel", IO::Input(1));
/// let masked = &bus & hex("a".repeat(125));
/// let data = sel.select(masked, 0);
/// let status = sel.select(bits(8, 3), 0);
/// top.add_port("data", IO::Output(500)).connect(&data);
/// top.add_port("status", IO::Output(8)).connect(&status);
/// ```
#[derive(Clone, Debug)]
pub enum LogicInput {
    Signal(PortSlice),
    Integer(BigInt),
    Bits(Tieoff),
}

/// Gives an unsigned literal an explicit hardware width, e.g. `sel.select(bits(8, 3), 0)`.
pub fn bits(width: usize, value: impl Into<BigInt>) -> Tieoff {
    assert!(width > 0, "a literal must have positive width");
    Tieoff::new(value, width)
}

/// Parses a nonnegative hexadecimal integer of arbitrary size. Its hardware width is inferred.
/// Accepts an optional `0x` prefix and underscore separators.
pub fn hex(digits: impl AsRef<str>) -> BigUint {
    let digits = digits.as_ref();
    let digits = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .unwrap_or(digits);
    BigUint::parse_bytes(digits.as_bytes(), 16).expect("invalid hexadecimal literal")
}

impl LogicInput {
    /// Returns the width when this operand supplies one.
    pub(super) fn width(&self) -> Option<usize> {
        match self {
            Self::Signal(pin) => Some(pin.width()),
            Self::Integer(_) => None,
            Self::Bits(tieoff) => Some(tieoff.width),
        }
    }
}

impl<T: ConvertibleToPortSlice> From<T> for LogicInput {
    fn from(signal: T) -> Self {
        Self::Signal(signal.to_port_slice())
    }
}

impl From<&LogicInput> for LogicInput {
    fn from(input: &LogicInput) -> Self {
        input.clone()
    }
}

impl From<Tieoff> for LogicInput {
    fn from(value: Tieoff) -> Self {
        Self::Bits(value)
    }
}

impl From<&Tieoff> for LogicInput {
    fn from(value: &Tieoff) -> Self {
        Self::Bits(value.clone())
    }
}

macro_rules! integer_inputs {
    ($($integer:ty),+ $(,)?) => {$(
        impl From<$integer> for LogicInput {
            fn from(value: $integer) -> Self {
                Self::Integer(value.into())
            }
        }
        impl From<&$integer> for LogicInput {
            fn from(value: &$integer) -> Self {
                Self::from(value.clone())
            }
        }
    )+};
}

integer_inputs!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, BigInt, BigUint
);
