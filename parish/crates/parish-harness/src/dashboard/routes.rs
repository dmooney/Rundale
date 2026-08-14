//! Axum route handlers for the harness dashboard API.

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;
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

// ── GET /api/runs/{id}/turns/{idx}/transcript ─────────────────────────────────

/// Serves a turn's inference log (`turns/NNN/llm.json`) as JSON. 404 when the
/// run, or that turn's log, does not exist — runs captured without per-turn logs
/// simply have no file here, and the dashboard renders such turns non-clickable.
pub async fn get_turn_transcript(
    State(state): State<AppState>,
    Path((run_id, turn_idx)): Path<(i64, u32)>,
) -> Response {
    let db = match state.open_db() {
        Ok(db) => db,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let artifact_dir = match db.run_artifact_dir(run_id) {
        Ok(Some(dir)) => PathBuf::from(dir),
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "run not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let log_path = artifact_dir.join(format!("turns/{turn_idx:03}/llm.json"));
    match std::fs::read(&log_path) {
        Ok(bytes) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "application/json; charset=utf-8",
            )],
            Body::from(bytes),
        )
            .into_response(),
        Err(_) => error_response(StatusCode::NOT_FOUND, "turn inference log not found"),
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

// ── GET /api/timeline?branch= ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    pub branch: Option<String>,
}

pub async fn get_timeline(
    State(state): State<AppState>,
    Query(params): Query<TimelineParams>,
) -> Response {
    let db = match state.open_db() {
        Ok(db) => db,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    match db.timeline(params.branch.as_deref()) {
        Ok(points) => Json(points).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── GET /api/compare?a=&b= ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CompareParams {
    pub a: i64,
    pub b: i64,
}

pub async fn get_compare(
    State(state): State<AppState>,
    Query(params): Query<CompareParams>,
) -> Response {
    let db = match state.open_db() {
        Ok(db) => db,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    match db.compare(params.a, params.b) {
        Ok(Some(cmp)) => Json(cmp).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "one or both run ids not found"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── GET /api/cost ─────────────────────────────────────────────────────────────

pub async fn get_cost(State(state): State<AppState>) -> Response {
    let db = match state.open_db() {
        Ok(db) => db,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    match db.cost_summary() {
        Ok(summary) => Json(summary).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── GET / ─────────────────────────────────────────────────────────────────────

/// Serves the embedded single-file dashboard UI.
///
/// The `__COMMIT_BASE__` token is replaced with the GitHub repo base derived
/// from the local `origin` remote so the UI can link each run's git sha to its
/// commit; it is replaced with an empty string when there is no GitHub origin
/// (the UI then renders shas unlinked).
pub async fn index_html() -> impl IntoResponse {
    const HTML: &str = include_str!("../../dashboard-ui/index.html");
    let body = HTML.replace(
        "__COMMIT_BASE__",
        github_commit_base().as_deref().unwrap_or(""),
    );
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
}

/// The GitHub repo base URL (`https://github.com/<owner>/<repo>`) for the
/// `origin` remote, computed once. `None` when there is no git origin or it is
/// not a GitHub remote.
fn github_commit_base() -> Option<String> {
    use std::sync::OnceLock;
    static BASE: OnceLock<Option<String>> = OnceLock::new();
    BASE.get_or_init(|| {
        let out = std::process::Command::new("git")
            .args(["remote", "get-url", "origin"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_github_base(&String::from_utf8_lossy(&out.stdout))
    })
    .clone()
}

/// Parses a git remote URL into a `https://github.com/<owner>/<repo>` base.
///
/// Handles the SSH (`git@github.com:owner/repo.git`), HTTPS
/// (`https://github.com/owner/repo(.git)`), and `ssh://` forms; returns `None`
/// for non-GitHub remotes or anything that is not exactly `owner/repo`. Pure for
/// unit testing.
fn parse_github_base(remote: &str) -> Option<String> {
    let s = remote.trim();
    let slug = s
        .strip_prefix("git@github.com:")
        .or_else(|| s.strip_prefix("https://github.com/"))
        .or_else(|| s.strip_prefix("ssh://git@github.com/"))?;
    let slug = slug
        .strip_suffix(".git")
        .unwrap_or(slug)
        .trim_end_matches('/');
    if slug.split('/').filter(|p| !p.is_empty()).count() != 2 {
        return None;
    }
    Some(format!("https://github.com/{slug}"))
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
    async fn get_api_timeline_returns_200_json() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("tl.db");
        Db::open(&db_path).unwrap();

        let state = make_state(db_path);
        let app = build_router(state);

        let req = Request::builder()
            .uri("/api/timeline")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn get_api_compare_unknown_returns_404() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("cmp.db");
        Db::open(&db_path).unwrap();

        let state = make_state(db_path);
        let app = build_router(state);

        let req = Request::builder()
            .uri("/api/compare?a=1&b=2")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_api_cost_returns_200_json_with_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("cost.db");
        Db::open(&db_path).unwrap();

        let state = make_state(db_path);
        let app = build_router(state);

        let req = Request::builder()
            .uri("/api/cost")
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
        assert!(
            json.get("total_runs").is_some(),
            "expected total_runs field"
        );
        assert!(
            json.get("total_cost_usd").is_some(),
            "expected total_cost_usd field"
        );
        assert!(
            json.get("total_player_tokens").is_some(),
            "expected total_player_tokens field"
        );
        assert!(
            json.get("total_judge_tokens").is_some(),
            "expected total_judge_tokens field"
        );
        assert_eq!(json["total_runs"], 0, "empty db should have 0 runs");
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

    #[tokio::test]
    async fn get_turn_transcript_serves_log_then_404() {
        use crate::ingest::load_and_ingest;
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let uuid = "00000000-0000-0000-0000-0000000000a7";
        let tdir = artifacts.join("runs").join(uuid).join("turns").join("000");
        std::fs::create_dir_all(&tdir).unwrap();
        std::fs::write(tdir.join("frame.png"), b"frame-bytes").unwrap();
        std::fs::write(tdir.join("lines.json"), b"[]").unwrap();
        std::fs::write(
            tdir.join("llm.json"),
            br#"{"turn_index":0,"player_input":"hi","exchanges":[],"inferences":[]}"#,
        )
        .unwrap();
        let payload_json = format!(
            r#"{{
              "config": {{ "player": {{ "mode": "subagent" }}, "judge": {{ "mode": "subagent" }} }},
              "git": {{ "sha": "abc", "branch": "main", "dirty": false, "pr_number": null }},
              "rubric_sha256": "r", "uuid": "{uuid}", "status": "completed", "quality_score": 70.0,
              "cost": {{ "cost_usd": 0.0, "player_tokens": 0, "judge_tokens": 0 }},
              "turns": [ {{ "turn_index": 0, "player_input": "hi", "frame_path": "turns/000/frame.png", "lines_path": "turns/000/lines.json", "llm_transcript_path": "turns/000/llm.json" }} ],
              "axes": [], "findings": []
            }}"#
        );
        let ppath = tmp.path().join("p.json");
        std::fs::write(&ppath, &payload_json).unwrap();
        let db_path = tmp.path().join("t.db");
        let db = Db::open(&db_path).unwrap();
        let run_id = load_and_ingest(&db, &ppath, &artifacts).unwrap();

        let app = build_router(make_state(db_path));
        // Present → 200 JSON.
        let ok = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{run_id}/turns/0/transcript"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
        let ct = ok
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("application/json"), "got: {ct}");
        // Absent turn → 404.
        let miss = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{run_id}/turns/9/transcript"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(miss.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn dashboard_html_has_lightbox() {
        let html = include_str!("../../dashboard-ui/index.html");
        // Lightbox overlay element is present.
        assert!(
            html.contains("id=\"zoom-overlay\""),
            "dashboard must contain the zoom-overlay element"
        );
        assert!(
            html.contains("id=\"zoom-img\""),
            "dashboard must contain the zoom-img element"
        );
        assert!(
            html.contains("id=\"zoom-close\""),
            "dashboard must contain the zoom-close button"
        );
        // JS functions for opening/closing the lightbox.
        assert!(
            html.contains("function openZoom"),
            "dashboard must define openZoom()"
        );
        assert!(
            html.contains("function closeZoom"),
            "dashboard must define closeZoom()"
        );
        // Images carry the data-zoom-src attribute for lightbox hookup.
        assert!(
            html.contains("data-zoom-src"),
            "dashboard must mark thumbnails with data-zoom-src for lightbox"
        );
        // Escape key closes the lightbox.
        assert!(
            html.contains("Escape") && html.contains("closeZoom"),
            "dashboard must close lightbox on Escape key"
        );
        // No external dependency introduced.
        let lower = html.to_lowercase();
        assert!(
            !lower.contains("glightbox")
                && !lower.contains("fancybox")
                && !lower.contains("lightbox.js"),
            "dashboard lightbox must be pure vanilla JS with no external dependency"
        );
    }

    #[test]
    fn dashboard_html_has_axes_radar() {
        let html = include_str!("../../dashboard-ui/index.html");
        // Radar renderer is present and wired into the detail view (C1, C3).
        assert!(
            html.contains("buildAxesRadar"),
            "dashboard must define the buildAxesRadar renderer"
        );
        assert!(
            html.contains("axes-radar"),
            "dashboard must include the axes-radar container"
        );
        assert!(
            html.contains("<polygon"),
            "radar must draw an SVG data polygon"
        );
        // The radar is the sole axis visualization — the bar chart was removed.
        assert!(
            !html.contains("axes-chart") && !html.contains("axis-bar"),
            "dashboard must not render the axis bar chart (radar replaces it)"
        );
        // No external/CDN dependency — the dashboard stays offline (C2).
        let lower = html.to_lowercase();
        assert!(
            !lower.contains("script src") && !lower.contains("cdn") && !lower.contains("chart.js"),
            "dashboard radar must be pure inline SVG with no external script dependency"
        );
    }

    #[test]
    fn dashboard_html_links_commit_sha() {
        let html = include_str!("../../dashboard-ui/index.html");
        // The sha-link helper + injected commit base are present so a run's git
        // sha can render as a GitHub commit link.
        assert!(html.contains("shaCell"), "dashboard must define shaCell");
        assert!(
            html.contains("window.COMMIT_BASE") && html.contains("/commit/"),
            "dashboard must build a /commit/ link from COMMIT_BASE"
        );
        assert!(
            html.contains("__COMMIT_BASE__"),
            "the raw HTML must carry the __COMMIT_BASE__ token for serve-time injection"
        );
    }

    #[test]
    fn dashboard_html_renders_per_run_cost_models_and_timing() {
        let html = include_str!("../../dashboard-ui/index.html");
        for contract in [
            "fmtUsd",
            "fmtDuration",
            "engineRouteSummary",
            "['player', config?.player]",
            "['judge', config?.judge]",
            "Cost:",
            "Tokens:",
            "Response time:",
            "Engine models:",
            "Actors:",
            "Feature flags:",
        ] {
            assert!(
                html.contains(contract),
                "dashboard must render per-run telemetry contract {contract:?}"
            );
        }
        assert!(
            html.contains("cost=${fmtUsd(s.cost_usd)}")
                && html.contains("avg response=${fmtDuration(s.avg_response_ms)}"),
            "A/B summaries must retain per-run cost and response-time context"
        );
    }

    #[test]
    fn dashboard_html_has_run_routing() {
        let html = include_str!("../../dashboard-ui/index.html");
        // Runs are deep-linkable pages via hash routing.
        assert!(
            html.contains("function router"),
            "must define a hash router"
        );
        assert!(
            html.contains("hashchange"),
            "must listen for hashchange to navigate run pages"
        );
        assert!(
            html.contains("'run/'") || html.contains("#run/"),
            "must use #run/<id> URLs for run pages"
        );
        // Per-turn inference log viewer: clickable turns open a log panel and
        // are deep-linkable as #run/<id>/turn/<idx>.
        assert!(
            html.contains("showTurnLog") && html.contains("/turn/"),
            "must support clicking a turn to view its inference log via #run/<id>/turn/<idx>"
        );
        assert!(
            html.contains("/transcript"),
            "must fetch the per-turn transcript endpoint"
        );
    }

    #[test]
    fn turn_log_is_appended_at_page_bottom_without_inner_scroll() {
        let html = include_str!("../../dashboard-ui/index.html");
        // The log container exists exactly once and sits after the footer (page
        // bottom), not inside the per-run detail render.
        assert_eq!(
            html.matches("id=\"turn-log\"").count(),
            1,
            "turn-log container must be declared exactly once"
        );
        let footer = html.find("</footer>").expect("footer present");
        let log = html.find("id=\"turn-log\"").expect("turn-log present");
        assert!(
            log > footer,
            "the turn-log block must come after the footer (page bottom)"
        );
        // No nested scrollbox — the raw prompt/response grows the page.
        let pre_rule = html
            .lines()
            .find(|l| l.contains(".tl-infer pre"))
            .expect(".tl-infer pre rule present");
        assert!(
            !pre_rule.contains("max-height") && !pre_rule.contains("overflow"),
            "raw prompt/response must not be a fixed-height scrollbox: {pre_rule}"
        );
    }

    #[test]
    fn parse_github_base_handles_remote_forms() {
        let want = Some("https://github.com/dmooney/Rundale".to_string());
        assert_eq!(
            parse_github_base("git@github.com:dmooney/Rundale.git\n"),
            want
        );
        assert_eq!(
            parse_github_base("https://github.com/dmooney/Rundale.git"),
            want
        );
        assert_eq!(
            parse_github_base("https://github.com/dmooney/Rundale"),
            want
        );
        assert_eq!(
            parse_github_base("ssh://git@github.com/dmooney/Rundale.git"),
            want
        );
        // Non-GitHub or malformed remotes yield no base (sha stays unlinked).
        assert_eq!(parse_github_base("git@gitlab.com:foo/bar.git"), None);
        assert_eq!(parse_github_base("https://github.com/onlyowner"), None);
        assert_eq!(parse_github_base(""), None);
    }

    #[tokio::test]
    async fn index_html_replaces_commit_base_token() {
        // The served HTML must not leak the raw placeholder — it is replaced with
        // the repo base (this repo has a GitHub origin) or an empty string.
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("idx.db");
        Db::open(&db_path).unwrap();
        let app = build_router(make_state(db_path));
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(
            !html.contains("__COMMIT_BASE__"),
            "served HTML must replace the __COMMIT_BASE__ token"
        );
    }
}
