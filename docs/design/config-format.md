---
name: config-format.md
created_by: <elioseverojunior@gmail.com>
---

<!--
SPDX-FileCopyrightText: ignorefile contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Design: the configuration format

Evaluation of the format proposals in
[../INSTRUCTIONS.md](../INSTRUCTIONS.md#ux), and the shape they resolve to.

Nothing here is implemented. The purpose of this document is to record which
options were considered, which were rejected, and on what evidence, so the
Phase 1 tests in [../ROADMAP.md](../ROADMAP.md) can be written against a decided
target instead of a guess.

## What the format has to do

In priority order. Later requirements yield to earlier ones.

1. **Round-trip losslessly.** `render(parse(x)) == x`, byte for byte. A format
   that cannot reproduce its input is not a representation of it.
2. **Preserve order.** Rule order is semantic, not cosmetic.
3. **Preserve anchoring.** `/target` and `target` are different patterns.
4. **Be interchangeable across TOML, JSON and YAML**, per the brief.
5. **Be schema-validatable**, so a bad config fails with a useful message.
6. **Be pleasant to hand-edit.** The premise of the project is that this file is
   nicer to work with than the `.gitignore` it replaces.

## Evidence

Each of these was measured, not assumed. They are the constraints the design has
to satisfy.

### Anchoring is semantic

```text
.gitignore: target      -> matches `target` AND `sub/target/f`
.gitignore: /target     -> matches `target`, NOT `sub/target/f`
```

Verified with `git check-ignore -v`. Dropping or adding a leading slash changes
which files git ignores. The draft in INSTRUCTIONS.md renders `/target` as
`ignores = ["target"]`, which is a behaviour change.

### Section names in key position lose their order in Rust

`serde_json` with default features backs its object type with a `BTreeMap`, so
keys come back sorted. Given input in the order `zebra, alpha, middle`:

```text
after parse  : alpha, middle, zebra
re-serialized: {"alpha":...,"middle":...,"zebra":...}
```

So a format that puts section names in key position (`[cargo]`, `[Mise]`)
silently alphabetizes its sections on any JSON round-trip with a default
`serde_json`. Because rule order is semantic, that is a correctness bug, and a
quiet one. Enabling the `preserve_order` feature hides it behind a dependency
flag that a consumer of the format cannot be relied on to set.

The JSON specification also states that objects are unordered, so this is not
merely one library's choice.

Note that `taplo`, this repository's TOML formatter, is **not** a threat here: it
was tested with `reorder_keys = true` and preserves both table order and
array-element order, reordering only keys within a table.

### JSON has no comments

Comments carry the section structure this project exists to expose, so they are
data. JSON has no comment syntax at all, which means any comment that must
survive a TOML -> JSON -> TOML trip has to be a named field. Relying on a
comment-preserving parser (`toml_edit` and similar) works for one encoding and
fails the interchange requirement.

### Regexes cannot round-trip to gitignore patterns

`.gitignore` does not support brace alternation:

```text
.gitignore: LICENSE-{MIT,APACHE}   -> matches nothing
```

Verified with `git check-ignore`. So the gitleaks allowlist regex
`(^|/)LICENSE-(MIT|APACHE)$` has no single-pattern `.gitignore` equivalent.
Regexes are strictly more expressive than gitignore globs, so a config that
stores regexes cannot be rendered back.

### Some rule interleavings cannot be split into two lists

```gitignore
build/
!build/keep
build/keep/tmp
```

Splitting this into an `ignore` list and an `allow` list discards the fact that
`build/keep/tmp` comes after the negation, and re-rendering changes the result.
Two flat lists per section are therefore lossy for some real inputs, even though
they are sufficient for most.

## The two proposals

### Proposal A: a gitleaks-shaped file

```toml
title = "..."

[extend]
useDefault = true

[[allowlists]]
paths = ['''Cargo\.lock$''', '''target/.*''']
```

**Adopted:** the overall file shape, and `[extend]`. A config that can pull in a
maintained upstream rule set instead of re-deriving it per repository is the
strongest idea in either proposal, and it is what makes the format worth having
across many repositories rather than one.

**Rejected:** `paths` as regexes, for the reason measured above. The rejection is
not stylistic. gitleaks' `paths` are a **filter** matched against files during a
scan, so regex is the right tool there. Ignore patterns are a **specification to
be rendered**, and the render direction is what regex cannot do. Borrowing the
field wholesale imports a category error.

### Proposal B: sections as tables

```toml
[[rules]]
[cargo]
description = "Cargo Ignore"
session = true
paths = [...]

[cargo.add]
```

**This does not parse as it reads.** Fed to `tomllib`:

```json
{ "rules": [ {} ], "cargo": { "description": "...", "add": { ... } } }
```

`[[rules]]` opens an array-of-tables element, and the next `[header]` closes it.
So `rules` gets one empty element and `cargo` becomes a separate top-level table.
Nesting would require `[[rules.cargo]]`.

Beyond the syntax, section-name-as-key has three further costs:

- **Order is lost on a JSON round-trip**, as measured above.
- **The schema becomes unbounded.** Every section name is a legal top-level key,
  so validation cannot distinguish a typo from a new section.
- **Control keys collide with section names.** A `.gitignore` with a section
  literally commented `# extend` or `# title` is unrepresentable, because those
  keys already mean something to the format.

**`session` is dropped.** It reads as "section", and it is redundant: a section
already exists by virtue of being in the list, and a comment already exists by
virtue of a description being present. A boolean that must agree with the
presence of another field is a second source of truth (YAGNI, and it invites
`session = false` with a description set, which has no meaning).

### Proposal C: `key = value` for per-pattern descriptions

> Maybe in the paths we can add `key = value` where key is the key to
> ignore|add and value is the description.

**Rejected.** This makes the pattern list a map, which reintroduces every problem
above at the pattern level: order is not guaranteed, `serde_json` sorts the keys,
and duplicate keys are illegal even though the same pattern may legitimately
appear twice. The intent, per-pattern descriptions, is kept; the encoding is a
list of records rather than a map.

## The resulting shape

Ordered arrays throughout, names and comments as values, never as keys.

```toml
version = 1
name = "ignore-as-config"

[[gitignore.section]]
name = "Logs"
note = "Ignore Logs Pattern"

  [[gitignore.section.rule]]
  ignore = ["*.log"]

  [[gitignore.section.rule]]
  note = "keep the one we actually read"
  add = ["important.log"]

[[gitignore.section]]
name = "Mise"
note = "Mise Ignore Pattern"

  [[gitignore.section.rule]]
  note = """
mise.local.toml is machine-specific (written by `mise run setup`); mise.lock is
committed on purpose, because [settings] lockfile = true pins tool versions."""
  ignore = ["/mise.local.toml"]

  [[gitignore.section.rule]]
  note = "Always Add Mise configs"
  add = ["/mise.lock"]
```

renders to

```gitignore
### ignore-as-config

## Logs
# Ignore Logs Pattern
*.log

# keep the one we actually read
!important.log

## Mise
# Mise Ignore Pattern

# mise.local.toml is machine-specific (written by `mise run setup`); mise.lock is
# committed on purpose, because [settings] lockfile = true pins tool versions.
/mise.local.toml

# Always Add Mise configs
!/mise.lock
```

### The comment layering

| Depth | Meaning |
| --- | --- |
| `#` above `###` | the configuration's `header`, once, before the banner |
| `###` | the configuration's own `name`, once, at the top |
| `##` | a section's `name`, the default `level` |
| `#` | a section `note` or a rule `note` |

Notes are multi-line: each line becomes its own `#` line at depth 1.

### Why each piece is shaped that way

- **`[[gitignore.section]]`, not `[[section]]`.** One configuration can describe
  more than one output file, so sections live under the file they belong to. A
  `[dockerignore]` sibling extends it without touching the schema of this one.
  Using a **key** for the output file is safe even though sections must stay an
  array: the order of two different output files is not semantic, whereas the
  order of two sections in the same file is.
- **`level` is stored.** The canonical depth for a section header is `##`, but
  nearly every real `.gitignore` writes `#`. Storing the depth is what lets both
  round-trip; `level = 2` is omitted because it is the default.
- **`add` holds patterns without the leading `!`.** The field name already says
  it re-includes. Storing the `!` as well would make `add = ["x"]` and
  `add = ["!x"]` two spellings of one rule, and validation rejects the second
  with a message naming the correct form.
- **A rule is a group, not a single pattern.** `ignore` and `add` are lists, so
  the common case stays compact, and a rule carries one `note` for the whole
  group, which is how comments actually read in a `.gitignore`.

### Rendering rules, exactly

1. If `header` is set, emit each of its lines as `# line` first, then a blank
   line before whatever follows.
2. If `name` is set, emit `### name`.
3. Before each section, emit one blank line unless nothing has been emitted yet.
4. Emit `#` repeated `level` times, a space, then `section.name`, when set.
5. Emit each line of the section `note` as `# line`.
6. For each rule: emit a blank line first **iff** the rule has a note *and*
   something has already been emitted in this section *and* the rule has at least
   one pattern; then the note lines; then each `ignore` pattern verbatim; then
   each `add` pattern prefixed with `!`.
7. A comment with empty text renders as a bare `#`, with no trailing space.

The pattern condition in rule 6 is what makes a **note-only rule** round-trip. A
rule with a note and no patterns is a trailing comment; a blank line before it
would split it into its own group on re-import, and a comment-only group is a
section, so the rule would come back as a section instead of a rule. Only
patternless rules are affected, and none could exist before `Rule::validate`
began accepting them.

### The import grammar

One sentence: **a comment block that opens a blank-line-delimited group is a
section header, and comments appearing after patterns are the note of the rule
that follows.**

That single rule does all the work:

- `## Logs` followed by `# Ignore Logs Pattern` is one block: the first line
  names the section at depth 2, the rest becomes the section note.
- `rust.gitignore`'s `# Generated by Cargo` block is a level-1 section, so real
  files keep importing.
- A comment sitting between patterns is a rule note. **This removes
  `Error::CommentAfterPattern` entirely**: the shape the previous design refused
  is now the format's most-used feature.
- An `ignore` pattern following an `add` pattern starts a new rule, because
  rendering emits all of a rule's `ignore` before its `add`. That is what keeps
  `build/ !build/keep build/keep/tmp` in the right order.
- Comments left over after the last pattern in a group become a **note-only
  rule** rather than being dropped. A trailing `# TODO` used to make the
  re-render differ from the source, so import refused a file it could represent.

There are two exceptions to "a comment block that opens a group is a section
header", both anchored at the top of the file:

- A single `###` block with no patterns is the configuration's `name`.
- A `#`-only block **directly above that banner** is the configuration's
  `header`. This is where a licence block goes. The rule is deliberately narrow:
  requiring the banner to follow is what keeps a top-of-file `# Editors` a
  section, which `a_comment_with_no_patterns_is_a_section_at_its_own_depth`
  pins. The cost is that a licence header on a file with no `###` name still
  imports as a section.

## Three corrections to the sketch

Measured, not argued.

- **`[[gitignore]]` followed by `[[section]]` does not nest.** Fed to `tomllib`
  it yields `{"gitignore": [{}], "section": [...]}`: an empty `gitignore`
  element and a separate top-level array. This is the same pitfall as Proposal B
  above. `[[gitignore.section]]` is the form that nests.
- **`add` was written both ways.** The sketch has `add = ["important.log"]` and
  `add = ["!/mise.lock"]`. Both render to a `!` line, so the `!` has to live in
  exactly one place; it lives in the renderer.
- **One blank line between sections, not two.** The sketch shows two. Two would
  make `rust.gitignore` and this repository's own `.gitignore` un-importable,
  which costs far more than the cosmetic difference gains.

## Schema validation

Two layers, because they catch different things at different times.

- **`Config::validate`** runs on every decode and reports what a schema cannot:
  a `!` inside `add`, a rule with no patterns, a blank pattern, `level = 0`, a
  note with no name to attach to. Errors name the exact path, for example
  `gitignore.section[0].rule[1]`.
- **`schema/ignorefile.schema.json`** is published for editors, so a
  configuration gets completion and inline errors while being written. A test
  compares the schema's declared properties against the field names serde
  actually emits, so the schema cannot silently fall behind the Rust types.

Both mirror `deny_unknown_fields`: a typo is an error, not a silently ignored
key.

## Status

**Implemented.** `Config`, `Target`, `Section` and `Rule` live in `config.rs`
with `Config::validate`; the grammar and rendering live in `convert.rs`; the
published schema is `schema/ignorefile.schema.json` and
`crates/ignorefile/tests/schema.rs` holds it to the Rust types.

Two things worth recording, because neither was obvious up front:

- **Depth decides section boundaries, not blank lines.** The first attempt
  delimited sections by blank lines, which broke the moment the renderer put a
  blank line before a noted rule: import read the note back as a section header,
  and the canonical example produced three sections where it should have made
  two. Render and import are only inverse because `##` opens a section and `#`
  does not.
- **A comment block with no patterns under it was a header at any depth.**
  Without that exception a trailing `# Always Add` would have been a note with
  nothing to attach to, and would have been silently dropped.

  **This exception has since been removed.** Note-only rules keep such a comment
  either way, so it bought nothing, and it cost correctness: a level-1 header
  renders as a line indistinguishable from a rule note, so a `#` group followed
  by a bare pattern group came back as a note on the next parse. The file was
  then not a fixed point, which `fmt` cannot tolerate. Depth now decides with no
  exception at all, and for the same reason a note-only rule takes its blank
  line like any other note rather than suppressing it.

The rules were checked against `rust.gitignore`, `sectioned.gitignore` and this
repository's own `.gitignore` before implementation, by simulating them; all
three reproduce byte for byte.

## Deferred, deliberately

- **`[dockerignore]` and other targets.** The shape exists for them; nothing
  implements one yet, because an unused field is code no test can reach and the
  100% coverage gate rejects it. The cost when a release wants it is bounded and
  known: one field on `Config` reusing `Target`, one `$ref` in the JSON Schema
  reusing `$defs/target`, and one more call to the renderer, which already takes
  a `&Target` rather than a `&Config` for exactly this reason. Only
  `Config::validate` needs reshaping, to carry a path prefix so errors read
  `dockerignore.section[0]`.

  **The matcher does not generalize.** `.dockerignore` shares the line grammar
  but not the semantics: Docker resolves patterns against the build context and
  has no equivalent of git's rule that an excluded directory cannot have its
  contents re-included. `GitIgnore::is_ignored` must not be reused for it without
  its own differential test against a Docker oracle.
- **`[extend]` and template pinning.** An unpinned upstream template makes
  `generate` non-deterministic, which makes `check` flap in CI for reasons
  unrelated to the repository. It needs a lockfile with the same job as
  `Cargo.lock`, and a decision on vendoring versus fetching. Phase 5.
- ~~**Normalizing non-canonical input.**~~ **Done.** Import still refuses what it
  cannot reproduce, such as two blank lines between sections; `ignorefile fmt`
  rewrites the file into the form it accepts, and `--check` reports without
  writing.

  Requirement 1 is untouched by this, and deliberately so. What made a rewriting
  command safe was establishing that a byte difference and a meaning difference
  are not the same thing: patterns reach the configuration verbatim and in
  order, so only comments and blank lines can ever move. Checked exhaustively
  over every source of up to four lines drawn from a vocabulary covering each
  parser branch - 563630 of them fail the byte check and not one changes the
  pattern list git acts on - and held as a property test over generated files
  (`canonicalizing_preserves_every_pattern`). `fmt` is therefore a formatter,
  not a rewriter of meaning, and its output lands as a reviewable diff.

  Two latent bugs surfaced while proving it, both of which made the canonical
  form fail to be a fixed point: the level-1 header ambiguity recorded above,
  and a renderer that dropped one trailing `\r` from a pattern. git strips
  exactly one `\r` from every line even in an LF-only file, so `Icon\r\r` means
  the file named `Icon\r`; emitting the pattern verbatim silently retargeted it.
