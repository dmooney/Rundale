//! MCP protocol surface — handshake plus the `tools/*` methods.
//!
//! The protocol version pinned here (`PROTOCOL_VERSION`) is the spec
//! revision Claude Desktop / Claude Code currently negotiate against; the
//! initialize response echoes a version the client can recognise.
//!
//! The server intentionally implements only the small subset of MCP needed
//! to expose tools. Resources, prompts, sampling, and roots are not
//! advertised in the capability map and respond with `method not found`
//! per the JSON-RPC spec.

use crate::backend::{BackendError, TauriBackend};
use crate::jsonrpc::{HandlerResult, MethodHandler, RpcError};
use crate::tools::{ToolDef, registry};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

/// MCP spec revision negotiated during `initialize`. Kept inline so the
/// pinned date is easy to grep / bump.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Server identity returned in `initialize` responses.
pub const SERVER_NAME: &str = "parish-mcp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Wires a [`TauriBackend`] to an MCP-shaped JSON-RPC method handler.
///
/// Holds the curated tool registry plus a backend trait object. One server
/// instance handles the lifetime of one client connection — clone it (it's
/// `Arc`-friendly) if you need to share across tasks.
pub struct McpServer {
    tools: Vec<ToolDef>,
    backend: Arc<dyn TauriBackend>,
}

impl McpServer {
    pub fn new(backend: Arc<dyn TauriBackend>) -> Self {
        Self {
            tools: registry(),
            backend,
        }
    }

    fn initialize_response(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
            },
            "capabilities": {
                // listChanged: false — the registry is static for the
                // lifetime of the process. If a future change makes it
                // dynamic, flip this and emit `notifications/tools/list_changed`.
                "tools": {"listChanged": false}
            }
        })
    }

    fn list_tools(&self) -> Value {
        json!({
            "tools": self.tools.iter().map(|t| t.descriptor_json()).collect::<Vec<_>>(),
        })
    }

    async fn call_tool(&self, params: Value) -> HandlerResult {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing required field: `name`"))?;
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| RpcError::invalid_params(format!("unknown tool: {name}")))?;

        let (command, args) = (tool.translate)(&arguments).map_err(RpcError::invalid_params)?;
        match self.backend.invoke(&command, args).await {
            Ok(value) => Ok(tool_call_result(&value, false)),
            Err(BackendError::Unimplemented(msg)) => Err(RpcError::internal(msg)),
            // Backend rejection / transport error is reported as `isError: true`
            // inside a successful tool response so the model can read the
            // error text and self-correct, rather than aborting the call.
            Err(e) => Ok(tool_call_result(&Value::String(e.to_string()), true)),
        }
    }
}

/// Wraps a JSON value in the MCP `tools/call` result envelope.
///
/// Per the spec, `content` is a list of typed parts. We always emit a single
/// `text` part containing the JSON-encoded value; the model can parse it
/// back to JSON when needed and gets a readable representation when not.
fn tool_call_result(value: &Value, is_error: bool) -> Value {
    let text = if value.is_string() {
        value.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    };
    json!({
        "content": [{"type": "text", "text": text}],
        "isError": is_error,
    })
}

#[async_trait]
impl MethodHandler for McpServer {
    async fn handle(&self, method: &str, params: Value) -> HandlerResult {
        match method {
            "initialize" => Ok(self.initialize_response()),
            // Spec-defined notification clients may send after initialize;
            // accept silently so the client doesn't see a `method not found`.
            "notifications/initialized" | "initialized" => Ok(Value::Null),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.list_tools()),
            "tools/call" => self.call_tool(params).await,
            other => Err(RpcError::method_not_found(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TauriBackend;
    use std::sync::Mutex;

    struct RecordingBackend {
        calls: Mutex<Vec<(String, Value)>>,
        response: Value,
    }

    #[async_trait]
    impl TauriBackend for RecordingBackend {
        fn name(&self) -> &'static str {
            "test"
        }
        async fn invoke(&self, command: &str, args: Value) -> Result<Value, BackendError> {
            self.calls.lock().unwrap().push((command.into(), args));
            Ok(self.response.clone())
        }
    }

    fn make_server(response: Value) -> (McpServer, Arc<RecordingBackend>) {
        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
            response,
        });
        let server = McpServer::new(backend.clone() as Arc<dyn TauriBackend>);
        (server, backend)
    }

    #[tokio::test]
    async fn initialize_returns_protocol_version_and_tools_capability() {
        let (server, _) = make_server(Value::Null);
        let result = server.handle("initialize", json!({})).await.unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(result["serverInfo"]["version"], SERVER_VERSION);
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn tools_list_includes_curated_set() {
        let (server, _) = make_server(Value::Null);
        let result = server.handle("tools/list", json!({})).await.unwrap();
        let tools = result["tools"].as_array().expect("tools array");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"tauri_invoke"));
        assert!(names.contains(&"parish_world_snapshot"));
        assert!(names.contains(&"parish_submit_input"));
    }

    #[tokio::test]
    async fn tools_call_routes_through_backend() {
        let (server, backend) = make_server(json!({"clock": 5}));
        let result = server
            .handle(
                "tools/call",
                json!({"name": "parish_world_snapshot", "arguments": {}}),
            )
            .await
            .unwrap();

        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "get_world_snapshot");

        // Result is wrapped in MCP content envelope as a JSON-encoded text part.
        let text = result["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["clock"], 5);
        assert_eq!(result["isError"], false);
    }

    #[tokio::test]
    async fn submit_input_translates_text_and_addressed_to() {
        let (server, backend) = make_server(Value::Null);
        let _ = server
            .handle(
                "tools/call",
                json!({
                    "name": "parish_submit_input",
                    "arguments": {"text": "look", "addressed_to": ["Mary"]}
                }),
            )
            .await
            .unwrap();
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls[0].0, "submit_input");
        assert_eq!(calls[0].1["text"], "look");
        assert_eq!(calls[0].1["addressed_to"], json!(["Mary"]));
    }

    #[tokio::test]
    async fn tool_translation_error_does_not_invoke_backend() {
        let (server, backend) = make_server(Value::Null);
        let err = server
            .handle(
                "tools/call",
                json!({"name": "parish_submit_input", "arguments": {"addressed_to": ["Mary"]}}),
            )
            .await
            .unwrap_err();

        assert_eq!(err.code, -32602);
        assert!(err.message.contains("text"));
        assert!(backend.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_tool_is_invalid_params() {
        let (server, _) = make_server(Value::Null);
        let err = server
            .handle("tools/call", json!({"name": "no_such_tool"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn unknown_method_is_method_not_found() {
        let (server, _) = make_server(Value::Null);
        let err = server.handle("frobnicate", json!({})).await.unwrap_err();
        assert_eq!(err.code, -32601);
    }

    #[tokio::test]
    async fn backend_rejection_is_surfaced_as_is_error() {
        struct Reject;
        #[async_trait]
        impl TauriBackend for Reject {
            fn name(&self) -> &'static str {
                "reject"
            }
            async fn invoke(&self, _: &str, _: Value) -> Result<Value, BackendError> {
                Err(BackendError::Rejected("nope".into()))
            }
        }
        let server = McpServer::new(Arc::new(Reject));
        let result = server
            .handle(
                "tools/call",
                json!({"name": "parish_world_snapshot", "arguments": {}}),
            )
            .await
            .unwrap();
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("nope")
        );
    }

    #[tokio::test]
    async fn backend_transport_error_is_successful_tool_error() {
        struct TransportFail;
        #[async_trait]
        impl TauriBackend for TransportFail {
            fn name(&self) -> &'static str {
                "transport-fail"
            }
            async fn invoke(&self, _: &str, _: Value) -> Result<Value, BackendError> {
                Err(BackendError::Transport("connection refused".into()))
            }
        }

        let server = McpServer::new(Arc::new(TransportFail));
        let result = server
            .handle(
                "tools/call",
                json!({"name": "parish_world_snapshot", "arguments": {}}),
            )
            .await
            .unwrap();

        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("transport error"));
        assert!(text.contains("connection refused"));
    }
}
