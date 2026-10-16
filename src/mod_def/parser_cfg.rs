// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU32;

use crate::Source;

use slang_rs::{
    Compilation, DiagnosticOptions, DiagnosticOverride, DiagnosticSeverity, LanguageVersion,
    SlangConfig, TimeScale, TranslateOff, WarningPolicy,
};

// Imported modules outlive the borrowed ParserConfig, so we retain compiler
// settings and share text buffers for future parameterization.
#[derive(Debug)]
pub(crate) struct ParserConfigOwned {
    pub(crate) sources: Vec<Source>,
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
    pub(crate) analysis_enabled: bool,
    pub(crate) lint_only: bool,
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
}

/// Verilog import inputs and options. Compiler settings have the same meaning
/// and defaults as [`SlangConfig`].
#[derive(Clone, Copy, Debug)]
pub struct ParserConfig<'a> {
    /// Files and text buffers, in compilation order. Text storage is
    /// shared when an imported module retains these sources.
    pub sources: &'a [Source],
    pub tops: &'a [&'a str],
    pub incdirs: &'a [&'a str],
    pub defines: &'a [(&'a str, &'a str)],
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
    pub analysis_enabled: bool,
    pub lint_only: bool,
    pub relax_enum_conversions: bool,
    pub allow_use_before_declare: bool,
    pub allow_toplevel_interface_ports: bool,
    pub num_threads: Option<NonZeroU32>,
    pub translate_off: &'a [TranslateOff<'a>],
    pub diagnostics: DiagnosticOptions<'a>,
    pub skip_unsupported: bool,
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
            analysis_enabled: config.analysis_enabled,
            lint_only: config.lint_only,
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
            analysis_enabled: self.analysis_enabled,
            lint_only: self.lint_only,
            relax_enum_conversions: self.relax_enum_conversions,
            allow_use_before_declare: self.allow_use_before_declare,
            allow_toplevel_interface_ports: self.allow_toplevel_interface_ports,
            num_threads: self.num_threads,
            translate_off: self.translate_off,
            diagnostics: self.diagnostics,
            skip_unsupported_ports: self.skip_unsupported,
            skip_unsupported_parameters: self.skip_unsupported,
        };
        Compilation::new(&cfg)
    }

    pub(crate) fn to_owned_config(self) -> ParserConfigOwned {
        ParserConfigOwned {
            sources: self.sources.to_vec(),
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
            analysis_enabled: self.analysis_enabled,
            lint_only: self.lint_only,
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
        }
    }
}

impl ParserConfigOwned {
    pub(crate) fn compile(
        &self,
        module_name: &str,
        parameters: &[(String, String)],
    ) -> Result<Compilation, slang_rs::Error> {
        ParserConfig {
            sources: &self.sources,
            tops: &[module_name],
            incdirs: &borrowed_strings(&self.incdirs),
            defines: &borrowed_pairs(&self.defines),
            parameters: &borrowed_pairs(parameters),
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
            analysis_enabled: self.analysis_enabled,
            lint_only: self.lint_only,
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
            // Skip unsupported ports and parameter/localparam metadata.
            // E.g. `input real voltage` is omitted from the resulting ModDef
            // instead of panicking. Needed by parameterize() when the module has
            // parameters with unsupported types, even if the user does not override them.
            skip_unsupported: true,
            include_hierarchy: false,
        }
        .compile()
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
