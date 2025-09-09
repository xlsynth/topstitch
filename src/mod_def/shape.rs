// SPDX-License-Identifier: Apache-2.0

use crate::mod_def::ModDef;

impl ModDef {
    /// Returns `true` when the module shape is a four-vertex rectangle. This
    /// helper assumes the shape has already been validated as rectilinear.
    pub fn shape_is_rectangular(&self) -> bool {
        let core = self.core.borrow();
        if let Some(shape) = &core.shape {
            // Shape is already checked to be rectilinear when it is
            // added to a ModDef, so we only need to check the number
            // of vertices here.
            shape.num_vertices() == 4
        } else {
            panic!("Shape is not defined");
        }
    }
}
