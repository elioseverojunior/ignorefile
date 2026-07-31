// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Conversion between a `.gitignore` and its configuration.
//!
//! The grammar is the `###` / `##` / `#` layering, read back:
//!
//! - `###` alone, at the top, is the configuration's name.
//! - `##` or deeper opens a **section**; its first line is the name and the rest
//!   of the block is the section note.
//! - a bare `#` is a **rule note**, attaching to the patterns beneath it and
//!   belonging to the section above. With no patterns to attach to it becomes a
//!   note-only rule, so a trailing `# Always Add` is kept rather than dropped.
//!
//! Depth decides, not blank lines. That matters because the renderer puts a
//! blank line before a noted rule: if blank lines delimited sections, rendering
//! and import would not be inverse.
//!
//! Import is lossless or it refuses: [`Config::try_from`] renders the
//! configuration it just built and compares it against the source.

use crate::config::{Config, NAME_LEVEL, Rule, SECTION_LEVEL, Section, Target, VERSION};
use crate::error::Error;
use crate::gitignore::{GitIgnore, Line};

/// A body line, already classified.
///
/// Classifying up front makes the body loop total: a line between blank lines is
/// a comment or a pattern and nothing else, so there is no third case to leave
/// unreachable.
enum Entry<'a> {
    Comment(&'a str),
    Pattern { negate: bool, text: &'a str },
}

fn entry(line: &Line) -> Option<Entry<'_>> {
    if let Some((_, text)) = line.comment() {
        return Some(Entry::Comment(text));
    }
    line.pattern()
        .map(|(negate, text)| Entry::Pattern { negate, text })
}

/// A run of lines between blank lines.
struct Group<'a> {
    /// Leading comments: `(depth, text)`.
    header: Vec<(usize, &'a str)>,
    /// Everything after the header, in order.
    body: Vec<Entry<'a>>,
}

/// Splits lines into blank-line-delimited groups, dropping empty ones.
fn groups(lines: &[Line]) -> Vec<Group<'_>> {
    lines
        .split(|line| line.comment().is_none() && line.pattern().is_none())
        .filter(|run| !run.is_empty())
        .map(|run| {
            let split = run
                .iter()
                .position(|line| line.comment().is_none())
                .unwrap_or(run.len());
            Group {
                header: run[..split].iter().filter_map(Line::comment).collect(),
                body: run[split..].iter().filter_map(entry).collect(),
            }
        })
        .collect()
}

/// Joins note lines, or `None` when there are none.
fn note_of(lines: &[&str]) -> Option<String> {
    (!lines.is_empty()).then(|| lines.join("\n"))
}

impl Group<'_> {
    /// Whether this group is the configuration's `###` banner rather than a
    /// section: one comment at the name depth, with no patterns under it.
    fn as_config_name(&self) -> Option<&str> {
        match self.header.as_slice() {
            [(level, text)] if *level == NAME_LEVEL && self.body.is_empty() => Some(text),
            _ => None,
        }
    }

    /// This group as a section, or `None` when it continues the one above.
    ///
    /// Depth decides, and nothing else: `##` and deeper open a section, a bare
    /// `#` is a rule note. A group of bare patterns has no header at all and
    /// continues whatever section precedes it.
    ///
    /// A comment group with no patterns under it used to be a header whatever
    /// its depth, so that a trailing `# Always Add` was not a note with nothing
    /// to attach to. Note-only rules removed that need - the comment is kept
    /// either way - and the exception cost more than it bought: a level-1
    /// header renders as a line indistinguishable from a rule note, so a `#`
    /// group followed by a bare pattern group came back as a note on the next
    /// parse. The file was then not a fixed point, which `fmt` has to be.
    fn as_section(&self) -> Option<Section> {
        let ((level, first), rest) = self.header.split_first()?;
        if *level < SECTION_LEVEL {
            return None;
        }
        let rest: Vec<&str> = rest.iter().map(|(_, text)| *text).collect();
        Some(Section {
            name: Some((*first).to_owned()),
            level: *level,
            note: note_of(&rest),
            rules: self.to_rules(Vec::new()),
        })
    }

    /// Whether this group could be the configuration's header: plain `#`
    /// comments with no patterns under them.
    ///
    /// Depth matters. `##` and deeper is structural, so a `## Foo` block is a
    /// section header even at the top of the file; only bare `#` lines read as
    /// prose, which is what a licence block is. The caller additionally requires
    /// the `###` banner to follow, so this alone never reclassifies a group.
    fn is_preamble(&self) -> bool {
        !self.header.is_empty()
            && self.body.is_empty()
            && self.header.iter().all(|(level, _)| *level == 1)
    }

    /// This group's comment text joined as a header block.
    fn as_preamble(&self) -> Option<String> {
        note_of(
            &self
                .header
                .iter()
                .map(|(_, text)| *text)
                .collect::<Vec<&str>>(),
        )
    }

    /// The rules this group contributes to the section above it, with its
    /// leading comments becoming the first rule's note.
    fn to_continuation_rules(&self) -> Vec<Rule> {
        self.to_rules(self.header.iter().map(|(_, text)| *text).collect())
    }

    /// Splits the body into rules.
    ///
    /// A comment closes the current rule and becomes the next one's note. An
    /// `ignore` pattern following an `add` pattern also starts a new rule,
    /// because rendering emits all of a rule's `ignore` before its `add`, so
    /// keeping them together would reorder them.
    fn to_rules<'a>(&'a self, leading: Vec<&'a str>) -> Vec<Rule> {
        let mut rules: Vec<Rule> = Vec::new();
        let mut pending: Vec<&str> = leading;
        // Index of the rule still accepting patterns, if any.
        let mut open: Option<usize> = None;

        for entry in &self.body {
            match entry {
                Entry::Comment(text) => {
                    // A comment closes the current rule and becomes the next note.
                    open = None;
                    pending.push(text);
                }
                Entry::Pattern { negate, text } => {
                    let index = match open {
                        // An `ignore` after an `add` must start a new rule:
                        // rendering emits all of a rule's `ignore` before its
                        // `add`, so keeping them together would reorder them.
                        Some(index) if *negate || rules[index].add.is_empty() => index,
                        _ => {
                            rules.push(Rule {
                                note: note_of(&std::mem::take(&mut pending)),
                                ..Rule::default()
                            });
                            let index = rules.len() - 1;
                            open = Some(index);
                            index
                        }
                    };
                    let list = if *negate {
                        &mut rules[index].add
                    } else {
                        &mut rules[index].ignore
                    };
                    list.push((*text).to_owned());
                }
            }
        }
        // Comments left over after the last pattern become a note-only rule.
        // Dropping them instead, which is what this used to do, lost the lines
        // and made the re-render differ, so import refused a file it could in
        // fact represent.
        //
        // This cannot swallow a section header: `groups` splits header from body
        // at the first non-comment line, so an all-comment group has an empty
        // body and `as_section` claims it before ever reaching here. Only
        // comments FOLLOWING a pattern can still be pending at this point.
        if let Some(note) = note_of(&pending) {
            rules.push(Rule {
                note: Some(note),
                ..Rule::default()
            });
        }
        rules
    }
}

/// Renders a comment, avoiding a trailing space when there is no text so that a
/// bare `#` survives a round-trip.
fn comment(level: usize, text: &str) -> String {
    let hashes = "#".repeat(level);
    if text.is_empty() {
        hashes
    } else {
        format!("{hashes} {text}")
    }
}

/// Renders a multi-line note, one `#` line per line.
fn note_lines(note: Option<&String>) -> Vec<String> {
    note.map(|note| note.split('\n').map(|line| comment(1, line)).collect())
        .unwrap_or_default()
}

fn section_lines(section: &Section) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(name) = &section.name {
        lines.push(comment(section.level, name));
    }
    lines.extend(note_lines(section.note.as_ref()));

    for rule in &section.rules {
        // A noted rule is set off by a blank line, unless it opens the section,
        // which would otherwise put a stray blank at the top of the file.
        //
        // There is no exception for a note-only rule. The blank is what makes
        // the note a group of its own on re-import: without one the comment
        // merges into whatever comment block precedes it, becoming part of a
        // section note, or touches the patterns below it and is read as their
        // note instead. Either way the file stops being a fixed point, which
        // `fmt` cannot afford. A note-only rule used to suppress the blank,
        // back when a comment-only group re-imported as a section rather than
        // as a rule; `as_section` no longer does that.
        if rule.note.is_some() && !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(note_lines(rule.note.as_ref()));
        lines.extend(rule.ignore.iter().cloned());
        lines.extend(rule.add.iter().map(|pattern| format!("!{pattern}")));
    }
    lines
}

/// Renders one target's sections.
///
/// Takes a `&Target` rather than a `&Config` on purpose: `.dockerignore` and
/// friends share this line grammar, so adding one is a second call here rather
/// than a second renderer.
fn target_lines(header: Option<&String>, name: Option<&String>, target: &Target) -> Vec<String> {
    // The header reuses `note_lines`, which renders an empty line as a bare `#`,
    // so the blank separator inside an SPDX block survives verbatim.
    let mut lines: Vec<String> = note_lines(header);
    if let Some(name) = name {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(comment(NAME_LEVEL, name));
    }
    for section in &target.sections {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(section_lines(section));
    }
    lines
}

/// The first line at which two renderings differ, one-based, or `None` if they
/// are identical.
fn first_difference(expected: &str, actual: &str) -> Option<(usize, String, String)> {
    let expected: Vec<&str> = expected.split('\n').collect();
    let actual: Vec<&str> = actual.split('\n').collect();
    (0..expected.len().max(actual.len())).find_map(|index| {
        let want = expected.get(index).copied().unwrap_or_default();
        let got = actual.get(index).copied().unwrap_or_default();
        (want != got).then(|| (index + 1, want.to_owned(), got.to_owned()))
    })
}

impl From<&Config> for GitIgnore {
    /// Renders a configuration as `.gitignore` source, one blank line between
    /// sections.
    fn from(config: &Config) -> Self {
        Self::from_lines(target_lines(
            config.header.as_ref(),
            config.name.as_ref(),
            &config.gitignore,
        ))
    }
}

/// Builds the configuration a `.gitignore` describes, without checking that it
/// renders back to the same bytes.
///
/// Split out from the import because `canonical` needs exactly this. The
/// canonical form of a file is whatever this configuration renders to, so
/// demanding the round-trip here would make the one operation that repairs a
/// non-canonical file refuse to run on one.
fn build(source: &GitIgnore) -> Result<Config, Error> {
    let groups = groups(source.lines());
    // The preamble is peeled first, and ONLY when the banner follows it. A
    // leading comment-only group is otherwise a section, which
    // `a_comment_with_no_patterns_is_a_section_at_its_own_depth` pins: a
    // broader rule would silently turn a top-of-file `# Editors` into a
    // header. Requiring the banner keeps that case untouched.
    let (header, after_header) = groups
        .split_first()
        .filter(|(first, rest)| {
            first.is_preamble()
                && rest
                    .first()
                    .is_some_and(|next| next.as_config_name().is_some())
        })
        .map_or((None, groups.as_slice()), |(first, rest)| {
            (first.as_preamble(), rest)
        });
    let (name, rest) = after_header
        .split_first()
        .and_then(|(first, rest)| {
            first
                .as_config_name()
                .map(|name| (Some(name.to_owned()), rest))
        })
        .unwrap_or((None, after_header));
    let mut sections: Vec<Section> = Vec::new();
    for group in rest {
        if let Some(section) = group.as_section() {
            sections.push(section);
            continue;
        }
        let rules = group.to_continuation_rules();
        match sections.last_mut() {
            Some(section) => section.rules.extend(rules),
            // Patterns before any header live in an unnamed leading section.
            None => sections.push(Section {
                rules,
                ..Section::default()
            }),
        }
    }
    let config = Config {
        version: VERSION,
        header,
        name,
        gitignore: Target { sections },
    };
    config.validate()?;
    Ok(config)
}

impl GitIgnore {
    /// This file rewritten into the form import reproduces.
    ///
    /// Idempotent, which is what lets `fmt --check` compare a file against its
    /// own canonical form and trust the answer. Only comments and blank lines
    /// move: see [`Error::NotCanonical`].
    ///
    /// ```
    /// use ignorefile::GitIgnore;
    ///
    /// // Rendering emits exactly one blank line between rules, never two.
    /// let messy = GitIgnore::parse("# A\n/a\n\n\n# B\n/b\n");
    /// let tidy = messy.canonical().expect("a valid configuration");
    /// assert_eq!(tidy.render(), "# A\n/a\n\n# B\n/b\n");
    /// ```
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] if the file cannot form a valid configuration, which
    /// no amount of reformatting would repair.
    pub fn canonical(&self) -> Result<Self, Error> {
        build(self).map(|config| Self::from(&config))
    }
}

impl TryFrom<&GitIgnore> for Config {
    type Error = Error;

    /// Imports a `.gitignore`, refusing rather than rewriting it silently.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] if the result would break a schema rule, or
    /// [`Error::NotCanonical`] if re-rendering the imported configuration would
    /// not reproduce the source byte for byte. The latter is a formatting
    /// difference, never a change to which paths are ignored; `canonical`
    /// produces the form that imports cleanly.
    fn try_from(source: &GitIgnore) -> Result<Self, Error> {
        let config = build(source)?;
        match first_difference(&source.render(), &GitIgnore::from(&config).render()) {
            Some((line, expected, actual)) => Err(Error::NotCanonical {
                line,
                expected,
                actual,
            }),
            None => Ok(config),
        }
    }
}
