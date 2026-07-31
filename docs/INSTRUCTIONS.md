---
name: INSTRUCTIONS.md
created_by: <elioseverojunior@gmail.com>
---

<!--
SPDX-FileCopyrightText: ignorefile contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Instructions

The original brief for this project, kept as the record of intent. Resolved
questions are recorded under [Decisions](#decisions); the executable plan lives
in [./ROADMAP.md](./ROADMAP.md).

I want to build a new project that must implement `gitignore-as-code` or
`gitignore-as-config`. Please help me define the project name.
It must be pure Rust.

## Project Scope

The project has multiple surfaces:

- **ignorefile** (core library) -- `.gitignore` parsing, pattern and
  anchoring semantics, section and comment structure, the structured
  configuration model, and rendering back to `.gitignore`
- **ignorefile-cli** -- CLI with subcommands: import, generate, check,
  fmt, plus intelligent management features
- **ignorefile-wasm** -- WASM target for browser-based conversion
- **ignorefile-mcp** -- MCP (Model Context Protocol) integration so AI
  agents can manage ignore rules without editing raw text
- **ignorefile-lsp** -- LSP (Language Server Protocol) for IDE real-time
  diagnostics on ignore rules

### UX

I want to be able to import any gitignore and generate a
ignorefile.{toml,json,yaml}

```gitignore
# Cargo
/target

## Mise
/mise.local.toml
!mise.lock

# Always Add
```

```toml
[[cargo]]
ignores = [
    "target"
]

[[Mise]]
ignores = [
    "/mise.local.toml"
]
add = [
    "mise.lock"
]

[["Always Add"]]
add = []
```

We could use like gitleaks configuration where we can define the default and setup our configs, of course validating the paths to valid .gitignore expressions.

```toml
# gitleaks toml config sample -- quoted for its SHAPE, not its contents.
# Note that these values are REGEXES; .gitignore uses globs, and the two are not
# interchangeable. See docs/design/config-format.md.

title = "gitleaks config"

[extend]
# Pull in gitleaks' maintained ruleset rather than re-deriving ~150 rules here.
# Upstream rule fixes then flow in on every gitleaks upgrade.
useDefault = true

[[allowlists]]
description = "Repo-wide false positives: vendored data, generated/build files, license texts"
# `paths` are regexes matched against the file path (relative to repo root).
# Allowlisting by path keeps detection strong on real source while carving out
# files that are data, not credentials.
paths = [
  # Cargo's lockfile — package checksums trip entropy heuristics.
  '''Cargo\.lock$''',
  # Build artifacts (gitignored, but excluded here for `gitleaks dir` scans).
  '''target/.*''',
  # License texts and their per-crate symlinks.
  '''LICENSES/.*''',
  '''(^|/)LICENSE-(MIT|APACHE)$''',
  # This config itself, so any example patterns below never self-trigger.
  '''\.gitleaks\.toml$''',
]
```

Another `gitignore-as-code` proposal

```toml
# ignorefile config sketch.
# NOTE: `[[rules]]` followed by `[cargo]` does not nest -- it parses as an empty
# `rules` element plus a separate top-level `cargo` table. Kept verbatim as the
# record of intent; see docs/design/config-format.md for the evaluation.

title = "GITIGNORE AS CODE config"

[extend]
# Pull in a maintained upstream ruleset rather than re-deriving the same rules in
# every repository. Upstream fixes then flow in on every upgrade.
useDefault = true

[[rules]]
[cargo]
description = "Cargo Ignore" # Optional
section = true # Define main section comment
# Paths to ignore
paths = [
  # Cargo's lockfile — package checksums trip entropy heuristics.
  '''Cargo\.lock$''',
  # Build artifacts (gitignored).
  '''target/.*''',
  # License texts and their per-crate symlinks.
  '''LICENSES/.*''',
  '''(^|/)LICENSE-(MIT|APACHE)$''',
  # This config itself, so any example patterns below never self-trigger.
  '''\.gitleaks\.toml$''',
]
[cargo.add]
description = "Cargo Always Add" # Optional
section = true # Define main section comment; with no description, use "# cargo"
               # (add becomes "Always Added" / "Not Ignored")
# Paths to always add
paths = [
    # Build artifacts (not gitignored).
    '''cli''',
]
```

Maybe in the paths we can add `key = value` where `key is the key to ignore|add` and `value is the description that by default is empty`
If we have description we add as comment before the key.

These sketches are drafts, not specifications. They have been evaluated against
measured evidence in [./design/config-format.md](./design/config-format.md),
which records what was adopted, what was rejected and why, and the shape they
resolve to. [./ROADMAP.md](./ROADMAP.md) Phase 2 summarizes the outcome; Phase 5
covers `[extend]`.

The headline findings, so they are not lost in the detail:

- **`session` reads as `section`** and is redundant with `description`. Dropped.
- **`paths` as regexes cannot be rendered back to `.gitignore`.** `.gitignore`
  has no brace alternation, so `(^|/)LICENSE-(MIT|APACHE)$` has no single-pattern
  equivalent. The `[extend]` idea is adopted; regex `paths` is not.
- **Section names must not be TOML keys.** `serde_json` sorts object keys by
  default, so `[cargo]` and `[Mise]` would be silently alphabetized on a JSON
  round-trip, and rule order is semantic.
- **`key = value` for descriptions makes the pattern list a map**, which loses
  order and forbids duplicate patterns. The intent is kept as a list of records.
- **`[[gitignore]]` followed by `[[section]]` does not nest.** `tomllib` reads it
  as an empty `gitignore` element plus a separate top-level `section` array, the
  same pitfall as the earlier `[[rules]]` sketch. The nesting form is
  `[[gitignore.section]]`.
- **`add` stores patterns without the leading `!`.** The sketch writes it both
  ways; the `!` belongs to the renderer, and validation rejects it in the value
  with a message naming the correct spelling.

## Rust SDK Requirements

- Cargo allow Rust (1.95)
- Must use Rust (1.97) that allows new features (edition 2024).
- We must focus on Security-first and Performance.

## AI Requirements

You must write the needed agents, skill, prompts and other configs including
AI-agentics. I want AI agnostic configurations that can be reused by Claude,
Codex, Copilot, Bedrock and so on. I want the AI to be able to do fully
autonomous work. Check the
[./guidelines/CONTRIBUTION.md](./guidelines/CONTRIBUTION.md).

## Coding Specifications

Ensure we use TDD before writing any code line.
Ensure the development principles:

1. TDD (If we need to change existing code without TDD, first write the
   test using TDD and ensure the test is working and then start to
   convert/refactor the context we intended to modify, always doing TDD
   implementation loop till green)
2. KISS (Keep It Simple, Stupid)
3. DRY (Don't Repeat Yourself)
4. YAGNI (You Aren't Gonna Need It)
5. TDA (Tell Don't Ask)
6. SOLID (Use the SOLID Principles that make sense to the project).

Write the plan into [./ROADMAP.md](./ROADMAP.md).

## Decisions

Questions this brief left open, and how they were settled.

### The project name is `ignorefile`

Settled in three steps, and the intermediate answers are worth keeping because
each was discarded for a measurable reason.

`gitignore-as-config` was rejected first: the configuration is the artifact the
tool produces, not the thing the project is, and `.gitignore` is already a
configuration file, so the phrase is close to a tautology. "As code" is also the
established category name, carrying a meaning readers already have.

`gitignore-as-code` held until the format became multi-target. Once one
configuration could describe a `.dockerignore` as well, a name covering only one
of its outputs was narrower than the product.

`ignore-as-code` was the obvious widening, and it collides. `ignore` on crates.io
is "a fast library for efficiently matching ignore files such as `.gitignore`
against file paths" -- the same domain as this crate's own matcher. The name
would read as a satellite of a well-known crate it has no relationship with.

`ignorefile` is the term the ecosystem already uses for the artifact class, and
it draws the right line: `ignore` reads ignore files, this authors them. It
covers `.gitignore`, `.dockerignore` and the rest without naming any of them.

The five surfaces are `ignorefile`, `-cli`, `-wasm`, `-mcp` and `-lsp`. The CLI
package carries the `-cli` suffix so it can be published next to the core crate
of the same base name, but the executable it ships is plain `ignorefile`.

### "Fully autonomous work" versus the contribution policy

These read as contradictory and are not. `guidelines/CONTRIBUTION.md` tells
fully autonomous agents not to contribute to this repository, and prohibits
AI-authored pull requests, commit messages and reviewer replies. That governs
what reaches the **upstream** project.

Autonomous agent work is expected and welcome **locally**: in a private fork or
worktree, driving the `mise` tasks, writing tests and implementation under the
maintainer's supervision. The boundary is the point of submission. An agent may
do the work; a human decides what is proposed, and writes the words that
accompany it.

Agents should treat [../AGENTS.md](../AGENTS.md) as their entry point.
