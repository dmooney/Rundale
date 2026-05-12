//! Minimal line-delimited JSON-RPC 2.0 over async streams.
//!
//! MCP supports several transports; the most common (and the only one
//! Claude Desktop / Claude Code currently configure) is **stdio with
//! newline-delimited JSON messages**. This module owns the wire layer
//! and is deliberately ignorant of MCP method names — that lives in
//! [`crate::mcp`].
//!
//! Errors are surfaced via JSON-RPC error objects rather than panics so a
//! malformed request from the client never tears down the server loop.
//!
//! Concurrency model: requests are processed serially in the order they
//! arrive. JSON-RPC clients correlate by `id` so out-of-order responses
//! are spec-legal, but Parish IPC handlers are stateful (a `submit_input`
//! that mutates the world followed by a `world_snapshot` must observe
//! the mutation) and the MCP clients we target — Claude Code and Claude
//! Desktop — issue requests sequentially. Serial processing keeps the
//! semantics predictable and avoids losing responses when stdin closes
//! while handlers are still in flight.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// JSON-RPC 2.0 request / notification.
///
/// `id` is `None` for notifications. We accept any JSON value for `id`
/// (string, number, null) per the spec and echo it back verbatim.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[allow(dead_code)] // verified during deserialisation; kept for completeness
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 response. Either `result` or `error` is set, never both.
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn parse_error(detail: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: detail.into(),
            data: None,
        }
    }
    pub fn invalid_request(detail: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: detail.into(),
            data: None,
        }
    }
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
            data: None,
        }
    }
    pub fn invalid_params(detail: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: detail.into(),
            data: None,
        }
    }
    pub fn internal(detail: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: detail.into(),
            data: None,
        }
    }
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }
    pub fn err(id: Value, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(error),
            id,
        }
    }
}

/// A handler is given the parsed `params` and must return either a
/// JSON-encodable result or a typed [`RpcError`].
pub type HandlerResult = Result<Value, RpcError>;

#[async_trait::async_trait]
pub trait MethodHandler: Send + Sync {
    async fn handle(&self, method: &str, params: Value) -> HandlerResult;
}

/// Writer wrapper that serialises responses one-per-line. Wrapped in a
/// mutex so concurrent handler tasks never interleave bytes on stdout.
pub struct ResponseWriter<W: AsyncWrite + Unpin + Send> {
    inner: Arc<Mutex<W>>,
}

impl<W: AsyncWrite + Unpin + Send + 'static> ResponseWriter<W> {
    pub fn new(w: W) -> Self {
        Self {
            inner: Arc::new(Mutex::new(w)),
        }
    }

    pub fn handle(&self) -> Arc<Mutex<W>> {
        Arc::clone(&self.inner)
    }
}

/// Writes a single JSON-RPC response, terminated by `\n`.
///
/// Holding the lock around the *whole* write (serialise + write_all + flush)
/// is what keeps concurrent handler responses from interleaving bytes.
pub async fn write_response<W: AsyncWrite + Unpin + Send>(
    writer: &Arc<Mutex<W>>,
    response: &Response,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(response).map_err(std::io::Error::other)?;
    let mut guard = writer.lock().await;
    guard.write_all(&body).await?;
    guard.write_all(b"\n").await?;
    guard.flush().await?;
    Ok(())
}

/// Drives the read loop: reads newline-delimited JSON-RPC messages from
/// `reader`, dispatches each one onto the tokio runtime, and writes the
/// response to `writer`. Returns when the reader hits EOF or a fatal I/O
/// error.
pub async fn serve<R, W, H>(
    reader: R,
    writer: ResponseWriter<W>,
    handler: Arc<H>,
) -> std::io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
    H: MethodHandler + 'static,
{
    let mut lines = BufReader::new(reader).lines();
    let writer_handle = writer.handle();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        dispatch_line(line, Arc::clone(&handler), Arc::clone(&writer_handle)).await;
    }
    Ok(())
}

async fn dispatch_line<W, H>(line: String, handler: Arc<H>, writer: Arc<Mutex<W>>)
where
    W: AsyncWrite + Unpin + Send,
    H: MethodHandler,
{
    // Parse first; on parse failure we cannot recover the `id`, so we reply
    // with `id: null` per the JSON-RPC spec.
    let request: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::err(Value::Null, RpcError::parse_error(e.to_string()));
            let _ = write_response(&writer, &resp).await;
            return;
        }
    };

    let id = request.id.clone();
    let result = handler.handle(&request.method, request.params).await;

    // Notifications (no `id`) get no response per the spec.
    let Some(id) = id else { return };

    let response = match result {
        Ok(value) => Response::ok(id, value),
        Err(err) => Response::err(id, err),
    };
    let _ = write_response(&writer, &response).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_serialises_only_result_when_ok() {
        let resp = Response::ok(Value::from(7), serde_json::json!({"hello": "world"}));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"result\""));
        assert!(!s.contains("\"error\""));
        assert!(s.contains("\"id\":7"));
    }

    #[test]
    fn response_serialises_only_error_when_err() {
        let resp = Response::err(Value::from("abc"), RpcError::method_not_found("foo"));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"error\""));
        assert!(!s.contains("\"result\""));
        assert!(s.contains("method not found: foo"));
    }

    /// Smoke test for the full read/dispatch/write loop using in-memory pipes.
    #[tokio::test]
    async fn serve_round_trips_a_request() {
        struct Echo;
        #[async_trait::async_trait]
        impl MethodHandler for Echo {
            async fn handle(&self, method: &str, params: Value) -> HandlerResult {
                Ok(serde_json::json!({"method": method, "params": params}))
            }
        }

        let (mut client_in, server_in) = tokio::io::duplex(4096);
        let (server_out, mut client_out) = tokio::io::duplex(4096);

        let handler = Arc::new(Echo);
        let writer = ResponseWriter::new(server_out);
        let serve_task = tokio::spawn(super::serve(server_in, writer, handler));

        let req = b"{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":{\"a\":1},\"id\":42}\n";
        client_in.write_all(req).await.unwrap();
        client_in.shutdown().await.unwrap();

        let mut buf = String::new();
        BufReader::new(&mut client_out)
            .read_line(&mut buf)
            .await
            .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&buf).unwrap();
        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["result"]["method"], "ping");
        assert_eq!(parsed["result"]["params"]["a"], 1);

        // Drain the loop.
        let _ = serve_task.await;
    }

    /// Notifications (no `id`) MUST NOT receive a response.
    #[tokio::test]
    async fn serve_does_not_respond_to_notifications() {
        struct Sink;
        #[async_trait::async_trait]
        impl MethodHandler for Sink {
            async fn handle(&self, _: &str, _: Value) -> HandlerResult {
                Ok(Value::Null)
            }
        }

        let (mut client_in, server_in) = tokio::io::duplex(4096);
        let (server_out, mut client_out) = tokio::io::duplex(4096);
        let writer = ResponseWriter::new(server_out);
        let serve_task = tokio::spawn(super::serve(server_in, writer, Arc::new(Sink)));

        client_in
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"shouldNotReply\"}\n")
            .await
            .unwrap();
        client_in.shutdown().await.unwrap();

        let _ = serve_task.await;

        // Reader is closed and no bytes were written.
        let mut buf = Vec::new();
        let _ = tokio::io::AsyncReadExt::read_to_end(&mut client_out, &mut buf).await;
        assert!(
            buf.is_empty(),
            "got unexpected bytes: {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    /// Malformed JSON yields a parse-error response with `id: null`.
    #[tokio::test]
    async fn serve_returns_parse_error_for_invalid_json() {
        struct Sink;
        #[async_trait::async_trait]
        impl MethodHandler for Sink {
            async fn handle(&self, _: &str, _: Value) -> HandlerResult {
                Ok(Value::Null)
            }
        }

        let (mut client_in, server_in) = tokio::io::duplex(4096);
        let (server_out, mut client_out) = tokio::io::duplex(4096);
        let writer = ResponseWriter::new(server_out);
        let serve_task = tokio::spawn(super::serve(server_in, writer, Arc::new(Sink)));

        client_in.write_all(b"{not json}\n").await.unwrap();
        client_in.shutdown().await.unwrap();

        let mut buf = String::new();
        BufReader::new(&mut client_out)
            .read_line(&mut buf)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&buf).unwrap();
        assert_eq!(parsed["id"], Value::Null);
        assert_eq!(parsed["error"]["code"], -32700);

        let _ = serve_task.await;
    }
}
