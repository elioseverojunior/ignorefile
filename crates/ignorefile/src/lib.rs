// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core library for `ignorefile`.
//!
//! Parses a `.gitignore` into a model that renders back to the original bytes,
//! so the ignore rules of a repository can be kept as reviewable configuration
//! instead of a hand-edited flat file.
//!
//! The invariant every later phase is built on top of, and none may weaken, is
//! that `render(parse(x)) == x` byte for byte. `.gitignore` is order-dependent
//! and anchor-sensitive, so a model that normalizes its input changes which
//! files git ignores. See `docs/ROADMAP.md`.

mod config;
mod convert;
mod error;
mod gitignore;
mod matcher;

pub use config::{Config, Format, NAME_LEVEL, Rule, SECTION_LEVEL, Section, Target, VERSION};
pub use error::Error;
pub use gitignore::{GitIgnore, Line, LineKind};
