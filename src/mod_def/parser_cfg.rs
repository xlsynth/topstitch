// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU32;

use crate::Source;

use slang_rs::{
    CheckLevel, Compilation, DiagnosticOptions, DiagnosticOverride, DiagnosticSeverity,
    HierarchyMode, LanguageVersion, SlangConfig, TimeScale, TranslateOff, WarningPolicy,
};

// Imported modules outlive the borrowed ParserConfig, so we retain compiler
// settings and share text buffers for future parameterization.
#[derive(Clone, Debug)]
pub(crate) struct ParserConfigOwned {
    pub(crate) sources: Vec<Source>,
    tops: Vec<String>,
    parameters: Vec<(String, String)>,
    pub(crate) incdirs: Vec<String>,
    pub(crate) defines: Vec<(String, String)>,
    pub(crate) libfiles: Vec<String>,
    pub(crate) libdirs: Vec<String>,
    pub(crate) libexts: Vec<String>,
    pub(crate) dir_prefixes: Vec<String>,
    pub(crate) exclude_extensions: Vec<String>,
    pub(crate) ignore_unknown_modules: bool,
    pub(crate) ignore_protected: bool,
    pub(crate) enable_legacy_protect: bool,
    pub(crate) timescale: Option<TimeScale>,
    pub(crate) language_version: LanguageVersion,
    pub(crate) single_unit: bool,
    pub(crate) libraries_inherit_macros: bool,
    pub(crate) check_level: CheckLevel,
    pub(crate) hierarchy_mode: HierarchyMode,
    pub(crate) relax_enum_conversions: bool,
    pub(crate) allow_use_before_declare: bool,
    pub(crate) allow_toplevel_interface_ports: bool,
    pub(crate) num_threads: Option<NonZeroU32>,
    pub(crate) translate_off: Vec<(String, String, String)>,
    warnings: WarningPolicy,
    warnings_as_errors: bool,
    error_limit: u32,
    diagnostic_overrides: Vec<(String, DiagnosticSeverity)>,
    pub(crate) skip_unsupported: bool,
    pub(crate) include_hierarchy: bool,
}

/// Verilog import inputs and options. Imports default to [`CheckLevel::Semantic`].
#[derive(Clone, Copy, Debug)]
pub struct ParserConfig<'a> {
    /// Files and text buffers, in compilation order. Text storage is
    /// shared when an imported module retains these sources.
    pub sources: &'a [Source],
    pub tops: &'a [&'a str],
    pub incdirs: &'a [&'a str],
    pub defines: &'a [(&'a str, &'a str)],
    /// Explicit SV parameter overrides. Imported modules own the evaluated bindings.
    /// Module and parameter names must be ordinary unescaped identifiers. Descendant
    /// overrides cannot be emitted on an instance.
    /// Enum bindings require a scalar typedef accessible through a package; compilation-unit
    /// typedefs and anonymous enums cannot be named reliably from a separately emitted module.
    pub parameters: &'a [(&'a str, &'a str)],
    pub libfiles: &'a [&'a str],
    pub libdirs: &'a [&'a str],
    pub libexts: &'a [&'a str],
    pub dir_prefixes: &'a [&'a str],
    pub exclude_extensions: &'a [&'a str],
    pub ignore_unknown_modules: bool,
    pub ignore_protected: bool,
    pub enable_legacy_protect: bool,
    pub timescale: Option<TimeScale>,
    pub language_version: LanguageVersion,
    pub single_unit: bool,
    pub libraries_inherit_macros: bool,
    /// Checks to run during import and when parameterizing the imported module.
    /// The default elaborates the design to resolve ports, parameters, and types.
    /// Select [`CheckLevel::Analysis`] to also check behavior such as conflicting drivers.
    pub check_level: CheckLevel,
    /// How Slang selects hierarchy roots and handles inputs without a complete design.
    pub hierarchy_mode: HierarchyMode,
    pub relax_enum_conversions: bool,
    pub allow_use_before_declare: bool,
    pub allow_toplevel_interface_ports: bool,
    pub num_threads: Option<NonZeroU32>,
    pub translate_off: &'a [TranslateOff<'a>],
    /// Warning policy, severity overrides, and error limit for the selected checks.
    pub diagnostics: DiagnosticOptions<'a>,
    pub skip_unsupported: bool,
    /// Import an editable hierarchy view with child instance paths. This view does not
    /// extract internal logic, ports on children, or connections. It is emitted by default;
    /// use [`crate::Usage::EmitNothingAndStop`] to instantiate the original external module.
    /// Specializing the view re-elaborates the source and rebuilds its hierarchy.
    pub include_hierarchy: bool,
}

impl Default for ParserConfig<'_> {
    fn default() -> Self {
        let config = SlangConfig::default();
        Self {
            sources: &[],
            tops: config.tops,
            incdirs: config.incdirs,
            defines: config.defines,
            parameters: config.parameters,
            libfiles: config.libfiles,
            libdirs: config.libdirs,
            libexts: config.libexts,
            dir_prefixes: config.dir_prefixes,
            exclude_extensions: config.exclude_extensions,
            ignore_unknown_modules: config.ignore_unknown_modules,
            ignore_protected: config.ignore_protected,
            enable_legacy_protect: config.enable_legacy_protect,
            timescale: config.timescale,
            language_version: config.language_version,
            single_unit: config.single_unit,
            libraries_inherit_macros: config.libraries_inherit_macros,
            check_level: CheckLevel::Semantic,
            hierarchy_mode: config.hierarchy_mode,
            relax_enum_conversions: config.relax_enum_conversions,
            allow_use_before_declare: config.allow_use_before_declare,
            allow_toplevel_interface_ports: config.allow_toplevel_interface_ports,
            num_threads: config.num_threads,
            translate_off: config.translate_off,
            diagnostics: config.diagnostics,
            skip_unsupported: false,
            include_hierarchy: false,
        }
    }
}

impl ParserConfig<'_> {
    /// Compile these inputs with Slang. Source descriptors borrow the retained
    /// text; creating them does not copy source buffers.
    pub fn compile(&self) -> Result<Compilation, slang_rs::Error> {
        self.compile_with(None)
    }

    /// Elaborate original sources or replacement tops against their package environment.
    fn compile_with(&self, modules: Option<&str>) -> Result<Compilation, slang_rs::Error> {
        if let Some(&(path, _)) = self.parameters.iter().find(|(path, _)| path.contains('=')) {
            return Err(slang_rs::Error {
                message: format!(
                    "Cannot use '=' in SV parameter override path {path:?}: Slang uses it to separate the name and value"
                ),
                diagnostics: Vec::new(),
            });
        }
        let sources: Vec<_> = self.sources.iter().map(Source::as_slang_source).collect();
        let cfg = SlangConfig {
            sources: &sources,
            tops: self.tops,
            incdirs: self.incdirs,
            defines: self.defines,
            parameters: self.parameters,
            libfiles: self.libfiles,
            libdirs: self.libdirs,
            libexts: self.libexts,
            dir_prefixes: self.dir_prefixes,
            exclude_extensions: self.exclude_extensions,
            ignore_unknown_modules: self.ignore_unknown_modules,
            ignore_protected: self.ignore_protected,
            enable_legacy_protect: self.enable_legacy_protect,
            timescale: self.timescale,
            language_version: self.language_version,
            single_unit: self.single_unit,
            libraries_inherit_macros: self.libraries_inherit_macros,
            check_level: self.check_level,
            hierarchy_mode: self.hierarchy_mode,
            relax_enum_conversions: self.relax_enum_conversions,
            allow_use_before_declare: self.allow_use_before_declare,
            allow_toplevel_interface_ports: self.allow_toplevel_interface_ports,
            num_threads: self.num_threads,
            translate_off: self.translate_off,
            diagnostics: self.diagnostics,
            skip_unsupported_ports: self.skip_unsupported,
            skip_unsupported_parameters: self.skip_unsupported,
        };
        match modules {
            Some(text) => Compilation::with_replacement_tops(&cfg, text),
            None => Compilation::new(&cfg),
        }
    }

    pub(crate) fn to_owned_config(self) -> ParserConfigOwned {
        ParserConfigOwned {
            sources: self.sources.to_vec(),
            tops: owned_strings(self.tops),
            parameters: owned_pairs(self.parameters),
            incdirs: owned_strings(self.incdirs),
            defines: owned_pairs(self.defines),
            libfiles: owned_strings(self.libfiles),
            libdirs: owned_strings(self.libdirs),
            libexts: owned_strings(self.libexts),
            dir_prefixes: owned_strings(self.dir_prefixes),
            exclude_extensions: owned_strings(self.exclude_extensions),
            ignore_unknown_modules: self.ignore_unknown_modules,
            ignore_protected: self.ignore_protected,
            enable_legacy_protect: self.enable_legacy_protect,
            timescale: self.timescale,
            language_version: self.language_version,
            single_unit: self.single_unit,
            libraries_inherit_macros: self.libraries_inherit_macros,
            check_level: self.check_level,
            hierarchy_mode: self.hierarchy_mode,
            relax_enum_conversions: self.relax_enum_conversions,
            allow_use_before_declare: self.allow_use_before_declare,
            allow_toplevel_interface_ports: self.allow_toplevel_interface_ports,
            num_threads: self.num_threads,
            translate_off: self
                .translate_off
                .iter()
                .map(|words| {
                    (
                        words.common.to_owned(),
                        words.start.to_owned(),
                        words.end.to_owned(),
                    )
                })
                .collect(),
            warnings: self.diagnostics.warnings,
            warnings_as_errors: self.diagnostics.warnings_as_errors,
            error_limit: self.diagnostics.error_limit,
            diagnostic_overrides: self
                .diagnostics
                .overrides
                .iter()
                .map(|setting| (setting.name.to_owned(), setting.severity))
                .collect(),
            skip_unsupported: self.skip_unsupported,
            include_hierarchy: self.include_hierarchy,
        }
    }
}

impl ParserConfigOwned {
    pub(crate) fn compile(
        &self,
        module_name: &str,
        parameters: &[(String, String)],
    ) -> Result<Compilation, slang_rs::Error> {
        let error = |message| slang_rs::Error {
            message,
            diagnostics: Vec::new(),
        };
        if !parameters.is_empty() {
            super::net_names::check_identifier(module_name, "module name", module_name)
                .map_err(error)?;
        }
        for (name, _) in parameters {
            super::net_names::check_identifier(name, "parameter name", module_name)
                .map_err(error)?;
        }
        let overrides = borrowed_pairs(parameters);
        self.compile_tops(&[module_name], &overrides, None)
    }

    /// Reproduce the exact original top set, command-line overrides, and source configuration.
    pub(super) fn original(&self) -> Result<Compilation, slang_rs::Error> {
        self.compile_tops(
            &borrowed_strings(&self.tops),
            &borrowed_pairs(&self.parameters),
            None,
        )
    }

    /// Elaborate a batch of source or generated tops in this import environment.
    pub(super) fn compile_tops(
        &self,
        tops: &[&str],
        parameters: &[(&str, &str)],
        modules: Option<&str>,
    ) -> Result<Compilation, slang_rs::Error> {
        ParserConfig {
            sources: &self.sources,
            tops,
            incdirs: &borrowed_strings(&self.incdirs),
            defines: &borrowed_pairs(&self.defines),
            parameters,
            libfiles: &borrowed_strings(&self.libfiles),
            libdirs: &borrowed_strings(&self.libdirs),
            libexts: &borrowed_strings(&self.libexts),
            dir_prefixes: &borrowed_strings(&self.dir_prefixes),
            exclude_extensions: &borrowed_strings(&self.exclude_extensions),
            ignore_unknown_modules: self.ignore_unknown_modules,
            ignore_protected: self.ignore_protected,
            enable_legacy_protect: self.enable_legacy_protect,
            timescale: self.timescale,
            language_version: self.language_version,
            single_unit: self.single_unit,
            libraries_inherit_macros: self.libraries_inherit_macros,
            check_level: if modules.is_some() {
                CheckLevel::Semantic
            } else {
                self.check_level
            },
            hierarchy_mode: self.hierarchy_mode,
            relax_enum_conversions: self.relax_enum_conversions,
            allow_use_before_declare: self.allow_use_before_declare,
            allow_toplevel_interface_ports: self.allow_toplevel_interface_ports,
            num_threads: self.num_threads,
            translate_off: &self
                .translate_off
                .iter()
                .map(|(common, start, end)| TranslateOff { common, start, end })
                .collect::<Vec<_>>(),
            diagnostics: DiagnosticOptions {
                warnings: self.warnings,
                warnings_as_errors: self.warnings_as_errors,
                error_limit: self.error_limit,
                overrides: &self
                    .diagnostic_overrides
                    .iter()
                    .map(|(name, severity)| DiagnosticOverride {
                        name,
                        severity: *severity,
                    })
                    .collect::<Vec<_>>(),
            },
            // Match the existing partial-interface policy for re-elaboration.
            skip_unsupported: true,
            include_hierarchy: false,
        }
        .compile_with(modules)
    }
}

fn borrowed_strings(values: &[String]) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}

fn borrowed_pairs(values: &[(String, String)]) -> Vec<(&str, &str)> {
    values
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect()
}

fn owned_strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn owned_pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}
