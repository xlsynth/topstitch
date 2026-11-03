// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::{ModDef, Package, PackageView, ParserConfig};
use slang_rs::{Compilation, Error};

/// One compilation shared by module and package queries. Imported modules and
/// package snapshots are owned; package views borrow this library.
///
/// ```
/// use topstitch::{PackageLookup, ParserConfig, Source, VerilogLibrary};
///
/// let sources = [Source::text(
///     "package sizes; parameter W = 4; endpackage
///      module sink(input [sizes::W-1:0] data); endmodule",
/// )];
/// let library = VerilogLibrary::from_config(&ParserConfig {
///     sources: &sources, ..Default::default()
/// })?;
/// let sink = library.module("sink")?.unwrap();
/// let sizes = library.package("sizes")?.unwrap();
/// assert_eq!(sink.get_port("data").io().width(), usize::try_from(sizes.parameter("W")?.unwrap().as_ref())?);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct VerilogLibrary<'a> {
    compilation: Compilation,
    config: ParserConfig<'a>,
}

impl<'a> VerilogLibrary<'a> {
    /// Compile the sources once, without constructing any modules or packages.
    pub fn from_config(config: &ParserConfig<'a>) -> Result<Self, Error> {
        Ok(Self {
            compilation: config.compile()?,
            config: *config,
        })
    }

    /// Names of the elaborated top modules available for import.
    pub fn module_names(&self) -> Result<Vec<String>, Error> {
        self.compilation.top_names()
    }

    /// Import one elaborated top and its optional hierarchy, or None if absent.
    ///
    /// Panics if the module contains ports that TopStitch cannot represent
    /// and `skip_unsupported` is disabled.
    pub fn module(&self, name: &str) -> Result<Option<ModDef>, Error> {
        ModDef::from_compilation(name, &self.config, &self.compilation)
    }

    /// Import every elaborated top and its optional hierarchy. The same
    /// unsupported-port policy applies as for [`Self::module`].
    pub fn modules(&self) -> Result<Vec<ModDef>, Error> {
        ModDef::all_from_compilation(&self.config, &self.compilation)
    }

    /// List user packages without extracting constants or types.
    pub fn package_names(&self) -> Result<Vec<String>, Error> {
        self.compilation.package_names()
    }

    /// Open a live package view without evaluating its members, or return None if absent.
    pub fn package(&self, name: &str) -> Result<Option<PackageView<'_>>, Error> {
        self.compilation.package(name)
    }

    /// Snapshot every user package into owned Rust data.
    pub fn packages(&self) -> Result<HashMap<String, Package>, Error> {
        self.compilation.packages()
    }
}
