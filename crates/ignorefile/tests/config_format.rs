// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Phase 2 of `docs/ROADMAP.md`: the configuration format.
//!
//! The assertions that matter are about **order** and **interchangeability**.
//! Section order is semantic, and `serde_json` sorts object keys by default, so
//! a format that put section names in key position would silently reorder them.
//! These tests are what hold the array-of-sections decision in place.

use std::path::Path;

use ignorefile::{Config, Error, Format, Rule, Section, Target, VERSION};

const FORMATS: [Format; 3] = [Format::Toml, Format::Json, Format::Yaml];

/// Deliberately not in alphabetical order, so a key-sorting encoder is caught.
fn unsorted() -> Config {
    Config {
        header: None,
        version: VERSION,
        name: Some("ignore-as-config".to_owned()),
        gitignore: Target {
            sections: ["Zebra", "Alpha", "Middle"]
                .into_iter()
                .map(|name| Section {
                    name: Some(name.to_owned()),
                    rules: vec![Rule {
                        ignore: vec![format!("/{}", name.to_lowercase())],
                        ..Rule::default()
                    }],
                    ..Section::default()
                })
                .collect(),
        },
    }
}

/// A section holding one rule with the given patterns.
fn section(name: &str, ignore: &[&str], add: &[&str]) -> Section {
    Section {
        name: Some(name.to_owned()),
        rules: vec![Rule {
            ignore: ignore.iter().map(|p| (*p).to_owned()).collect(),
            add: add.iter().map(|p| (*p).to_owned()).collect(),
            ..Rule::default()
        }],
        ..Section::default()
    }
}

/// A config wrapping one section, for the validation tests.
fn wrapping(section: Section) -> Config {
    Config {
        gitignore: Target {
            sections: vec![section],
        },
        ..Config::new()
    }
}

#[test]
fn every_format_round_trips() {
    let config = unsorted();
    for format in FORMATS {
        let text = config.encode(format).expect("encodes");
        let back = Config::decode(&text, format).expect("decodes");
        assert_eq!(back, config, "{format:?} did not round-trip");
    }
}

#[test]
fn section_order_survives_every_format() {
    let config = unsorted();
    let expected = vec!["Zebra", "Alpha", "Middle"];
    for format in FORMATS {
        let text = config.encode(format).expect("encodes");
        let back = Config::decode(&text, format).expect("decodes");
        let names: Vec<&str> = back
            .gitignore
            .sections
            .iter()
            .filter_map(|s| s.name.as_deref())
            .collect();
        assert_eq!(names, expected, "{format:?} reordered the sections");
    }
}

#[test]
fn the_three_encodings_are_interchangeable() {
    let config = unsorted();
    for from in FORMATS {
        for to in FORMATS {
            let once =
                Config::decode(&config.encode(from).expect("encodes"), from).expect("decodes");
            let twice = Config::decode(&once.encode(to).expect("encodes"), to).expect("decodes");
            assert_eq!(twice, config, "{from:?} -> {to:?} lost information");
        }
    }
}

#[test]
fn toml_nests_sections_under_the_target() {
    let text = unsorted().encode(Format::Toml).expect("encodes");
    assert!(text.contains("[[gitignore.section]]"), "{text}");
    assert!(text.contains("[[gitignore.section.rule]]"), "{text}");
    assert!(text.contains(r#"name = "ignore-as-config""#), "{text}");
    // level is omitted when it is the default of 2.
    assert!(!text.contains("level"), "{text}");
}

#[test]
fn rule_notes_and_both_lists_round_trip() {
    let config = wrapping(Section {
        name: Some("Logs".to_owned()),
        note: Some("Ignore Logs Pattern".to_owned()),
        rules: vec![
            Rule {
                ignore: vec!["*.log".to_owned()],
                ..Rule::default()
            },
            Rule {
                note: Some("keep the one\nwe actually read".to_owned()),
                add: vec!["important.log".to_owned()],
                ..Rule::default()
            },
        ],
        ..Section::default()
    });
    for format in FORMATS {
        let text = config.encode(format).expect("encodes");
        assert_eq!(Config::decode(&text, format).expect("decodes"), config);
    }
}

#[test]
fn comment_depth_round_trips() {
    let config = wrapping(Section {
        level: 1,
        ..section("Cargo", &["/target"], &[])
    });
    for format in FORMATS {
        let text = config.encode(format).expect("encodes");
        assert_eq!(Config::decode(&text, format).expect("decodes"), config);
    }
}

#[test]
fn defaults_are_omitted_and_restored() {
    let minimal = "version = 1\n\n[[gitignore.section]]\n\n[[gitignore.section.rule]]\nignore = [\"/target\"]\n";
    let config = Config::decode(minimal, Format::Toml).expect("decodes");
    let section = &config.gitignore.sections[0];
    assert_eq!(section.level, 2, "level defaults to 2");
    assert_eq!(section.name, None);
    assert_eq!(section.note, None);
    assert_eq!(section.rules[0].note, None);
    assert!(section.rules[0].add.is_empty());
    assert_eq!(config.name, None);
}

#[test]
fn a_config_with_no_sections_round_trips() {
    let config = Config::new();
    assert_eq!(config.version, VERSION);
    assert!(config.gitignore.is_empty());
    for format in FORMATS {
        let text = config.encode(format).expect("encodes");
        assert_eq!(Config::decode(&text, format).expect("decodes"), config);
    }
}

#[test]
fn validation_rejects_a_bang_in_an_add_pattern() {
    let err = wrapping(section("Logs", &[], &["!keep.log"]))
        .validate()
        .expect_err("rejects");
    assert!(err.to_string().contains("without the leading `!`"), "{err}");
    assert!(
        err.to_string().contains("gitignore.section[0].rule[0]"),
        "{err}"
    );
    assert!(err.to_string().contains("\"keep.log\""), "{err}");
}

#[test]
fn validation_rejects_an_empty_rule() {
    let config = wrapping(Section {
        rules: vec![Rule::default()],
        ..Section::default()
    });
    let err = config.validate().expect_err("rejects");
    assert!(err.to_string().contains("at least one"), "{err}");
}

/// A note with no patterns is a placeholder, not an error.
///
/// It still renders a line, so the rule cannot silently vanish the way a rule
/// with neither note nor patterns would.
#[test]
fn validation_accepts_a_note_only_rule() {
    let config = wrapping(Section {
        name: Some("Placeholder".to_owned()),
        rules: vec![Rule {
            note: Some("TODO: decide what to ignore here".to_owned()),
            ..Rule::default()
        }],
        ..Section::default()
    });
    config
        .validate()
        .expect("a note-only rule is a placeholder");
}

#[test]
fn validation_rejects_level_zero() {
    let config = wrapping(Section {
        level: 0,
        ..section("X", &["/x"], &[])
    });
    let err = config.validate().expect_err("rejects");
    assert!(err.to_string().contains("at least 1"), "{err}");
}

#[test]
fn validation_rejects_a_blank_pattern() {
    let err = wrapping(section("X", &["   "], &[]))
        .validate()
        .expect_err("rejects");
    assert!(err.to_string().contains("blank"), "{err}");
}

#[test]
fn validation_rejects_a_newline_in_a_pattern() {
    let err = wrapping(section("X", &["a\nb"], &[]))
        .validate()
        .expect_err("rejects");
    assert!(err.to_string().contains("newline"), "{err}");
}

#[test]
fn validation_rejects_a_note_without_a_name() {
    let config = wrapping(Section {
        name: None,
        note: Some("orphan".to_owned()),
        rules: vec![Rule {
            ignore: vec!["/x".to_owned()],
            ..Rule::default()
        }],
        ..Section::default()
    });
    let err = config.validate().expect_err("rejects");
    assert!(err.to_string().contains("needs a name"), "{err}");
}

#[test]
fn decode_runs_validation() {
    let bad =
        "version = 1\n\n[[gitignore.section]]\n\n[[gitignore.section.rule]]\nadd = [\"!x\"]\n";
    let err = Config::decode(bad, Format::Toml).expect_err("rejects");
    assert!(err.to_string().contains("without the leading `!`"), "{err}");
}

#[test]
fn an_unsupported_version_is_rejected() {
    let err = Config::decode("version = 99\n", Format::Toml).expect_err("rejects");
    assert!(
        matches!(
            err,
            Error::UnsupportedVersion {
                found: 99,
                expected: 1
            }
        ),
        "{err}"
    );
    assert!(err.to_string().contains("version 99 is not supported"));
}

#[test]
fn an_unknown_field_is_rejected_in_every_format() {
    let bad = [
        (
            Format::Toml,
            "version = 1\n\n[[gitignore.section]]\nignores = []\n",
        ),
        (
            Format::Json,
            r#"{"version":1,"gitignore":{"section":[{"ignores":[]}]}}"#,
        ),
        (
            Format::Yaml,
            "version: 1\ngitignore:\n  section:\n    - ignores: []\n",
        ),
    ];
    for (format, text) in bad {
        let err = Config::decode(text, format).expect_err("rejects unknown field");
        assert!(
            err.to_string().contains("ignores"),
            "{format:?} error should name the bad field, got: {err}"
        );
    }
}

#[test]
fn malformed_text_is_rejected_in_every_format() {
    for format in FORMATS {
        let err = Config::decode("{{{ not valid", format).expect_err("rejects");
        // Each variant carries the underlying parser's message.
        assert!(!err.to_string().is_empty(), "{format:?}");
    }
}

#[test]
fn format_is_inferred_from_the_extension() {
    for (path, expected) in [
        ("ignorefile.toml", Format::Toml),
        ("ignorefile.json", Format::Json),
        ("ignorefile.yaml", Format::Yaml),
        ("ignorefile.yml", Format::Yaml),
    ] {
        assert_eq!(Format::from_path(Path::new(path)).expect("known"), expected);
    }
}

#[test]
fn an_unknown_or_missing_extension_is_rejected() {
    let err = Format::from_path(Path::new("config.ini")).expect_err("rejects");
    assert!(
        matches!(err, Error::UnknownFormat(ref ext) if ext == "ini"),
        "{err}"
    );
    assert!(err.to_string().contains("expected one of toml, json, yaml"));

    let err = Format::from_path(Path::new("config")).expect_err("rejects");
    assert!(
        matches!(err, Error::UnknownFormat(ref ext) if ext.is_empty()),
        "{err}"
    );
}

#[test]
fn extension_names_the_conventional_suffix() {
    assert_eq!(Format::Toml.extension(), "toml");
    assert_eq!(Format::Json.extension(), "json");
    assert_eq!(Format::Yaml.extension(), "yaml");
}

#[test]
fn encoded_text_ends_with_a_newline() {
    // Every formatter in this repo, and hk's end-of-file-fixer, expects one.
    let config = unsorted();
    for format in FORMATS {
        let text = config.encode(format).expect("encodes");
        assert!(
            text.ends_with('\n'),
            "{format:?} did not end with a newline"
        );
    }
}

/// Fully populated, so no field is skipped during serialization.
fn every_field_set() -> Config {
    Config {
        header: Some("licence header".to_owned()),
        name: Some("n".to_owned()),
        version: VERSION,
        gitignore: Target {
            sections: vec![Section {
                level: 1,
                name: Some("s".to_owned()),
                note: Some("section note".to_owned()),
                rules: vec![Rule {
                    add: vec!["a".to_owned()],
                    ignore: vec!["/i".to_owned()],
                    note: Some("rule note".to_owned()),
                }],
            }],
        },
    }
}

#[test]
fn emitted_toml_is_already_formatter_canonical() {
    // `taplo` and most TOML formatters sort keys alphabetically while keeping
    // values ahead of tables. Emitting in that order means a generated config is
    // not rewritten by the first formatter that touches it, which would otherwise
    // show up as spurious diff noise on every `import` and `generate`.
    let text = every_field_set().encode(Format::Toml).expect("encodes");

    let position = |key: &str| {
        text.find(key)
            .unwrap_or_else(|| panic!("{key} missing from:\n{text}"))
    };
    // Root: name, version, then the gitignore table.
    assert!(position("name = ") < position("version = "), "{text}");
    assert!(
        position("version = ") < position("[[gitignore.section]]"),
        "{text}"
    );
    // Section: level, name, note, then the rule array of tables.
    assert!(position("level = ") < position("name = \"s\""), "{text}");
    assert!(position("name = \"s\"") < position("note = "), "{text}");
    assert!(
        position("note = ") < position("[[gitignore.section.rule]]"),
        "{text}"
    );
    // Rule: add, ignore, note.
    assert!(position("add = ") < position("ignore = "), "{text}");
    assert!(
        position("ignore = ") < position("note = \"rule note\""),
        "{text}"
    );
}

#[test]
fn reordering_fields_did_not_change_what_decodes() {
    // Field order is serialization order only: serde still accepts any order in.
    let scrambled = "\
version = 1
name = \"n\"
header = \"licence header\"

[[gitignore.section]]
note = \"section note\"
name = \"s\"
level = 1

  [[gitignore.section.rule]]
  note = \"rule note\"
  ignore = [\"/i\"]
  add = [\"a\"]
";
    assert_eq!(
        Config::decode(scrambled, Format::Toml).expect("decodes"),
        every_field_set()
    );
}

#[test]
fn format_is_parsed_from_a_bare_extension() {
    for (extension, expected) in [
        ("toml", Format::Toml),
        ("json", Format::Json),
        ("yaml", Format::Yaml),
        ("yml", Format::Yaml),
    ] {
        assert_eq!(Format::from_extension(extension).expect("known"), expected);
    }
    let err = Format::from_extension("ini").expect_err("rejects");
    assert!(
        matches!(err, Error::UnknownFormat(ref e) if e == "ini"),
        "{err}"
    );
}
