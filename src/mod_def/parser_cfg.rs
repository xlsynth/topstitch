// SPDX-License-Identifier: Apache-2.0

use slang_rs::SlangConfig;

#[derive(Debug)]
pub struct ParserConfig<'a> {
    pub sources: &'a [&'a str],
    pub tops: &'a [&'a str],
    pub incdirs: &'a [&'a str],
    pub defines: &'a [(&'a str, &'a str)],
    pub parameters: &'a [(&'a str, &'a str)],
    pub libfiles: &'a [&'a str],
    pub libdirs: &'a [&'a str],
    pub libexts: &'a [&'a str],
    pub ignore_unknown_modules: bool,
    pub ignore_protected: bool,
    pub timescale: Option<&'a str>,
    pub skip_unsupported: bool,
    pub include_hierarchy: bool,
    pub extra_arguments: &'a [&'a str],
}

impl Default for ParserConfig<'_> {
    fn default() -> Self {
        ParserConfig {
            sources: &[],
            tops: &[],
            incdirs: &[],
            defines: &[],
            parameters: &[],
            libfiles: &[],
            libdirs: &[],
            libexts: &[],
            ignore_unknown_modules: true,
            ignore_protected: true,
            timescale: None,
            skip_unsupported: false,
            include_hierarchy: false,
            extra_arguments: &[],
        }
    }
}

impl ParserConfig<'_> {
    pub fn to_slang_config(&self) -> SlangConfig {
        SlangConfig {
            sources: self.sources,
            tops: self.tops,
            incdirs: self.incdirs,
            defines: self.defines,
            parameters: self.parameters,
            libfiles: self.libfiles,
            libdirs: self.libdirs,
            libexts: self.libexts,
            ignore_unknown_modules: self.ignore_unknown_modules,
            ignore_protected: self.ignore_protected,
            timescale: self.timescale,
            extra_arguments: self.extra_arguments,
        }
    }
}
