//! Parish MCP — a Model Context Protocol server that lets an LLM client
//! drive a running Parish (or any compatible Tauri) instance.
//!
//! The crate is intentionally split into three layers so the parts can be
//! tested independently and a future generic-Tauri backend (WebDriver /
//! `tauri-driver`) can plug in without touching the protocol code:
//!
//! 1. [`jsonrpc`] — line-delimited JSON-RPC 2.0 over an `AsyncRead`/`AsyncWrite`.
//! 2. [`mcp`] — the MCP handshake (`initialize`, `tools/list`, `tools/call`)
//!    plus the tool registry that translates a `tools/call` into a backend
//!    invocation.
//! 3. [`backend`] — the [`backend::TauriBackend`] trait and its
//!    implementations.  [`backend::ParishHttpBackend`] wraps the existing
//!    `parish-server` HTTP API; the same routes are mode-parity with the
//!    Tauri IPC commands, so driving the server is equivalent to driving
//!    the desktop app for everything that participates in the IPC parity
//!    sensor.
//!
//! See `README.md` for transport rationale (stdio) and client-config snippets.

pub mod backend;
pub mod jsonrpc;
pub mod mcp;
pub mod tools;
