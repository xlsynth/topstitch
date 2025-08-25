// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::mod_def::dtypes::{BoundingBox, Mat3, RectilinearShape};
use crate::{ModDef, Usage};

impl ModDef {
    pub fn bbox(&self) -> Option<BoundingBox> {
        if let Some(shape) = &self.core.borrow().shape {
            Some(shape.bbox())
        } else {
            let mut combined_bbox: Option<BoundingBox> = None;

            for child in self.get_instances() {
                let child_bbox = child.get_mod_def().bbox();

                if let Some(mut child_bbox) = child_bbox {
                    let child_mod_inst_name = child.name();
                    if let Some(placement) =
                        self.core.borrow().inst_placements.get(child_mod_inst_name)
                    {
                        child_bbox =
                            child_bbox.apply_transform(&Mat3::from_orientation_then_translation(
                                &placement.orientation,
                                &placement.coordinate,
                            ));
                    }

                    combined_bbox = if let Some(combined_bbox) = combined_bbox {
                        Some(combined_bbox.union(&child_bbox))
                    } else {
                        Some(child_bbox)
                    };
                }
            }

            combined_bbox
        }
    }

    /// Collect placements and shapes for modules where usage stops descent
    /// (EmitStubAndStop or EmitNothingAndStop).
    pub fn collect_placements_and_shapes(
        &self,
    ) -> (
        IndexMap<String, CalculatedPlacement>,
        IndexMap<String, RectilinearShape>,
    ) {
        let mut placements = IndexMap::new();
        let mut shapes = IndexMap::new();
        self.collect_placements_and_shapes_helper(
            &mut placements,
            &mut shapes,
            &self.get_name(),
            Mat3::identity(),
        );
        (placements, shapes)
    }

    fn collect_placements_and_shapes_helper(
        &self,
        placements: &mut IndexMap<String, CalculatedPlacement>,
        shapes: &mut IndexMap<String, RectilinearShape>,
        prefix: &str,
        m_curr: Mat3,
    ) {
        for child in self.get_instances() {
            let child_mod_def = child.get_mod_def();
            let child_mod_def_name = child_mod_def.get_name();
            let child_mod_inst_name = child.name();
            let child_path = format!("{prefix}/{child_mod_inst_name}");

            // Instance-local placement matrix: Translation * Orientation
            let child_m = if let Some(placement) =
                self.core.borrow().inst_placements.get(child_mod_inst_name)
            {
                &m_curr
                    * &Mat3::from_orientation_then_translation(
                        &placement.orientation,
                        &placement.coordinate,
                    )
            } else {
                m_curr
            };

            match child.get_mod_def().core.borrow().usage {
                Usage::EmitStubAndStop | Usage::EmitDefinitionAndStop => {
                    // Add placement information for this instance
                    placements.insert(
                        child_path.clone(),
                        CalculatedPlacement {
                            module: child_mod_def_name.clone(),
                            transform: child_m,
                        },
                    );
                    // Add shape information for this instance if not already present
                    shapes
                        .entry(child_mod_def_name.to_string())
                        .or_insert_with(|| {
                            if let Some(shape) = &child.get_mod_def().core.borrow().shape {
                                shape.clone()
                            } else {
                                panic!(
                                "Module '{}' marked to stop (usage={:?}) but has no shape defined",
                                child_mod_def_name, child.get_mod_def().core.borrow().usage
                            );
                            }
                        });
                }
                Usage::EmitNothingAndStop => (),
                Usage::EmitDefinitionAndDescend => {
                    child_mod_def.collect_placements_and_shapes_helper(
                        placements,
                        shapes,
                        &child_path,
                        child_m,
                    );
                }
            }
        }
    }
}

/// Public placement info for block instances.
pub struct CalculatedPlacement {
    pub module: String,
    pub transform: Mat3,
}
