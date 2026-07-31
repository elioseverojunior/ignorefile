---
name: ROADMAP.md
created_by: <elioseverojunior@gmail.com>
---

<!--
SPDX-FileCopyrightText: ignorefile contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Roadmap

The plan requested by [INSTRUCTIONS.md](INSTRUCTIONS.md).

Phases are ordered by dependency, not by calendar. Each phase lists the tests
that define it, because the project is strictly test-driven: the tests are
written first and the phase is done when they pass with the coverage gate at
100%.

## Phase 0: scaffold

Done.

- Virtual workspace with one crate per surface.
- Toolchain pinned (Rust 1.97, MSRV 1.95, edition 2024), warnings as errors,
  `unsafe_code = "deny"`.
- Every quality gate wired as a `mise` task: fmt, clippy, nextest, doctests,
  coverage, `cargo audit`, `cargo deny`, `cargo vet`, gitleaks, REUSE/SPDX.
- `hk` pre-commit hooks and generated commit tooling.

The crates start empty. The first test in Phase 1 is what makes `mise run cargo:test`,
`ci:quick` and the coverage gate green for the first time.

## Phase 1: lossless round-trip

**This is the phase that decides whether the project is correct.** Everything
else builds on it.

`.gitignore` is order-dependent and anchor-sensitive. Any model that loses
ordering or anchoring changes which files git ignores, silently. So the first
test is a property test, not a feature test:

```text
render(parse(input)) == input, byte for byte
```

over a corpus of real `.gitignore` files. That single property forces every
representation decision into the open before any convenience feature exists.

The semantics the model must preserve:

- **Anchoring.** `/target` matches only at the repository root; `target` matches
  at any depth. `build/` matches directories only. Dropping or adding a leading
  slash is a behaviour change, not a formatting change.
- **Order.** A negation (`!pattern`) only takes effect if it appears after the
  pattern that excluded the file. Reordering rules can change the result.
- **The directory rule.** `!` cannot re-include a file if one of its parent
  directories is excluded. This is not expressible as a per-pattern property, so
  the linting in Phase 4 has to reason about rule interaction, not rules alone.
- **Comments and blank lines.** Comments carry the section structure this project
  is built to expose, so they are data, not noise. A comment's association with
  the rules beneath it must survive the round-trip.
- **Escaping and edge cases.** `\#`, `\!`, trailing-space handling, `**`
  wildcards, character ranges.

Tests: property-based round-trip over a corpus; unit tests per semantic rule
above; a differential test asserting the parsed model agrees with `git
check-ignore` on a fixture repository.

### Progress

**Done: the round-trip harness.** `crates/ignorefile/tests/roundtrip.rs`
holds the property test, a table of edge cases and a corpus under
`tests/corpus/`. `GitIgnore` is currently a line-preserving decomposition:
`split('\n')` paired with `join("\n")`, which is exactly inverse for any input,
with a trailing `\r` carried as line content so mixed CRLF and LF endings need no
line-ending model at all.

That model is deliberately the minimum that satisfies the property, and that is
the point of doing it first: the invariant is now a regression guard, so every
structural test below has to keep it green rather than discovering at the end
that structure and losslessness conflict.

Note that edge cases are Rust string literals, not corpus files. `hk`'s
`trailing-whitespace-fixer` and `end-of-file-fixer` run at `glob = "**"`, so a
checked-in file asserting "trailing spaces survive" would be normalized by our
own pre-commit hook and the test would pass for the wrong reason.

**Done: classification and section association.** `Line::kind` distinguishes
blank, comment and pattern lines; `Line::comment` reports `#` depth and text and
`Line::pattern` reports negation. Grouping into sections lives in `convert.rs`,
since that is its only consumer. All of it was verified against `git
check-ignore` rather than assumed: only a `#` at position 0 opens a comment,
leading whitespace is significant, trailing spaces are stripped when matching,
and whitespace-only lines are inert.

**Still open:**

1. **Pattern structure.** Anchoring and directory-only as inspectable properties
   rather than punctuation. Not needed yet: the configuration stores patterns
   verbatim, so anchoring survives without being modelled.
2. **The differential test** against `git check-ignore`, which is what would
   prove the model's semantics rather than only its bytes. This is the most
   valuable remaining test in Phase 1.

## Phase 2: the configuration format

Only once the model is proven lossless does a serialized format make sense.

The proposals in [INSTRUCTIONS.md](INSTRUCTIONS.md#ux) have been evaluated
against measured evidence, and the resulting target shape is recorded in
[design/config-format.md](design/config-format.md). Read that before writing any
of these tests. In summary:

- **Sections are an ordered array with the name as a value**, not a table keyed
  by section name. `serde_json` with default features sorts object keys, so a
  name in key position loses its order on a JSON round-trip, and rule order is
  semantic.
- **Sections live under the file they describe**, as `[[gitignore.section]]`, so
  a `[dockerignore]` sibling can be added without disturbing them. A key is safe
  for the output file because the order of two different files is not semantic.
- **Patterns are gitignore globs stored verbatim**, never regexes. Anchoring
  lives in the string, so `/target` and `target` stay distinct.
- **Comments are fields, not comments.** JSON has no comment syntax, so anything
  that must survive a TOML to JSON round-trip has to be data.
- **A rule is a group**: an optional `note` plus `ignore` and `add` lists, with
  `add` storing patterns without the leading `!`. Comment depth follows
  `###` for the config name, `##` for a section, `#` for any note.

Tests: schema round-trip for TOML, JSON and YAML; a test asserting the three
encodings are interchangeable and order-preserving; a test that an `ignore`
following an `add` starts a new rule so ordering survives; validation tests with
actionable messages; and a drift test holding the published JSON Schema to the
Rust types.

### Progress

**Done.** `Config`, `Target`, `Section` and `Rule` live in `config.rs` with
`Config::validate`; the grammar, rendering and the lossless-or-refuse
verification live in `convert.rs`; the published schema is
`schema/ignorefile.schema.json`, held to the Rust types by
`tests/schema.rs`. Import re-renders the config it just built and compares it
against the source, so a `.gitignore` it cannot reproduce is refused with the
first differing line rather than silently normalized.

The most interesting outcome is a subtraction: **`Error::CommentAfterPattern` is
gone.** A comment between patterns was the one shape the earlier grammar refused;
it is now a rule note, so the format is strictly more expressive and the error
has nothing left to report.

The one design correction worth remembering: **depth decides section boundaries,
not blank lines.** Delimiting by blank lines broke as soon as the renderer put a
blank line before a noted rule, because import read that note back as a section
header. `##` opens a section, `#` does not, and a comment block with no patterns
under it is a header at any depth.

## Phase 3: CLI

Subcommands over the proven core: `import` (`.gitignore` to config), `generate`
(config to `.gitignore`), `check` (drift between the two), `fmt`.

Tests: end-to-end tests driving the built binary over fixture repositories;
exit-code contract tests, since `check` is meant for CI.

### Progress

**Done: `init`, `import`, `add`, `generate`**, built with clap's derive API. The
logic lives in the CLI crate's library so it is testable; `main.rs` is a thin
shim, which is what justifies its exclusion from the coverage gate.

`run` accumulates its messages into a `Vec<String>` rather than writing to a
stream, so the library performs no output I/O of its own and every line of it is
reachable by a test. Printing is `main`'s job.

**Still open:** `check` (drift between config and `.gitignore`, meant for CI, so
its exit-code contract is the point) and `fmt` (normalize a `.gitignore` into the
canonical form, which is what would let import accept files it currently refuses,
such as ones with double blank lines between sections).

## Phase 4: intelligent management

The reason the model keeps comments and order:

- Patterns shadowed by an earlier rule.
- Rules that match nothing in the repository (dead rules).
- Negations defeated by an excluded parent directory.
- Duplicates within and across sections.

Tests: one fixture repository per diagnostic, asserting both detection and the
absence of false positives on a clean repository.

## Phase 5: template composition

`[extend]` from the Proposal A sketch in [INSTRUCTIONS.md](INSTRUCTIONS.md#ux):
pull in a maintained upstream rule set, the way gitleaks' `useDefault` pulls in
its ruleset, rather than re-deriving the same Rust or Node ignore rules in every
repository. This is what makes the format worth having across many repositories
instead of one, so it is a phase rather than a footnote.

It cannot ship as a bare "fetch the upstream template", because that would make
`generate` non-deterministic: the same config would produce a different
`.gitignore` next month, and `check` would then fail in CI for reasons unrelated
to the repository. What it needs:

- **A lockfile** with the same job as `Cargo.lock`: pin the resolved template
  content by revision or digest, commit it, and resolve from it by default.
- **A decision on vendoring versus fetching.** Fetching at generate time makes
  the build non-hermetic and adds a network dependency to a formatting tool.
- **A precedence rule** for a local section that collides with a template
  section, and for a local `allow` that contradicts a template `ignore`.

Tests: a test that `generate` is byte-identical across runs with the lockfile
present; a test that a changed upstream template does NOT change output until the
lock is refreshed explicitly; precedence tests for every collision case above.

## Phase 6: WASM

**Done.** `import`, `generate`, `convert`, `validate` and `isIgnored` are
exported via `wasm-bindgen`. `src/lib.rs` holds only one-line shims and stays
excluded from the coverage gate; the logic lives in `src/api.rs`, which is plain
Rust over plain strings and is measured like any other module. That split is what
makes the exclusion honest rather than a hole.

Tests: the api module round-trips through all three encodings, converts between
them, and covers every error path. `wasm-bindgen-test` is not used, because there
is nothing in the shim layer left to test.

## Phase 7: MCP server

**Done.** A JSON-RPC 2.0 server over line-delimited stdio, exposing `import`,
`generate`, `validate` and `explain`. `handle` is a pure function from request
string to optional response string, so the protocol is tested without a process
or a client; `main.rs` adds only the loop.

Tool failures are returned inside a successful response with `isError`, as MCP
specifies, so a model reads the refusal and retries rather than seeing a
transport error. `initialize`, `tools/list` and `tools/call` are implemented,
notifications are answered with silence, and unknown methods, malformed JSON, a
wrong protocol version and a missing tool name each return the right JSON-RPC
error code.

Still open: the server is stateless and takes text as arguments rather than
reading the repository. Filesystem access needs a root-confinement rule and a
test that nothing is written outside it.

## Phase 8: LSP

**Done.** `initialize`, `shutdown`, `exit`, and `didOpen` / `didChange` /
`didClose` with full-text sync, publishing diagnostics on every change. Split
three ways so only the outermost layer is untestable: `diagnostics.rs` is pure
analysis, `lib.rs` is pure protocol, `main.rs` is the stdio loop and the
`Content-Length` framing.

Three diagnostics ship: a file that cannot be kept as configuration (warning,
on the offending line), a re-inclusion defeated by an excluded parent directory
(warning), and a duplicated pattern (hint).

Still open from the original list: patterns shadowed by an earlier rule, rules
that match nothing in the working tree, and drift between config and
`.gitignore`. The last two need filesystem access, which the server does not yet
have.

## Cross-cutting, not yet started

- **CI does not exist yet.** There is no `.github/workflows/`, but `mise.toml`
  already mirrors the pipelines it expects (`ci`, `ci:perf`, `ci:nightly`), and
  `mise run act`, `mise run actionlint`, `mise run actions:pins`,
  `mise run gh:branch:protect` and `mise run codecov` all assume workflow files.
  Writing those workflows is the single largest gap in the repository.
- **No benchmarks yet**, though `Criterion.toml`, `mise run bench` and
  `mise run bench:bencher` are configured. The performance gate has nothing to
  measure until the parser exists.
- **No fuzz targets yet.** `mise run fuzz` and `mise run fuzz:all` expect
  `crates/ignorefile-fuzz`. A parser over untrusted repository input is
  exactly what fuzzing is for; this should land with Phase 1.
- **`cargo vet` is non-blocking** until the dependency tree is certified.
- **The release profile optimizes for size** (`opt-level = "z"`, `lto = "fat"`,
  `panic = "abort"`), which is in tension with the performance goal in
  INSTRUCTIONS.md. Revisit once benchmarks exist and the trade-off is measurable
  rather than assumed.
