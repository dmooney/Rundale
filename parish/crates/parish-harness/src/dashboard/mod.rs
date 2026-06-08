//! Dashboard HTTP service — read API + static UI.
//!
//! Serves the `parish-harness serve` subcommand. Opens a fresh `Db` per
//! request (rusqlite `Connection` is not `Sync`) and fans out live `TurnEvent`s
//! via a `tokio::broadcast` channel for SSE.

pub mod routes;
pub mod server;
pub mod sse;

pub use server::serve;
