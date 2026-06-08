//! Dashboard HTTP server — axum + SSE.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::get;

use crate::dashboard::routes::{
    AppState, get_compare, get_frame, get_run, get_timeline, index_html, list_runs, stream_run,
};
use crate::dashboard::sse;
use crate::error::Result;

/// Start the dashboard HTTP server. Binds to `0.0.0.0:{port}` and serves
/// until interrupted.
pub async fn serve(db_path: PathBuf, artifact_root: PathBuf, port: u16) -> Result<()> {
    let live = Arc::new(sse::channel());
    let state = AppState {
        db_path,
        artifact_root,
        live,
    };

    let app = build_router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(port, "dashboard listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| crate::error::HarnessError::io(addr.to_string(), e))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| crate::error::HarnessError::io(addr.to_string(), e))?;

    Ok(())
}

/// Build the axum router (extracted for testing with `oneshot`).
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_html))
        .route("/api/runs", get(list_runs))
        .route("/api/runs/{id}", get(get_run))
        .route("/api/runs/{id}/turns/{idx}/frame.png", get(get_frame))
        .route("/api/runs/{id}/stream", get(stream_run))
        .route("/api/timeline", get(get_timeline))
        .route("/api/compare", get(get_compare))
        .with_state(state)
}
