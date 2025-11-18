// SPDX-License-Identifier: Apache-2.0

/// Represents the direction (`Input` or `Output`) and bit width of a port.
#[derive(Clone, Debug)]
pub enum IO {
    Input(usize),
    Output(usize),
    InOut(usize),
}

impl IO {
    /// Returns the width of the port in bits.
    pub fn width(&self) -> usize {
        match self {
            IO::Input(width) => *width,
            IO::Output(width) => *width,
            IO::InOut(width) => *width,
        }
    }

    /// Returns a new IO enum with the same width but the opposite direction.
    pub fn flip(&self) -> IO {
        match self {
            IO::Input(width) => IO::Output(*width),
            IO::Output(width) => IO::Input(*width),
            IO::InOut(width) => IO::InOut(*width),
        }
    }

    /// Returns a new IO enum with the same direction but a different width.
    pub fn with_width(&self, width: usize) -> IO {
        match self {
            IO::Input(_) => IO::Input(width),
            IO::Output(_) => IO::Output(width),
            IO::InOut(_) => IO::InOut(width),
        }
    }

    pub fn to_def_direction(&self) -> String {
        match self {
            IO::Input(_) => "INPUT",
            IO::Output(_) => "OUTPUT",
            IO::InOut(_) => "INOUT",
        }
        .to_string()
    }

    pub fn to_lef_direction(&self) -> String {
        match self {
            IO::Input(_) => "INPUT",
            IO::Output(_) => "OUTPUT",
            IO::InOut(_) => "INOUT",
        }
        .to_string()
    }
}
