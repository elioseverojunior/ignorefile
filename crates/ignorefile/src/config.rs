// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The serialized configuration format.
//!
//! See `docs/design/config-format.md` for why the shape is what it is. The two
//! load-bearing decisions: sections are an ordered **array** with the name as a
//! **value**, because `serde_json` sorts object keys by default and rule order is
//! semantic; and comments are **fields**, because JSON has no comment syntax and
//! the three encodings have to be interchangeable.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The only schema version this build understands.
pub const VERSION: u32 = 1;

/// Comment depth of the configuration's own name: `### name`.
pub const NAME_LEVEL: usize = 3;

/// Default comment depth of a section header: `## Section`.
pub const SECTION_LEVEL: usize = 2;

/// Which encoding a configuration is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// TOML, the default.
    Toml,
    /// JSON.
    Json,
    /// YAML.
    Yaml,
}

impl Format {
    /// Infers the format from a path's extension.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownFormat`] if the extension is missing or unrecognized.
    pub fn from_path(path: &Path) -> Result<Self, Error> {
        Self::from_extension(path.extension().and_then(|ext| ext.to_str()).unwrap_or(""))
    }

    /// Parses a format from a bare extension, without a path.
    ///
    /// The surfaces that have no filesystem, WebAssembly and the MCP server,
    /// take the format as a string; this keeps the mapping in one place.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownFormat`] if the extension is not recognized.
    pub fn from_extension(extension: &str) -> Result<Self, Error> {
        match extension {
            "toml" => Ok(Self::Toml),
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            other => Err(Error::UnknownFormat(other.to_owned())),
        }
    }

    /// The conventional extension for this format.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }
}

/// A repository's ignore rules, as reviewable configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Verbatim comment block above everything else, before the `###` banner.
    ///
    /// This is where a licence header lives. Without it, a leading
    /// `# SPDX-FileCopyrightText: ...` block imported as an ordinary section,
    /// which displaced the banner as the first group and left [`Self::name`]
    /// `None`.
    ///
    /// Declared first: field order is serialization order, and `taplo` sorts
    /// values alphabetically ahead of tables, so `header` < `name` < `version`
    /// keeps emitted TOML already formatter-canonical. See [`Config`].
    ///
    /// On `Config` rather than [`Target`] because a licence header is the same
    /// for every file generated from one configuration, exactly like
    /// [`Self::name`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// The configuration's own name, rendered as a `###` banner at the top.
    ///
    /// Declared before `version` on purpose. Field order is serialization order,
    /// and `taplo` (this repository's TOML formatter, and a common one) sorts
    /// keys alphabetically while keeping values ahead of tables. Emitting in that
    /// order means a generated config is already formatted, instead of being
    /// rewritten by the first formatter that touches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Schema version. See [`VERSION`].
    pub version: u32,
    /// The `.gitignore` this configuration describes.
    ///
    /// A table rather than an array because a future `[dockerignore]` sibling
    /// describes a different output file. Keys are safe here, unlike for
    /// sections: the order of two different output files is not semantic.
    #[serde(default, skip_serializing_if = "Target::is_empty")]
    pub gitignore: Target,
}

/// One ignore file's worth of sections.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Sections, in output order. Named `section` so it reads as
    /// `[[gitignore.section]]`.
    #[serde(default, rename = "section", skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<Section>,
}

impl Target {
    /// Whether there is nothing to render.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}

/// One commented group of rules, rendered under a `##` header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    /// Comment depth of the header. Defaults to [`SECTION_LEVEL`].
    ///
    /// Stored so a `.gitignore` using `#` for headers, which is what nearly
    /// every real file does, still round-trips byte for byte.
    ///
    /// Fields are declared in the order a TOML formatter would sort them; see
    /// [`Config`].
    #[serde(default = "default_level", skip_serializing_if = "is_default_level")]
    pub level: usize,
    /// Header text, without its leading `#`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Explanation under the header, one `#` line per line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Rules, in order. Named `rule` so it reads as `[[gitignore.section.rule]]`.
    #[serde(default, rename = "rule", skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
}

/// One group of patterns, optionally preceded by its own comment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Patterns to re-include, stored **without** the leading `!` that
    /// `.gitignore` writes. The field name already says what it means, and
    /// storing the `!` too would make `add = ["!x"]` and `add = ["x"]` two
    /// spellings of one rule.
    ///
    /// Fields are declared in the order a TOML formatter would sort them; see
    /// [`Config`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<String>,
    /// Patterns to ignore, verbatim, including any leading `/`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
    /// Explanation above the patterns, one `#` line per line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Default for Section {
    /// Cannot be derived: a derived `Default` would give `level` zero, meaning a
    /// header written with no `#` at all, and serde's `default = "default_level"`
    /// only applies when deserializing.
    fn default() -> Self {
        Self {
            level: default_level(),
            name: None,
            note: None,
            rules: Vec::new(),
        }
    }
}

const fn default_level() -> usize {
    SECTION_LEVEL
}

// serde calls `skip_serializing_if` as `path(&field)`, so the signature must take
// a reference. clippy's `trivially_copy_pass_by_ref` fires on any `Copy` field
// used this way regardless of its size, so the lint is structurally inapplicable
// here rather than describing a real inefficiency.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_default_level(level: &usize) -> bool {
    *level == default_level()
}

impl Config {
    /// A new, empty configuration at the current schema version.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: VERSION,
            ..Self::default()
        }
    }

    /// Decodes a configuration from text and validates it.
    ///
    /// # Errors
    ///
    /// A format-specific parse error, [`Error::UnsupportedVersion`] if the schema
    /// version is not one this build understands, or [`Error::Invalid`] if the
    /// contents break a rule the schema cannot express.
    pub fn decode(text: &str, format: Format) -> Result<Self, Error> {
        let config: Self = match format {
            Format::Toml => toml::from_str(text)?,
            Format::Json => serde_json::from_str(text)?,
            Format::Yaml => serde_norway::from_str(text)?,
        };
        if config.version != VERSION {
            return Err(Error::UnsupportedVersion {
                found: config.version,
                expected: VERSION,
            });
        }
        config.validate()?;
        Ok(config)
    }

    /// Checks the rules a serde schema cannot express.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] naming the offending field and why.
    pub fn validate(&self) -> Result<(), Error> {
        for (index, section) in self.gitignore.sections.iter().enumerate() {
            let at = format!("gitignore.section[{index}]");
            if section.level == 0 {
                return Err(Error::Invalid {
                    at,
                    why: "level must be at least 1: a header needs at least one `#`".to_owned(),
                });
            }
            if section.name.is_none() && section.note.is_some() {
                return Err(Error::Invalid {
                    at,
                    why: "a note needs a name: without a header there is nothing to attach it to"
                        .to_owned(),
                });
            }
            for (position, rule) in section.rules.iter().enumerate() {
                rule.validate(&format!("{at}.rule[{position}]"))?;
            }
        }
        Ok(())
    }

    /// Encodes a configuration as text in the given format.
    ///
    /// # Errors
    ///
    /// A format-specific serialization error. This crate's own types cannot
    /// trigger one, since they contain only strings, integers, booleans and
    /// sequences, but the encoders are fallible in general.
    pub fn encode(&self, format: Format) -> Result<String, Error> {
        encode_value(self, format)
    }
}

/// The single place that knows how to reach each encoder.
///
/// Generic over the value so the error paths can be exercised by a test: this
/// crate's own types cannot fail to serialize, but a map with non-string keys
/// fails in all three formats.
impl Rule {
    fn validate(&self, at: &str) -> Result<(), Error> {
        // A `note` alone is a placeholder: it still renders a comment line, so
        // the rule survives a generate. A rule with none of the three renders
        // nothing at all, which would let it disappear from the output silently,
        // so that stays an error.
        if self.ignore.is_empty() && self.add.is_empty() && self.note.is_none() {
            return Err(Error::Invalid {
                at: at.to_owned(),
                why:
                    "a rule needs at least one `ignore` or `add` pattern, or a `note` to stand as \
                      a placeholder"
                        .to_owned(),
            });
        }
        for pattern in self.ignore.iter().chain(&self.add) {
            if pattern.trim().is_empty() {
                return Err(Error::Invalid {
                    at: at.to_owned(),
                    why: "a pattern cannot be blank".to_owned(),
                });
            }
            if pattern.contains('\n') {
                return Err(Error::Invalid {
                    at: at.to_owned(),
                    why: format!("a pattern cannot contain a newline: {pattern:?}"),
                });
            }
        }
        for pattern in &self.add {
            if let Some(stripped) = pattern.strip_prefix('!') {
                return Err(Error::Invalid {
                    at: at.to_owned(),
                    why: format!(
                        "`add` patterns are stored without the leading `!`, which is added when \
                         rendering. Write {stripped:?} instead of {pattern:?}"
                    ),
                });
            }
        }
        Ok(())
    }
}

// Combinators rather than `?` on purpose. `?` desugars to a match whose error
// arm is a region in THIS file, and it is unreachable for `Config`, which cannot
// fail to serialize; that leaves the 100% region gate unsatisfiable. `map_err`
// keeps the branch inside core's own source while behaving identically.
fn encode_value<T: Serialize>(value: &T, format: Format) -> Result<String, Error> {
    match format {
        // `to_string`, not `to_string_pretty`: the pretty encoder wraps every
        // multi-element array over several lines with a four-space indent, which
        // no TOML formatter agrees with, so a generated config was rewritten the
        // first time one ran. The compact form inlines arrays, which matches
        // `taplo` exactly for anything inside its column limit.
        Format::Toml => toml::to_string(value).map_err(Error::from),
        Format::Json => serde_json::to_string_pretty(value)
            // serde_json omits the trailing newline that every formatter in this
            // repository, and hk's end-of-file-fixer, expects.
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(Error::from),
        Format::Yaml => serde_norway::to_string(value).map_err(Error::from),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Format, encode_value};

    /// A map whose keys are themselves maps. None of the three formats can
    /// represent that, which is what makes the error arms reachable.
    fn unserializable() -> BTreeMap<BTreeMap<String, i32>, i32> {
        BTreeMap::from([(BTreeMap::from([("key".to_owned(), 1)]), 2)])
    }

    #[test]
    fn every_encoder_reports_its_own_failure() {
        for format in [Format::Toml, Format::Json, Format::Yaml] {
            let err = encode_value(&unserializable(), format).expect_err("cannot encode");
            assert!(
                !err.to_string().is_empty(),
                "{format:?} should carry the encoder's message"
            );
        }
    }
}
