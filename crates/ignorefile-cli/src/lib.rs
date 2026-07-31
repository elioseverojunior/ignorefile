// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `ignorefile` command line interface.
//!
//! The logic lives here rather than in `main.rs` so it can be tested: `main.rs`
//! is excluded from the coverage gate precisely because a binary entry point
//! cannot be exercised without spawning a subprocess.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use ignorefile::{Config, Format, GitIgnore, Rule, Section};

/// Manage `.gitignore` as structured, reviewable configuration.
#[derive(Debug, Parser)]
#[command(
    name = "ignorefile",
    version,
    long_version = concat!(
        env!("CARGO_PKG_VERSION"),
        " (", env!("TARGET"),
        ", built ", env!("BUILD_TIMESTAMP"),
        ", ", env!("RUSTC_VERSION"), ")"
    ),
    about,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a configuration, importing an existing `.gitignore` if there is one.
    Init(InitArgs),
    /// Convert a `.gitignore` into a configuration file.
    Import(CommonArgs),
    /// Add patterns to a section of the configuration.
    Add(AddArgs),
    /// Write the `.gitignore` from the configuration.
    Generate(CommonArgs),
    /// Check the configuration without writing anything.
    Validate(CommonArgs),
    /// Rewrite a `.gitignore` into the canonical form `import` accepts.
    Fmt(FmtArgs),
}

/// Paths every subcommand needs.
#[derive(Debug, Args)]
struct CommonArgs {
    /// Configuration file. The encoding is inferred from the extension: toml,
    /// json, yaml or yml.
    #[arg(long, short, default_value = "ignorefile.toml")]
    config: PathBuf,
    /// The `.gitignore` to read from or write to.
    #[arg(long, short, default_value = ".gitignore")]
    gitignore: PathBuf,
}

#[derive(Debug, Args)]
struct InitArgs {
    #[command(flatten)]
    common: CommonArgs,
    /// Overwrite the configuration if it already exists.
    #[arg(long)]
    force: bool,
}

/// `fmt` touches the `.gitignore` only, so it takes no configuration path.
#[derive(Debug, Args)]
struct FmtArgs {
    /// The `.gitignore` to rewrite.
    #[arg(long, short, default_value = ".gitignore")]
    gitignore: PathBuf,
    /// Report whether the file is canonical, writing nothing. For CI.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Args)]
struct AddArgs {
    #[command(flatten)]
    common: CommonArgs,
    /// Section to add to, created if absent. Defaults to the last section.
    #[arg(long, short)]
    section: Option<String>,
    /// Add as a re-inclusion (`!pattern`) rather than an ignore.
    #[arg(long)]
    allow: bool,
    /// Comment rendered above the added patterns.
    #[arg(long)]
    note: Option<String>,
    /// Patterns to add, verbatim. Keep the leading `/` to anchor to the root.
    #[arg(required = true)]
    patterns: Vec<String>,
}

/// Runs the parsed command, appending human-readable progress to `report`.
///
/// `program` is the name the binary was invoked as, so a hint can name the
/// command the user actually typed rather than a hardcoded one. The binary
/// ships under two names.
///
/// Messages are accumulated rather than written to a stream so that this
/// function performs no output I/O of its own: printing is `main`'s job. That
/// keeps every line here reachable by a test, which a fallible writer would not.
///
/// # Errors
///
/// Any filesystem failure, or a configuration that cannot be parsed or a
/// `.gitignore` that cannot be represented.
pub fn run(cli: &Cli, program: &str, report: &mut Vec<String>) -> Result<()> {
    match &cli.command {
        Command::Init(args) => init(args, report),
        Command::Import(args) => import(args, report),
        Command::Add(args) => add(args, program, report),
        Command::Generate(args) => generate(args, report),
        Command::Validate(args) => validate(args, report),
        Command::Fmt(args) => fmt(args, program, report),
    }
}

/// Rewrites a `.gitignore` into the form `import` reproduces byte for byte.
///
/// Only comments and blank lines move: patterns reach the configuration
/// verbatim and in order, so the set of ignored paths is identical either side.
/// That is what makes rewriting the user's file the right answer to a refused
/// import rather than an overreach - the result is reviewable as a diff, and
/// cannot change what git does.
fn fmt(args: &FmtArgs, program: &str, report: &mut Vec<String>) -> Result<()> {
    let path = &args.gitignore;
    let source = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    // A file that cannot form a valid configuration fails here rather than
    // being written out: no amount of reformatting would repair it.
    let canonical = GitIgnore::parse(&source).canonical()?.render();
    if canonical == source {
        report.push(format!("{} is already canonical", path.display()));
        return Ok(());
    }
    if args.check {
        bail!(
            "{} is not in canonical form; run `{program} fmt` to rewrite it",
            path.display()
        );
    }
    fs::write(path, &canonical).with_context(|| format!("writing {}", path.display()))?;
    report.push(format!("formatted {}", path.display()));
    Ok(())
}

fn read_config(args: &CommonArgs) -> Result<Config> {
    let format = Format::from_path(&args.config)?;
    let text = fs::read_to_string(&args.config)
        .with_context(|| format!("reading {}", args.config.display()))?;
    Config::decode(&text, format).with_context(|| format!("parsing {}", args.config.display()))
}

fn write_config(args: &CommonArgs, config: &Config) -> Result<()> {
    let format = Format::from_path(&args.config)?;
    // `encode` is fallible in general but cannot fail for a `Config`, which holds
    // only strings, integers, booleans and sequences. Combinators keep that
    // unreachable error branch inside core rather than making it a region here
    // that the 100% gate could never cover.
    config
        .encode(format)
        .map_err(anyhow::Error::from)
        .and_then(|text| {
            fs::write(&args.config, text)
                .with_context(|| format!("writing {}", args.config.display()))
        })
}

fn read_gitignore(path: &Path) -> Result<GitIgnore> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(GitIgnore::parse(&text))
}

/// Imports the `.gitignore` named by `args`, reporting what it found.
fn import_gitignore(args: &CommonArgs, report: &mut Vec<String>) -> Result<Config> {
    let config = Config::try_from(&read_gitignore(&args.gitignore)?)?;
    report.push(format!(
        "imported {} section(s) from {}",
        config.gitignore.sections.len(),
        args.gitignore.display()
    ));
    Ok(config)
}

fn init(args: &InitArgs, report: &mut Vec<String>) -> Result<()> {
    let path = &args.common.config;
    if path.exists() && !args.force {
        bail!(
            "{} already exists. Pass --force to overwrite it",
            path.display()
        );
    }
    let config = if args.common.gitignore.exists() {
        import_gitignore(&args.common, report)?
    } else {
        report.push(format!(
            "no {} found, starting an empty configuration",
            args.common.gitignore.display()
        ));
        Config::new()
    };
    write_config(&args.common, &config)?;
    report.push(format!("wrote {}", path.display()));
    Ok(())
}

fn import(args: &CommonArgs, report: &mut Vec<String>) -> Result<()> {
    let config = import_gitignore(args, report)?;
    write_config(args, &config)?;
    report.push(format!("wrote {}", args.config.display()));
    Ok(())
}

fn generate(args: &CommonArgs, report: &mut Vec<String>) -> Result<()> {
    let config = read_config(args)?;
    let text = GitIgnore::from(&config).render();
    fs::write(&args.gitignore, text)
        .with_context(|| format!("writing {}", args.gitignore.display()))?;
    report.push(format!(
        "wrote {} from {} section(s)",
        args.gitignore.display(),
        config.gitignore.sections.len()
    ));
    Ok(())
}

fn validate(args: &CommonArgs, report: &mut Vec<String>) -> Result<()> {
    // `read_config` already validates; reaching here means it passed.
    let config = read_config(args)?;
    report.push(format!(
        "{} is valid: {} section(s)",
        args.config.display(),
        config.gitignore.sections.len()
    ));
    Ok(())
}

/// Index of the section to add to, creating one when necessary.
fn target_section(config: &mut Config, name: Option<&str>) -> usize {
    if let Some(name) = name {
        if let Some(index) = config
            .gitignore
            .sections
            .iter()
            .position(|section| section.name.as_deref() == Some(name))
        {
            return index;
        }
        config.gitignore.sections.push(Section {
            name: Some(name.to_owned()),
            ..Section::default()
        });
    } else if config.gitignore.sections.is_empty() {
        config.gitignore.sections.push(Section::default());
    }
    config.gitignore.sections.len() - 1
}

fn add(args: &AddArgs, program: &str, report: &mut Vec<String>) -> Result<()> {
    let mut config = read_config(&args.common)?;
    let index = target_section(&mut config, args.section.as_deref());
    let section = &mut config.gitignore.sections[index];

    // Find-or-create the rule, the same way `--section` finds-or-creates the
    // section. `--note X` targets the rule already noted X; no note targets the
    // last un-noted rule. Repeated `add` calls then collect into one rule
    // instead of stacking one-pattern rules that all render the same.
    let found = section
        .rules
        .iter()
        .rposition(|rule| rule.note == args.note);
    let position = found.unwrap_or_else(|| {
        section.rules.push(Rule {
            note: args.note.clone(),
            ..Rule::default()
        });
        section.rules.len() - 1
    });
    let rule = &mut section.rules[position];
    let list = if args.allow {
        &mut rule.add
    } else {
        &mut rule.ignore
    };
    list.extend(args.patterns.iter().cloned());

    let verb = if found.is_some() { "appended" } else { "added" };
    let label = section
        .name
        .clone()
        .unwrap_or_else(|| "(unnamed)".to_owned());
    write_config(&args.common, &config)?;
    report.push(format!(
        "{verb} {} pattern(s) to section {label:?} in {}",
        args.patterns.len(),
        args.common.config.display()
    ));
    report.push(format!(
        "run `{program} generate` to update {}",
        args.common.gitignore.display()
    ));
    Ok(())
}
