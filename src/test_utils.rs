// SPDX-License-Identifier: Apache-2.0

use crate::{ModDef, ParserConfig, Source, VerilogLibrary};

/// Compile the fixture sources afresh and import the named top.
#[track_caller]
pub(crate) fn module_from_sources(name: &str, sources: &[Source]) -> ModDef {
    VerilogLibrary::from_config(&ParserConfig {
        sources,
        ..Default::default()
    })
    .expect("fixture should compile")
    .module(name)
    .expect("fixture module should import")
    .unwrap_or_else(|| panic!("missing top module {name}"))
}
