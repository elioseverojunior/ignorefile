// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entry point for the `ignorefile` executable.
//!
//! Deliberately thin: everything testable lives in the library. This file is the
//! only place that performs output I/O, and it is excluded from the coverage gate
//! because a binary entry point cannot be exercised without spawning a
//! subprocess.

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use ignorefile_cli::{Cli, run};

/// The name this binary was invoked as, so hints name the command the user
/// typed. The same executable ships as both `ignorefile` and `ign`.
fn program_name() -> String {
    std::env::args_os()
        .next()
        .as_deref()
        .map(Path::new)
        .and_then(Path::file_stem)
        .map_or_else(
            || "ignorefile".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        )
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut report = Vec::new();
    match run(&cli, &program_name(), &mut report) {
        Ok(()) => {
            for line in &report {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            // Anything the command managed to do before failing is still worth
            // showing, so the report is printed either way.
            for line in &report {
                println!("{line}");
            }
            // `{:#}` prints the anyhow context chain, which carries the useful
            // detail: which file, and what the parser said.
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
