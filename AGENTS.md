<!--
SPDX-FileCopyrightText: ignorefile contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# AGENTS.md

Tool-agnostic instructions for AI coding agents working in this repository.
Claude, Codex, Copilot, Bedrock and friends all read this file; nothing here is
specific to one vendor.

Contributor-facing policy that is NOT negotiable lives in
[docs/guidelines/CONTRIBUTION.md](docs/guidelines/CONTRIBUTION.md). Read it
before acting. The "Hard prohibitions" section below is a summary, not a
replacement.

## Hard prohibitions

- Do NOT commit or push without explicit human approval for each individual
  action.
- Do NOT run `git push` or create a pull request (`gh pr create`).
- Do NOT write pull-request descriptions, commit messages, issue bodies, code
  review comments, or replies to reviewers. This is non-overridable. Automated
  PR submissions can get the contributor banned from the project.
- If the user explicitly asks you to commit on their behalf, use
  `Assisted-by: <assistant name>`. Never `Co-authored-by:`.
- Do NOT implement features the contributor does not fully understand, and do
  NOT generate changes too large for them to review.

When uncertain, err toward minimal assistance.

## Everything runs through mise

`mise` is the single entry point. It provisions the toolchain and owns every
task, so prefer `mise run <task>` over a bare `cargo` invocation.

```sh
mise run setup     # install all pinned tools (aliases: install, dev)
mise run doctor    # one row per tool; reports MISSING for anything absent
mise tasks         # full task list with descriptions
```

`[hooks] enter = { task = "setup" }` means entering the directory triggers
`setup` automatically.

### The tasks you will use most

| Task | What it does |
| --- | --- |
| `mise run coverage:tarpaulin` (`cov`) | **THE test gate**: tests + doctests + 100% coverage, one pass |
| `mise run build` (`b`) | `cargo build --workspace` |
| `mise run cargo:test` (`t`) | TDD inner loop only: `cargo nextest run`, filterable, NO doctests, NO coverage |
| `mise run cargo:test:doc` (`td`) | TDD inner loop only: the doctest half nextest cannot run |
| `mise run cargo:fmt` | `cargo fmt --all`, writes changes |
| `mise run cargo:fmt:check` (`fc`) | formatting gate, read-only |
| `mise run cargo:clippy` (`cl`) | `cargo clippy --all-targets --all-features -- -D warnings` |
| `mise run lint` (`l`) | `fmt:check` + `clippy` |
| `mise run comply` | REUSE/SPDX licence gate |
| `mise run security` (`sec`) | `cargo audit` + `cargo deny`, in that order |
| `mise run pre-commit` (`pc`) | run the hk hooks over staged files |
| `mise run ci:quick` (`ciq`) | fast gate: fmt + clippy + the test gate |
| `mise run ci` | full PR pipeline |
| `mise run pr:ready` | auto-format, then check everything |

**There is exactly one test gate: `mise run coverage:tarpaulin`.** It is what
`pr-ready`, `ci:quick`, `ci` and the Tests job in `.github/workflows/ci.yml` all
run, so a local green and a CI green mean the same thing. `test` and `test:doc`
survive as the fast, filterable TDD inner loop and are **not** gates -- a green
`mise run cargo:test` proves less, because it skips the doctests and measures no
coverage.

Run `mise run pr:ready` before handing work back. When a gate goes red,
[docs/RUNBOOK.md](docs/RUNBOOK.md) has the diagnosis steps rather than guesswork,
including the four causes of a sub-line coverage gap.

## Gotchas

These are the things that will bite you. None are guessable from the code.

- **Warnings are hard errors.** `.cargo/config.toml` sets
  `rustflags = ["-C", "target-cpu=native", "-D", "warnings"]` and
  `rustdocflags = ["-D", "warnings"]`. A warning fails the build, including
  rustdoc warnings such as a broken intra-doc link.
- **Never set `RUSTFLAGS` to work around that.** The `RUSTFLAGS` env var
  REPLACES `[build] rustflags` rather than merging with it, silently dropping
  the warning gate and `target-cpu=native`. This is exactly why
  `[tasks.clippy]` repeats `-- -D warnings` even though the config file already
  sets it. Do not "DRY away" that duplication.
- **`target-cpu=native` makes binaries non-portable.** Anything built here is
  tuned for the building machine's CPU. Do not ship a release artifact produced
  with the default local config.
- **nextest cannot run doctests.** `mise run cargo:test` and `mise run cargo:test:doc` are
  two halves of one suite. Running only the first hides doctest failures. This is
  an inner-loop hazard only: the gate runs both halves in one tarpaulin pass, so
  it cannot be half-run.
- **A green `mise run cargo:test` may have run zero tests.** `mise.toml`'s `[env]` sets
  `NEXTEST_NO_TESTS = "warn"` so the empty scaffold crates do not fail for having
  nothing to run. It prints `warning: no tests to run` and exits 0, which means a
  mistyped filter looks like success. Read the test count. Also nextest-specific,
  so also inner-loop only -- the gate drives `cargo test`, which has no such
  silent-pass mode. It cannot live in `.cargo/config.toml`: that block is not
  applied to external cargo subcommands, so it would have no effect on nextest.
- **Never substitute `mise run cargo:test` for the gate.** It skips the doctests and
  measures no coverage, so it can be green on a change the gate rejects. If you
  are claiming work passes, you ran `mise run coverage:tarpaulin`.
- **Coverage threshold is 100%** (`RUST_COVERAGE_THRESHOLD` in `mise.toml`).
  Adding an untested function fails the gate. Only genuinely host-untestable
  entry points are excluded, and the exclusion list lives in exactly one place,
  `tarpaulin.toml`'s `exclude-files`. It used to be duplicated into a second
  coverage engine's `IGNORE` regex, which meant the verdict depended on which
  engine ran; that engine has been removed. Do not reintroduce a second list.
- **The gate rebuilds the workspace with instrumentation.** Do not run it
  concurrently with `mise run cargo:test`, another coverage run, or anything else
  touching `target/`. They clobber each other's fingerprints and the error names
  an unrelated dependency (`error: extern location for bumpalo does not exist`),
  which reads like a dependency problem and is not one.
- **MSRV is 1.95, the toolchain is 1.97.** `rust-version = "1.95"` and
  `.clippy.toml`'s `msrv = "1.95"` mean clippy rejects APIs newer than 1.95
  even though the compiler you have is newer. Edition is 2024.
- **`unsafe_code = "deny"` workspace-wide.** There is no `unsafe` escape hatch.
- **`unwrap_used` and `expect_used` are warnings, and warnings are denied**, so
  they are effectively forbidden in production code. `.clippy.toml` allows both
  in tests.
- **Some config files are generated. Do not hand-edit them.**
  `commit-types.toml` is the single source of truth; `scripts/commit-config.py`
  generates `committed.toml` (whole file), `cliff.toml`'s `commit_parsers`
  array, and `.gitmessage`. Edit `commit-types.toml`, then run
  `mise run commit:config`. `mise run commit:config:check` is the drift guard.
- **Errors:** `thiserror` in libraries, `anyhow` in binaries.
- **`.clippy.toml`'s `allow-expect-in-tests` does not cover helper functions in
  an integration-test crate.** Clippy needs a `#[test]` function or a
  `cfg(test)` module, so `expect`/`unwrap` in a plain helper inside `tests/*.rs`
  is a denied warning. Use `let ... else { panic!(...) }` or `assert!` instead.
- **An unreachable `?` error arm makes the 100% region gate unsatisfiable.**
  `?` desugars to a match whose error arm is a coverage region in your file. If
  the call cannot actually fail for your types, that region can never be
  covered. Use `map_err`/`and_then` instead, which keep the branch inside core.
  Before assuming an arm is unreachable, check: most of them turn out to be a
  missing test.

## Layout

A virtual workspace; the root manifest has no `[package]`. Five surfaces, one
crate each:

| Crate | Role |
| --- | --- |
| [`crates/ignorefile`](crates/ignorefile/README.md) | core library: parse, model, render, match |
| [`crates/ignorefile-cli`](crates/ignorefile-cli/README.md) | the CLI; ships `ignorefile` and `ign` |
| [`crates/ignorefile-wasm`](crates/ignorefile-wasm/README.md) | WebAssembly bindings; logic in `api.rs`, shims in `lib.rs` |
| [`crates/ignorefile-mcp`](crates/ignorefile-mcp/README.md) | MCP server; `handle` is pure, `main.rs` is the stdio loop |
| [`crates/ignorefile-lsp`](crates/ignorefile-lsp/README.md) | Language Server; analysis, protocol and loop in three layers |

Each crate has its own README with a module and dependency diagram. Read the one
for the crate you are changing before you change it.

The CLI package is named `-cli` so it can be published next to the core crate
of the same base name. It declares **two** `[[bin]]` targets over the same
`src/main.rs`: `ignorefile` and the short alias `ign`. Cargo has no first-class
alias, so a second binary target is the mechanism; both are excluded from the
coverage gate by the `crates/[^/]+/src/main\.rs$` pattern.

**Every surface is a thin layer over the core library, and that is a rule.** All
logic lives in `ignorefile`; the CLI, WASM, MCP and LSP crates translate between
it and their transport. It is what lets one differential test against
`git check-ignore` cover the semantics for all of them.

The corollary is how the coverage exclusions stay honest: only `main.rs` files
and the wasm `lib.rs` are excluded, and each contains nothing but a loop or a
one-line shim. If you find yourself putting a decision in one of them, it belongs
one layer down.

## Workflow

**TDD, strictly.** Write the test, watch it fail, write the minimum code to
pass, then refactor. Never write production code first. To change existing code
that has no test, first add a passing test for the current behaviour, then
refactor against it.

Design principles, in the project's order of preference: TDD, KISS, DRY, YAGNI,
TDA (Tell Don't Ask), and whichever SOLID principles genuinely apply.

### Worktrees

Each feature branch gets its own worktree under `worktrees/` (gitignored).

```sh
git worktree add worktrees/<branch> <branch>
```

Reintegrate by rebase so `main` stays a straight line:

```sh
# from the main worktree
git pull --rebase
git rebase main worktrees/<branch>
git merge worktrees/<branch>   # fast-forward
```

Never merge with `--no-ff`.

## Conventions

- **ASCII only.** No em dash, no unicode arrows, no multiplication sign, no
  ellipsis character. Use `-`, `->`, `x`, `...`. This applies to code, comments,
  documentation, and commit messages.
- **Comments explain non-obvious invariants, nothing else.** Do not restate what
  the code already says, and do not leave comments that only make sense in the
  context of the task you were asked to do.
- **Reuse existing infrastructure** rather than introducing new subsystems. If a
  change is large or introduces a new pattern, PAUSE and ask before proceeding.
- **Read the surrounding code first.** Changes must blend in.
- **Rust imports** are ordered std, then external crates, then local
  (`rustfmt.toml` sets `reorder_imports` and `reorder_modules`).
- **SPDX headers** on source files use comply's canonical 3-line form, with a
  bare comment marker between the two tags:

  ```rust
  // SPDX-FileCopyrightText: 2026 ignorefile contributors
  //
  // SPDX-License-Identifier: MIT OR Apache-2.0
  ```

  The middle line is load-bearing, not decoration. `mise run comply:format:check`
  runs `comply format --check`, which rejects the 2-line shape. This repository
  used to collapse to 2 lines with a hand-rolled `awk` pass; that pass has been
  removed, and re-adding one would fight the gate on every commit, because hk
  runs `comply:fix` as a pre-commit FIX step over every staged `.rs`.

  `REUSE.toml`'s `**` rule already makes every file compliant, so headers are a
  per-file convention on source files, not a blanket operation. Write them with
  `mise run comply:fix <paths>`; `mise run comply:format:check` guards the shape.

  **Never write the licence-identifier tag anywhere below a file's header** --
  not in a string literal, not in a comment. comply joins every occurrence in a
  file into ONE licence expression, so a second one turns into
  `invalid expression: MIT OR Apache-2.0 AND MIT OR Apache-2.0 AND ...`. If a
  test needs licence-header-shaped input, put it in a fixture under
  `crates/ignorefile/tests/corpus/`, which is exempt from the scan.

  Do NOT annotate `crates/ignorefile/tests/corpus/`. Those `.gitignore` files are
  byte-exact test data: a header there is parsed as content, not metadata, and
  breaks the lossless round-trip tests. `REUSE.toml` lists them under
  `[tool.comply].ignore` for that reason.
- **Commits** follow Conventional Commits, with a lowercase imperative subject
  of at most 50 characters and no trailing period. Allowed types live in
  `commit-types.toml`. Remember that you do not write commit messages.
