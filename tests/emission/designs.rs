// SPDX-License-Identifier: Apache-2.0

use topstitch::{IO, ModDef};

/// Swap a pair of two-bit fields in a four-bit word.
pub fn bitblast_assignments() -> ModDef {
    let mod_def = ModDef::new("BitblastAssignments");
    let y = mod_def.add_port("y", IO::Input(4));
    let x = mod_def.add_port("x", IO::Output(4));

    x.slice(3, 2).connect(&y.slice(1, 0));
    x.slice(1, 0).connect(&y.slice(3, 2));
    mod_def
}
