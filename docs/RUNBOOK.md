---
name: RUNBOOK.md
created_by: <elioseverojunior@gmail.com>
---

<!--
SPDX-FileCopyrightText: ignorefile contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Runbook

Operating this repository: the routine loops, the release path, and what to do
when a gate goes red.

Design rationale lives in [design/config-format.md](design/config-format.md),
conventions in [../AGENTS.md](../AGENTS.md), and the plan in
[ROADMAP.md](ROADMAP.md). This file is only about running things.

Every command goes through [mise](https://mise.jdx.dev). `mise tasks` lists them
all with descriptions.

## First run on a new machine

```sh
mise run setup           # provisions the toolchain and every pinned tool
mise run git:hooks:install   # installs the hk git hooks
mise run doctor          # one row per tool, MISSING for anything absent
```

`mise` is the only prerequisite. The Rust version comes from
`rust-toolchain.toml`; everything else from `mise.toml`. Entering the directory
triggers `setup` automatically via `[hooks] enter`.

`git` is a hard requirement for the test suite, not just for version control:
`crates/ignorefile/tests/differential.rs` uses `git check-ignore` as its oracle.

## The loop while working

```sh
mise run cargo:test      # inner loop: cargo-nextest, fast and filterable
mise run cargo:test:doc  # inner loop: the doctest half nextest cannot run
mise run lint            # fmt:check + clippy, read-only
mise run coverage        # THE gate: tests + doctests + coverage, one pass
mise run pr:ready        # auto-format, then run that gate
```

**There is one test gate, and it is `mise run coverage`** (`cov`,
`coverage:tarpaulin` and `tarpaulin` are aliases of the same task). Tarpaulin's
`--run-types Tests --run-types Doctests` builds and runs the unit, integration
and doc tests under instrumentation, then fails under the threshold. So the tests
a gate ran and the coverage it gated on always describe the same build. The same
task backs `pr:ready`, `ci:quick`, `ci` and the Tests job in
`.github/workflows/cicd.yml`; a local green and a CI green mean the same thing.

`cargo:test` and `cargo:test:doc` remain, but only as the TDD inner loop. They
are fast and they take a filter, which the gate does not; they are **not** a
gate. Expect the gate to take tens of minutes where `cargo:test` takes seconds.

**Read the test count, not just the exit code.** `159 tests run: 159 passed` is
evidence; `exit 0` alone is not. `NEXTEST_NO_TESTS` is deliberately unset, so a
filter matching nothing fails rather than exiting 0 having run nothing -- but
that only proves the filter matched something, not that it matched what you
meant.

## Running each surface

```sh
cargo build --release

./target/release/ignorefile --help      # or the short alias: ign
./target/release/ignorefile-mcp         # JSON-RPC on stdin/stdout, line-delimited
./target/release/ignorefile-lsp         # LSP on stdin/stdout, Content-Length framed
```

Smoke-test the MCP server without a client:

```sh
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ./target/release/ignorefile-mcp
```

The LSP server needs `Content-Length` framing, so it is not shell-friendly;
point an editor's language client at the binary instead.

The WebAssembly module needs its own target. `mise run ci` covers it via
`mise run cargo:wasm`, which is the only thing in the gate that compiles this crate
for wasm at all -- a host build proves nothing, because wasm-bindgen's macros
expand to almost nothing off a wasm target:

```sh
mise run cargo:wasm                          # clippy + release build for wasm32
wasm-pack build crates/ignorefile-wasm # the publishable npm package
```

## Release

Nothing has been published yet, so the first release runs this end to end for the
first time. There is no `publish` mise task, and there is no tag to push by hand:
the pipeline does both. What is easy to get wrong is assuming otherwise.

### What runs where

**A release is a push to `main`, not a tag you push.** `cicd.yml` cuts the tag
itself, then CALLS `publish.yml` directly. Nothing here is triggered by the tag,
and that is deliberate: GitHub raises no workflow-triggering event for anything
pushed with `GITHUB_TOKEN`, so a `push: tags` trigger would never fire for an
automated release. `needs:` does not reach across workflow files, so calling is
also what lets the crates.io upload depend on provenance and signing having
finished.

```mermaid
flowchart TD
    subgraph local["Local, before the PR"]
        A["mise run pr:ready"] --> B["mise run release:prepare --check<br/>previews the version + changelog entries"]
        B --> C["mise run release:prepare<br/>writes the version into Cargo.toml"]
    end

    C --> D["PR -> merge to main"]

    D -->|"on: push to main"| E1

    subgraph ci["cicd.yml"]
        E1["version (calls version.yml)<br/>GitVersion, once, for the whole run"] --> E2["lint / sast / msrv"]
        E2 --> E3["tests: mise run coverage<br/>100% or the run stops"]
        E3 --> E4["build: release binaries + wasm + smoke test"]
        E4 --> E5["release: tag vX.Y.Z and the<br/>floating vX.Y / vX, then gh release create"]
    end

    E5 -->|"uses: ./.github/workflows/publish.yml<br/>dry-run: false"| F1

    subgraph pub["publish.yml"]
        F1{"validate<br/>SemVer + should_publish"} --> F2["package: cargo package<br/>minus crates:dev-only -> dist/*.crate"]
        F2 --> F3["provenance: SLSA L3"]
        F2 --> F4["sign: cosign keyless OIDC"]
        F3 --> F5["publish-crates<br/>environment: crates-io"]
        F4 --> F5
        F5 --> F6["gh-pages + changelog"]
    end

    F5 --> G["docs.rs builds automatically<br/>from the uploaded crate"]
```

Five properties of that graph are worth stating outright, because each one was
false at some point and the symptom was never obvious:

- **The tag is created by CI, not by you.** The `release` job runs only on a
  push to the default branch, and tags the exact version plus the floating
  `vX.Y` and `vX` pointers. Pushing a tag by hand still works -- `publish.yml`
  accepts `push: tags` -- but a hand-pushed tag arrives with no `inputs`, so
  `dry-run` is empty and it packages, attests and signs while publishing
  **nothing**. That is intentional; see the next point.
- **`dry-run` fails closed.** It defaults to `true`, and only the literal string
  `false` publishes. `cicd.yml` therefore passes `dry-run: false` explicitly --
  omitting the key does not mean "inherit the caller's intent", it means "never
  publish", which once let the whole release path run green while uploading
  nothing.
- **The publish waits on provenance AND signing.** That dependency could not be
  expressed while these were two workflows, so a crate could reach the registry
  unattested whenever the signing job was slower or failed.
- **`publish-crates` is gated by the `crates-io` environment**, which requires a
  human reviewer and protected branches. That environment is the only thing
  narrowing crates.io Trusted Publishing: the OIDC claim names the CALLING
  workflow, which is `cicd.yml`, so the grant would otherwise cover any job in
  it. `mise run gh:env:protect` configures it.
- **A first publish needs a real token.** Trusted Publishing cannot be configured
  for a crate that does not exist yet, so `crates:status` reports
  `bootstrap_needed=true` and the workflow falls back to
  `CRATES_IO_USER_TOKEN`. crates.io also rate-limits new crates to a burst of
  five then one per ten minutes, so `publish-crates` retries up to 12 times,
  recomputing what already landed from the registry each round. Delete the
  secret once every crate exists and has a Trusted Publishing config.

### Step by step

1. Green gate, from a clean tree.

   ```sh
   mise run ci
   hk run check --all
   ```

2. Preview the version and the changelog entries it would cut. This writes
   nothing:

   ```sh
   mise run release:prepare --check
   ```

3. Check for accidental breaking changes.

   ```sh
   mise run cargo:semver
   ```

   **Before the first publish this fails**, and the failure is expected:
   `cargo-semver-checks` needs a baseline to compare against and reports
   `ignorefile not found in registry (crates.io)`. Skip the step for the first
   release, or give it a git baseline instead:

   ```sh
   cargo semver-checks --baseline-rev <tag-or-sha>
   ```

4. Write the version. GitVersion computes it from the tag graph and the
   conventional-commit history; `release:prepare` propagates it into the one
   place it is declared, `[workspace.package]`, and into the path dependencies
   that must agree with it:

   ```sh
   mise run release:prepare
   ```

   Do **not** write `CHANGELOG.md` here. It is generated and committed by
   `changelog.yml` after the publish succeeds, so the file is cut from the tag
   that actually shipped rather than from a local guess at the next one.

5. Open a pull request and merge it. `main` is protected -- signed commits,
   linear history, a review, and the `CICD` check -- so this is the only route
   in. Merging is what starts the release; there is no tag to push.

6. Approve the `crates-io` deployment when GitHub asks. That is the required
   reviewer on the environment, and it is the last point at which the release
   can be stopped: crates.io uploads are immutable and a version number cannot
   be reclaimed once spent.

7. Confirm every crate landed:

   ```sh
   mise run crates:status --verify
   ```

   `PENDING` shortly after a publish usually means the sparse index has not
   caught up yet, not that a crate is missing. The same task without `--verify`
   is the pre-flight the workflow runs, and it is safe to run locally at any
   time.

**Packaging is crate-relative.** `readme = "README.md"` and the licence files are
resolved under the package root, and `cargo package` includes nothing above it.
Each crate therefore carries its own `LICENSE-MIT` and `LICENSE-APACHE` symlinks
into the shared `LICENSES/` texts -- cargo dereferences them and ships the real
content, so the licences stay single-sourced while every published archive is
self-contained. Confirm with:

```sh
cargo package -p ignorefile --list | grep -E 'README|LICENSE'
```

## When a gate goes red

### `mise run cargo:test` fails with "no tests to run"

A filter matched nothing. Not a regression. Check the filter spelling.

### Coverage is below 100%

The threshold is 100%. `mise run coverage:tarpaulin` prints the uncovered lines
per file directly, so start by reading its output; for a browsable view of the
same run:

```sh
mise run coverage:open     # HTML report, opens in the browser
```

Exclusions live in exactly one place, `tarpaulin.toml`'s `exclude-files`. Do not
add a second list anywhere.

A gap on a line that has no uncovered *statement* means a **branch**. In this
codebase that is almost always one of four things, in rough order of frequency:

1. A genuinely missing test. Check this first; most of them are.
2. An unreachable `?` error arm. `?` desugars to a match whose error arm is a
   region in your file; if the call cannot fail for your types, that region can
   never be covered. Use `map_err`/`and_then`, which keep the branch in core.
3. An `assert!` format argument, which is only evaluated on failure. Bind it to a
   variable first.
4. A `serde_json::to_string` over a struct that cannot fail. Build a
   `serde_json::Value` and call `to_string()`, which is infallible.

Never widen `exclude-files` or lower `RUST_COVERAGE_THRESHOLD` to make it pass.
If a gate is genuinely wrong, say so and stop.

### Coverage reports functions as unreached that the tests plainly reach

Stale profile data from an earlier build being merged into this one. Clear it:

```sh
mise run clean:cov && mise run coverage:tarpaulin
```

### `coverage:tarpaulin` fails with "Failed to compile tests" or a missing `.rmeta`

Two cargo processes are sharing `target/`. Tarpaulin rebuilds the workspace with
instrumentation, so a concurrent `mise run cargo:test`, a rust-analyzer check, or a
second coverage run will clobber each other's fingerprints. The error names a
dependency that has nothing to do with your change (`bumpalo`, `zerocopy`).
Let the other build finish and re-run; nothing is actually broken.

### clippy fails on a lint that looks wrong

Two have come up and are documented in the code rather than suppressed globally:

- `trivially_copy_pass_by_ref` on serde's `skip_serializing_if`. Serde requires
  `fn(&T) -> bool`, so the lint is structurally inapplicable. Suppressed at that
  one function with a comment.
- `allow-expect-in-tests` does **not** cover helper functions in an
  integration-test crate. Use `let ... else { panic!() }` or `assert!`.

### `hk run check --all` fails on a generated file

`ignorefile.toml` is emitted formatter-canonical: keys alphabetical, values
before tables, arrays inline. If `taplo` rewrites it, either the emitted field
order regressed, in which case
`tests/config_format.rs::emitted_toml_is_already_formatter_canonical` should have
caught it, or an array exceeded the 80-column limit, which is a known and
accepted residual.

### `hk run pre-commit` behaves oddly on a repository with no commits

`hk.pkl` sets `stash = "git"`, and `git stash` cannot run before the first
commit. Use `hk run check --all`, which is read-only and does not stash.

### `mise run cargo:audit` reports an advisory

`.cargo/audit.toml` sets `severity_threshold = "medium"`. Fix or justify; do not
raise the threshold. Note `serde_yaml` is deliberately not used because it is
unmaintained; `serde_norway` is the maintained fork.

## Known gaps

These fail by design, because the thing they need does not exist yet. Do not
treat them as regressions.

| Command | Why it fails |
| --- | --- |
| `mise run act` | needs a running Docker daemon |
| `mise run gh:branch:protect` | no GitHub remote configured |
| `mise run codecov` | needs `CODECOV_TOKEN` and the repo activated |
| `mise run fuzz` | no `crates/ignorefile-fuzz` yet |
| `mise run bench` | no benchmarks yet |
| `mise run cargo:vet` | non-blocking by design; swallows its own failure |
| `mise run cargo:semver` | no published baseline yet; needs `--baseline-rev` |
| `mise run docs:build` and friends | no `docs/package.json`; the VitePress site is not written yet |
| `mise run docker:build` | no `docker/Dockerfile` yet |

`mise run ci` mirrors the `lint`, `msrv`, `tests` and `build` jobs in
`.github/workflows/cicd.yml`, so a green local run is good evidence CI will be
green too. The exceptions are the jobs that need GitHub itself: CodeQL, SLSA
provenance, Scorecard and the crates.io publish.

Two workflows still point at paths that were never ported into this repository,
so they fail if dispatched: `gh-pages.yml` needs the `docs/` site, and
`performance.yml` needs `scripts/bench-to-json.py` and a benchmark target.
Neither runs on push, so neither blocks a merge.

## Things an agent must not do

From [guidelines/CONTRIBUTION.md](guidelines/CONTRIBUTION.md), and
non-negotiable: do not commit or push without explicit per-action human
approval, do not open pull requests, and do not write pull-request descriptions,
commit messages or reviewer replies. If asked to commit on a human's behalf, use
`Assisted-by:`, never `Co-authored-by:`.
