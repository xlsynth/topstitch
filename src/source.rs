// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

/// Verilog text retained for import and subsequent parameterization.
///
/// Passing a [`String`] transfers its existing buffer; passing an [`Arc<String>`]
/// or [`Arc<str>`] shares its buffer. Borrowed text is copied once. Cloning this
/// value shares the text without copying it.
#[derive(Clone, Debug)]
pub struct SourceText(TextStorage);

#[derive(Clone, Debug)]
enum TextStorage {
    String(Arc<String>),
    Slice(Arc<str>),
}

impl SourceText {
    /// Borrow the retained source text.
    pub fn as_str(&self) -> &str {
        match &self.0 {
            TextStorage::String(text) => text.as_str(),
            TextStorage::Slice(text) => text,
        }
    }
}

impl AsRef<str> for SourceText {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<String> for SourceText {
    fn from(text: String) -> Self {
        Self(TextStorage::String(Arc::new(text)))
    }
}

impl From<&str> for SourceText {
    fn from(text: &str) -> Self {
        Self(TextStorage::Slice(Arc::from(text)))
    }
}

impl From<&String> for SourceText {
    fn from(text: &String) -> Self {
        Self::from(text.as_str())
    }
}

impl From<Arc<String>> for SourceText {
    fn from(text: Arc<String>) -> Self {
        Self(TextStorage::String(text))
    }
}

impl From<Arc<str>> for SourceText {
    fn from(text: Arc<str>) -> Self {
        Self(TextStorage::Slice(text))
    }
}

/// A source file or source text held in memory.
///
/// Inputs are processed in the order supplied in [`crate::ParserConfig::sources`].
/// Text is retained for later parameterization, not appended to emitted Verilog.
#[derive(Clone, Debug)]
pub enum Source {
    /// A file path or pattern, resolved and loaded by Slang.
    File(String),
    /// Source text with a buffer name assigned by Slang.
    Text(SourceText),
    /// Named source text to parse as a compilation input. The name is used for
    /// diagnostics and relative include resolution; no file is written there.
    /// An empty name lets Slang assign a buffer name.
    NamedText { name: String, text: SourceText },
}

impl Source {
    /// Refer to a source file or pattern without reading it yet.
    pub fn file(path: impl Into<String>) -> Self {
        Self::File(path.into())
    }

    /// Supply source text without a filename or temporary file. Slang assigns
    /// a buffer name for diagnostics.
    ///
    /// Owned strings and shared buffers are retained without copying their text.
    /// Use [`Source::named_text`] to provide a name for diagnostics or relative
    /// include resolution.
    pub fn text(text: impl Into<SourceText>) -> Self {
        Self::Text(text.into())
    }

    /// Supply source text with a logical filename for diagnostics and relative
    /// include resolution. No file is created at that path.
    ///
    /// Owned strings and shared buffers are retained without copying their text.
    pub fn named_text(name: impl Into<String>, text: impl Into<SourceText>) -> Self {
        Self::NamedText {
            name: name.into(),
            text: text.into(),
        }
    }

    pub(crate) fn as_slang_source(&self) -> slang_rs::Source<'_> {
        match self {
            Self::File(path) => slang_rs::Source::file(path),
            Self::Text(text) => slang_rs::Source::text(text.as_str()),
            Self::NamedText { name, text } => slang_rs::Source::named_text(name, text.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModDef, ParserConfig};

    // Unnamed buffers must all be compiled and retained for later parameterization,
    // even after the original source list is dropped.
    #[test]
    fn unnamed_sources_survive_parameterization() {
        let module = {
            let sources = [
                Source::text("package widths; parameter WIDTH = 8; endpackage"),
                Source::text(
                    "module M #(parameter MULTIPLIER = 1)
                     (output [widths::WIDTH*MULTIPLIER-1:0] data); endmodule",
                ),
            ];
            ModDef::from_verilog_with_config(
                "M",
                &ParserConfig {
                    sources: &sources,
                    ..Default::default()
                },
            )
        };

        assert_eq!(module.get_port("data").io().width(), 8);
        let variant = module.parameterize(&[("MULTIPLIER", 2)]);
        assert_eq!(variant.get_port("data").io().width(), 16);
    }

    // A logical filename must resolve nearby includes without requiring the
    // source buffer itself to exist on disk.
    #[test]
    fn named_text_resolves_relative_includes() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("width.svh"), "`define WIDTH 12\n").unwrap();
        let path = directory.path().join("module.sv");
        let sources = [Source::named_text(
            path.to_str().unwrap(),
            "`include \"width.svh\"\nmodule M(output [`WIDTH-1:0] data); endmodule",
        )];
        let module = ModDef::from_verilog_with_config(
            "M",
            &ParserConfig {
                sources: &sources,
                ..Default::default()
            },
        );

        assert_eq!(module.get_port("data").io().width(), 12);
        assert!(!path.exists());
    }

    #[test]
    fn shared_text_keeps_both_string_and_slice_allocations() {
        let string = Arc::new("module M; endmodule".to_owned());
        let slice: Arc<str> = Arc::from(string.as_str());
        for (source, expected) in [
            (SourceText::from(string.clone()), string.as_ptr()),
            (SourceText::from(slice.clone()), slice.as_ptr()),
        ] {
            assert_eq!(source.clone().as_str().as_ptr(), expected);
        }
    }
}
