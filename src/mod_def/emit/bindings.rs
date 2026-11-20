// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::mem::discriminant;
use std::sync::Arc;

use indexmap::IndexMap;
use indexmap::map::Entry;

use crate::{ModDef, ModDefCore, ParameterValue, Usage};

/// The definition and instance path at which a binding was first seen.
struct ModuleUse {
    module: ModDef,
    path: String,
}

/// Everything that must bind to one Verilog module name in the emitted hierarchy.
#[derive(Default)]
pub(super) struct ModuleBinding {
    emitted: Option<ModuleUse>,
    // Integer literal widths can affect elaboration of an untyped SV parameter.
    external: IndexMap<BTreeMap<String, (ParameterValue, Option<usize>)>, ModuleUse>,
}

impl ModuleBinding {
    /// Records a reference and returns whether its definition still needs to be emitted.
    pub(super) fn register(&mut self, module: &ModDef, core: &ModDefCore, path: &str) -> bool {
        let current = || ModuleUse {
            module: module.clone(),
            path: path.to_owned(),
        };
        match core.usage {
            Usage::EmitNothingAndStop | Usage::EmitDefinitionAndStop => {
                let overrides = core
                    .parameters
                    .iter()
                    .map(|(name, spec)| (name.clone(), (spec.value(), spec.literal_width())))
                    .collect();
                match self.external.entry(overrides) {
                    Entry::Occupied(entry) => {
                        let first = entry.get();
                        if !Arc::ptr_eq(&first.module.core, &module.core) {
                            let other = first.module.core.read();
                            assert!(
                                core.ports == other.ports && core.enum_ports == other.enum_ports,
                                "Incompatible external references to {} with the same parameter overrides at {} and {path}",
                                core.name,
                                first.path
                            );
                        }
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(current());
                    }
                }
                false
            }
            Usage::EmitDefinitionAndDescend | Usage::EmitStubAndStop => {
                if let Some(first) = &self.emitted {
                    assert!(
                        Arc::ptr_eq(&first.module.core, &module.core),
                        "Two distinct modules with the same name: {} at {} and {path}",
                        core.name,
                        first.path
                    );
                    return false;
                }
                self.emitted = Some(current());
                true
            }
        }
    }

    /// Checks external specializations against one another and any definition emitted here.
    pub(super) fn check_external_references(&self) {
        let Some((_, first)) = self.external.first() else {
            return;
        };
        let first_core = first.module.core.read();
        for other in self.external.values().skip(1) {
            let view = other.module.core.read();
            assert!(
                first_core.ports.len() == view.ports.len()
                    && first_core.ports.iter().all(|(name, port)| {
                        view.ports
                            .get(name)
                            .is_some_and(|other| discriminant(port) == discriminant(other))
                    }),
                "Incompatible external port names or directions for {} at {} and {}",
                first_core.name,
                first.path,
                other.path
            );
        }
        drop(first_core);

        if let Some(definition) = &self.emitted {
            let core = definition.module.core.read();
            assert!(
                core.usage == Usage::EmitStubAndStop,
                "{} has both an external implementation and a generated definition at {} and {}",
                core.name,
                first.path,
                definition.path
            );
            for reference in self.external.values() {
                let view = reference.module.core.read();
                assert!(
                    view.parameters.is_empty(),
                    "Instance {} overrides parameters on emitted stub {} at {}, which declares none",
                    reference.path,
                    core.name,
                    definition.path
                );
                assert!(
                    view.ports == core.ports,
                    "External reference to {} at {} is incompatible with its emitted stub at {}",
                    core.name,
                    reference.path,
                    definition.path
                );
            }
        }
    }
}
