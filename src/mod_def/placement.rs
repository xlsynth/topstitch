// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::mod_def::core::{ModDefCore, PlacementNode};
use crate::mod_def::dtypes::{BoundingBox, Mat3};
use crate::validate::inst_overlap;
use crate::{LefDefOptions, ModDef, ModInst, Usage};

impl ModDef {
    /// Return the explicit outline's bounds, or the union of placed child bounds.
    pub fn bbox(&self) -> Option<BoundingBox> {
        if let Some(shape) = &self.core.read().shape {
            return Some(shape.bbox());
        }
        self.get_instances()
            .iter()
            .filter_map(ModInst::bounds_in_root)
            .reduce(|a, b| a.union(&b))
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
        Self::collect_placements_and_mod_defs_helper(
            &self.core.read(),
            &Mat3::identity(),
            &[],
            &mut placements,
            &mut mod_defs,
            if opts.omit_top_module_in_hierarchy {
                None
            } else {
                Some(&name)
            },
            opts.divider_char.as_str(),
        );

        for (inst_path, expected_placement) in &opts.expected_placements {
            let calculated_placement = placements.get(inst_path).unwrap_or_else(|| {
                panic!(
                    "Expected placement specified for unknown or non-emitted instance '{inst_path}'"
                )
            });
            if calculated_placement.transform != expected_placement.transform() {
                let actual_placement = crate::Placement {
                    coordinate: calculated_placement.transform.as_coordinate(),
                    orientation: calculated_placement.transform.as_orientation(),
                };
                panic!(
                    "Placement for instance '{inst_path}' does not match expectation: expected {expected_placement:?}, actual {actual_placement:?}"
                );
            }
        }

        if opts.check_for_instance_overlaps {
            inst_overlap::check(
                &placements
                    .iter()
                    .map(|(inst_name, p)| {
                        let mod_def = mod_defs
                            .get(&p.module)
                            .unwrap_or_else(|| panic!("ModDef for module {} not found when checking for instance overlaps", &p.module));
                        let shape = mod_def
                            .get_keepout()
                            .or_else(|| mod_def.get_shape())
                            .map(|shape| shape.apply_transform(&p.transform));
                        (inst_name.clone(), shape)
                    })
                    .filter_map(|(inst_name, shape)| shape.map(|s| (inst_name, s)))
                    .collect::<Vec<_>>(),
            );
        }

        (placements, mod_defs)
    }

    /// Carry the parent transform and any enclosing placements that reach this subtree.
    fn collect_placements_and_mod_defs_helper(
        parent: &ModDefCore,
        parent_transform: &Mat3,
        inherited: &[&IndexMap<String, PlacementNode>],
        placements: &mut IndexMap<String, CalculatedPlacement>,
        mod_defs: &mut IndexMap<String, ModDef>,
        prefix: Option<&str>,
        divider_char: &str,
    ) {
        let mut child_scopes = Vec::new();
        for (name, child_core) in &parent.instances {
            let child = child_core.read();
            let descend = match child.usage {
                Usage::EmitNothingAndStop => continue,
                Usage::EmitDefinitionAndDescend => true,
                Usage::EmitStubAndStop | Usage::EmitDefinitionAndStop => false,
            };

            let mut local = None;
            child_scopes.clear();
            // The outermost explicit placement wins. Other scopes can still place descendants.
            for scope in inherited.iter().copied().chain([&parent.inst_placements]) {
                if let Some(node) = scope.get(name) {
                    local = local.or(node.placement);
                    if !node.children.is_empty() {
                        child_scopes.push(&node.children);
                    }
                }
            }
            let transform = local.map_or(*parent_transform, |p| parent_transform * &p.transform());
            let path = match prefix {
                Some(prefix) => format!("{prefix}{divider_char}{name}"),
                None => name.clone(),
            };
            if descend {
                Self::collect_placements_and_mod_defs_helper(
                    &child,
                    &transform,
                    &child_scopes,
                    placements,
                    mod_defs,
                    Some(&path),
                    divider_char,
                );
            } else {
                placements.insert(
                    path,
                    CalculatedPlacement {
                        module: child.name.clone(),
                        transform,
                    },
                );
                mod_defs
                    .entry(child.name.clone())
                    .or_insert_with(|| ModDef {
                        core: child_core.clone(),
                    });
            }
        }
    }
}

impl ModInst {
    /// Keep the lookup path when an unshaped instance inherits bounds from its children.
    fn bounds_in_root(&self) -> Option<BoundingBox> {
        if let Some(shape) = &self.get_mod_def().core.read().shape {
            return Some(shape.bbox().apply_transform(&self.get_transform()));
        }
        self.get_instances()
            .iter()
            .filter_map(Self::bounds_in_root)
            .reduce(|a, b| a.union(&b))
    }
}

/// Public placement info for block instances.
pub struct CalculatedPlacement {
    pub module: String,
    pub transform: Mat3,
}
