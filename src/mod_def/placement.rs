// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::mod_def::dtypes::{BoundingBox, Mat3};
use crate::validate::inst_overlap;
use crate::{LefDefOptions, ModDef, Usage};

impl ModDef {
    pub fn bbox(&self) -> Option<BoundingBox> {
        if let Some(shape) = &self.core.read().shape {
            Some(shape.bbox())
        } else {
            let mut combined_bbox: Option<BoundingBox> = None;

            for child in self.get_instances() {
                let child_bbox = child.get_mod_def().bbox();

                if let Some(mut child_bbox) = child_bbox {
                    let child_mod_inst_name = child.name();
                    if let Some(placement) =
                        self.core.read().inst_placements.get(child_mod_inst_name)
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

    /// Collect placements and referenced ModDefs where usage stops descent
    /// (EmitStubAndStop or EmitDefinitionAndStop).
    pub fn collect_placements_and_mod_defs(
        &self,
        opts: &LefDefOptions,
    ) -> (
        IndexMap<String, CalculatedPlacement>,
        IndexMap<String, ModDef>,
    ) {
        let mut placements = IndexMap::new();
        let mut mod_defs = IndexMap::new();
        let name = self.get_name();
        self.collect_placements_and_mod_defs_helper(
            &mut placements,
            &mut mod_defs,
            if opts.omit_top_module_in_hierarchy {
                None
            } else {
                Some(&name)
            },
            opts.divider_char.as_str(),
            Mat3::identity(),
        );

        if opts.check_for_instance_overlaps {
            inst_overlap::check(
                &placements
                    .iter()
                    .map(|(inst_name, p)| {
                        let shape = mod_defs
                            .get(&p.module)
                            .unwrap_or_else(|| panic!("ModDef for module {} not found or has no shape when checking for instance overlaps", &p.module))
                            .get_shape()
                            .map(|shape| shape.apply_transform(&p.transform));
                        (inst_name.clone(), shape)
                    })
                    .filter_map(|(inst_name, shape)| shape.map(|s| (inst_name, s)))
                    .collect::<Vec<_>>(),
            );
        }

        (placements, mod_defs)
    }

    fn collect_placements_and_mod_defs_helper(
        &self,
        placements: &mut IndexMap<String, CalculatedPlacement>,
        mod_defs: &mut IndexMap<String, ModDef>,
        prefix: Option<&str>,
        divider_char: &str,
        m_curr: Mat3,
    ) {
        for child in self.get_instances() {
            let child_mod_def = child.get_mod_def();
            let child_mod_def_name = child_mod_def.get_name();
            let child_mod_inst_name = child.name();
            let child_path = if let Some(prefix) = prefix {
                format!("{prefix}{divider_char}{child_mod_inst_name}")
            } else {
                child_mod_inst_name.to_string()
            };

            // Instance-local placement matrix: Translation * Orientation
            let child_m = if let Some(placement) =
                self.core.read().inst_placements.get(child_mod_inst_name)
            {
                &m_curr
                    * &Mat3::from_orientation_then_translation(
                        &placement.orientation,
                        &placement.coordinate,
                    )
            } else {
                m_curr
            };

            match child.get_mod_def().core.read().usage {
                Usage::EmitStubAndStop | Usage::EmitDefinitionAndStop => {
                    // Add placement information for this instance
                    placements.insert(
                        child_path.clone(),
                        CalculatedPlacement {
                            module: child_mod_def_name.clone(),
                            transform: child_m,
                        },
                    );
                    // Add referenced ModDef if not already present
                    mod_defs
                        .entry(child_mod_def_name.to_string())
                        .or_insert_with(|| child_mod_def.clone());
                }
                Usage::EmitNothingAndStop => (),
                Usage::EmitDefinitionAndDescend => {
                    child_mod_def.collect_placements_and_mod_defs_helper(
                        placements,
                        mod_defs,
                        Some(&child_path),
                        divider_char,
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
