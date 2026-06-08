//! Transport / dispatch layer for line-delimited JSON-RPC 2.0.
//!
//! Owns the [`MethodHandler`] trait, the [`ResponseWriter`], the
//! `write_response` helper, and the [`serve`] read loop. Split out of the
//! monolithic `jsonrpc` module (#1200) so the framing/dispatch logic is
//! separable from the protocol value types in [`super::message`].
//!
//! Concurrency model: requests are processed serially in the order they
//! arrive. JSON-RPC clients correlate by `id` so out-of-order responses
//! are spec-legal, but Parish IPC handlers are stateful (a `submit_input`
//! that mutates the world followed by a `world_snapshot` must observe
//! the mutation) and the MCP clients we target — Claude Code and Claude
//! Desktop — issue requests sequentially. Serial processing keeps the
//! semantics predictable and avoids losing responses when stdin closes
//! while handlers are still in flight.

use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use super::message::{Request, Response, RpcError};

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
        let serve_task = tokio::spawn(serve(server_in, writer, handler));

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

    #[tokio::test]
    async fn serve_echoes_string_and_explicit_null_ids() {
        struct OkHandler;
        #[async_trait::async_trait]
        impl MethodHandler for OkHandler {
            async fn handle(&self, _: &str, _: Value) -> HandlerResult {
                Ok(serde_json::json!({"ok": true}))
            }
        }

        let (mut client_in, server_in) = tokio::io::duplex(4096);
        let (server_out, mut client_out) = tokio::io::duplex(4096);

        let writer = ResponseWriter::new(server_out);
        let serve_task = tokio::spawn(serve(server_in, writer, Arc::new(OkHandler)));

        client_in
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":\"abc\"}\n\
                  {\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":null}\n",
            )
            .await
            .unwrap();
        client_in.shutdown().await.unwrap();

        let mut lines = BufReader::new(&mut client_out).lines();
        let string_id_line = lines.next_line().await.unwrap().unwrap();
        let null_id_line = lines.next_line().await.unwrap().unwrap();

        let string_id_response: Value = serde_json::from_str(&string_id_line).unwrap();
        let null_id_response: Value = serde_json::from_str(&null_id_line).unwrap();
        assert_eq!(string_id_response["id"], "abc");
        assert_eq!(string_id_response["result"]["ok"], true);
        assert_eq!(null_id_response["id"], Value::Null);
        assert_eq!(null_id_response["result"]["ok"], true);

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
        let serve_task = tokio::spawn(serve(server_in, writer, Arc::new(Sink)));

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
        let serve_task = tokio::spawn(serve(server_in, writer, Arc::new(Sink)));

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
