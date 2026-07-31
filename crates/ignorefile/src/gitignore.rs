// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `.gitignore` model.
//!
//! Every line is kept verbatim, and classification is derived rather than
//! stored, so rendering can never disagree with the source it was parsed from.

/// What a line does, per git's own parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Empty or whitespace-only. Verified with `git check-ignore`: git treats
    /// these as inert.
    Blank,
    /// The first character is `#`. Verified with `git check-ignore`: only a `#`
    /// at position 0 opens a comment, because leading whitespace is significant.
    /// So `   # x` is a `Pattern` matching a file named `   # x`, not a comment.
    Comment,
    /// Anything else: an ignore rule.
    Pattern,
}

/// One line, preserved verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    raw: String,
}

impl Line {
    /// The line exactly as it appeared, including any trailing `\r`.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The line without a trailing `\r`.
    ///
    /// A CRLF file's `\r` is carried as line content rather than modelled as a
    /// line ending, so classification has to look past it.
    fn content(&self) -> &str {
        self.raw.strip_suffix('\r').unwrap_or(&self.raw)
    }

    /// What this line does.
    #[must_use]
    pub fn kind(&self) -> LineKind {
        let content = self.content();
        if content.trim().is_empty() {
            LineKind::Blank
        } else if content.starts_with('#') {
            LineKind::Comment
        } else {
            LineKind::Pattern
        }
    }

    /// For a comment line, the number of leading `#` and the text after them
    /// with one optional space removed. `None` for any other kind.
    ///
    /// The single space is removed rather than the text being trimmed, so that
    /// rendering `"#".repeat(level) + " " + text` reproduces the original for any
    /// comment written in the conventional form.
    #[must_use]
    pub fn comment(&self) -> Option<(usize, &str)> {
        if self.kind() != LineKind::Comment {
            return None;
        }
        let content = self.content();
        // Every `#` is one byte, so the count doubles as a byte offset.
        let level = content.chars().take_while(|c| *c == '#').count();
        let text = &content[level..];
        Some((level, text.strip_prefix(' ').unwrap_or(text)))
    }

    /// For a pattern line, whether it is negated and the pattern without the
    /// leading `!`. `None` for any other kind.
    #[must_use]
    pub fn pattern(&self) -> Option<(bool, &str)> {
        if self.kind() != LineKind::Pattern {
            return None;
        }
        let content = self.content();
        Some(
            content
                .strip_prefix('!')
                .map_or((false, content), |rest| (true, rest)),
        )
    }
}

/// The raw line that [`Line::content`] reads back as `content`: its inverse.
///
/// `content` strips one trailing `\r`, so content ending in `\r` has to be
/// written with one more than it means. git performs the same strip on every
/// line, even in a file with no CRLF ending anywhere, so this is not a Windows
/// concern. Verified with `git check-ignore` against an LF-only `.gitignore`
/// holding `Icon\r\r`: the file matched is the one named `Icon\r`, neither
/// `Icon` nor `Icon\r\r`. Emitting such a pattern verbatim would drop the `\r`
/// and silently change which file git ignores.
fn raw_of(mut content: String) -> String {
    if content.ends_with('\r') {
        content.push('\r');
    }
    content
}

/// A parsed `.gitignore`.
///
/// ```
/// use ignorefile::GitIgnore;
///
/// let source = "# Cargo\r\n/target\n\n!keep.log";
/// assert_eq!(GitIgnore::parse(source).render(), source);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitIgnore {
    lines: Vec<Line>,
    /// Whether the source ended with a newline.
    ///
    /// Tracked separately so a trailing newline does not masquerade as an extra
    /// blank line, which would otherwise put a phantom empty section at the end
    /// of every well-formed file.
    trailing_newline: bool,
}

impl GitIgnore {
    /// Parses `.gitignore` source.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let (body, trailing_newline) = source
            .strip_suffix('\n')
            .map_or((source, false), |body| (body, true));
        // An empty source has no lines at all, but a lone "\n" has one empty
        // line. Splitting "" would otherwise yield a spurious [""] for both.
        let lines = if body.is_empty() && !trailing_newline {
            Vec::new()
        } else {
            body.split('\n')
                .map(|raw| Line {
                    raw: raw.to_owned(),
                })
                .collect()
        };
        Self {
            lines,
            trailing_newline,
        }
    }

    /// Renders the model back to `.gitignore` source.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = self
            .lines
            .iter()
            .map(Line::raw)
            .collect::<Vec<_>>()
            .join("\n");
        if self.trailing_newline {
            out.push('\n');
        }
        out
    }

    /// The lines of the file, in order.
    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Builds a `.gitignore` from rendered lines.
    ///
    /// A non-empty result always ends with a newline, which is what POSIX text
    /// files and every formatter in this repository expect. An empty result has
    /// no newline, so an empty config does not produce a one-blank-line file.
    pub(crate) fn from_lines(lines: Vec<String>) -> Self {
        Self {
            trailing_newline: !lines.is_empty(),
            lines: lines
                .into_iter()
                .map(|content| Line {
                    raw: raw_of(content),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GitIgnore, LineKind};

    fn kinds(source: &str) -> Vec<LineKind> {
        GitIgnore::parse(source)
            .lines()
            .iter()
            .map(super::Line::kind)
            .collect()
    }

    #[test]
    fn classifies_blank_comment_and_pattern() {
        assert_eq!(
            kinds("# Cargo\n\n/target\n"),
            vec![LineKind::Comment, LineKind::Blank, LineKind::Pattern]
        );
    }

    #[test]
    fn whitespace_only_line_is_blank() {
        assert_eq!(kinds("   \n\t\n"), vec![LineKind::Blank, LineKind::Blank]);
    }

    #[test]
    fn hash_after_leading_space_is_a_pattern() {
        assert_eq!(kinds("   # not a comment\n"), vec![LineKind::Pattern]);
    }

    #[test]
    fn escaped_hash_is_a_pattern() {
        assert_eq!(kinds("\\#literal\n"), vec![LineKind::Pattern]);
    }

    #[test]
    fn classification_looks_past_a_trailing_cr() {
        assert_eq!(
            kinds("# c\r\n\r\n/t\r\n"),
            vec![LineKind::Comment, LineKind::Blank, LineKind::Pattern]
        );
    }

    #[test]
    fn comment_reports_level_and_text() {
        let g = GitIgnore::parse("# Cargo\n## Mise\n###\n#NoSpace\n");
        let parts: Vec<_> = g.lines().iter().filter_map(super::Line::comment).collect();
        assert_eq!(
            parts,
            vec![(1, "Cargo"), (2, "Mise"), (3, ""), (1, "NoSpace")]
        );
    }

    #[test]
    fn comment_strips_a_trailing_cr_from_its_text() {
        let g = GitIgnore::parse("# Cargo\r\n");
        assert_eq!(g.lines()[0].comment(), Some((1, "Cargo")));
    }

    #[test]
    fn comment_is_none_for_other_kinds() {
        let g = GitIgnore::parse("/target\n\n");
        assert!(g.lines().iter().all(|l| l.comment().is_none()));
    }

    #[test]
    fn pattern_reports_negation() {
        let g = GitIgnore::parse("/target\n!keep.log\n");
        let parts: Vec<_> = g.lines().iter().filter_map(super::Line::pattern).collect();
        assert_eq!(parts, vec![(false, "/target"), (true, "keep.log")]);
    }

    #[test]
    fn pattern_strips_a_trailing_cr() {
        let g = GitIgnore::parse("/target\r\n");
        assert_eq!(g.lines()[0].pattern(), Some((false, "/target")));
    }

    #[test]
    fn pattern_is_none_for_other_kinds() {
        let g = GitIgnore::parse("# c\n\n");
        assert!(g.lines().iter().all(|l| l.pattern().is_none()));
    }
}
