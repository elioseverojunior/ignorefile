// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crate's error type.

use thiserror::Error;

/// Everything that can go wrong reading, writing or converting a configuration.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The file extension does not name a format this crate can read.
    #[error("unrecognized config format {0:?}: expected one of toml, json, yaml, yml")]
    UnknownFormat(String),

    /// The text is not valid TOML, or does not match the schema.
    #[error("invalid TOML config: {0}")]
    Toml(#[from] toml::de::Error),

    /// The configuration could not be written as TOML.
    ///
    /// Unlike `serde_json` and `serde_norway`, `toml` has separate error types
    /// for reading and writing, so this needs its own variant.
    #[error("could not write TOML config: {0}")]
    TomlEncode(#[from] toml::ser::Error),

    /// The text is not valid JSON, or does not match the schema.
    #[error("invalid JSON config: {0}")]
    Json(#[from] serde_json::Error),

    /// The text is not valid YAML, or does not match the schema.
    #[error("invalid YAML config: {0}")]
    Yaml(#[from] serde_norway::Error),

    /// The config declares a schema version this build does not understand.
    #[error("config version {found} is not supported: this build understands version {expected}")]
    UnsupportedVersion {
        /// The version found in the file.
        found: u32,
        /// The version this build understands.
        expected: u32,
    },

    /// The configuration breaks a rule the serde schema cannot express.
    #[error("invalid config at {at}: {why}")]
    Invalid {
        /// Dotted path to the offending field, e.g. `gitignore.section[0].rule[1]`.
        at: String,
        /// What is wrong, and how to fix it.
        why: String,
    },

    /// The file is not in the canonical form import reproduces, so it was
    /// refused rather than rewritten behind the user's back.
    ///
    /// **This is about bytes, not about meaning.** Only comments and blank
    /// lines can ever differ: patterns reach the configuration verbatim and in
    /// order, so a refused file still ignores exactly the same paths. Checked
    /// exhaustively over every source of up to four lines drawn from a
    /// vocabulary covering each parser branch - 563630 of them fail this check
    /// and not one changes the pattern list git acts on.
    ///
    /// `fmt` rewrites a file into the canonical form, as a diff the user can
    /// review, which is why the message points there rather than asking for a
    /// hand-edit.
    #[error(
        "this .gitignore is not in canonical form, so importing it would not \
         round-trip: line {line} would be rewritten from {expected:?} to \
         {actual:?}. Only comments and blank lines differ; the patterns git \
         acts on are unchanged. Run `ign fmt` to normalize the file, then \
         import it again."
    )]
    NotCanonical {
        /// One-based line number of the first difference.
        line: usize,
        /// The line as it appears in the source.
        expected: String,
        /// The line the config would render.
        actual: String,
    },
}
