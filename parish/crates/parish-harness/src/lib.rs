//! `parish-harness` — game quality-control harness.
//!
//! Runs automated multi-turn playtests where an LLM plays the player and an LLM
//! judges the transcript, then scores each run (deterministic gate + quality
//! axes), records discrete findings, and persists everything to SQLite +
//! on-disk artifacts. The harness drives a *running* Parish backend over HTTP;
//! it never links the game runtime.
//!
//! See [`docs/design/game-quality-harness.md`](../../../docs/design/game-quality-harness.md)
//! for the design of record and the full architecture plan in
//! `docs/design/game-quality-harness-architecture.md`.

pub mod actor;
pub mod client;
pub mod config;
pub mod error;
pub mod frame;
pub mod git;
pub mod persist;
pub mod run;
pub mod score;

pub use error::{HarnessError, Result};
