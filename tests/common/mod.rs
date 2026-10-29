// SPDX-License-Identifier: Apache-2.0

use topstitch::{ModDef, ParserConfig, Source, SourceText, VerilogLibrary};

/// Import a single top from an inline fixture with the default parser settings.
pub(crate) fn module_from_text(name: &str, verilog: impl Into<SourceText>) -> ModDef {
    module_from_config(
        name,
        &ParserConfig {
            sources: &[Source::text(verilog)],
            ..Default::default()
        },
    )
}

/// Compile a fixture with its specified parser options and import the named top.
#[track_caller]
pub(crate) fn module_from_config(name: &str, config: &ParserConfig) -> ModDef {
    VerilogLibrary::from_config(config)
        .expect("fixture should compile")
        .module(name)
        .expect("fixture module should import")
        .unwrap_or_else(|| panic!("missing top module {name}"))
}
