// SPDX-License-Identifier: Apache-2.0

use crate::ModDef;
pub(crate) fn populate_hierarchy(parent: &ModDef, inst: &slang_rs::Instance) {
    for child in inst.contents.iter() {
        let child_def_name = &child.borrow().def_name;
        let mut child_inst_name = child.borrow().inst_name.clone();
        let mut child_hier_prefix = child.borrow().hier_prefix.clone();
        if child_hier_prefix.starts_with(".") {
            child_hier_prefix = child_hier_prefix[1..].to_string();
        }
        if child_hier_prefix.ends_with(".") {
            child_hier_prefix = child_hier_prefix[..child_hier_prefix.len() - 1].to_string();
        }
        if !child_hier_prefix.is_empty() {
            child_inst_name = format!("{}.{}", child_hier_prefix, &child.borrow().inst_name);
        }
        let child_mod_def = ModDef::new(child_def_name);
        parent.instantiate(&child_mod_def, Some(&child_inst_name), None);
        populate_hierarchy(&child_mod_def, &child.borrow());
    }
}

impl ModDef {
    /// Report all instances of this module, descending hierarchically. The
    /// returned vector contains tuples of (module definition name, module
    /// instance name).
    pub fn report_all_instances(&self) -> Vec<(String, String)> {
        let mut result = Vec::new();
        report_all_instances_helper(self, &self.get_name(), &mut result);
        result
    }
}

fn report_all_instances_helper(
    parent: &ModDef,
    parent_path: &str,
    result: &mut Vec<(String, String)>,
) {
    for child_inst in parent.get_instances().iter() {
        let full_inst_name = format!("{}.{}", parent_path, child_inst.name());
        let child_mod_def = child_inst.get_mod_def();
        report_all_instances_helper(&child_mod_def, &full_inst_name, result);
        result.push((child_mod_def.get_name().to_string(), full_inst_name));
    }
}
