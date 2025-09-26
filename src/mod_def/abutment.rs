// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::rc::Rc;

use crate::{ModDef, ModInst, PortSlice};

impl ModDef {
    pub(crate) fn mark_adjacent(&mut self, inst_a: &ModInst, inst_b: &ModInst) {
        // Check that the two instances are in this module definition.
        for inst in [inst_a, inst_b] {
            let inst_core = inst.mod_def_core_where_instantiated();
            assert!(Rc::ptr_eq(&inst_core, &self.core),
                "Cannot annotate adjacency property for instance {} because it is not an instance of {}",
                inst.debug_string(),
                self.get_name()
            );
        }

        // Mark the two instances as adjacent.
        let mut core = self.core.borrow_mut();
        for adjacency_pair in [(inst_a, inst_b), (inst_b, inst_a)] {
            core.adjacency_matrix
                .entry(adjacency_pair.0.name().to_string())
                .or_default()
                .insert(adjacency_pair.1.name().to_string());
        }
    }

    fn should_consider_adjacency(&self, inst_name: impl AsRef<str>) -> bool {
        !self
            .core
            .borrow()
            .ignore_adjacency
            .contains(inst_name.as_ref())
    }

    pub(crate) fn is_non_abutted(
        &self,
        port_slice_a: &PortSlice,
        port_slice_b: &PortSlice,
    ) -> bool {
        let mut inst_names = Vec::new();
        for port_slice in [port_slice_a, port_slice_b] {
            if let Some(inst_name) = port_slice.get_inst_name() {
                if !self.should_consider_adjacency(&inst_name) {
                    return false; // i.e., we won't be able to definitively say
                                  // that
                                  // the two ports are non-abutted
                }
                inst_names.push(inst_name);
            } else {
                return false; // i.e., this is a port slice associated with a
                              // module definition, and hence we can't check if
                              // it is non-abutted.
            }
        }

        !self
            .core
            .borrow()
            .adjacency_matrix
            .get(&inst_names[0])
            .unwrap_or(&HashSet::new())
            .contains(&inst_names[1])
    }

    /// Returns a vector of all connections that are not known to be abutted,
    /// excluding connections involving instances that have been marked with
    /// `ignore_adjacency()`. Each connection returned is a tuple of the form
    /// `(port_slice_name_a, port_slice_name_b)`, where `port_slice_name_a`
    /// and `port_slice_name_b` are the names of the port slices involved in
    /// the non-abutted connection.
    pub fn find_non_abutted_connections(&self) -> Vec<(String, String)> {
        let mut result = Vec::new();

        for assignment in self.core.borrow().assignments.iter() {
            if assignment.is_non_abutted {
                continue;
            }
            if self.is_non_abutted(&assignment.lhs, &assignment.rhs) {
                result.push((assignment.lhs.debug_string(), assignment.rhs.debug_string()));
            }
        }

        result
    }
}
