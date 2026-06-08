//! HTTP game client + wire mirror. The harness's only contact with the game
//! under test.

pub mod backend;
pub mod wire;

pub use backend::{GameClient, HttpGameClient};
pub use wire::{CommandResponse, Outcome};

use std::time::Duration;

use crate::error::Result;

/// Poll `health` then `engine_state` until both answer or the deadline passes.
/// Mirrors the readiness gate in `parish-mcp-backend.sh` (which waits on
/// `/api/health` for up to 60s before declaring the bridge live).
pub async fn wait_ready(client: &dyn GameClient, timeout: Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    let mut last_err = None;
    loop {
        match client.health().await {
            Ok(()) => {
                // Health is up; confirm the engine answers too.
                match client.engine_state().await {
                    Ok(_) => return Ok(()),
                    Err(e) => last_err = Some(e),
                }
            }
            Err(e) => last_err = Some(e),
        }
        if std::time::Instant::now() >= deadline {
            return Err(last_err.unwrap_or_else(|| {
                crate::error::HarnessError::Config("backend never became ready".into())
            }));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
