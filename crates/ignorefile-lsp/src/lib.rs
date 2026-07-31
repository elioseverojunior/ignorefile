// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A Language Server for ignore files.
//!
//! Reports problems as they are typed: a file that cannot be kept as
//! configuration, a duplicated pattern, a re-inclusion that can never apply.
//!
//! Split the same way as the MCP server. [`diagnostics`] is pure analysis,
//! [`Server`] is pure protocol, and `main.rs` adds only the stdio loop and the
//! `Content-Length` framing, which is why it is the sole part excluded from the
//! coverage gate.

pub mod diagnostics;

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::diagnostics::Diagnostic;

#[derive(Debug, Deserialize)]
struct Request {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// `Value::to_string` cannot fail, unlike `serde_json::to_string` over a struct,
/// so building messages this way removes an error path no test could reach.
fn response(id: &Value, result: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn notification(method: &str, params: &Value) -> String {
    json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
}

fn as_lsp(diagnostic: &Diagnostic) -> Value {
    json!({
        "range": {
            "start": { "line": diagnostic.line, "character": 0 },
            "end": { "line": diagnostic.line, "character": diagnostic.length },
        },
        "severity": diagnostic.severity as u8,
        "source": "ignorefile",
        "message": diagnostic.message,
    })
}

/// Builds the `textDocument/publishDiagnostics` notification for one document.
fn publish(uri: &str, text: &str) -> String {
    let found: Vec<Value> = diagnostics::analyze(text).iter().map(as_lsp).collect();
    notification(
        "textDocument/publishDiagnostics",
        &json!({ "uri": uri, "diagnostics": found }),
    )
}

/// Open documents, by URI. The editor owns the text; the server mirrors it.
#[derive(Debug, Default)]
pub struct Server {
    documents: HashMap<String, String>,
    shutdown_requested: bool,
}

/// What the caller should do after a message.
#[derive(Debug, PartialEq, Eq)]
pub enum Reaction {
    /// Send these messages, in order.
    Send(Vec<String>),
    /// Send these, then stop the loop.
    Exit(Vec<String>),
}

fn uri_of(params: &Value) -> Option<&str> {
    params["textDocument"]["uri"].as_str()
}

impl Server {
    /// A server with no open documents.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The text the editor last told us a document contains.
    #[must_use]
    pub fn document(&self, uri: &str) -> Option<&str> {
        self.documents.get(uri).map(String::as_str)
    }

    /// Handles one decoded message and says what to send back.
    #[must_use]
    pub fn handle(&mut self, message: &str) -> Reaction {
        let Ok(request) = serde_json::from_str::<Request>(message) else {
            // A malformed message has no id to answer, so there is nothing to
            // send. Staying up beats dying on one bad frame.
            return Reaction::Send(Vec::new());
        };
        let params = request.params;

        match (request.method.as_str(), request.id) {
            ("initialize", Some(id)) => Reaction::Send(vec![response(
                &id,
                &json!({
                    "capabilities": {
                        // 1 = full text on every change: an ignore file is small,
                        // and incremental sync would buy nothing.
                        "textDocumentSync": 1,
                        "diagnosticProvider": { "interFileDependencies": false, "workspaceDiagnostics": false },
                    },
                    "serverInfo": {
                        "name": env!("CARGO_PKG_NAME"),
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                }),
            )]),
            ("shutdown", Some(id)) => {
                self.shutdown_requested = true;
                Reaction::Send(vec![response(&id, &Value::Null)])
            }
            ("exit", _) => Reaction::Exit(Vec::new()),
            ("textDocument/didOpen", _) => {
                let Some(uri) = uri_of(&params) else {
                    return Reaction::Send(Vec::new());
                };
                let text = params["textDocument"]["text"].as_str().unwrap_or_default();
                self.documents.insert(uri.to_owned(), text.to_owned());
                Reaction::Send(vec![publish(uri, text)])
            }
            ("textDocument/didChange", _) => {
                let Some(uri) = uri_of(&params) else {
                    return Reaction::Send(Vec::new());
                };
                // Full sync, so the last change carries the whole document.
                let Some(text) = params["contentChanges"]
                    .as_array()
                    .and_then(|changes| changes.last())
                    .and_then(|change| change["text"].as_str())
                else {
                    return Reaction::Send(Vec::new());
                };
                self.documents.insert(uri.to_owned(), text.to_owned());
                Reaction::Send(vec![publish(uri, text)])
            }
            ("textDocument/didClose", _) => {
                let Some(uri) = uri_of(&params) else {
                    return Reaction::Send(Vec::new());
                };
                self.documents.remove(uri);
                // Clear the editor's squiggles for a file we no longer track.
                Reaction::Send(vec![publish_empty(uri)])
            }
            (_, Some(id)) => Reaction::Send(vec![response(&id, &Value::Null)]),
            // An unknown notification is ignored, as the specification requires.
            (_, None) => Reaction::Send(Vec::new()),
        }
    }

    /// Whether the client asked to shut down.
    #[must_use]
    pub const fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }
}

fn publish_empty(uri: &str) -> String {
    notification(
        "textDocument/publishDiagnostics",
        &json!({ "uri": uri, "diagnostics": [] }),
    )
}

/// Wraps a payload in the `Content-Length` framing LSP uses on the wire.
#[must_use]
pub fn frame(payload: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{payload}", payload.len())
}

#[cfg(test)]
mod tests {
    use super::{Reaction, Server, frame};
    use serde_json::{Value, json};

    fn sent(reaction: &Reaction) -> Vec<Value> {
        let messages = match reaction {
            Reaction::Send(messages) | Reaction::Exit(messages) => messages,
        };
        messages
            .iter()
            .map(|text| serde_json::from_str(text).expect("valid JSON"))
            .collect()
    }

    fn open(server: &mut Server, uri: &str, text: &str) -> Vec<Value> {
        let reaction = server.handle(
            &json!({
                "jsonrpc": "2.0", "method": "textDocument/didOpen",
                "params": { "textDocument": { "uri": uri, "text": text } }
            })
            .to_string(),
        );
        sent(&reaction)
    }

    #[test]
    fn initialize_advertises_full_sync_and_diagnostics() {
        let mut server = Server::new();
        let reaction =
            server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize"}).to_string());
        let messages = sent(&reaction);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["id"], 1);
        assert_eq!(messages[0]["result"]["capabilities"]["textDocumentSync"], 1);
        assert_eq!(
            messages[0]["result"]["serverInfo"]["name"],
            "ignorefile-lsp"
        );
    }

    #[test]
    fn opening_a_clean_document_publishes_no_diagnostics() {
        let mut server = Server::new();
        let messages = open(&mut server, "file:///.gitignore", "## Logs\n*.log\n");
        assert_eq!(messages[0]["method"], "textDocument/publishDiagnostics");
        assert_eq!(messages[0]["params"]["uri"], "file:///.gitignore");
        assert_eq!(
            messages[0]["params"]["diagnostics"]
                .as_array()
                .expect("a list")
                .len(),
            0
        );
        assert_eq!(
            server.document("file:///.gitignore"),
            Some("## Logs\n*.log\n")
        );
    }

    #[test]
    fn opening_a_problem_document_publishes_a_ranged_diagnostic() {
        let mut server = Server::new();
        let messages = open(&mut server, "file:///x", "## S\n*.log\n*.log\n");
        let found = &messages[0]["params"]["diagnostics"][0];
        assert_eq!(found["range"]["start"]["line"], 2);
        assert_eq!(found["range"]["end"]["character"], 5);
        assert_eq!(found["severity"], 4, "hint");
        assert_eq!(found["source"], "ignorefile");
        assert!(
            found["message"]
                .as_str()
                .unwrap_or_default()
                .contains("duplicate")
        );
    }

    #[test]
    fn a_warning_uses_the_warning_severity() {
        let mut server = Server::new();
        let messages = open(&mut server, "file:///x", "build/\n!build/keep\n");
        assert_eq!(messages[0]["params"]["diagnostics"][0]["severity"], 2);
    }

    #[test]
    fn changing_a_document_republishes_and_replaces_the_text() {
        let mut server = Server::new();
        open(&mut server, "file:///x", "*.log\n*.log\n");
        let reaction = server.handle(
            &json!({
                "jsonrpc": "2.0", "method": "textDocument/didChange",
                "params": {
                    "textDocument": { "uri": "file:///x" },
                    "contentChanges": [{ "text": "*.log\n" }]
                }
            })
            .to_string(),
        );
        let messages = sent(&reaction);
        assert_eq!(
            messages[0]["params"]["diagnostics"]
                .as_array()
                .expect("a list")
                .len(),
            0,
            "the duplicate was removed"
        );
        assert_eq!(server.document("file:///x"), Some("*.log\n"));
    }

    #[test]
    fn closing_a_document_clears_its_diagnostics() {
        let mut server = Server::new();
        open(&mut server, "file:///x", "*.log\n*.log\n");
        let reaction = server.handle(
            &json!({
                "jsonrpc": "2.0", "method": "textDocument/didClose",
                "params": { "textDocument": { "uri": "file:///x" } }
            })
            .to_string(),
        );
        let messages = sent(&reaction);
        assert_eq!(
            messages[0]["params"]["diagnostics"]
                .as_array()
                .expect("a list")
                .len(),
            0
        );
        assert_eq!(server.document("file:///x"), None);
    }

    #[test]
    fn shutdown_then_exit_stops_the_loop() {
        let mut server = Server::new();
        assert!(!server.shutdown_requested());
        let reaction =
            server.handle(&json!({"jsonrpc":"2.0","id":9,"method":"shutdown"}).to_string());
        assert_eq!(sent(&reaction)[0]["id"], 9);
        assert!(server.shutdown_requested());

        let reaction = server.handle(&json!({"jsonrpc":"2.0","method":"exit"}).to_string());
        assert_eq!(reaction, Reaction::Exit(Vec::new()));
        // Also drives the Exit arm of the `sent` helper.
        assert!(sent(&reaction).is_empty());
    }

    #[test]
    fn an_unknown_request_gets_a_null_result_and_a_notification_gets_silence() {
        let mut server = Server::new();
        let reaction = server.handle(&json!({"jsonrpc":"2.0","id":7,"method":"nope"}).to_string());
        let messages = sent(&reaction);
        assert_eq!(messages[0]["id"], 7);
        assert!(messages[0]["result"].is_null());

        let reaction = server.handle(&json!({"jsonrpc":"2.0","method":"nope"}).to_string());
        assert!(sent(&reaction).is_empty());
    }

    #[test]
    fn malformed_and_incomplete_messages_are_survived() {
        let mut server = Server::new();
        assert!(sent(&server.handle("{ not json")).is_empty());

        // A document notification with no uri has nothing to act on.
        for method in [
            "textDocument/didOpen",
            "textDocument/didChange",
            "textDocument/didClose",
        ] {
            let reaction =
                server.handle(&json!({"jsonrpc":"2.0","method":method,"params":{}}).to_string());
            assert!(sent(&reaction).is_empty(), "{method}");
        }

        // A change with no content has nothing to apply.
        let reaction = server.handle(
            &json!({
                "jsonrpc": "2.0", "method": "textDocument/didChange",
                "params": { "textDocument": { "uri": "file:///x" }, "contentChanges": [] }
            })
            .to_string(),
        );
        assert!(sent(&reaction).is_empty());
    }

    #[test]
    fn an_open_without_text_is_treated_as_empty() {
        let mut server = Server::new();
        let reaction = server.handle(
            &json!({
                "jsonrpc": "2.0", "method": "textDocument/didOpen",
                "params": { "textDocument": { "uri": "file:///x" } }
            })
            .to_string(),
        );
        assert_eq!(
            sent(&reaction)[0]["params"]["diagnostics"]
                .as_array()
                .expect("a list")
                .len(),
            0
        );
        assert_eq!(server.document("file:///x"), Some(""));
    }

    #[test]
    fn framing_declares_the_byte_length() {
        assert_eq!(frame("hi"), "Content-Length: 2\r\n\r\nhi");
    }
}
