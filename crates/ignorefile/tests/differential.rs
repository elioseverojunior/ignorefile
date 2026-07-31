// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Differential test: our matcher must agree with `git check-ignore`.
//!
//! The round-trip property in `roundtrip.rs` proves the model preserves the
//! *bytes* of a `.gitignore`. This proves it understands their *meaning*. Nothing
//! else in the suite would catch a pattern that we match and git does not.
//!
//! Git is the oracle, so it is a hard requirement rather than a skip: this is a
//! git tool, every other part of the toolchain already needs git, and a silent
//! skip would let the matcher rot while the suite stayed green.
//!
//! One repository is created for the whole run and only its `.gitignore` is
//! rewritten per case. Creating a repository per case would dominate the runtime,
//! and the interesting space is the patterns, not the paths.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use ignorefile::GitIgnore;
use proptest::prelude::*;
use tempfile::TempDir;

/// Every path the oracle is asked about, and whether it is a directory.
///
/// Deliberately nested, so the rule that an ignored directory cannot have its
/// contents re-included is actually exercised.
const UNIVERSE: &[(&str, bool)] = &[
    ("a", true),
    ("a/b", true),
    ("a/b/c", true),
    ("build", true),
    ("build/keep", true),
    ("logs", true),
    ("sub", true),
    ("f", false),
    ("top.log", false),
    ("a/f", false),
    ("a/f.log", false),
    ("a/b/f", false),
    ("a/b/c/deep.log", false),
    ("build/out", false),
    ("build/keep/x", false),
    ("logs/app.log", false),
    ("sub/f", false),
    ("README.md", false),
    (".hidden", false),
    // A literal `[` in a name, so an unterminated bracket expression has
    // something to match as an ordinary character.
    ("br[acket", false),
];

struct Repo {
    dir: TempDir,
}

impl Repo {
    fn new() -> Self {
        let Ok(dir) = TempDir::new() else {
            panic!("could not create a temporary directory")
        };
        let repo = Self { dir };
        repo.git(&["init", "-q", "."]);
        // Keep the oracle honest: a global or user gitignore would otherwise
        // contribute rules our matcher never sees.
        repo.git(&["config", "core.excludesFile", "/dev/null"]);
        for (path, is_dir) in UNIVERSE {
            let full = repo.dir.path().join(path);
            let created = if *is_dir {
                fs::create_dir_all(&full)
            } else {
                full.parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .and_then(|()| fs::write(&full, b""))
            };
            assert!(created.is_ok(), "could not create {path}");
        }
        repo
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.dir.path())
            .output();
        let Ok(output) = output else {
            panic!(
                "could not run `git {}`. Git is required for the differential test",
                args.join(" ")
            )
        };
        assert!(
            output.status.success(),
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The set of paths git considers ignored under `gitignore`.
    fn ignored(&self, gitignore: &str) -> BTreeSet<String> {
        let path = self.dir.path().join(".gitignore");
        assert!(
            fs::write(&path, gitignore).is_ok(),
            "could not write .gitignore"
        );

        let mut stdin = String::new();
        for (path, _) in UNIVERSE {
            stdin.push_str(path);
            stdin.push('\n');
        }
        let output = Command::new("git")
            .args(["check-ignore", "--stdin"])
            .current_dir(self.dir.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write as _;
                if let Some(mut sink) = child.stdin.take() {
                    sink.write_all(stdin.as_bytes())?;
                }
                child.wait_with_output()
            });
        let Ok(output) = output else {
            panic!("could not run `git check-ignore`")
        };
        // Exit 0 = something matched, 1 = nothing matched. Anything else is a
        // real failure and must not be read as "nothing is ignored".
        assert!(
            matches!(output.status.code(), Some(0 | 1)),
            "git check-ignore failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn repo() -> &'static Mutex<Repo> {
    static REPO: OnceLock<Mutex<Repo>> = OnceLock::new();
    REPO.get_or_init(|| Mutex::new(Repo::new()))
}

/// What our matcher says is ignored, over the same universe.
fn ours(gitignore: &str) -> BTreeSet<String> {
    let parsed = GitIgnore::parse(gitignore);
    UNIVERSE
        .iter()
        .filter(|(path, is_dir)| parsed.is_ignored(path, *is_dir))
        .map(|(path, _)| (*path).to_owned())
        .collect()
}

/// Asserts our verdicts match git's for every path in the universe.
fn assert_agrees(gitignore: &str) {
    let theirs = {
        let Ok(repo) = repo().lock() else {
            panic!("the shared repository mutex was poisoned")
        };
        repo.ignored(gitignore)
    };
    let mine = ours(gitignore);
    assert_eq!(
        mine,
        theirs,
        "\n.gitignore:\n{gitignore}\nwe say ignored: {mine:?}\ngit says:       {theirs:?}\n\
         only we ignore: {:?}\nonly git ignores: {:?}",
        mine.difference(&theirs).collect::<Vec<_>>(),
        theirs.difference(&mine).collect::<Vec<_>>()
    );
}

/// Cases chosen because each isolates one rule that is easy to get wrong.
const CASES: &[(&str, &str)] = &[
    ("empty", ""),
    ("comment only", "# nothing here\n"),
    ("plain name, any depth", "f\n"),
    ("anchored to the root", "/f\n"),
    ("extension glob", "*.log\n"),
    ("anchored extension glob", "/*.log\n"),
    ("directory only", "logs/\n"),
    ("directory only, nested", "build/\n"),
    ("a file cannot match a directory-only rule", "f/\n"),
    ("negation after the rule", "*.log\n!top.log\n"),
    (
        "negation before the rule has no effect",
        "!top.log\n*.log\n",
    ),
    (
        "an ignored directory cannot be re-included",
        "build/\n!build/keep\n!build/keep/x\n",
    ),
    ("mid-path slash anchors", "a/f\n"),
    ("slash in the middle, deeper", "a/b/f\n"),
    ("leading double star", "**/f\n"),
    ("trailing double star", "a/**\n"),
    ("double star in the middle", "a/**/f\n"),
    ("single star does not cross a slash", "a*f\n"),
    ("question mark", "?\n"),
    ("character class", "[fF]\n"),
    ("negated character class", "[!a-e]\n"),
    ("range class", "[a-c]/b\n"),
    ("escaped hash is a literal", "\\#nope\n"),
    ("escaped bang is a literal", "\\!nope\n"),
    ("dotfile", ".hidden\n"),
    ("trailing spaces are stripped", "f   \n"),
    ("escaped trailing space is kept", "f\\ \n"),
    ("blank lines are inert", "\n\n*.log\n\n"),
    ("crlf line endings", "*.log\r\n!top.log\r\n"),
    ("several rules interacting", "*\n!a\n!a/**\n"),
    (
        "re-include a file under an allowed directory",
        "a/*\n!a/f\n",
    ),
    ("directory glob", "*/\n"),
    ("everything", "**\n"),
    // Degenerate rules that must compile to nothing rather than matching all.
    ("a lone slash", "/\n"),
    ("a double slash", "//\n"),
    // `?` with nothing left to consume.
    ("question mark past the end", "f?\n"),
    // An unterminated bracket expression aborts the match, so the rule matches
    // nothing. Only the escaped form matches a literal `[`.
    ("unterminated bracket matches nothing", "br[acket\n"),
    ("unterminated bracket, no candidate", "[abc\n"),
    ("escaped bracket matches literally", "br\\[acket\n"),
    // An escape that actually matches, not just one that fails.
    ("escaped dot matches a literal dot", "README\\.md\n"),
    ("escaped star is a literal", "\\*\n"),
    // A class evaluated when the name has already been consumed.
    ("class past the end", "f[a-c]\n"),
    ("class with a literal closing bracket first", "[]f]\n"),
];

#[test]
fn agrees_with_git_on_curated_cases() {
    for (label, gitignore) in CASES {
        let theirs = {
            let Ok(repo) = repo().lock() else {
                panic!("the shared repository mutex was poisoned")
            };
            repo.ignored(gitignore)
        };
        let mine = ours(gitignore);
        assert_eq!(mine, theirs, "case {label:?} disagreed on {gitignore:?}");
    }
}

/// Pattern fragments the generator composes. Chosen to collide with the universe
/// above, so most generated rules actually match something.
fn pattern() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("f".to_owned()),
        Just("a".to_owned()),
        Just("b".to_owned()),
        Just("build".to_owned()),
        Just("logs".to_owned()),
        Just("*.log".to_owned()),
        Just("*".to_owned()),
        Just("?".to_owned()),
        Just("[a-f]".to_owned()),
        Just("[!a-f]".to_owned()),
        Just("a/f".to_owned()),
        Just("a/*".to_owned()),
        Just("a/**".to_owned()),
        Just("a/**/f".to_owned()),
        Just("**/f".to_owned()),
        Just("**/*.log".to_owned()),
        Just("/f".to_owned()),
        Just("/a".to_owned()),
        Just("build/keep".to_owned()),
        Just(".hidden".to_owned()),
        Just("READ*.md".to_owned()),
    ]
}

/// A whole `.gitignore`: a few rules, each optionally negated or directory-only.
fn gitignore_text() -> impl Strategy<Value = String> {
    let rule = (pattern(), any::<bool>(), any::<bool>()).prop_map(|(body, negate, dir_only)| {
        let mut line = String::new();
        if negate {
            line.push('!');
        }
        line.push_str(&body);
        // A trailing slash after a wildcard is legal but makes the rule
        // directory-only, which is exactly the interaction worth generating.
        if dir_only && !body.ends_with('*') {
            line.push('/');
        }
        line
    });
    prop::collection::vec(rule, 1..5).prop_map(|rules| {
        let mut text = rules.join("\n");
        text.push('\n');
        text
    })
}

proptest! {
    // Each case spawns git once, so keep the count modest but meaningful.
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// The real assertion: for any generated `.gitignore`, our verdict for every
    /// path matches git's exactly.
    #[test]
    fn agrees_with_git_on_generated_rules(gitignore in gitignore_text()) {
        assert_agrees(&gitignore);
    }
}

#[test]
fn the_universe_is_materialized_as_described() {
    // Guards the oracle itself: if a "directory" were created as a file, every
    // directory-only case would silently pass for the wrong reason.
    let Ok(repo) = repo().lock() else {
        panic!("the shared repository mutex was poisoned")
    };
    for (path, is_dir) in UNIVERSE {
        let full: &Path = &repo.dir.path().join(path);
        assert_eq!(
            full.is_dir(),
            *is_dir,
            "{path} was created with the wrong kind"
        );
    }
}
