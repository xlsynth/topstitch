// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use num_bigint::{BigInt, BigUint};

/// An enum member's four-state integer encoding, with its declared width and signedness.
/// Use `TryFrom<&EnumValue>` for checked conversion to a Rust integer or [`BigInt`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumValue {
    pub(crate) integer: slang_rs::IntegerValue,
}

impl EnumValue {
    /// Constructs a four-state encoding from its known bits and its X and Z masks.
    /// Bit zero is the least significant bit. When `signed` is true, integer
    /// conversions interpret the encoding as two's complement.
    /// Panics for zero width, out-of-range bits, or overlapping bit patterns.
    pub fn from_bits(
        width: usize,
        signed: bool,
        bits: BigUint,
        x_mask: BigUint,
        z_mask: BigUint,
    ) -> Self {
        assert!(width > 0, "an enum must have at least one bit");
        assert!(
            bits.bits().max(x_mask.bits()).max(z_mask.bits()) <= width as u64,
            "enum encoding exceeds its declared width of {width}"
        );
        assert!(
            (&bits & &x_mask) == BigUint::default()
                && (&bits & &z_mask) == BigUint::default()
                && (&x_mask & &z_mask) == BigUint::default(),
            "enum encoding has overlapping known, X or Z bits"
        );
        Self {
            integer: slang_rs::IntegerValue {
                width,
                signed,
                bits,
                x_mask,
                z_mask,
            },
        }
    }

    /// Returns the declared width in bits.
    pub fn width(&self) -> usize {
        self.integer.width
    }

    /// Returns whether the encoding represents a signed integer.
    pub fn is_signed(&self) -> bool {
        self.integer.signed
    }

    /// Returns the bit pattern with X and Z positions cleared to zero.
    pub fn bits(&self) -> &BigUint {
        &self.integer.bits
    }

    /// Returns a mask with a one at each X position.
    pub fn x_mask(&self) -> &BigUint {
        &self.integer.x_mask
    }

    /// Returns a mask with a one at each Z position.
    pub fn z_mask(&self) -> &BigUint {
        &self.integer.z_mask
    }

    /// Returns whether the encoding contains any X or Z bits.
    pub fn has_unknown(&self) -> bool {
        self.integer.has_unknown()
    }
}

/// An enum member contains X or Z bits, or its value is outside the target integer's range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumValueConversionError(slang_rs::IntegerConversionError);

impl fmt::Display for EnumValueConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for EnumValueConversionError {}

macro_rules! impl_enum_value_conversions {
    ($($target:ty),+ $(,)?) => {
        $(
            impl TryFrom<&EnumValue> for $target {
                type Error = EnumValueConversionError;

                fn try_from(value: &EnumValue) -> Result<Self, Self::Error> {
                    Self::try_from(&value.integer).map_err(EnumValueConversionError)
                }
            }
        )+
    };
}

impl_enum_value_conversions!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, BigInt
);
