// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end tests for the `init`, `import`, `add` and `generate` commands.
//!
//! Each test drives the real argument parser via [`Cli::try_parse_from`], so the
//! flags, defaults and subcommand names are part of what is under test rather
//! than being bypassed.
//!
//! Note the absence of `expect` in the helpers below: clippy's
//! `allow-expect-in-tests` covers `#[test]` functions and `cfg(test)` modules,
//! not free functions in an integration-test crate, and `expect_used` is denied.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use ignorefile_cli::{Cli, run};
use tempfile::TempDir;

/// A scratch directory with paths for a config and a `.gitignore`.
struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let Ok(dir) = TempDir::new() else {
            panic!("could not create a temporary directory")
        };
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn config(&self) -> PathBuf {
        self.path("ignorefile.toml")
    }

    fn gitignore(&self) -> PathBuf {
        self.path(".gitignore")
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path(name);
        assert!(
            fs::write(&path, contents).is_ok(),
            "could not write {}",
            path.display()
        );
        path
    }
}

fn read(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        panic!("could not read {}", path.display())
    };
    text
}

/// Runs the CLI as `program`, returning its report joined into one string.
fn cli_as(program: &str, args: &[&str]) -> Result<String, String> {
    let mut command_line = vec![program];
    command_line.extend_from_slice(args);
    let parsed = match Cli::try_parse_from(&command_line) {
        Ok(parsed) => parsed,
        Err(error) => return Err(error.to_string()),
    };
    let mut report = Vec::new();
    match run(&parsed, program, &mut report) {
        Ok(()) => Ok(report.join("\n")),
        Err(error) => Err(format!("{error:#}")),
    }
}

fn cli(args: &[&str]) -> Result<String, String> {
    cli_as("ignorefile", args)
}

fn ok(args: &[&str]) -> String {
    match cli(args) {
        Ok(out) => out,
        Err(error) => panic!("expected success, got: {error}"),
    }
}

fn err(args: &[&str]) -> String {
    match cli(args) {
        Ok(out) => panic!("expected failure, got success: {out}"),
        Err(error) => error,
    }
}

const SECTIONED: &str = "## Cargo\n/target\n\n## Mise\n/mise.local.toml\n!mise.lock\n";

#[test]
fn init_imports_an_existing_gitignore() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", SECTIONED);
    let config = fixture.config();

    // `-g` must be explicit: its default is relative to the process working
    // directory, which during a test run is the repository, not the fixture.
    let out = ok(&[
        "init",
        "-c",
        &config.to_string_lossy(),
        "-g",
        &fixture.gitignore().to_string_lossy(),
    ]);
    assert!(out.contains("imported 2 section(s)"), "{out}");
    assert!(out.contains("wrote"), "{out}");

    let text = read(&config);
    assert!(text.contains("version = 1"), "{text}");
    assert!(text.contains("[[gitignore.section]]"), "{text}");
    assert!(text.contains(r#"name = "Cargo""#), "{text}");
    assert!(text.contains("[[gitignore.section.rule]]"), "{text}");
}

#[test]
fn init_without_a_gitignore_writes_an_empty_config() {
    let fixture = Fixture::new();
    let config = fixture.config();

    let out = ok(&["init", "-c", &config.to_string_lossy()]);
    assert!(out.contains("no"), "{out}");
    assert!(out.contains("empty configuration"), "{out}");
    assert_eq!(read(&config).trim(), "version = 1");
}

#[test]
fn init_refuses_to_clobber_an_existing_config_without_force() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");

    let error = err(&["init", "-c", &config.to_string_lossy()]);
    assert!(error.contains("already exists"), "{error}");
    assert!(error.contains("--force"), "{error}");
}

#[test]
fn init_force_overwrites() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    fixture.write(".gitignore", "# Cargo\n/target\n");

    let out = ok(&[
        "init",
        "--force",
        "-c",
        &config.to_string_lossy(),
        "-g",
        &fixture.gitignore().to_string_lossy(),
    ]);
    assert!(out.contains("imported 1 section(s)"), "{out}");
    assert!(read(&config).contains("Cargo"));
}

#[test]
fn import_then_generate_reproduces_the_original() {
    let fixture = Fixture::new();
    let original = fixture.write(".gitignore", SECTIONED);
    let config = fixture.config();
    let original_arg = original.to_string_lossy().into_owned();
    let config_arg = config.to_string_lossy().into_owned();

    ok(&["import", "-g", &original_arg, "-c", &config_arg]);
    // Regenerate to a different path so the comparison is meaningful.
    let regenerated = fixture.path("regenerated");
    let out = ok(&[
        "generate",
        "-c",
        &config_arg,
        "-g",
        &regenerated.to_string_lossy(),
    ]);
    assert!(out.contains("wrote"), "{out}");
    assert_eq!(read(&regenerated), SECTIONED, "round-trip through the CLI");
}

#[test]
fn every_encoding_works_end_to_end() {
    for name in ["c.toml", "c.json", "c.yaml", "c.yml"] {
        let fixture = Fixture::new();
        fixture.write(".gitignore", SECTIONED);
        let config = fixture.path(name);
        let gitignore = fixture.gitignore();
        ok(&[
            "import",
            "-c",
            &config.to_string_lossy(),
            "-g",
            &gitignore.to_string_lossy(),
        ]);
        let regenerated = fixture.path("out");
        ok(&[
            "generate",
            "-c",
            &config.to_string_lossy(),
            "-g",
            &regenerated.to_string_lossy(),
        ]);
        assert_eq!(read(&regenerated), SECTIONED, "{name} did not round-trip");
    }
}

#[test]
fn import_rejects_an_unrepresentable_gitignore() {
    let fixture = Fixture::new();
    // A comment after a pattern is representable now -- it becomes a note-only
    // rule -- so this uses a header whose depth drops mid-block, which still
    // cannot be rendered back. Still line 2, so the assertions are unchanged.
    let gitignore = fixture.write(".gitignore", "# one\n## two\n/target\n");
    let config = fixture.config();

    let error = err(&[
        "import",
        "-g",
        &gitignore.to_string_lossy(),
        "-c",
        &config.to_string_lossy(),
    ]);
    assert!(error.contains("line 2"), "{error}");
    assert!(!config.exists(), "nothing should be written on refusal");
}

#[test]
fn import_reports_a_missing_gitignore() {
    let fixture = Fixture::new();
    let error = err(&[
        "import",
        "-g",
        &fixture.path("absent").to_string_lossy(),
        "-c",
        &fixture.config().to_string_lossy(),
    ]);
    assert!(error.contains("reading"), "{error}");
    assert!(error.contains("absent"), "{error}");
}

#[test]
fn an_unrecognized_config_extension_is_reported() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", "/target\n");
    let error = err(&[
        "import",
        "-c",
        &fixture.path("config.ini").to_string_lossy(),
        "-g",
        &fixture.gitignore().to_string_lossy(),
    ]);
    assert!(error.contains("unrecognized config format"), "{error}");
}

#[test]
fn generate_reports_an_unparsable_config() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "this is not toml {{{\n");
    let error = err(&["generate", "-c", &config.to_string_lossy()]);
    assert!(error.contains("parsing"), "{error}");
}

#[test]
fn generate_reports_an_unwritable_target() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let unwritable = fixture.path("no-such-dir").join(".gitignore");
    let error = err(&[
        "generate",
        "-c",
        &config.to_string_lossy(),
        "-g",
        &unwritable.to_string_lossy(),
    ]);
    assert!(error.contains("writing"), "{error}");
}

#[test]
fn add_appends_to_a_named_section() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", SECTIONED);
    let config = fixture.config();
    let config_arg = config.to_string_lossy().into_owned();
    ok(&[
        "init",
        "-c",
        &config_arg,
        "-g",
        &fixture.gitignore().to_string_lossy(),
    ]);

    // "appended", not "added": the Cargo section already has an un-noted rule
    // holding /target, and a note-less add targets it rather than stacking a
    // second rule that would render identically.
    let out = ok(&["add", "-c", &config_arg, "-s", "Cargo", "/debug", "*.rs.bk"]);
    assert!(out.contains("appended 2 pattern(s)"), "{out}");
    assert!(out.contains("\"Cargo\""), "{out}");
    assert!(
        out.contains("generate"),
        "should hint at the next step: {out}"
    );

    let regenerated = fixture.path("out");
    ok(&[
        "generate",
        "-c",
        &config_arg,
        "-g",
        &regenerated.to_string_lossy(),
    ]);
    assert_eq!(
        read(&regenerated),
        "## Cargo\n/target\n/debug\n*.rs.bk\n\n## Mise\n/mise.local.toml\n!mise.lock\n"
    );
}

#[test]
fn add_creates_a_section_that_does_not_exist() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let config_arg = config.to_string_lossy().into_owned();

    ok(&["add", "-c", &config_arg, "-s", "Logs", "*.log"]);
    let regenerated = fixture.path("out");
    ok(&[
        "generate",
        "-c",
        &config_arg,
        "-g",
        &regenerated.to_string_lossy(),
    ]);
    assert_eq!(read(&regenerated), "## Logs\n*.log\n");
}

#[test]
fn add_with_allow_writes_a_negation() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let config_arg = config.to_string_lossy().into_owned();

    ok(&["add", "-c", &config_arg, "-s", "Logs", "*.log"]);
    ok(&[
        "add",
        "-c",
        &config_arg,
        "-s",
        "Logs",
        "--allow",
        "keep.log",
    ]);
    let regenerated = fixture.path("out");
    ok(&[
        "generate",
        "-c",
        &config_arg,
        "-g",
        &regenerated.to_string_lossy(),
    ]);
    assert_eq!(read(&regenerated), "## Logs\n*.log\n!keep.log\n");
}

#[test]
fn add_without_a_section_uses_the_last_one() {
    let fixture = Fixture::new();
    let config = fixture.write(
        "ignorefile.toml",
        "version = 1\n\n[[gitignore.section]]\nname = \"A\"\n\n[[gitignore.section]]\nname = \"B\"\n",
    );
    let config_arg = config.to_string_lossy().into_owned();

    ok(&["add", "-c", &config_arg, "/late"]);
    let regenerated = fixture.path("out");
    ok(&[
        "generate",
        "-c",
        &config_arg,
        "-g",
        &regenerated.to_string_lossy(),
    ]);
    assert_eq!(read(&regenerated), "## A\n\n## B\n/late\n");
}

#[test]
fn add_to_an_empty_config_creates_an_unnamed_section() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let config_arg = config.to_string_lossy().into_owned();

    let out = ok(&["add", "-c", &config_arg, "/target"]);
    assert!(out.contains("(unnamed)"), "{out}");
    let regenerated = fixture.path("out");
    ok(&[
        "generate",
        "-c",
        &config_arg,
        "-g",
        &regenerated.to_string_lossy(),
    ]);
    assert_eq!(read(&regenerated), "/target\n");
}

#[test]
fn add_accepts_a_note() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let config_arg = config.to_string_lossy().into_owned();

    ok(&["add", "-c", &config_arg, "-s", "Logs", "*.log"]);
    ok(&[
        "add",
        "-c",
        &config_arg,
        "-s",
        "Logs",
        "--note",
        "why",
        "*.tmp",
    ]);
    let regenerated = fixture.path("out");
    ok(&[
        "generate",
        "-c",
        &config_arg,
        "-g",
        &regenerated.to_string_lossy(),
    ]);
    assert_eq!(read(&regenerated), "## Logs\n*.log\n\n# why\n*.tmp\n");
}

#[test]
fn add_keeps_interleaved_rules_in_order() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", "build/\n!build/keep\nbuild/keep/tmp\n");
    let config = fixture.config();
    let config_arg = config.to_string_lossy().into_owned();
    ok(&[
        "import",
        "-c",
        &config_arg,
        "-g",
        &fixture.gitignore().to_string_lossy(),
    ]);
    assert!(read(&config).contains("[[gitignore.section.rule]]"));

    ok(&["add", "-c", &config_arg, "--allow", "build/keep/keepme"]);
    let regenerated = fixture.path("out");
    ok(&[
        "generate",
        "-c",
        &config_arg,
        "-g",
        &regenerated.to_string_lossy(),
    ]);
    assert_eq!(
        read(&regenerated),
        "build/\n!build/keep\nbuild/keep/tmp\n!build/keep/keepme\n"
    );
}

#[test]
fn validate_accepts_a_good_config_and_rejects_a_bad_one() {
    let fixture = Fixture::new();
    let good = fixture.write("good.toml", "version = 1\n");
    let out = ok(&["validate", "-c", &good.to_string_lossy()]);
    assert!(out.contains("is valid"), "{out}");

    let bad = fixture.write(
        "bad.toml",
        "version = 1\n\n[[gitignore.section]]\n\n[[gitignore.section.rule]]\nadd = [\"!x\"]\n",
    );
    let error = err(&["validate", "-c", &bad.to_string_lossy()]);
    assert!(error.contains("without the leading `!`"), "{error}");
}

#[test]
fn add_requires_at_least_one_pattern() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let error = err(&["add", "-c", &config.to_string_lossy()]);
    assert!(
        error.contains("required") || error.contains("PATTERNS"),
        "{error}"
    );
}

#[test]
fn add_reports_a_missing_config() {
    let fixture = Fixture::new();
    let error = err(&["add", "-c", &fixture.config().to_string_lossy(), "/x"]);
    assert!(error.contains("reading"), "{error}");
}

#[test]
fn init_refuses_an_unrepresentable_gitignore() {
    let fixture = Fixture::new();
    // Unrepresentable for the same reason as in
    // `import_rejects_an_unrepresentable_gitignore`.
    fixture.write(".gitignore", "# one\n## two\n/target\n");
    let error = err(&[
        "init",
        "-c",
        &fixture.config().to_string_lossy(),
        "-g",
        &fixture.gitignore().to_string_lossy(),
    ]);
    assert!(error.contains("line 2"), "{error}");
    assert!(!fixture.config().exists(), "nothing written on refusal");
}

#[test]
fn init_reports_an_unwritable_config_path() {
    let fixture = Fixture::new();
    let config = fixture.path("no-such-dir").join("ignorefile.toml");
    let error = err(&[
        "init",
        "-c",
        &config.to_string_lossy(),
        "-g",
        &fixture.path("absent").to_string_lossy(),
    ]);
    assert!(error.contains("writing"), "{error}");
}

#[test]
fn generate_reports_an_unrecognized_config_extension() {
    let fixture = Fixture::new();
    let error = err(&[
        "generate",
        "-c",
        &fixture.path("config.ini").to_string_lossy(),
        "-g",
        &fixture.gitignore().to_string_lossy(),
    ]);
    assert!(error.contains("unrecognized config format"), "{error}");
}

/// Read succeeds but write fails, which is the only way to reach `add`'s write
/// error path. Unix-only because it needs file permissions.
#[cfg(unix)]
#[test]
fn add_reports_an_unwritable_config() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let Ok(metadata) = fs::metadata(&config) else {
        panic!("could not stat the config")
    };
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o444);
    assert!(fs::set_permissions(&config, permissions).is_ok());

    let error = err(&["add", "-c", &config.to_string_lossy(), "/x"]);
    assert!(error.contains("writing"), "{error}");
}

#[test]
fn the_version_flag_reports_the_build_stamp() {
    // Proves build.rs is genuinely wired in, not vestigial.
    let error = err(&["--version"]);
    assert!(error.contains(env!("CARGO_PKG_VERSION")), "{error}");
}

#[test]
fn repeated_add_collects_into_one_rule() {
    // Two calls with no note target the same un-noted rule, rather than stacking
    // two one-pattern rules that render identically.
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let config_arg = config.to_string_lossy().into_owned();

    ok(&["add", "-c", &config_arg, "-s", "Logs", "*.log"]);
    let out = ok(&["add", "-c", &config_arg, "-s", "Logs", "*.tmp"]);
    assert!(out.contains("appended 1 pattern(s)"), "{out}");

    let text = read(&config);
    assert_eq!(
        text.matches("[[gitignore.section.rule]]").count(),
        1,
        "should be one rule: {text}"
    );
    assert!(text.contains(r#"ignore = ["*.log", "*.tmp"]"#), "{text}");
}

#[test]
fn add_appends_to_the_rule_with_a_matching_note() {
    // Repeating `--note` targets the rule already carrying it, which is how a
    // multi-pattern commented group is built up over several calls.
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let config_arg = config.to_string_lossy().into_owned();

    ok(&[
        "add",
        "-c",
        &config_arg,
        "-s",
        "P",
        "--note",
        "Git worktrees",
        "/.worktrees/",
    ]);
    let out = ok(&[
        "add",
        "-c",
        &config_arg,
        "-s",
        "P",
        "--note",
        "Git worktrees",
        "/worktrees/",
    ]);
    assert!(out.contains("appended"), "{out}");

    let text = read(&config);
    assert_eq!(
        text.matches("[[gitignore.section.rule]]").count(),
        1,
        "{text}"
    );
    assert!(
        text.contains(r#"ignore = ["/.worktrees/", "/worktrees/"]"#),
        "{text}"
    );
    assert!(text.contains(r#"note = "Git worktrees""#), "{text}");
}

#[test]
fn a_different_note_still_starts_a_new_rule() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let config_arg = config.to_string_lossy().into_owned();

    ok(&["add", "-c", &config_arg, "-s", "P", "--note", "one", "/a"]);
    ok(&["add", "-c", &config_arg, "-s", "P", "--note", "two", "/b"]);
    let text = read(&config);
    assert_eq!(
        text.matches("[[gitignore.section.rule]]").count(),
        2,
        "{text}"
    );
}

#[test]
fn the_hint_names_the_command_that_was_invoked() {
    let fixture = Fixture::new();
    let config = fixture.write("ignorefile.toml", "version = 1\n");
    let config_arg = config.to_string_lossy().into_owned();

    let Ok(out) = cli_as("ign", &["add", "-c", &config_arg, "/x"]) else {
        panic!("add should succeed")
    };
    assert!(out.contains("run `ign generate`"), "{out}");

    let Ok(out) = cli_as("ignorefile", &["add", "-c", &config_arg, "/y"]) else {
        panic!("add should succeed")
    };
    assert!(out.contains("run `ignorefile generate`"), "{out}");
}

#[test]
fn fmt_rewrites_a_non_canonical_gitignore_so_import_accepts_it() {
    let fixture = Fixture::new();
    // Two blank lines between sections, and a header whose depth drops
    // mid-block. Both are formatting, so both survive `fmt` as the same rules.
    let gitignore = fixture.write(".gitignore", "# one\n## two\n/target\n\n\n## Mise\n/x\n");
    let path = gitignore.to_string_lossy().into_owned();
    let config = fixture.config();
    let config_arg = config.to_string_lossy().into_owned();

    let before = err(&["import", "-g", &path, "-c", &config_arg]);
    assert!(before.contains("canonical"), "{before}");

    let out = ok(&["fmt", "-g", &path]);
    assert!(out.contains("formatted"), "{out}");

    // The point of the command: its output is what `import` accepts.
    ok(&["import", "-g", &path, "-c", &config_arg]);
}

#[test]
fn fmt_leaves_a_canonical_file_untouched() {
    let fixture = Fixture::new();
    let gitignore = fixture.write(".gitignore", SECTIONED);
    let path = gitignore.to_string_lossy().into_owned();

    let out = ok(&["fmt", "-g", &path]);
    assert!(out.contains("already canonical"), "{out}");
    assert_eq!(
        read(&gitignore),
        SECTIONED,
        "a canonical file must not move"
    );
}

#[test]
fn fmt_check_reports_without_writing() {
    let fixture = Fixture::new();
    let source = "# one\n## two\n/target\n";
    let gitignore = fixture.write(".gitignore", source);
    let path = gitignore.to_string_lossy().into_owned();

    let error = err(&["fmt", "--check", "-g", &path]);
    assert!(error.contains("not in canonical form"), "{error}");
    assert!(error.contains("ignorefile fmt"), "{error}");
    assert_eq!(read(&gitignore), source, "--check must not write");

    // Clean files pass the check, so CI stays quiet once `fmt` has run.
    ok(&["fmt", "-g", &path]);
    let out = ok(&["fmt", "--check", "-g", &path]);
    assert!(out.contains("already canonical"), "{out}");
}

#[test]
fn fmt_reports_a_file_it_cannot_repair() {
    let fixture = Fixture::new();
    // A lone `!` negates the empty pattern. Validation rejects it, and no
    // amount of reformatting would help, so `fmt` refuses rather than writing.
    let gitignore = fixture.write(".gitignore", "## S\n!\n");
    let path = gitignore.to_string_lossy().into_owned();

    let error = err(&["fmt", "-g", &path]);
    assert!(error.contains("invalid config"), "{error}");
    assert_eq!(read(&gitignore), "## S\n!\n", "nothing written on refusal");
}

#[test]
fn fmt_reports_a_missing_gitignore() {
    let fixture = Fixture::new();
    let missing = fixture.path("nope.gitignore");
    let error = err(&["fmt", "-g", &missing.to_string_lossy()]);
    assert!(error.contains("reading"), "{error}");
}
