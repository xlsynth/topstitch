// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::{ModDef, Package, ParserConfig};
use slang_rs::{Compilation, Error};

/// One compiled set of Verilog sources. Queries share the compilation, but
/// only snapshot the requested declarations. The returned objects are owned;
/// drop the library after importing everything you need. Each query returns a
/// fresh snapshot, so keep it if you need it more than once.
///
/// ```
/// use topstitch::{ParserConfig, Source, VerilogLibrary};
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
/// assert_eq!(sink.get_port("data").io().width(), usize::try_from(&sizes["W"])?);
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

    /// Extract the constants and types of one package, or None if absent.
    pub fn package(&self, name: &str) -> Result<Option<Package>, Error> {
        Ok(self.compilation.package(name)?.map(Into::into))
    }

    /// Extract the constants and types of every user package.
    pub fn packages(&self) -> Result<HashMap<String, Package>, Error> {
        Ok(self
            .compilation
            .packages()?
            .into_iter()
            .map(|(name, package)| (name, package.into()))
            .collect())
    }
}
