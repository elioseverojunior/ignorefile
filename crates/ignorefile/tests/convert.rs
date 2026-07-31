// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `.gitignore` <-> configuration conversion.
//!
//! The central claim of `docs/design/config-format.md` is that import is
//! lossless, or it refuses. These tests hold that in place: every successful
//! import must render back to the exact source bytes, and every input that
//! cannot is rejected with a message naming the line.

use ignorefile::{Config, Error, Format, GitIgnore, Rule, Section, Target, VERSION};

/// Imports and asserts the config renders back to the identical source.
///
/// `expect` is avoided here on purpose: clippy's `allow-expect-in-tests` only
/// covers `#[test]` functions and `cfg(test)` modules, not helpers in an
/// integration-test crate, and `expect_used` is a denied warning.
fn round_trip(source: &str) -> Config {
    let parsed = GitIgnore::parse(source);
    let Ok(config) = Config::try_from(&parsed) else {
        panic!("should be importable: {source:?}")
    };
    assert_eq!(
        GitIgnore::from(&config).render(),
        source,
        "config did not render back to the source"
    );
    config
}

#[test]
fn imports_the_canonical_fixture_losslessly() {
    let config = round_trip(include_str!("corpus/canonical.gitignore"));
    assert_eq!(config.name.as_deref(), Some("ignore-as-config"));

    let sections = &config.gitignore.sections;
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].name.as_deref(), Some("Logs"));
    assert_eq!(sections[0].level, 2);
    assert_eq!(sections[0].note.as_deref(), Some("Ignore Logs Pattern"));
    assert_eq!(sections[0].rules.len(), 2);
    assert_eq!(sections[0].rules[0].ignore, vec!["*.log"]);
    assert_eq!(
        sections[0].rules[1].note.as_deref(),
        Some("keep the one we actually read")
    );
    assert_eq!(sections[0].rules[1].add, vec!["important.log"]);
    // The `!` lives in the renderer, never in the stored pattern.
    assert!(!sections[0].rules[1].add[0].starts_with('!'));
}

#[test]
fn renders_the_canonical_layering() {
    let config = Config {
        header: None,
        version: VERSION,
        name: Some("ignore-as-config".to_owned()),
        gitignore: Target {
            sections: vec![
                Section {
                    name: Some("Logs".to_owned()),
                    note: Some("Ignore Logs Pattern".to_owned()),
                    rules: vec![
                        Rule {
                            ignore: vec!["*.log".to_owned()],
                            ..Rule::default()
                        },
                        Rule {
                            note: Some("keep the one we actually read".to_owned()),
                            ignore: Vec::new(),
                            add: vec!["important.log".to_owned()],
                        },
                    ],
                    ..Section::default()
                },
                Section {
                    name: Some("Mise".to_owned()),
                    note: Some("Mise Ignore Pattern".to_owned()),
                    rules: vec![Rule {
                        note: Some("first\nsecond".to_owned()),
                        ignore: vec!["/mise.local.toml".to_owned()],
                        add: vec!["/mise.lock".to_owned()],
                    }],
                    ..Section::default()
                },
            ],
        },
    };
    assert_eq!(
        GitIgnore::from(&config).render(),
        "### ignore-as-config\n\
         \n\
         ## Logs\n\
         # Ignore Logs Pattern\n\
         *.log\n\
         \n\
         # keep the one we actually read\n\
         !important.log\n\
         \n\
         ## Mise\n\
         # Mise Ignore Pattern\n\
         \n\
         # first\n\
         # second\n\
         /mise.local.toml\n\
         !/mise.lock\n"
    );
}

#[test]
fn a_mid_section_comment_becomes_a_rule_note() {
    // The one shape the previous grammar refused outright.
    let config = round_trip("## S\n/a\n\n# why\n/b\n");
    let rules = &config.gitignore.sections[0].rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].ignore, vec!["/a"]);
    assert_eq!(rules[1].note.as_deref(), Some("why"));
    assert_eq!(rules[1].ignore, vec!["/b"]);
}

#[test]
fn an_ignore_after_an_add_starts_a_new_rule() {
    // Rendering emits all of a rule's `ignore` before its `add`, so keeping
    // these in one rule would reorder them and change what git ignores.
    let config = round_trip("build/\n!build/keep\nbuild/keep/tmp\n");
    let rules = &config.gitignore.sections[0].rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].ignore, vec!["build/"]);
    assert_eq!(rules[0].add, vec!["build/keep"]);
    assert_eq!(rules[1].ignore, vec!["build/keep/tmp"]);
}

#[test]
fn a_level_one_header_over_patterns_is_a_rule_note() {
    // Depth decides: `#` is a rule note, so this belongs to an unnamed leading
    // section rather than naming one.
    let config = round_trip("# Cargo\n/target\n");
    let section = &config.gitignore.sections[0];
    assert_eq!(section.name, None);
    assert_eq!(section.rules[0].note.as_deref(), Some("Cargo"));
    assert_eq!(section.rules[0].ignore, vec!["/target"]);
}

/// A licence block above the `###` banner is the configuration's header.
///
/// Without this it became `Section { level: 1, name: "SPDX-FileCopyrightText:
/// ..." }`, which round-tripped but displaced the banner as the first group, so
/// `as_config_name` never matched and `name` came back `None`.
#[test]
fn imports_a_licence_preamble_above_the_banner() {
    // Deliberately NOT a real licence header, even though that is the motivating
    // case. `mise run comply` scans source files for the licence-identifier tag
    // and parses the rest of the LINE as a licence expression, joining every
    // occurrence in the file. rustfmt wraps a long string literal with a
    // trailing `\`, so the expression it read ended in a backslash and the gate
    // died with "SPDX error: unexpected token '\' after expression". Writing the
    // tag anywhere below the file header -- even inside a comment like this one
    // -- feeds it the same way.
    //
    // The grammar rule under test is "a bare-`#` block directly above the `###`
    // banner is the header", which has nothing to do with the text. The real
    // SPDX shape is covered by `imports_the_preamble_fixture_losslessly`, whose
    // fixture lives under `tests/corpus/` and is exempt from that scan.
    let config = round_trip(
        "# Copyright 2026 the ignorefile contributors\n\
         #\n\
         # Licensed under MIT or Apache-2.0\n\
         \n\
         ### ignore-as-config\n\
         \n\
         ## Logs\n\
         *.log\n",
    );
    assert_eq!(
        config.header.as_deref(),
        Some("Copyright 2026 the ignorefile contributors\n\nLicensed under MIT or Apache-2.0")
    );
    assert_eq!(config.name.as_deref(), Some("ignore-as-config"));
    assert_eq!(config.gitignore.sections.len(), 1);
    assert_eq!(
        config.gitignore.sections[0].name.as_deref(),
        Some("Logs"),
        "the preamble must not become a section"
    );
}

/// The real-world shape: a licence header, a banner, patterns, and a section
/// whose only content is a placeholder comment. Every feature at once, on a file
/// shaped like one a repository would actually carry.
#[test]
fn imports_the_preamble_fixture_losslessly() {
    let config = round_trip(include_str!("corpus/preamble.gitignore"));
    assert!(
        config
            .header
            .as_deref()
            .is_some_and(|header| header.starts_with("SPDX-FileCopyrightText:")),
        "{:?}",
        config.header
    );
    assert_eq!(config.name.as_deref(), Some("ignore-as-config"));

    let sections = &config.gitignore.sections;
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].rules[0].ignore, vec!["/target"]);

    let placeholder = &sections[1];
    assert_eq!(placeholder.name.as_deref(), Some("Placeholder"));
    assert_eq!(
        placeholder.note.as_deref(),
        Some("TODO: decide what belongs here"),
        "a comment-only group is the section's note, not a rule"
    );
    assert!(placeholder.rules.is_empty());
}

/// The narrow rule: only a comment block sitting directly above the banner is a
/// preamble. Without a banner after it, it is ordinary content.
#[test]
fn a_leading_comment_without_a_banner_is_not_a_preamble() {
    let config = round_trip("# Editors\n\n## Rust\n/target\n");
    assert_eq!(config.header, None);
    // `# Editors` is a bare `#`, so it is a note, not a header. With no
    // patterns beneath it, it becomes a note-only rule in a leading unnamed
    // section; only `## Rust` opens a named one.
    let names: Vec<&str> = config
        .gitignore
        .sections
        .iter()
        .filter_map(|section| section.name.as_deref())
        .collect();
    assert_eq!(names, vec!["Rust"]);
    assert_eq!(
        config.gitignore.sections[0].rules[0].note.as_deref(),
        Some("Editors")
    );
}

#[test]
fn a_comment_with_no_patterns_is_a_note_only_rule() {
    // Depth decides, with no exception. A bare `#` is a note, and a note with
    // no patterns under it is a note-only rule rather than a level-1 section:
    // such a section rendered as a line indistinguishable from a note, so the
    // file stopped being a fixed point. The comment is preserved either way,
    // which is why the exception could go.
    let config = round_trip("# Always Add\n");
    let section = &config.gitignore.sections[0];
    assert_eq!(section.name, None);
    assert_eq!(section.rules.len(), 1);
    assert_eq!(section.rules[0].note.as_deref(), Some("Always Add"));
    assert!(section.rules[0].ignore.is_empty());
}

/// A comment after a pattern, with nothing under it, becomes a note-only rule.
///
/// It used to be dropped: `to_rules` only drained its pending comments when a
/// pattern arrived, so the re-render lost the line and import refused with
/// `NotCanonical { line: 3, expected: "# TODO...", actual: "" }`.
///
/// Note the blank line before the comment. Every note takes one, so the comment
/// is a group of its own; without it the comment would touch the pattern above
/// and re-import would fold it back into that rule.
#[test]
fn imports_a_trailing_comment_inside_a_section() {
    let config = round_trip("## Cargo\n/target\n\n# TODO: decide about build artifacts\n");
    assert_eq!(config.gitignore.sections.len(), 1);
    let section = &config.gitignore.sections[0];
    assert_eq!(section.name.as_deref(), Some("Cargo"));
    assert_eq!(section.rules.len(), 2);
    assert_eq!(section.rules[0].ignore, vec!["/target"]);

    let placeholder = &section.rules[1];
    assert_eq!(
        placeholder.note.as_deref(),
        Some("TODO: decide about build artifacts")
    );
    assert!(placeholder.ignore.is_empty());
    assert!(placeholder.add.is_empty());
}

#[test]
fn imports_multi_line_rule_notes() {
    // rust.gitignore uses `#` throughout, so every block is a rule note in one
    // unnamed section. This is the real-world shape the grammar must not break.
    let config = round_trip(include_str!("corpus/rust.gitignore"));
    let sections = &config.gitignore.sections;
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].name, None);
    assert_eq!(
        sections[0].rules[0].note.as_deref(),
        Some("Generated by Cargo\nwill have compiled files and executables")
    );
    assert_eq!(sections[0].rules[0].ignore, vec!["debug/", "target/"]);
    assert_eq!(sections[0].rules.len(), 4);
}

#[test]
fn imports_a_sectioned_file_losslessly() {
    let config = round_trip(include_str!("corpus/sectioned.gitignore"));
    let names: Vec<&str> = config
        .gitignore
        .sections
        .iter()
        .filter_map(|section| section.name.as_deref())
        .collect();
    assert_eq!(
        names,
        vec!["Cargo", "Mise", "Logs", "Editors", "Always Add"]
    );
    assert_eq!(config.gitignore.sections[1].level, 2, "## Mise keeps depth");
    // A trailing comment with no patterns is still a section.
    let last = config.gitignore.sections.last().expect("has sections");
    assert!(last.rules.is_empty());
}

#[test]
fn a_section_with_no_comment_has_no_name() {
    let config = round_trip("/target\n*.log\n");
    let sections = &config.gitignore.sections;
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].name, None);
    assert_eq!(sections[0].rules[0].ignore, vec!["/target", "*.log"]);
}

#[test]
fn an_empty_file_imports_to_an_empty_config() {
    let config = round_trip("");
    assert!(config.gitignore.is_empty());
    assert_eq!(config.version, VERSION);
    assert_eq!(config.name, None);
}

#[test]
fn a_bare_hash_comment_round_trips() {
    let config = round_trip("#\n/target\n");
    let section = &config.gitignore.sections[0];
    assert_eq!(section.name, None);
    assert_eq!(section.rules[0].note.as_deref(), Some(""));
}

#[test]
fn a_bare_hash_above_a_section_round_trips() {
    // A lone `#` is a note at depth 1, so it is a note-only rule in the leading
    // unnamed section rather than a section named "". Only `## S` opens one.
    let config = round_trip("#\n\n## S\n/a\n");
    let sections = &config.gitignore.sections;
    assert_eq!(sections[0].name, None);
    assert_eq!(sections[0].rules[0].note.as_deref(), Some(""));
    assert_eq!(sections[1].name.as_deref(), Some("S"));
    assert_eq!(sections[1].level, 2);
}

#[test]
fn generating_from_an_empty_config_produces_an_empty_file() {
    assert_eq!(GitIgnore::from(&Config::new()).render(), "");
}

#[test]
fn a_double_blank_line_is_refused_with_the_line() {
    // Rendering emits exactly one blank between sections, so this cannot be
    // reproduced and import must refuse rather than normalize it.
    let parsed = GitIgnore::parse("# A\n/a\n\n\n# B\n/b\n");
    let err = Config::try_from(&parsed).expect_err("refused");
    let Error::NotCanonical {
        line,
        expected,
        actual,
    } = &err
    else {
        panic!("expected NotCanonical, got {err}");
    };
    assert_eq!(*line, 4);
    assert_eq!(expected, "");
    assert_eq!(actual, "# B");
    assert!(err.to_string().contains("line 4"), "{err}");
    // The message has to point at the fix rather than claim the patterns are
    // at risk: a blank line is not something git acts on.
    assert!(err.to_string().contains("fmt"), "{err}");
}

#[test]
fn a_mixed_depth_header_is_refused() {
    let parsed = GitIgnore::parse("# one\n## two\n/target\n");
    let err = Config::try_from(&parsed).expect_err("refused");
    assert!(matches!(err, Error::NotCanonical { line: 2, .. }), "{err}");
}

#[test]
fn a_comment_with_no_space_after_the_hash_is_refused() {
    let parsed = GitIgnore::parse("#NoSpace\n/target\n");
    let err = Config::try_from(&parsed).expect_err("refused");
    assert!(matches!(err, Error::NotCanonical { line: 1, .. }), "{err}");
}

/// Replaces `a_trailing_comment_block_is_refused`, which asserted the opposite.
///
/// A comment with no rule beneath it used to have nowhere to attach: rendering
/// dropped it and the verification caught the loss. It now becomes a note-only
/// rule, so the same input imports rather than refusing. The refusal path itself
/// is still covered by `a_mixed_depth_header_is_refused` and its neighbours.
#[test]
fn a_trailing_comment_block_becomes_a_note_only_rule() {
    let parsed = GitIgnore::parse("## S\n/a\n\n# dangling\n");
    let config = Config::try_from(&parsed).expect("importable");
    let rules = &config.gitignore.sections[0].rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].note.as_deref(), Some("dangling"));
    assert!(rules[1].ignore.is_empty() && rules[1].add.is_empty());
}

#[test]
fn import_reports_a_config_that_would_be_invalid() {
    // A lone `!` is a negation of the empty pattern, which validation rejects
    // before the round-trip check gets a chance to run.
    let parsed = GitIgnore::parse("## S\n!\n");
    let err = Config::try_from(&parsed).expect_err("refused");
    assert!(matches!(err, Error::Invalid { .. }), "{err}");
}

#[test]
fn a_note_only_rule_is_set_off_like_any_other_note() {
    // A note-only rule takes a blank line before it, exactly as a noted rule
    // does. It used to be the exception, suppressing the blank so that a
    // comment-only group would not re-import as a section; with the depth rule
    // now absolute such a group is a note-only rule, and the blank is what
    // keeps it one. Without it the comment merges with the block above or is
    // read as the note of the patterns below, and the file stops being a fixed
    // point. Found by `canonicalizing_is_idempotent` in tests/roundtrip.rs.
    let config = round_trip("*.log\n\n# TODO: prune\n");
    let rules = &config.gitignore.sections[0].rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].note.as_deref(), Some("TODO: prune"));
    assert!(rules[1].ignore.is_empty() && rules[1].add.is_empty());

    // A note-only rule survives only with nothing after it. Put a rule below
    // and the comment is that rule's note instead, which is why the canonical
    // form has no blank between them.
    let merged = round_trip("*.log\n\n# TODO: prune\n/target\n");
    let rules = &merged.gitignore.sections[0].rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].note.as_deref(), Some("TODO: prune"));
    assert_eq!(rules[1].ignore, vec!["/target"]);
}

#[test]
fn a_pattern_ending_in_a_carriage_return_round_trips() {
    // git strips exactly one trailing `\r` from every line, even in a file with
    // no CRLF ending anywhere, so a pattern that means `Icon\r` has to be
    // written as `Icon\r\r`. Verified with `git check-ignore` against an
    // LF-only `.gitignore`: only the file named `Icon\r` is ignored, not `Icon`
    // and not `Icon\r\r`. Rendering the pattern back verbatim would drop one
    // `\r` and silently change which file is matched.
    let config = round_trip("Icon\r\r\n");
    assert_eq!(config.gitignore.sections[0].rules[0].ignore, vec!["Icon\r"]);
}

#[test]
fn a_config_survives_gitignore_and_every_encoding() {
    let source = include_str!("corpus/canonical.gitignore");
    let config = Config::try_from(&GitIgnore::parse(source)).expect("importable");
    for format in [Format::Toml, Format::Json, Format::Yaml] {
        let text = config.encode(format).expect("encodes");
        let decoded = Config::decode(&text, format).expect("decodes");
        assert_eq!(
            GitIgnore::from(&decoded).render(),
            source,
            "{format:?} lost information on the way through"
        );
    }
}
