// SPDX-License-Identifier: Apache-2.0

use slang_rs::SlangConfig;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ParserConfigOwnedKey {
    pub(crate) sources: Vec<String>,
    pub(crate) tops: Vec<String>,
    pub(crate) incdirs: Vec<String>,
    pub(crate) defines: Vec<(String, String)>,
    pub(crate) parameters: Vec<(String, String)>,
    pub(crate) libfiles: Vec<String>,
    pub(crate) libdirs: Vec<String>,
    pub(crate) libexts: Vec<String>,
    pub(crate) ignore_unknown_modules: bool,
    pub(crate) ignore_protected: bool,
    pub(crate) timescale: Option<String>,
    pub(crate) skip_unsupported: bool,
    pub(crate) include_hierarchy: bool,
    pub(crate) extra_arguments: Vec<String>,
}

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
    pub fn to_slang_config(&self) -> SlangConfig<'_> {
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

    pub(crate) fn to_owned_key(&self) -> ParserConfigOwnedKey {
        ParserConfigOwnedKey {
            sources: self.sources.iter().map(|s| (*s).to_string()).collect(),
            tops: self.tops.iter().map(|s| (*s).to_string()).collect(),
            incdirs: self.incdirs.iter().map(|s| (*s).to_string()).collect(),
            defines: self
                .defines
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            parameters: self
                .parameters
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            libfiles: self.libfiles.iter().map(|s| (*s).to_string()).collect(),
            libdirs: self.libdirs.iter().map(|s| (*s).to_string()).collect(),
            libexts: self.libexts.iter().map(|s| (*s).to_string()).collect(),
            ignore_unknown_modules: self.ignore_unknown_modules,
            ignore_protected: self.ignore_protected,
            timescale: self.timescale.map(|t| t.to_string()),
            skip_unsupported: self.skip_unsupported,
            include_hierarchy: self.include_hierarchy,
            extra_arguments: self
                .extra_arguments
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }
}
