// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
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
                let mut overrides = BTreeMap::new();
                for (name, spec) in &core.parameters {
                    let identifier = sv_identifier(name);
                    let previous = overrides
                        .insert(identifier.to_owned(), (spec.value(), spec.literal_width()));
                    assert!(
                        previous.is_none(),
                        "Duplicate parameter override {identifier} for {} at {path}",
                        core.name
                    );
                }
                match self.external.entry(overrides) {
                    Entry::Occupied(entry) => {
                        let first = entry.get();
                        if !Arc::ptr_eq(&first.module.core, &module.core) {
                            let other = first.module.core.read();
                            assert!(
                                named_ports_match(&core.ports, &other.ports, PartialEq::eq)
                                    && named_ports_match(
                                        &core.enum_ports,
                                        &other.enum_ports,
                                        PartialEq::eq
                                    ),
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
                named_ports_match(&first_core.ports, &view.ports, |a, b| {
                    discriminant(a) == discriminant(b)
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
                    named_ports_match(&view.ports, &core.ports, PartialEq::eq),
                    "External reference to {} at {} is incompatible with its emitted stub at {}",
                    core.name,
                    reference.path,
                    definition.path
                );
            }
        }
    }
}

/// Compares port properties by their SystemVerilog identifiers, independent of declaration order.
fn named_ports_match<T>(
    left: &IndexMap<String, T>,
    right: &IndexMap<String, T>,
    compatible: impl Fn(&T, &T) -> bool,
) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut remaining: HashMap<_, _> = right
        .iter()
        .map(|(name, port)| (sv_identifier(name), port))
        .collect();
    left.iter().all(|(name, port)| {
        remaining
            .remove(sv_identifier(name))
            .is_some_and(|other| compatible(port, other))
    })
}

/// An escaped identifier's leading backslash and terminating whitespace are lexical syntax.
pub(super) fn sv_identifier(spelling: &str) -> &str {
    let spelling = spelling.trim_ascii();
    spelling.strip_prefix('\\').unwrap_or(spelling)
}
