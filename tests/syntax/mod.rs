// SPDX-License-Identifier: Apache-2.0

use crate::common::module_from_config;
use topstitch::{ModDef, ParserConfig, Source, SourceText};

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
