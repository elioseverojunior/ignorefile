// SPDX-FileCopyrightText: ignorefile contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A Model Context Protocol server for `ignorefile`.
//!
//! MCP is JSON-RPC 2.0. Everything here is a pure function from a request string
//! to an optional response string, so the whole protocol surface is testable
//! without a process, a socket or a client. `main.rs` adds only the stdio loop,
//! which is why it is the sole part excluded from the coverage gate.
//!
//! Why an agent should use this rather than edit the file directly: a text edit
//! can reorder a negation past the pattern it overrides and silently change what
//! git ignores. Going through the core library means every change is validated
//! and round-trip verified, so the failure mode is a refusal with a line number.

use ignorefile::{Config, Error, Format, GitIgnore};
use serde::Deserialize;
use serde_json::{Value, json};

/// The protocol revision this server implements.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC error codes this server can return.
mod code {
    pub(super) const PARSE_ERROR: i32 = -32700;
    pub(super) const INVALID_REQUEST: i32 = -32600;
    pub(super) const METHOD_NOT_FOUND: i32 = -32601;
    pub(super) const INVALID_PARAMS: i32 = -32602;
}

#[derive(Debug, Deserialize)]
struct Request {
    #[serde(default)]
    jsonrpc: String,
    /// Absent for a notification, which takes no response.
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// `Value::to_string` cannot fail, unlike `serde_json::to_string` over a struct,
/// so building responses this way removes an error path that no test could ever
/// reach and the 100% coverage gate could never satisfy.
fn success(id: &Value, result: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn failure(id: &Value, code: i32, message: impl Into<String>) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() },
    })
    .to_string()
}

/// A tool this server exposes, and the shape of its arguments.
struct Tool {
    name: &'static str,
    description: &'static str,
    /// `(name, JSON type, description)`.
    properties: &'static [(&'static str, &'static str, &'static str)],
    required: &'static [&'static str],
}

const FORMAT_DOC: &str = "Encoding: toml, json, yaml or yml. Defaults to toml.";

const TOOLS: &[Tool] = &[
    Tool {
        name: "import",
        description: "Convert .gitignore text into structured configuration. \
                      Refuses rather than lose information.",
        properties: &[
            ("gitignore", "string", "The .gitignore text to import."),
            ("format", "string", FORMAT_DOC),
        ],
        required: &["gitignore"],
    },
    Tool {
        name: "generate",
        description: "Render configuration text as .gitignore text.",
        properties: &[
            ("config", "string", "The configuration text."),
            ("format", "string", FORMAT_DOC),
        ],
        required: &["config"],
    },
    Tool {
        name: "validate",
        description: "Check configuration text and report the first problem.",
        properties: &[
            ("config", "string", "The configuration text."),
            ("format", "string", FORMAT_DOC),
        ],
        required: &["config"],
    },
    Tool {
        name: "explain",
        description: "Say whether git would ignore a path under the given rules.",
        properties: &[
            ("gitignore", "string", "The .gitignore text."),
            (
                "path",
                "string",
                "Repository-relative path, using / separators.",
            ),
            ("is_dir", "boolean", "Whether the path is a directory."),
        ],
        required: &["gitignore", "path"],
    },
];

impl Tool {
    fn schema(&self) -> Value {
        let properties: serde_json::Map<String, Value> = self
            .properties
            .iter()
            .map(|(name, kind, description)| {
                (
                    (*name).to_owned(),
                    json!({ "type": kind, "description": description }),
                )
            })
            .collect();
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": self.required,
            }
        })
    }
}

/// A required string argument, or the reason it is missing.
fn string_arg(arguments: &Value, name: &str) -> Result<String, String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing required string argument {name:?}"))
}

/// One place that turns a typed error into the string a tool result carries.
///
/// Shared rather than repeated per call site: several of those call sites cannot
/// actually fail, and a `map_err` closure at each would leave a region the
/// coverage gate could never satisfy.
fn plain<T>(result: Result<T, Error>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
}

fn format_arg(arguments: &Value) -> Result<Format, String> {
    let extension = arguments
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("toml");
    plain(Format::from_extension(extension))
}

/// Runs one tool, returning its text output or the reason it failed.
fn call_tool(name: &str, arguments: &Value) -> Result<String, String> {
    match name {
        "import" => {
            let source = string_arg(arguments, "gitignore")?;
            let config = plain(Config::try_from(&GitIgnore::parse(&source)))?;
            plain(config.encode(format_arg(arguments)?))
        }
        "generate" => {
            let text = string_arg(arguments, "config")?;
            let config = plain(Config::decode(&text, format_arg(arguments)?))?;
            Ok(GitIgnore::from(&config).render())
        }
        "validate" => {
            let text = string_arg(arguments, "config")?;
            let config = plain(Config::decode(&text, format_arg(arguments)?))?;
            Ok(format!(
                "valid: {} section(s)",
                config.gitignore.sections.len()
            ))
        }
        "explain" => {
            let source = string_arg(arguments, "gitignore")?;
            let path = string_arg(arguments, "path")?;
            let is_dir = arguments
                .get("is_dir")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let ignored = GitIgnore::parse(&source).is_ignored(&path, is_dir);
            Ok(if ignored {
                format!("{path} is ignored")
            } else {
                format!("{path} is not ignored")
            })
        }
        other => Err(format!("unknown tool {other:?}")),
    }
}

/// MCP reports tool failures inside a successful response, so the model can read
/// and act on them, rather than as transport errors.
fn tool_result(outcome: Result<String, String>) -> Value {
    let (text, is_error) = outcome.map_or_else(|error| (error, true), |text| (text, false));
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

fn handle_call(id: &Value, params: &Value) -> String {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return failure(id, code::INVALID_PARAMS, "missing tool name");
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    success(id, &tool_result(call_tool(name, &arguments)))
}

/// Handles one JSON-RPC message.
///
/// Returns `None` for a notification, which by specification takes no response.
#[must_use]
pub fn handle(message: &str) -> Option<String> {
    let request: Request = match serde_json::from_str(message) {
        Ok(request) => request,
        Err(error) => {
            return Some(failure(&Value::Null, code::PARSE_ERROR, error.to_string()));
        }
    };
    // A notification has no id and expects silence, even on error.
    let id = request.id.clone()?;

    if request.jsonrpc != "2.0" {
        return Some(failure(
            &id,
            code::INVALID_REQUEST,
            format!("expected jsonrpc 2.0, got {:?}", request.jsonrpc),
        ));
    }

    Some(match request.method.as_str() {
        "initialize" => success(
            &id,
            &json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": env!("CARGO_PKG_NAME"),
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        ),
        "tools/list" => success(
            &id,
            &json!({ "tools": TOOLS.iter().map(Tool::schema).collect::<Vec<_>>() }),
        ),
        "tools/call" => handle_call(&id, &request.params),
        other => failure(
            &id,
            code::METHOD_NOT_FOUND,
            format!("unknown method {other:?}"),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::{PROTOCOL_VERSION, handle};
    use serde_json::{Value, json};

    fn reply(message: &Value) -> Value {
        let text = handle(&message.to_string()).expect("a request gets a response");
        serde_json::from_str(&text).expect("the response is JSON")
    }

    fn call(name: &str, arguments: &Value) -> Value {
        reply(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))
    }

    /// The text a tool returned, and whether it was reported as an error.
    fn content(response: &Value) -> (String, bool) {
        let result = &response["result"];
        (
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            result["isError"].as_bool().unwrap_or_default(),
        )
    }

    const SOURCE: &str = "## Logs\n*.log\n\n# keep this one\n!important.log\n";

    #[test]
    fn initialize_reports_the_protocol_and_the_server() {
        let response = reply(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}));
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["result"]["serverInfo"]["name"], "ignorefile-mcp");
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_describes_every_tool() {
        let response = reply(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
        let tools = response["result"]["tools"].as_array().expect("a list");
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(names, vec!["import", "generate", "validate", "explain"]);
        for tool in tools {
            assert!(!tool["description"].as_str().unwrap_or_default().is_empty());
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert!(tool["inputSchema"]["required"].is_array());
        }
    }

    #[test]
    fn import_then_generate_reproduces_the_source() {
        let (config, is_error) = content(&call("import", &json!({"gitignore": SOURCE})));
        assert!(!is_error, "{config}");
        let (rendered, is_error) = content(&call("generate", &json!({"config": config})));
        assert!(!is_error, "{rendered}");
        assert_eq!(rendered, SOURCE);
    }

    #[test]
    fn the_format_argument_selects_the_encoding() {
        let (json_config, _) = content(&call(
            "import",
            &json!({"gitignore": SOURCE, "format": "json"}),
        ));
        assert!(json_config.starts_with('{'), "{json_config}");
        let (rendered, _) = content(&call(
            "generate",
            &json!({"config": json_config, "format": "json"}),
        ));
        assert_eq!(rendered, SOURCE);
    }

    #[test]
    fn validate_reports_both_outcomes() {
        let (config, _) = content(&call("import", &json!({"gitignore": SOURCE})));
        let (message, is_error) = content(&call("validate", &json!({"config": config})));
        assert!(!is_error);
        assert!(message.contains("valid: 1 section(s)"), "{message}");

        let bad =
            "version = 1\n\n[[gitignore.section]]\n\n[[gitignore.section.rule]]\nadd = [\"!x\"]\n";
        let (message, is_error) = content(&call("validate", &json!({"config": bad})));
        assert!(is_error);
        assert!(message.contains("without the leading `!`"), "{message}");
    }

    #[test]
    fn explain_answers_for_files_and_directories() {
        let (message, is_error) = content(&call(
            "explain",
            &json!({"gitignore": SOURCE, "path": "app.log"}),
        ));
        assert!(!is_error);
        assert!(message.contains("is ignored"), "{message}");

        let (message, _) = content(&call(
            "explain",
            &json!({"gitignore": SOURCE, "path": "important.log"}),
        ));
        assert!(message.contains("is not ignored"), "{message}");

        let (message, _) = content(&call(
            "explain",
            &json!({"gitignore": "build/\n", "path": "build", "is_dir": true}),
        ));
        assert!(message.contains("is ignored"), "{message}");
    }

    #[test]
    fn a_tool_failure_is_reported_in_band() {
        // MCP puts tool errors in a successful response so the model can read
        // them, rather than failing the transport.
        let response = call("import", &json!({"gitignore": "# A\n/a\n\n\n# B\n/b\n"}));
        assert!(response["error"].is_null(), "transport must succeed");
        let (message, is_error) = content(&response);
        assert!(is_error);
        assert!(message.contains("line 4"), "{message}");
    }

    #[test]
    fn missing_and_bad_arguments_are_reported() {
        let (message, is_error) = content(&call("import", &json!({})));
        assert!(is_error);
        assert!(message.contains("gitignore"), "{message}");

        let (message, is_error) = content(&call(
            "import",
            &json!({"gitignore": SOURCE, "format": "ini"}),
        ));
        assert!(is_error);
        assert!(message.contains("ini"), "{message}");

        let (message, is_error) = content(&call("generate", &json!({})));
        assert!(is_error && message.contains("config"), "{message}");
        let (message, is_error) = content(&call("validate", &json!({})));
        assert!(is_error && message.contains("config"), "{message}");
        let (message, is_error) = content(&call("explain", &json!({"gitignore": SOURCE})));
        assert!(is_error && message.contains("path"), "{message}");
        // The other required argument of the same tool.
        let (message, is_error) = content(&call("explain", &json!({"path": "x"})));
        assert!(is_error && message.contains("gitignore"), "{message}");
    }

    #[test]
    fn every_tool_reports_an_unknown_format() {
        for tool in ["generate", "validate"] {
            let (message, is_error) = content(&call(
                tool,
                &json!({"config": "version = 1", "format": "ini"}),
            ));
            assert!(is_error, "{tool}");
            assert!(message.contains("ini"), "{tool}: {message}");
        }
    }

    #[test]
    fn malformed_config_reaches_the_caller() {
        let (message, is_error) = content(&call("generate", &json!({"config": "{{{ bad"})));
        assert!(is_error && !message.is_empty(), "{message}");
        let (message, is_error) = content(&call("validate", &json!({"config": "{{{ bad"})));
        assert!(is_error && !message.is_empty(), "{message}");
    }

    #[test]
    fn an_unknown_tool_is_reported() {
        let (message, is_error) = content(&call("nope", &json!({})));
        assert!(is_error);
        assert!(message.contains("unknown tool"), "{message}");
    }

    #[test]
    fn a_call_without_a_name_is_an_invalid_params_error() {
        let response = reply(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {}
        }));
        assert_eq!(response["error"]["code"], -32602);
    }

    #[test]
    fn a_call_without_arguments_still_runs_the_tool() {
        let response = reply(&json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "import"}
        }));
        let (message, is_error) = content(&response);
        assert!(is_error && message.contains("gitignore"), "{message}");
    }

    #[test]
    fn an_unknown_method_is_reported() {
        let response = reply(&json!({"jsonrpc": "2.0", "id": 5, "method": "nope"}));
        assert_eq!(response["error"]["code"], -32601);
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("nope")
        );
    }

    #[test]
    fn a_wrong_protocol_version_is_rejected() {
        let response = reply(&json!({"jsonrpc": "1.0", "id": 6, "method": "initialize"}));
        assert_eq!(response["error"]["code"], -32600);
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        let text = handle("{ not json").expect("a response");
        let response: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(response["error"]["code"], -32700);
        assert!(response["id"].is_null());
    }

    #[test]
    fn a_notification_gets_no_response() {
        // No id, so the specification says stay silent, even for a bad method.
        assert!(handle(&json!({"jsonrpc": "2.0", "method": "initialized"}).to_string()).is_none());
        assert!(handle(&json!({"jsonrpc": "1.0", "method": "nope"}).to_string()).is_none());
    }

    #[test]
    fn responses_carry_the_request_id_back() {
        let response = reply(&json!({"jsonrpc": "2.0", "id": "abc", "method": "tools/list"}));
        assert_eq!(response["id"], "abc");
        assert_eq!(response["jsonrpc"], "2.0");
    }
}
