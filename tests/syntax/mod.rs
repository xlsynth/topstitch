// SPDX-License-Identifier: Apache-2.0

use topstitch::{ModDef, ParserConfig, Source, SourceText, VerilogLibrary};

mod enums;
mod import;
mod inout;
mod params;
mod protected;

/// Import a syntax fixture with unresolved module references treated as errors.
fn strict_module_from_text(name: &str, verilog: impl Into<SourceText>) -> ModDef {
    module_from_config(
        name,
        &ParserConfig {
            sources: &[Source::text(verilog)],
            ignore_unknown_modules: false,
            ..Default::default()
        },
    )
}

/// Compile a fixture with its specified parser options and import the named top.
#[track_caller]
fn module_from_config(name: &str, config: &ParserConfig) -> ModDef {
    VerilogLibrary::from_config(config)
        .expect("fixture should compile")
        .module(name)
        .expect("fixture module should import")
}
