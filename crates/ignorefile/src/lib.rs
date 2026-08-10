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

/// Compiles the usage example in `README.md` as a doctest.
///
/// `cfg(doctest)` means this module exists only while rustdoc is collecting
/// doctests: it adds nothing to the rendered documentation and nothing to a
/// normal build. Without it the README sample is never compiled, so it can
/// drift from the API it demonstrates while still looking correct -- which is
/// the one kind of documentation rot no gate in this repository would catch.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme {}

mod config;
mod convert;
mod error;
mod gitignore;
mod matcher;

pub use config::{Config, Format, NAME_LEVEL, Rule, SECTION_LEVEL, Section, Target, VERSION};
pub use error::Error;
pub use gitignore::{GitIgnore, Line, LineKind};
