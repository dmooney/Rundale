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
//! Structure (#1200 decomposition):
//! - [`message`] — protocol value types (`Request`, `Response`, `RpcError`).
//! - [`dispatch`] — framing/transport: `MethodHandler`, `ResponseWriter`,
//!   `write_response`, and the [`serve`] read loop.
//!
//! Both submodules are re-exported flat here so the public paths
//! (`jsonrpc::Request`, `jsonrpc::serve`, …) are unchanged.

pub mod dispatch;
pub mod message;

pub use dispatch::{HandlerResult, MethodHandler, ResponseWriter, serve, write_response};
pub use message::{Request, Response, RpcError};
