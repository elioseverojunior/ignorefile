// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entry point for the `ignorefile-lsp` server.
//!
//! Deliberately thin: read `Content-Length`-framed messages from stdin, hand
//! each to the library, write what comes back. Every decision lives in the
//! library, where it is a pure function and is tested. This file is excluded
//! from the coverage gate because a stdio loop cannot be exercised without
//! spawning a process.

// `read_exact` comes from `Read`, reachable here as a supertrait of `BufRead`.
use std::io::{BufRead, Write};
use std::process::ExitCode;

use ignorefile_lsp::{Reaction, Server, frame};

/// Reads one LSP frame: headers, a blank line, then exactly `Content-Length`
/// bytes. Returns `None` at end of input.
fn read_message(input: &mut impl BufRead) -> Option<String> {
    let mut length = None;
    loop {
        let mut header = String::new();
        if input.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut payload = vec![0; length?];
    input.read_exact(&mut payload).ok()?;
    String::from_utf8(payload).ok()
}

fn main() -> ExitCode {
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let mut server = Server::new();

    while let Some(message) = read_message(&mut input) {
        let reaction = server.handle(&message);
        let (messages, stop) = match reaction {
            Reaction::Send(messages) => (messages, false),
            Reaction::Exit(messages) => (messages, true),
        };
        for payload in messages {
            if write!(output, "{}", frame(&payload)).is_err() || output.flush().is_err() {
                eprintln!("error: could not write to stdout");
                return ExitCode::FAILURE;
            }
        }
        if stop {
            break;
        }
    }
    ExitCode::SUCCESS
}
