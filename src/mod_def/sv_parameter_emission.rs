// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use parking_lot::RwLock;
use slang_rs::{Compilation, ModuleParameterState};

use super::net_names::check_identifier;
use super::parameterize::{ImportContext, ParameterBinding, expressions};
use crate::{IO, ModDef, ModDefCore, Usage};

type Core = Arc<RwLock<ModDefCore>>;
type Bindings = IndexMap<String, ParameterBinding>;
pub(super) type ParameterHeaders = IndexMap<String, Declarations>;

/// Text before the port list and declarations after it, in their original module scope.
#[derive(Clone, Debug, Default)]
pub(super) struct Declarations {
    pub header: String,
    pub body: String,
    pub names: Vec<String>,
}

#[derive(Debug, Default)]
struct SvParameterHeader {
    declarations: Declarations,
    parameters: IndexMap<String, Initializer>,
    state_count: usize,
    invalid_alias: Option<String>,
}

#[derive(Debug)]
struct Initializer {
    range: Range<usize>,
    in_body: bool,
    required: bool,
}

/// Source declarations and effective time scales of the imported top modules.
#[derive(Debug)]
pub(super) struct ImportedHeaders {
    definitions: HashMap<String, SvParameterHeader>,
    timescales: HashMap<String, String>,
}

/// Retain source syntax at import. Values and resolved types are queried only for emitted modules.
pub(super) fn capture<'a>(
    compilation: &Compilation,
    names: impl IntoIterator<Item = &'a str>,
) -> Result<ImportedHeaders, slang_rs::Error> {
    let mut definitions = HashMap::new();
    let mut timescales = HashMap::new();
    for name in names {
        let source = compilation
            .parameter_declarations_for(name)
            .map_err(|e| match e {
                slang_rs::LookupError::Extraction(error) => error,
                other => slang_rs::Error {
                    message: other.to_string(),
                    diagnostics: Vec::new(),
                },
            })?;
        if let Some(scale) = source.timescale {
            timescales.insert(name.to_owned(), scale);
        }
        if source.port_list.is_none() && source.body.is_empty() {
            continue;
        }
        let mut header = String::new();
        for import in source.imports {
            write!(header, " {import}").unwrap();
        }
        // A legacy body value declaration can use a package typedef imported by
        // the enclosing compilation unit. Carry that specific type into the stub.
        let mut invalid_alias = None;
        if source.port_list.is_none() {
            let mut aliases = IndexSet::new();
            for declaration in source.parameters.values() {
                if let Some((pkg, member)) = &declaration.package_type
                    && declaration.data_type.as_deref().is_some_and(|ty| {
                        let ty = ty.trim();
                        ty == member || ty.strip_prefix('\\').is_some_and(|ty| ty == member)
                    })
                {
                    aliases.insert((pkg, member));
                }
            }
            for (pkg, member) in aliases {
                if let Err(error) = check_identifier(pkg, "package name", name)
                    .and_then(|()| check_identifier(member, "type name", name))
                {
                    invalid_alias.get_or_insert(error);
                } else {
                    write!(header, " import {pkg}::{member};").unwrap();
                }
            }
        }
        let mut offset = 0;
        if let Some(port_list) = source.port_list {
            header.push(' ');
            offset = header.len();
            header.push_str(&port_list);
        }
        let mut parameters = IndexMap::new();
        for (name, declaration) in source.parameters {
            let (range, in_body) = if let Some(r) = declaration.port_list_initializer {
                (offset + r.start..offset + r.end, false)
            } else {
                (
                    declaration
                        .body_initializer
                        .expect("body parameter initializer position"),
                    true,
                )
            };
            parameters.insert(
                name,
                Initializer {
                    range,
                    in_body,
                    required: declaration.default.is_none(),
                },
            );
        }
        definitions.insert(
            name.to_owned(),
            SvParameterHeader {
                declarations: Declarations {
                    header,
                    body: source.body,
                    names: source.declaration_names,
                },
                parameters,
                state_count: source.parameter_count,
                invalid_alias,
            },
        );
    }
    Ok(ImportedHeaders {
        definitions,
        timescales,
    })
}

impl ImportedHeaders {
    /// Returns whether the selected overrides cover all value, local and type parameters.
    pub(super) fn covers_state(&self, name: &str, selected: usize) -> bool {
        self.definitions
            .get(name)
            .is_some_and(|header| header.state_count == selected)
    }
}

/// Select a declaration for each emitted module and verify the specializations that use it.
pub(super) fn plan(root: &ModDef) -> ParameterHeaders {
    let mut references = IndexMap::new();
    collect(&root.core, &mut references, &mut HashSet::new());
    let mut headers = IndexMap::new();
    let mut checks = Checks::default();
    let mut timescales = HashMap::new();
    let mut timed_header = false;
    for (name, definitions) in &references {
        let Some(emitted) = definitions.iter().find(|definition| {
            let core = definition.read();
            core.usage == Usage::EmitStubAndStop
                || (core.verilog_import.is_some() && core.usage == Usage::EmitDefinitionAndDescend)
        }) else {
            continue;
        };
        let (ports, root_body, owner_import, bindings) = {
            let core = emitted.read();
            (
                core.ports.clone(),
                Arc::ptr_eq(emitted, &root.core) && core.usage == Usage::EmitDefinitionAndDescend,
                core.verilog_import.clone(),
                core.parameters.clone(),
            )
        };
        for reference in definitions {
            check_interface(name, &ports, &reference.read().ports);
        }
        let source = owner_import.or_else(|| {
            definitions
                .iter()
                .find_map(|r| r.read().verilog_import.clone())
        });
        if let Some(import) = &source
            && let Some(scale) = import.headers.timescales.get(name)
        {
            timed_header |= import.headers.definitions.contains_key(name);
            timescales.insert(name.clone(), scale.clone());
        }
        let declarations = match source {
            Some(import) => checks.imported(
                name,
                &import,
                definitions,
                &ports,
                root_body.then_some(&bindings),
            ),
            None => Declarations {
                header: native_header(name, definitions, &bindings),
                body: String::new(),
                names: bindings.keys().cloned().collect(),
            },
        };
        if !declarations.header.is_empty()
            || !declarations.body.is_empty()
            || !declarations.names.is_empty()
        {
            headers.insert(name.clone(), declarations);
        }
    }
    checks.verify();
    if timed_header {
        for (name, definitions) in references {
            let scale = if let Some(scale) = timescales.get(&name) {
                scale.as_str()
            } else if definitions
                .iter()
                .all(|d| d.read().verilog_import.is_none())
                && definitions.iter().any(|d| {
                    matches!(
                        d.read().usage,
                        Usage::EmitStubAndStop | Usage::EmitDefinitionAndDescend
                    )
                })
            {
                // SV requires a timescale on every design unit when any has one.
                // Native structural modules have no time expressions of their own.
                "1ns / 1ns"
            } else {
                continue;
            };
            headers
                .entry(name)
                .or_default()
                .body
                .insert_str(0, &format!("  timeunit {scale};\n"));
        }
    }
    headers
}

/// Validate different modules in the same compilation; each specialization of a
/// given module must occupy its actual top-level name to preserve SV scope identity.
#[derive(Default)]
struct Checks {
    batches: IndexMap<*const super::ParserConfigOwned, Batch>,
}

struct Batch {
    config: Arc<super::ParserConfigOwned>,
    modules: IndexMap<String, Vec<Case>>,
}

struct Case {
    declaration: String,
    references: Vec<Core>,
}

impl Checks {
    /// Check each referenced instance against the emitted declaration in its owning environment.
    fn imported(
        &mut self,
        name: &str,
        owner: &Arc<ImportContext>,
        references: &[Core],
        ports: &IndexMap<String, IO>,
        root_bindings: Option<&Bindings>,
    ) -> Declarations {
        let empty = SvParameterHeader::default();
        let source = owner.headers.definitions.get(name).unwrap_or(&empty);
        for r in references {
            if let Some(other) = &r.read().verilog_import {
                let sibling = other.headers.definitions.get(name).unwrap_or(&empty);
                assert!(
                    source.parameters.keys().eq(sibling.parameters.keys())
                        && (owner.headers.definitions.contains_key(name)
                            == other.headers.definitions.contains_key(name)),
                    "emitted module {name} references disagree on their overridable value parameters"
                );
            }
        }
        if !owner.headers.definitions.contains_key(name) {
            assert!(
                references.iter().all(|r| r.read().parameters.is_empty()),
                "emitted module {name} has no overridable value parameters"
            );
            return Declarations::default();
        }
        let declarations = if let Some(bindings) = root_bindings {
            source.with_initializers(&expressions(bindings))
        } else {
            source.declarations.clone()
        };
        if let Some(error) = &source.invalid_alias {
            panic!("{error}");
        }
        check_declaration_identifiers(name, &declarations);
        let port_list = ports
            .iter()
            .map(|(n, io)| {
                let dir = match io {
                    IO::Input(_) => "input",
                    IO::Output(_) => "output",
                    IO::InOut(_) => "inout",
                };
                let range = if io.width() == 1 {
                    String::new()
                } else {
                    format!(" [{}:0]", io.width() - 1)
                };
                format!("{dir} wire{range} {}", identifier(n, "port name", name))
            })
            .collect::<Vec<_>>()
            .join(", ");
        // The probe file is private to this compilation. A directive also works for
        // sources parsed in Verilog-2005 mode, which has no module-local timeunit.
        let directive = if owner.headers.timescales.is_empty() {
            String::new()
        } else {
            let scale = owner
                .headers
                .timescales
                .get(name)
                .map_or("1ns / 1ns", String::as_str);
            format!("`timescale {scale}\n")
        };
        let candidates = self
            .batches
            .entry(Arc::as_ptr(&owner.config))
            .or_insert_with(|| Batch {
                config: owner.config.clone(),
                modules: IndexMap::new(),
            })
            .modules
            .entry(name.to_owned())
            .or_default();
        let mut cases = IndexMap::<Vec<(String, String)>, Vec<Core>>::new();
        for reference in references {
            let core = reference.read();
            let values = expressions(&core.parameters);
            assert!(
                values.keys().all(|n| source.parameters.contains_key(n)),
                "reference to {name} overrides a parameter missing from its emitted header"
            );
            if root_bindings.is_none() {
                // Check the optional defaults on the candidate only, keeping required bindings.
                cases
                    .entry(
                        values
                            .iter()
                            .filter(|(n, _)| source.parameters[*n].required)
                            .map(|(n, v)| (n.clone(), v.clone()))
                            .collect(),
                    )
                    .or_default();
            }
            let bindings = if root_bindings.is_some() {
                vec![]
            } else {
                values.into_iter().collect()
            };
            let refs = cases.entry(bindings).or_default();
            if core.verilog_import.is_some() {
                refs.push(reference.clone());
            }
        }
        for (bindings, references) in cases {
            // A top has no parent to supply instance arguments. The emitted literals
            // give its parameter initializers the same type inference as named bindings.
            let candidate;
            let decl = if bindings.is_empty() {
                &declarations
            } else {
                candidate = source.with_initializers(&bindings.into_iter().collect());
                &candidate
            };
            candidates.push(Case {
                declaration: format!(
                    "{directive}module {}{} ({port_list});\n{}\nendmodule\n",
                    identifier(name, "module name", name),
                    decl.header,
                    decl.body
                ),
                references,
            });
        }
        declarations
    }

    /// Compare the candidate in the real module's lexical and hierarchical scope.
    fn verify(self) {
        let mut originals = HashMap::new();
        for batch in self.batches.into_values() {
            // Each round can include one version of each module at its real top name.
            let mut rounds: Vec<Vec<(String, Case)>> = Vec::new();
            for (name, cases) in batch.modules {
                for (i, case) in cases.into_iter().enumerate() {
                    if i == rounds.len() {
                        rounds.push(vec![]);
                    }
                    rounds[i].push((name.clone(), case));
                }
            }
            // Most parameter declarations need only built-in SV types. Avoid reparsing
            // the source library for every specialization when recreating the same top.
            let mut standalone = (*batch.config).clone();
            standalone.sources.clear();
            standalone.libfiles.clear();
            standalone.libdirs.clear();
            let mut try_standalone = true;
            for (index, round) in rounds.into_iter().enumerate() {
                let mut text = String::new();
                let mut tops = Vec::new();
                for (name, case) in &round {
                    text.push_str(&case.declaration);
                    tops.push(name.as_str());
                }
                // The first round includes defaults that might have no original instance.
                // For later specializations, accept the standalone result only if it proves
                // the same state. Resolve diagnostics and mismatches in the full environment.
                if index > 0
                    && try_standalone
                    && round.iter().all(|(_, case)| !case.references.is_empty())
                {
                    if let Ok(compiled) = standalone.compile_tops(&tops, &[], Some(&text))
                        && verify_cases(&compiled, &round, &batch.config, &mut originals, true)
                            .is_ok()
                    {
                        continue;
                    }
                    try_standalone = false;
                }
                let compiled = batch
                    .config
                    .compile_tops(&tops, &[], Some(&text))
                    .unwrap_or_else(|e| {
                        panic!(
                            "cannot reproduce value parameter declarations of {}: {e}",
                            tops.join(", ")
                        )
                    });
                verify_cases(&compiled, &round, &batch.config, &mut originals, false)
                    .unwrap_or_else(|e| panic!("{e}"));
            }
        }
    }
}

/// Compare each candidate to its source in the original module's elaboration context.
fn verify_cases(
    compiled: &Compilation,
    cases: &[(String, Case)],
    owner: &Arc<super::ParserConfigOwned>,
    originals: &mut HashMap<*const super::ParserConfigOwned, Compilation>,
    standalone: bool,
) -> Result<(), String> {
    for (name, case) in cases {
        if case.references.is_empty() {
            continue;
        }
        let actual = compiled
            .parameter_state_for(name)
            .map_err(|e| format!("cannot read parameter state of {name}: {e}"))?;
        for reference in &case.references {
            let core = reference.read();
            let import = core.verilog_import.as_ref().unwrap();
            let original;
            let expected = if let Some(specialization) = &core.specialization {
                specialization.state(&import.config, name)
            } else {
                // Preserve all original roots, including peers named by bound or discarded logic.
                let compilation =
                    originals
                        .entry(Arc::as_ptr(&import.config))
                        .or_insert_with(|| {
                            import.config.original().unwrap_or_else(|e| {
                                panic!("cannot reproduce original module {name}: {e}")
                            })
                        });
                original = compilation
                    .parameter_state_for(name)
                    .map_err(|e| format!("cannot read parameter state of {name}: {e}"))?;
                &original
            };
            check_state(
                name,
                expected,
                &actual,
                standalone || !Arc::ptr_eq(&import.config, owner),
            )?;
        }
    }
    Ok(())
}

impl SvParameterHeader {
    /// Replace requested initializers, working back to front in each source fragment.
    fn with_initializers(&self, values: &IndexMap<String, String>) -> Declarations {
        let mut declarations = self.declarations.clone();
        for (param, value) in values
            .iter()
            .sorted_by_key(|(n, _)| std::cmp::Reverse(self.parameters[*n].range.start))
        {
            let init = &self.parameters[param];
            let text = if init.in_body {
                &mut declarations.body
            } else {
                &mut declarations.header
            };
            text.replace_range(init.range.clone(), &format!(" = {value}"));
        }
        declarations
    }
}

/// Matching errors from two distinct sources cannot establish equivalent parameter types.
fn check_state(
    name: &str,
    expected: &ModuleParameterState,
    actual: &ModuleParameterState,
    independent: bool,
) -> Result<(), String> {
    if expected.types.keys().ne(actual.types.keys()) {
        return Err(format!(
            "emitted module {name} references disagree on their type parameters"
        ));
    }
    for (n, ty) in &expected.types {
        if independent && ty.is_err() {
            return Err(format!(
                "cannot verify the resolved type of {name}.{n} across imports"
            ));
        }
        if *ty != actual.types[n] {
            return Err(format!(
                "emitted declaration changes the resolved type of {name}.{n}: expected {ty:?}, got {:?}",
                actual.types[n]
            ));
        }
    }
    if expected.values.len() != actual.values.len() {
        return Err(format!(
            "emitted module {name} references disagree on their value parameters"
        ));
    }
    for (n, value) in &expected.values {
        let Some(observed) = actual.values.get(n) else {
            return Err(format!(
                "emitted module {name} is missing value parameter {n}"
            ));
        };
        if independent
            && value.ty.is_err()
            && !matches!(value.value, slang_rs::ConstantValue::String(_))
        {
            return Err(format!(
                "cannot verify the resolved type of {name}.{n} across imports"
            ));
        }
        if value != observed {
            return Err(format!(
                "emitted declaration changes the value or type of {name}.{n}: expected {value:?}, got {observed:?}"
            ));
        }
    }
    Ok(())
}

/// Native parameterized interfaces carry explicit values and need no SV source.
fn native_header(name: &str, references: &[Core], bindings: &Bindings) -> String {
    for reference in references {
        for parameter in reference.read().parameters.keys() {
            assert!(
                bindings.contains_key(parameter),
                "no declaration for native parameter {name}.{parameter}"
            );
        }
    }
    if bindings.is_empty() {
        return String::new();
    }
    let declarations = expressions(bindings)
        .into_iter()
        .map(|(n, v)| {
            format!(
                "  parameter {} = {v}",
                identifier(&n, "parameter name", name)
            )
        })
        .collect::<Vec<_>>();
    format!(" #(\n{}\n)", declarations.join(",\n"))
}

/// Check a name before inserting it into generated SystemVerilog.
fn identifier<'a>(name: &'a str, role: &str, module: &str) -> &'a str {
    check_identifier(name, role, module).unwrap_or_else(|error| panic!("{error}"));
    name
}

/// Validate copied parameter syntax while ignoring comments, strings, and SV system names.
fn check_declaration_identifiers(module: &str, declarations: &Declarations) {
    for name in &declarations.names {
        identifier(name, "parameter declaration name", module);
    }
    static TOKENS: OnceLock<regex::Regex> = OnceLock::new();
    let tokens = TOKENS.get_or_init(|| {
        regex::Regex::new(
            r#"(?s)//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|\\[^\s]+|\$[A-Za-z_][A-Za-z0-9_$]*|[A-Za-z_][A-Za-z0-9_$]*"#,
        )
        .unwrap()
    });
    for text in [&declarations.header, &declarations.body] {
        for token in tokens.find_iter(text).map(|token| token.as_str()) {
            let first = token.as_bytes()[0];
            if first == b'\\'
                || ((first.is_ascii_alphabetic() || first == b'_') && token.contains('$'))
            {
                identifier(token, "parameter declaration identifier", module);
            }
        }
    }
}

/// Collect distinct definitions referenced by modules whose bodies will be emitted.
fn collect(
    core: &Core,
    references: &mut IndexMap<String, Vec<Core>>,
    visited: &mut HashSet<*const RwLock<ModDefCore>>,
) {
    if !visited.insert(Arc::as_ptr(core)) {
        return;
    }
    let definition = core.read();
    references
        .entry(definition.name.clone())
        .or_default()
        .push(core.clone());
    if definition.usage == Usage::EmitDefinitionAndDescend {
        for child in definition.instances.values() {
            collect(child, references, visited);
        }
    }
}

/// Ensure a fixed emitted interface fits every instance using that module name.
fn check_interface(name: &str, expected: &IndexMap<String, IO>, actual: &IndexMap<String, IO>) {
    for (port, io) in expected {
        let other = actual
            .get(port)
            .unwrap_or_else(|| panic!("emitted module {name} reference is missing port {port}"));
        assert_eq!(
            io.width(),
            other.width(),
            "emitted module {name} port {port} width mismatch"
        );
        assert_eq!(
            std::mem::discriminant(io),
            std::mem::discriminant(other),
            "emitted module {name} port {port} direction mismatch"
        );
    }
    assert_eq!(
        expected.len(),
        actual.len(),
        "emitted module {name} port count mismatch"
    );
}
