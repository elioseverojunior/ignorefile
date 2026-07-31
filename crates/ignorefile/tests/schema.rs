// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The published JSON Schema must not drift from the Rust types.
//!
//! There is no runtime schema validator: `Config::validate` is the enforcing
//! layer, and the schema exists so editors can offer completion and inline
//! errors. That split is only safe if the two cannot disagree, which is what
//! these tests hold in place.

use std::collections::BTreeSet;

use ignorefile::{Config, Format, Rule, Section, Target, VERSION};
use serde_json::Value;

const SCHEMA: &str = include_str!("../../../schema/ignorefile.schema.json");

fn schema() -> Value {
    let Ok(value) = serde_json::from_str::<Value>(SCHEMA) else {
        panic!("the schema is not valid JSON")
    };
    value
}

/// Property names the schema declares for one object.
fn declared(at: &Value) -> BTreeSet<String> {
    at.get("properties")
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

/// Field names serde emits for a value with every field populated.
fn emitted(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

/// Every optional field set, so nothing is skipped during serialization.
fn fully_populated() -> Config {
    Config {
        header: Some("h".to_owned()),
        version: VERSION,
        name: Some("n".to_owned()),
        gitignore: Target {
            sections: vec![Section {
                name: Some("s".to_owned()),
                // Not the default, so `skip_serializing_if` does not hide it.
                level: 1,
                note: Some("note".to_owned()),
                rules: vec![Rule {
                    note: Some("rule note".to_owned()),
                    ignore: vec!["/i".to_owned()],
                    add: vec!["a".to_owned()],
                }],
            }],
        },
    }
}

fn populated_json() -> Value {
    let Ok(json) = serde_json::to_value(fully_populated()) else {
        panic!("a Config must serialize")
    };
    json
}

#[test]
fn the_schema_is_valid_json_and_self_describing() {
    let schema = schema();
    assert_eq!(
        schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
        "the schema must declare its own dialect"
    );
    assert_eq!(schema["properties"]["version"]["const"], 1);
    assert_eq!(schema["required"][0], "version");
}

#[test]
fn the_schema_matches_the_rust_fields() {
    let schema = schema();
    let json = populated_json();

    assert_eq!(declared(&schema), emitted(&json), "top level drifted");
    assert_eq!(
        declared(&schema["$defs"]["target"]),
        emitted(&json["gitignore"]),
        "target drifted"
    );
    assert_eq!(
        declared(&schema["$defs"]["section"]),
        emitted(&json["gitignore"]["section"][0]),
        "section drifted"
    );
    assert_eq!(
        declared(&schema["$defs"]["rule"]),
        emitted(&json["gitignore"]["section"][0]["rule"][0]),
        "rule drifted"
    );
}

#[test]
fn every_schema_object_forbids_unknown_fields() {
    // Mirrors serde's `deny_unknown_fields`, so a typo fails in an editor too.
    let schema = schema();
    assert_eq!(schema["additionalProperties"], false);
    for name in ["target", "section", "rule"] {
        assert_eq!(
            schema["$defs"][name]["additionalProperties"], false,
            "{name} allows unknown fields"
        );
    }
}

#[test]
fn the_schema_encodes_the_rules_validation_enforces() {
    let schema = schema();
    let rule = &schema["$defs"]["rule"];
    // `add` patterns carry no leading `!`; Config::validate says the same.
    assert_eq!(rule["properties"]["add"]["items"]["not"]["pattern"], "^!");
    // Both pattern lists are optional in the schema. "at least one" is a
    // cross-field rule that belongs to Config::validate, where the message can
    // name the offending rule instead of reporting a failed subschema.
    assert!(
        rule.get("required").is_none(),
        "no field is required on a rule"
    );
    assert!(rule.get("anyOf").is_none(), "no disjunction on a rule");
    // A header needs at least one `#`.
    assert_eq!(
        schema["$defs"]["section"]["properties"]["level"]["minimum"],
        1
    );
    assert_eq!(
        schema["$defs"]["section"]["properties"]["level"]["default"],
        2
    );
}

#[test]
fn the_documented_example_decodes_and_renders() {
    // The example in docs/design/config-format.md must actually parse.
    let example = include_str!("corpus/canonical-config.toml");
    let config = Config::decode(example, Format::Toml).expect("decodes");
    assert_eq!(config.name.as_deref(), Some("ignore-as-config"));
    assert_eq!(config.gitignore.sections.len(), 2);
    assert_eq!(
        ignorefile::GitIgnore::from(&config).render(),
        include_str!("corpus/canonical.gitignore"),
        "the documented config and the documented output must agree"
    );
}
