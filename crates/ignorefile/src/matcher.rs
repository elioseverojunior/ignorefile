// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deciding whether git would ignore a path.
//!
//! Every rule here was checked against `git check-ignore` rather than read off
//! the documentation, and `tests/differential.rs` keeps it that way.

use crate::gitignore::GitIgnore;

/// One component of a compiled pattern.
enum Segment {
    /// `**`: zero or more path segments.
    AnyDepth,
    /// One path segment, matched with `*`, `?`, `[...]` and `\` escapes.
    Name(String),
}

/// A compiled ignore rule.
struct Pattern {
    /// A `!` rule re-includes rather than ignores.
    negate: bool,
    /// A trailing `/` restricts the rule to directories.
    dir_only: bool,
    segments: Vec<Segment>,
}

/// Removes trailing spaces, which git ignores unless they are backslash-quoted.
fn strip_trailing_spaces(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b' ' {
        // An odd number of preceding backslashes quotes the space.
        let mut backslashes = 0;
        let mut index = end - 1;
        while index > 0 && bytes[index - 1] == b'\\' {
            backslashes += 1;
            index -= 1;
        }
        if backslashes % 2 == 1 {
            break;
        }
        end -= 1;
    }
    &text[..end]
}

impl Pattern {
    /// Compiles one rule, or `None` if it can never match.
    fn compile(negate: bool, text: &str) -> Option<Self> {
        let text = strip_trailing_spaces(text);
        let (text, dir_only) = text.strip_suffix('/').map_or((text, false), |t| (t, true));
        if text.is_empty() {
            return None;
        }
        // A rule containing a slash is anchored to the directory holding the
        // `.gitignore`; one without matches a name at any depth, which is the
        // same thing as an implicit leading `**/`.
        let anchored = text.contains('/');
        let body = text.strip_prefix('/').unwrap_or(text);
        if body.is_empty() {
            return None;
        }
        let segments = if anchored {
            body.split('/').map(Segment::parse).collect()
        } else {
            std::iter::once(Segment::AnyDepth)
                .chain(std::iter::once(Segment::parse(body)))
                .collect()
        };
        Some(Self {
            negate,
            dir_only,
            segments,
        })
    }

    fn matches(&self, path: &[&str]) -> bool {
        match_segments(&self.segments, path)
    }
}

impl Segment {
    fn parse(text: &str) -> Self {
        if text == "**" {
            Self::AnyDepth
        } else {
            Self::Name(text.to_owned())
        }
    }
}

fn match_segments(pattern: &[Segment], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((Segment::AnyDepth, rest)) => {
            if rest.is_empty() {
                // Verified with git: a trailing `/**` matches what is inside a
                // directory but not the directory itself, so it needs at least
                // one segment left to consume.
                return !path.is_empty();
            }
            (0..=path.len()).any(|skip| match_segments(rest, &path[skip..]))
        }
        Some((Segment::Name(name), rest)) => match path.split_first() {
            Some((head, tail)) => matches_name(name, head) && match_segments(rest, tail),
            None => false,
        },
    }
}

/// Glob-matches one path segment. `*` and `?` never cross a `/`, which holds
/// here because they only ever see a single segment.
fn matches_name(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    match_here(&pattern, 0, &name, 0)
}

fn match_here(pattern: &[char], mut p: usize, name: &[char], mut n: usize) -> bool {
    while p < pattern.len() {
        match pattern[p] {
            '*' => return (n..=name.len()).any(|skip| match_here(pattern, p + 1, name, skip)),
            '?' if n < name.len() => {
                p += 1;
                n += 1;
            }
            '?' => return false,
            '[' => {
                // Verified with `git check-ignore`: git's wildmatch ABORTS on an
                // unterminated bracket expression, so the rule matches nothing at
                // all. It does not fall back to treating `[` as a literal the way
                // POSIX fnmatch does; `br[acket` ignores nothing, and only the
                // escaped `br\[acket` matches a file of that name.
                let Some((matched, next)) = match_class(pattern, p, name.get(n).copied()) else {
                    return false;
                };
                if !matched {
                    return false;
                }
                p = next;
                n += 1;
            }
            '\\' if p + 1 < pattern.len() => {
                if name.get(n) != Some(&pattern[p + 1]) {
                    return false;
                }
                p += 2;
                n += 1;
            }
            expected => {
                if name.get(n) != Some(&expected) {
                    return false;
                }
                p += 1;
                n += 1;
            }
        }
    }
    n == name.len()
}

/// Matches a `[...]` bracket expression against one character.
///
/// Returns whether it matched and the index just past the `]`, or `None` when
/// the expression is unterminated.
fn match_class(pattern: &[char], start: usize, candidate: Option<char>) -> Option<(bool, usize)> {
    let mut index = start + 1;
    let negated = matches!(pattern.get(index), Some('!' | '^'));
    if negated {
        index += 1;
    }
    let mut found = false;
    let mut first = true;
    while index < pattern.len() {
        // A `]` in the first position is a literal, per glob convention.
        if pattern[index] == ']' && !first {
            return Some((found != negated, index + 1));
        }
        first = false;
        let is_range = pattern.get(index + 1) == Some(&'-')
            && pattern.get(index + 2).is_some_and(|end| *end != ']');
        if is_range {
            if let Some(candidate) = candidate {
                if pattern[index] <= candidate && candidate <= pattern[index + 2] {
                    found = true;
                }
            }
            index += 3;
        } else {
            if candidate == Some(pattern[index]) {
                found = true;
            }
            index += 1;
        }
    }
    None
}

impl GitIgnore {
    /// Whether git would ignore `path`.
    ///
    /// `path` is relative to the directory holding this `.gitignore`, uses `/`
    /// separators, and has no trailing slash. `is_dir` matters because a rule
    /// ending in `/` only matches directories, and git decides that from the
    /// filesystem rather than from the path text.
    #[must_use]
    pub fn is_ignored(&self, path: &str, is_dir: bool) -> bool {
        let patterns: Vec<Pattern> = self
            .lines()
            .iter()
            .filter_map(|line| line.pattern())
            .filter_map(|(negate, text)| Pattern::compile(negate, text))
            .collect();

        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        // Git never descends into an excluded directory, so a `!` rule cannot
        // rescue anything beneath one. Checking ancestors outermost-first is what
        // makes `build/` beat a later `!build/keep`.
        for depth in 1..segments.len() {
            if decide(&patterns, &segments[..depth], true) == Some(true) {
                return true;
            }
        }
        decide(&patterns, &segments, is_dir).unwrap_or(false)
    }
}

/// The verdict of the last rule that matched, or `None` if none did.
fn decide(patterns: &[Pattern], path: &[&str], is_dir: bool) -> Option<bool> {
    patterns
        .iter()
        .filter(|pattern| is_dir || !pattern.dir_only)
        .filter(|pattern| pattern.matches(path))
        .map(|pattern| !pattern.negate)
        .next_back()
}
