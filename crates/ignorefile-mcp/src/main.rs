// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entry point for the `ignorefile-mcp` server.
//!
//! Deliberately thin: one line-delimited JSON-RPC loop over stdio, and nothing
//! else. Every decision lives in the library, where it is a pure function and is
//! tested. This file is excluded from the coverage gate because a stdio loop
//! cannot be exercised without spawning a process.

use std::io::{BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();

    for line in stdin.lines() {
        let Ok(line) = line else {
            eprintln!("error: could not read from stdin");
            return ExitCode::FAILURE;
        };
        if line.trim().is_empty() {
            continue;
        }
        // `None` is a notification, which by specification takes no response.
        let Some(response) = ignorefile_mcp::handle(&line) else {
            continue;
        };
        if writeln!(stdout, "{response}").is_err() || stdout.flush().is_err() {
            eprintln!("error: could not write to stdout");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
