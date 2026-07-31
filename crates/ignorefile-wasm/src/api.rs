// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The logic behind the WebAssembly exports.
//!
//! Kept out of `lib.rs` on purpose. `lib.rs` is excluded from the coverage gate
//! because `#[wasm_bindgen]` shims cannot be exercised by a native test run, and
//! that exclusion is only honest if the shims contain nothing worth testing.
//! Everything here is plain Rust over plain strings, so it is measured like any
//! other module.

use ignorefile::{Config, Error, Format, GitIgnore};

/// Errors cross the boundary as strings: JavaScript has no use for the typed
/// variants, and `Display` already carries the actionable message.
type Outcome = Result<String, String>;

/// One place that turns a typed error into the string the boundary carries.
///
/// Shared rather than repeated per call site: several of those call sites cannot
/// actually fail, and a `map_err` closure at each would leave a region the
/// coverage gate could never satisfy.
fn plain<T>(result: Result<T, Error>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
}

fn format_of(extension: &str) -> Result<Format, String> {
    plain(Format::from_extension(extension))
}

/// Converts `.gitignore` text into configuration text.
pub(crate) fn import(gitignore: &str, extension: &str) -> Outcome {
    let format = format_of(extension)?;
    let config = plain(Config::try_from(&GitIgnore::parse(gitignore)))?;
    plain(config.encode(format))
}

/// Renders configuration text as `.gitignore` text.
pub(crate) fn generate(config: &str, extension: &str) -> Outcome {
    let format = format_of(extension)?;
    let config = plain(Config::decode(config, format))?;
    Ok(GitIgnore::from(&config).render())
}

/// Re-encodes configuration text into another format.
pub(crate) fn convert(config: &str, from: &str, to: &str) -> Outcome {
    let decoded = plain(Config::decode(config, format_of(from)?))?;
    plain(decoded.encode(format_of(to)?))
}

/// Checks configuration text, returning the reason it is not usable.
pub(crate) fn validate(config: &str, extension: &str) -> Result<(), String> {
    plain(Config::decode(config, format_of(extension)?)).map(|_| ())
}

/// Whether git would ignore `path` under these rules.
pub(crate) fn is_ignored(gitignore: &str, path: &str, is_dir: bool) -> bool {
    GitIgnore::parse(gitignore).is_ignored(path, is_dir)
}

#[cfg(test)]
mod tests {
    use super::{convert, generate, import, is_ignored, validate};

    const SOURCE: &str = "## Logs\n*.log\n\n# keep this one\n!important.log\n";

    #[test]
    fn import_then_generate_reproduces_the_source() {
        for extension in ["toml", "json", "yaml", "yml"] {
            let config = import(SOURCE, extension).expect("imports");
            assert_eq!(generate(&config, extension).expect("generates"), SOURCE);
        }
    }

    #[test]
    fn convert_moves_between_formats() {
        let toml = import(SOURCE, "toml").expect("imports");
        let json = convert(&toml, "toml", "json").expect("converts");
        assert_eq!(generate(&json, "json").expect("generates"), SOURCE);
    }

    #[test]
    fn validate_accepts_and_rejects() {
        let toml = import(SOURCE, "toml").expect("imports");
        assert!(validate(&toml, "toml").is_ok());

        let bad =
            "version = 1\n\n[[gitignore.section]]\n\n[[gitignore.section.rule]]\nadd = [\"!x\"]\n";
        let error = validate(bad, "toml").expect_err("rejects");
        assert!(error.contains("without the leading `!`"), "{error}");
    }

    #[test]
    fn an_unknown_format_is_reported_by_every_entry_point() {
        assert!(import(SOURCE, "ini").expect_err("rejects").contains("ini"));
        assert!(generate("", "ini").expect_err("rejects").contains("ini"));
        assert!(
            convert("", "ini", "toml")
                .expect_err("rejects")
                .contains("ini")
        );
        // The second format is checked too, after the first decodes.
        let toml = import(SOURCE, "toml").expect("imports");
        assert!(
            convert(&toml, "toml", "ini")
                .expect_err("rejects")
                .contains("ini")
        );
        assert!(validate("", "ini").expect_err("rejects").contains("ini"));
    }

    #[test]
    fn an_unrepresentable_gitignore_is_refused() {
        let error = import("# A\n/a\n\n\n# B\n/b\n", "toml").expect_err("refuses");
        assert!(error.contains("line 4"), "{error}");
    }

    #[test]
    fn convert_reports_a_config_it_cannot_decode() {
        let error = convert("{{{ bad", "toml", "json").expect_err("rejects");
        assert!(!error.is_empty(), "{error}");
    }

    #[test]
    fn malformed_config_is_reported() {
        assert!(
            !generate("{{{ not toml", "toml")
                .expect_err("rejects")
                .is_empty()
        );
    }

    #[test]
    fn is_ignored_answers_from_the_rules() {
        assert!(is_ignored(SOURCE, "app.log", false));
        assert!(!is_ignored(SOURCE, "important.log", false));
        assert!(!is_ignored(SOURCE, "notes.txt", false));
        assert!(is_ignored("build/\n", "build", true));
        assert!(!is_ignored("build/\n", "build", false));
    }
}
