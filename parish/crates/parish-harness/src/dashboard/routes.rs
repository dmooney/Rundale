//! Axum route handlers for the harness dashboard API.

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use tokio::sync::broadcast;

use crate::dashboard::sse::TurnEventMsg;
use crate::error::HarnessError;
use crate::persist::Db;

/// Shared state available to every handler.
#[derive(Clone)]
pub struct AppState {
    /// Path to `harness.db` — a fresh `Db` is opened per handler since
    /// `rusqlite::Connection` is not `Sync`.
    pub db_path: PathBuf,
    /// Root dir where per-run artifact subdirectories live.
    pub artifact_root: PathBuf,
    /// Broadcast sender for in-progress turn events (Phase 3 publisher wired
    /// here; Phase 2 the channel is created but has no writers).
    pub live: Arc<broadcast::Sender<TurnEventMsg>>,
}

impl AppState {
    fn open_db(&self) -> Result<Db, HarnessError> {
        Db::open(&self.db_path)
    }
}

// ── GET /api/runs ──────────────────────────────────────────────────────────────

pub async fn list_runs(State(state): State<AppState>) -> Response {
    let db = match state.open_db() {
        Ok(db) => db,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    match db.list_runs(200) {
        Ok(runs) => Json(runs).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── GET /api/runs/{id} ─────────────────────────────────────────────────────────

pub async fn get_run(State(state): State<AppState>, Path(run_id): Path<i64>) -> Response {
    let db = match state.open_db() {
        Ok(db) => db,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    match db.run_detail(run_id) {
        Ok(Some(detail)) => Json(detail).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "run not found"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── GET /api/runs/{id}/turns/{idx}/frame.png ──────────────────────────────────

pub async fn get_frame(
    State(state): State<AppState>,
    Path((run_id, turn_idx)): Path<(i64, u32)>,
) -> Response {
    // Look up the artifact_dir for this run.
    let db = match state.open_db() {
        Ok(db) => db,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // Resolve the run's artifact dir.
    let artifact_dir = match db.run_artifact_dir(run_id) {
        Ok(Some(dir)) => PathBuf::from(dir),
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "run not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let frame_path = artifact_dir.join(format!("turns/{turn_idx:03}/frame.png"));
    match std::fs::read(&frame_path) {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "image/png")],
            Body::from(bytes),
        )
            .into_response(),
        Err(_) => error_response(StatusCode::NOT_FOUND, "frame not found"),
    }
}

// ── GET /api/runs/{id}/stream (SSE) ───────────────────────────────────────────

pub async fn stream_run(
    State(state): State<AppState>,
    Path(run_id): Path<i64>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.live.subscribe();
    // Convert the broadcast receiver into a stream by polling in `unfold`.
    let stream = futures_util::stream::unfold(rx, move |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(msg) if msg.run_id == run_id => {
                    return Some((Ok(msg.to_sse_event()), rx));
                }
                Ok(_) => continue, // Different run, skip.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── GET / ─────────────────────────────────────────────────────────────────────

/// Serves the embedded single-file dashboard UI.
pub async fn index_html() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../../dashboard-ui/index.html"),
    )
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn error_response(status: StatusCode, msg: &str) -> Response {
    (
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        format!(r#"{{"error":{}}}"#, serde_json::json!(msg)),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::server::build_router;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    fn make_state(db_path: PathBuf) -> AppState {
        AppState {
            db_path,
            artifact_root: PathBuf::from("/tmp"),
            live: Arc::new(crate::dashboard::sse::channel()),
        }
    }

    #[tokio::test]
    async fn get_api_runs_returns_200_json() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        // Ensure schema is created.
        Db::open(&db_path).unwrap();

        let state = make_state(db_path);
        let app = build_router(state);

        let req = Request::builder()
            .uri("/api/runs")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("application/json"), "expected JSON, got: {ct}");

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.is_array(), "expected a JSON array");
    }

    #[tokio::test]
    async fn get_api_run_unknown_returns_404() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test2.db");
        Db::open(&db_path).unwrap();

        let state = make_state(db_path);
        let app = build_router(state);

        let req = Request::builder()
            .uri("/api/runs/9999")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_index_returns_200_html() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test3.db");
        Db::open(&db_path).unwrap();

        let state = make_state(db_path);
        let app = build_router(state);

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("text/html"), "expected HTML, got: {ct}");
    }
}
