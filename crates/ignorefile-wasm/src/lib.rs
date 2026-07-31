// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WebAssembly bindings for `ignorefile`.
//!
//! Converts an ignore file to structured configuration and back, in the browser,
//! with nothing installed.
//!
//! This file holds **only** `#[wasm_bindgen]` shims. Every one is a one-line
//! delegate to [`api`], which is plain Rust over plain strings and is tested
//! like any other module. That split is what makes excluding this file from the
//! coverage gate honest: a `#[wasm_bindgen]` export cannot be exercised by a
//! native test run, so there must be nothing here worth exercising.

mod api;

use wasm_bindgen::prelude::wasm_bindgen;

/// Converts `.gitignore` text into configuration text.
///
/// `format` is one of `toml`, `json`, `yaml` or `yml`.
///
/// # Errors
///
/// The reason the file cannot be represented, as a string.
#[wasm_bindgen]
pub fn import(gitignore: &str, format: &str) -> Result<String, String> {
    api::import(gitignore, format)
}

/// Renders configuration text as `.gitignore` text.
///
/// # Errors
///
/// The reason the configuration cannot be read, as a string.
#[wasm_bindgen]
pub fn generate(config: &str, format: &str) -> Result<String, String> {
    api::generate(config, format)
}

/// Re-encodes configuration text from one format into another.
///
/// # Errors
///
/// The reason either format cannot be handled, as a string.
#[wasm_bindgen]
pub fn convert(config: &str, from: &str, to: &str) -> Result<String, String> {
    api::convert(config, from, to)
}

/// Checks configuration text.
///
/// # Errors
///
/// The reason the configuration is not usable, as a string.
#[wasm_bindgen]
pub fn validate(config: &str, format: &str) -> Result<(), String> {
    api::validate(config, format)
}

/// Whether git would ignore `path` under these rules.
#[wasm_bindgen(js_name = isIgnored)]
#[must_use]
pub fn is_ignored(gitignore: &str, path: &str, is_dir: bool) -> bool {
    api::is_ignored(gitignore, path, is_dir)
}
