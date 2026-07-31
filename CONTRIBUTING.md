<!--
SPDX-FileCopyrightText: ignorefile contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Contributing

Thanks for considering a contribution.

## Read the policy first

The binding contribution policy, including the rules on AI-assisted work, lives
in [docs/guidelines/CONTRIBUTION.md](docs/guidelines/CONTRIBUTION.md). This file
is the practical how-to; that file is the agreement. Read it before opening a
pull request.

The short version: a pull request is a long-term commitment, because maintainers
have to review, integrate and support your code indefinitely. What is valuable
is your understanding and your willingness to maintain the work, not the diff
itself. Concretely, you should be able to explain any line of your change to a
reviewer without AI assistance.

AI-written pull-request descriptions, commit messages and reviewer replies are
prohibited and result in the pull request being closed.

## Set up

```sh
git clone https://github.com/elioseverojunior/ignorefile
cd ignorefile
mise run setup          # provisions the pinned toolchain and every tool
mise run hooks-install  # install the hk git hooks
mise run doctor         # verify; reports MISSING for anything absent
```

[mise](https://mise.jdx.dev) is the only prerequisite. It reads the Rust version
from `rust-toolchain.toml` and installs everything else from `mise.toml`.

## Make a change

The project is strictly test-driven. Write the failing test first.

```sh
mise run test        # cargo-nextest; does NOT run doctests
mise run test:doc    # the doctest half of the suite
mise run lint        # fmt:check + clippy, read-only
mise run pr-ready    # auto-format, then check everything
```

Work on a feature branch in its own worktree, and rebase rather than merge so
`main` stays linear:

```sh
git worktree add worktrees/my-feature -b my-feature
```

Before you push, `mise run pr-ready` should be clean. `mise run ci` runs the
full pipeline, including the licence, security and coverage gates.
[docs/RUNBOOK.md](docs/RUNBOOK.md) has the diagnosis steps for each gate, and the
list of commands that fail by design because CI does not exist yet.

Things that will surprise you if you have not read
[AGENTS.md](AGENTS.md#gotchas):

- Warnings are hard errors, and setting `RUSTFLAGS` silently disables that gate.
- The coverage threshold is 100%.
- MSRV is 1.95 even though the toolchain is 1.97, so clippy rejects newer APIs.
- `committed.toml`, `.gitmessage` and part of `cliff.toml` are generated from
  `commit-types.toml`. Edit the source, then run `mise run commit-config`.
- Text is ASCII only: no em dash, no unicode arrows.

## Commits

Conventional Commits, lowercase imperative subject, at most 50 characters, no
trailing period. The allowed types are defined in `commit-types.toml` and shown
in `.gitmessage`.

```text
feat(cli): add gitignore import subcommand
```

`mise run commit-lint` checks a message; the `commit-msg` git hook runs it
automatically.

## Report a bug

Search existing issues and pull requests first, so you do not duplicate work
that is already in flight. A useful report has the `.gitignore` input, the
config you got, and the config you expected.
