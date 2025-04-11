// SPDX-License-Identifier: Apache-2.0

use crate::ModDef;
use std::collections::HashMap;
pub(crate) fn populate_hierarchy(
    parent: &ModDef,
    inst: &slang_rs::Instance,
    memo: &mut HashMap<String, ModDef>,
) {
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
        if let Some(child_mod_def) = memo.get(child_def_name) {
            parent.instantiate(child_mod_def, Some(&child_inst_name), None);
        } else {
            let child_mod_def = ModDef::new(child_def_name);
            parent.instantiate(&child_mod_def, Some(&child_inst_name), None);
            memo.insert(child_def_name.clone(), child_mod_def.clone());
            populate_hierarchy(&child_mod_def, &child.borrow(), memo);
        }
    }
}
