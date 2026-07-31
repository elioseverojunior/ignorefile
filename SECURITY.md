<!--
SPDX-FileCopyrightText: ignorefile contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Security policy

`ignorefile` reads and writes ignore files. It runs no network code, spawns no
processes outside the test suite, and holds no credentials. The realistic
threat is a malformed or hostile `.gitignore` or configuration file causing a
panic, an infinite loop, or a rendered file that ignores something the author
did not intend to ignore -- the last of these being the most serious, because it
can silently commit a secret.

## Reporting a vulnerability

Report privately. Do **not** open a public issue.

Preferred: use GitHub's private reporting. Go to the
[Security tab](https://github.com/elioseverojunior/ignorefile/security), and
under "Reporting" in the left sidebar choose Advisories, then "Report a
vulnerability". Private vulnerability reporting is enabled on this repository.

If that is not available to you, email the maintainer:
Elio Severo Junior <elioseverojunior@gmail.com> or <elio@elio.eti.br>.

When emailing, prefer a provider that does hop-to-hop (transport) encryption --
ideally one using MTA-STS, otherwise STARTTLS. That is weaker than end-to-end
encryption, but it is enough here and it is something everyone can actually do.

Please include what the input was, what happened, and what you expected. A
`.gitignore` or config file that reproduces the problem is worth more than a
description of it.

You will get an acknowledgement within a week. Credit is given to reporters by
default; say so if you would rather stay anonymous or pseudonymous, and that
will be respected.

Patches are welcome. See
[docs/guidelines/CONTRIBUTION.md](docs/guidelines/CONTRIBUTION.md), and note
that it forbids AI-generated pull requests.

## Supported versions

Pre-alpha, nothing published to crates.io yet. Only `main` is supported; there
are no maintained release branches and no backports. This section will be
replaced once there is a released version.

## What the project does to stay secure

Not promises -- these run in CI on every push, and you can read them in
[`.github/workflows/`](.github/workflows/):

| Control | Where |
| --- | --- |
| `unsafe_code = "deny"` workspace-wide; no escape hatch | `Cargo.toml` |
| `unwrap`/`expect` denied in production code (a panic is a DoS) | `.clippy.toml` |
| Warnings are hard errors, including rustdoc | `.cargo/config.toml` |
| RustSec advisory scan (`cargo audit`) | CI, Supply Chain job |
| Licence, ban and source policy (`cargo deny`) | CI, Supply Chain job |
| Secret scanning (gitleaks) | CI, Secrets Scan job |
| Static analysis (CodeQL) | `codeql.yml` |
| OpenSSF Scorecard | `scorecards.yml` |
| SLSA L3 provenance and Sigstore signing of release artifacts | `release.yml` |
| Dependency updates and GitHub secret-scanning push protection | Dependabot, repo settings |
| 100% test coverage gate, including doctests | `mise run coverage:tarpaulin` |

Correctness is a security property in this project. The differential test in
`crates/ignorefile/tests/differential.rs` checks agreement with `git
check-ignore` on generated inputs, because a rendered file that disagrees with
git is exactly how a secret ends up committed.

## Scope

In scope: a panic, hang, or excessive memory use on untrusted input; any case
where importing then re-generating changes which files git ignores; a path
traversal or unintended write outside the paths given on the command line.

Out of scope: the behaviour of `git` itself; a `.gitignore` that ignores the
wrong files because it was written that way; anything requiring an attacker who
already has write access to the repository or to your machine.
