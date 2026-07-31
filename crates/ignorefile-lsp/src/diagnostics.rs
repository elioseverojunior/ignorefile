// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What is wrong with an ignore file, and on which line.
//!
//! Pure functions over text. No protocol, no I/O, so every rule below is a
//! direct unit test rather than something you have to drive an editor to reach.

use std::collections::HashMap;

use ignorefile::{Config, Error, GitIgnore, LineKind};

/// How serious a finding is. The numbers are the LSP severity values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Something that changes behaviour, or stops the file being representable.
    Warning = 2,
    /// Something redundant, which is safe but worth cleaning up.
    Hint = 4,
}

/// One problem, anchored to a zero-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Zero-based line, as LSP expects.
    pub line: usize,
    /// Number of characters on that line, so the whole line is underlined.
    pub length: usize,
    pub severity: Severity,
    pub message: String,
}

/// Every problem in `source`, ordered by line.
#[must_use]
pub fn analyze(source: &str) -> Vec<Diagnostic> {
    let parsed = GitIgnore::parse(source);
    let mut found = Vec::new();
    found.extend(not_representable(source, &parsed));
    found.extend(duplicate_patterns(&parsed));
    found.extend(defeated_negations(&parsed));
    found.sort_by_key(|diagnostic| diagnostic.line);
    found
}

/// Length of a one-based line, for the underline span.
fn width(source: &str, one_based: usize) -> usize {
    source
        .split('\n')
        .nth(one_based.saturating_sub(1))
        .map_or(0, |line| line.chars().count())
}

/// The file is not in the canonical form `import` reproduces.
fn not_representable(source: &str, parsed: &GitIgnore) -> Option<Diagnostic> {
    let error = Config::try_from(parsed).err()?;
    let (line, message) = match error {
        Error::NotCanonical { line, .. } => (
            line,
            "this line is not in canonical form, so `ignorefile import` would \
             refuse the file; run `ignorefile fmt` to normalize it"
                .to_owned(),
        ),
        // Validation failures carry no line, so they anchor to the first.
        other => (1, other.to_string()),
    };
    Some(Diagnostic {
        line: line.saturating_sub(1),
        length: width(source, line),
        severity: Severity::Warning,
        message,
    })
}

/// The same pattern written twice. Harmless to git, noise to a reader.
fn duplicate_patterns(parsed: &GitIgnore) -> Vec<Diagnostic> {
    let mut first_seen: HashMap<&str, usize> = HashMap::new();
    let mut found = Vec::new();
    for (index, line) in parsed.lines().iter().enumerate() {
        if line.kind() != LineKind::Pattern {
            continue;
        }
        let raw = line.raw();
        if let Some(original) = first_seen.get(raw) {
            found.push(Diagnostic {
                line: index,
                length: raw.chars().count(),
                severity: Severity::Hint,
                message: format!("duplicate of the pattern on line {}", original + 1),
            });
        } else {
            first_seen.insert(raw, index);
        }
    }
    found
}

/// A `!` rule that can never take effect, because a parent directory is already
/// excluded and git does not descend into one.
fn defeated_negations(parsed: &GitIgnore) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for (index, line) in parsed.lines().iter().enumerate() {
        let Some((true, pattern)) = line.pattern() else {
            continue;
        };
        let Some(parent) = pattern
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|(p, _)| p)
        else {
            continue;
        };
        if parent.is_empty() || !parsed.is_ignored(parent, true) {
            continue;
        }
        found.push(Diagnostic {
            line: index,
            length: line.raw().chars().count(),
            severity: Severity::Warning,
            message: format!(
                "this re-inclusion has no effect: {parent:?} is excluded, and git \
                 does not descend into an excluded directory"
            ),
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::{Severity, analyze};

    #[test]
    fn a_clean_file_has_nothing_to_report() {
        assert!(analyze("## Logs\n*.log\n\n# keep\n!important.log\n").is_empty());
        assert!(analyze("").is_empty());
    }

    #[test]
    fn a_non_canonical_file_is_reported_on_its_line() {
        // Two blank lines between sections: rendering emits one, so import
        // refuses rather than normalize.
        let found = analyze("# A\n/a\n\n\n# B\n/b\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3, "zero-based line 4");
        assert_eq!(found[0].severity, Severity::Warning);
        assert!(found[0].message.contains("canonical"));
        // The diagnostic has to name the command that fixes it, or the reader
        // is left to guess which of the blank lines is wrong.
        assert!(found[0].message.contains("fmt"), "{}", found[0].message);
    }

    #[test]
    fn a_validation_failure_anchors_to_the_first_line() {
        // A lone `!` negates the empty pattern, which validation rejects. That
        // error carries no line, so it lands on line 0.
        let found = analyze("## S\n!\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 0);
        let message = &found[0].message;
        assert!(message.contains("blank"), "{message}");
    }

    #[test]
    fn a_repeated_pattern_is_a_hint() {
        let found = analyze("## S\n*.log\n*.tmp\n*.log\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
        assert_eq!(found[0].severity, Severity::Hint);
        let message = &found[0].message;
        assert!(message.contains("line 2"), "{message}");
        assert_eq!(found[0].length, "*.log".len());
    }

    #[test]
    fn a_negation_under_an_excluded_directory_is_reported() {
        // git does not descend into `build/`, so `!build/keep` can never apply.
        let found = analyze("build/\n!build/keep\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[0].severity, Severity::Warning);
        let message = &found[0].message;
        assert!(message.contains("no effect"), "{message}");
        assert!(message.contains("build"), "{message}");
    }

    #[test]
    fn a_negation_under_an_allowed_directory_is_fine() {
        assert!(analyze("build/*\n!build/keep\n").is_empty());
    }

    #[test]
    fn a_top_level_negation_has_no_parent_to_check() {
        assert!(analyze("*.log\n!important.log\n").is_empty());
        // A leading slash leaves an empty parent, which is not a directory rule.
        assert!(analyze("*.log\n!/important.log\n").is_empty());
    }

    #[test]
    fn findings_come_back_in_line_order() {
        let found = analyze("build/\n*.log\n*.log\n!build/keep\n");
        let lines: Vec<usize> = found.iter().map(|d| d.line).collect();
        assert_eq!(lines, vec![2, 3]);
    }

    #[test]
    fn the_underline_covers_the_whole_line() {
        let found = analyze("## S\n*.log\n*.log\n");
        assert_eq!(found[0].length, 5);
    }
}
