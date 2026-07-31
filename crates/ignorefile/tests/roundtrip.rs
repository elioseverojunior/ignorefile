// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Phase 1 of `docs/ROADMAP.md`: `render(parse(x)) == x`, byte for byte.
//!
//! This is the invariant the whole project rests on. `.gitignore` is
//! order-dependent and anchor-sensitive, so any model that normalizes its input
//! changes which files git ignores, silently. Every later phase adds structure
//! underneath this property; none of them may weaken it.
//!
//! Edge cases live here as string literals rather than as corpus files on
//! purpose: `hk`'s `trailing-whitespace-fixer` and `end-of-file-fixer` both run
//! at `glob = "**"`, so a checked-in file asserting "trailing spaces survive" or
//! "there is no final newline" would be silently normalized by our own
//! pre-commit hook, and the test would then pass for the wrong reason. Only
//! realistic, already-normalized files go in `tests/corpus/`.

use ignorefile::{Config, GitIgnore};
use proptest::prelude::*;

/// The ordered `(negate, text)` list git actually acts on.
///
/// Comments and blank lines are absent by construction, so two files with equal
/// lists ignore exactly the same paths. This is the operational definition of
/// "meaning" used by the canonicalization properties below.
fn patterns(model: &GitIgnore) -> Vec<(bool, String)> {
    model
        .lines()
        .iter()
        .filter_map(|line| {
            line.pattern()
                .map(|(negate, text)| (negate, text.to_owned()))
        })
        .collect()
}

/// Inputs whose exact bytes a lossless model must reproduce.
///
/// Each entry is a distinction git actually makes, or a shape a naive
/// line-splitter gets wrong.
const EDGE_CASES: &[(&str, &str)] = &[
    ("empty file", ""),
    ("single newline", "\n"),
    ("only blank lines", "\n\n\n"),
    ("no final newline", "/target"),
    ("final newline", "/target\n"),
    // Anchoring is semantic: `/target` matches only at the root, `target`
    // matches at any depth. A model that trims the slash changes behaviour.
    ("anchored", "/target\n"),
    ("unanchored", "target\n"),
    ("directory only", "build/\n"),
    // Order is semantic: the negation only works because it follows the pattern
    // it overrides.
    ("negation after pattern", "*.log\n!important.log\n"),
    ("negation before pattern", "!important.log\n*.log\n"),
    // Git strips trailing spaces when matching, but the file still contains
    // them, so a round-trip must keep them.
    ("trailing spaces", "logs   \n"),
    ("escaped trailing space", "logs\\ \n"),
    // A leading `\` escapes the character that would otherwise be special.
    ("escaped hash", "\\#not-a-comment\n"),
    ("escaped bang", "\\!not-a-negation\n"),
    ("comment depths", "# Cargo\n## Mise\n"),
    // Verified with git: only a `#` at position 0 starts a comment, and leading
    // whitespace is significant. So this is a PATTERN matching a file literally
    // named "   # indented", not a comment.
    ("hash after leading space is a pattern", "   # indented\n"),
    ("whitespace-only line", "   \n"),
    ("crlf", "/target\r\n"),
    ("mixed line endings", "a\r\nb\nc\r\n"),
    ("lone cr, no final newline", "no-eol-crlf\r"),
    // A byte-order mark is content, not syntax; git does not skip it.
    ("byte order mark", "\u{feff}# bom\n"),
    ("glob syntax", "**/*.rs.bk\n?.tmp\n[Dd]ebug/\n"),
];

/// Realistic files, read from `tests/corpus/`.
const CORPUS: &[(&str, &str)] = &[
    ("rust.gitignore", include_str!("corpus/rust.gitignore")),
    (
        "sectioned.gitignore",
        include_str!("corpus/sectioned.gitignore"),
    ),
];

#[test]
fn edge_cases_round_trip() {
    for (label, source) in EDGE_CASES {
        assert_eq!(
            GitIgnore::parse(source).render(),
            *source,
            "{label} did not round-trip (input {source:?})"
        );
    }
}

#[test]
fn corpus_round_trips() {
    for (name, source) in CORPUS {
        assert_eq!(
            GitIgnore::parse(source).render(),
            *source,
            "{name} did not round-trip"
        );
    }
}

/// One line of `.gitignore` content, guaranteed free of `\n` so the generated
/// file's line structure is exactly what the generator intends.
///
/// The regex literals are used as strategies directly. `prop::string::string_regex`
/// would return a `Result`, and `expect` on it is a hard error here: clippy's
/// `allow-expect-in-tests` only covers `#[test]` functions and `cfg(test)`
/// modules, not helper functions in an integration-test crate.
fn line() -> impl Strategy<Value = String> {
    prop_oneof![
        2 => Just(String::new()),
        1 => r"[ \t]{1,4}",
        3 => (1_usize..=3, r"[a-zA-Z0-9 _.\-]{0,20}")
            .prop_map(|(depth, text)| format!("{} {text}", "#".repeat(depth))),
        8 => (
            prop::option::of(Just('!')),
            prop::option::of(Just('/')),
            r"[a-zA-Z0-9_.\-]{1,8}(/[a-zA-Z0-9_.\-]{1,8}){0,2}",
            prop::option::of(prop_oneof![Just("*"), Just("**"), Just("?"), Just("[Dd]")]),
            prop::option::of(Just('/')),
            r"[ ]{0,2}",
        )
            .prop_map(|(negate, anchor, body, glob, dir_only, trailing)| {
                let mut out = String::new();
                if let Some(bang) = negate {
                    out.push(bang);
                }
                if let Some(slash) = anchor {
                    out.push(slash);
                }
                out.push_str(&body);
                if let Some(glob) = glob {
                    out.push_str(glob);
                }
                if let Some(slash) = dir_only {
                    out.push(slash);
                }
                out.push_str(&trailing);
                out
            }),
    ]
}

/// A whole `.gitignore` file: any number of lines, each with its own terminator
/// so mixed endings are generated, and an optional final newline.
fn gitignore_text() -> impl Strategy<Value = String> {
    let terminated = (line(), prop_oneof![Just("\n"), Just("\r\n")]);
    (prop::collection::vec(terminated, 0..24), any::<bool>()).prop_map(|(lines, final_newline)| {
        let last = lines.len().saturating_sub(1);
        let mut out = String::new();
        for (index, (text, ending)) in lines.iter().enumerate() {
            out.push_str(text);
            if index < last || final_newline {
                out.push_str(ending);
            }
        }
        out
    })
}

proptest! {
    /// The Phase 1 property. Holds for every generated file, not just the
    /// examples above.
    #[test]
    fn any_gitignore_round_trips(source in gitignore_text()) {
        prop_assert_eq!(GitIgnore::parse(&source).render(), source);
    }

    /// The property is about bytes, so it must hold for text that is not
    /// gitignore-shaped at all. Nothing in the model may depend on the input
    /// being well formed.
    #[test]
    fn arbitrary_text_round_trips(source in r"(?s).{0,200}") {
        prop_assert_eq!(GitIgnore::parse(&source).render(), source);
    }

    /// Canonicalizing is a formatting operation, never a semantic one.
    ///
    /// This is what lets `import` refuse a file and `fmt` repair it without a
    /// human auditing the diff for meaning: the patterns git acts on are
    /// identical either side. Also checked exhaustively for short inputs -
    /// 585120 sources, 563630 of them byte-differing, none pattern-differing -
    /// which this generalizes.
    #[test]
    fn canonicalizing_preserves_every_pattern(source in gitignore_text()) {
        let parsed = GitIgnore::parse(&source);
        // A file that cannot form a valid config is `fmt`'s problem to report,
        // not a counterexample to the claim about the ones that can.
        if let Ok(tidy) = parsed.canonical() {
            prop_assert_eq!(patterns(&parsed), patterns(&tidy));
        }
    }

    /// The canonical form is a fixed point.
    ///
    /// `fmt --check` compares a file against its own canonical form, so a
    /// second pass that kept moving lines would make the check never settle.
    #[test]
    fn canonicalizing_is_idempotent(source in gitignore_text()) {
        if let Ok(once) = GitIgnore::parse(&source).canonical() {
            let Ok(twice) = once.canonical() else {
                return Err(TestCaseError::fail("canonical form failed to re-canonicalize"));
            };
            prop_assert_eq!(once.render(), twice.render());
        }
    }

    /// The point of `fmt`: what it writes is what `import` accepts. Without
    /// this the command would be busywork.
    #[test]
    fn canonical_output_always_imports(source in gitignore_text()) {
        if let Ok(tidy) = GitIgnore::parse(&source).canonical() {
            prop_assert!(Config::try_from(&tidy).is_ok(), "{:?}", tidy.render());
        }
    }
}
