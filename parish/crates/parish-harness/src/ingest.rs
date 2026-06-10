//! Ingest an externally-produced run into `harness.db`.
//!
//! The `parish-harness` **binary** produces runs via its own `run` loop. The
//! quality-harness **skill** produces runs a different way: an agent drives the
//! live game over the parish MCP, judges the transcript by hand, and files bugs.
//! Those skill runs never touched the DB, so they never appeared on the
//! dashboard. This module is the bridge: it deserializes a complete skill-run
//! record (`IngestPayload`), validates its per-turn frame artifacts (rule #14 —
//! never accept a blank frame), maps it onto the domain types, and replays it
//! through the *same* sink writers the live pipeline uses
//! ([`Db::ingest_complete_run`]). The result is indistinguishable from a binary
//! run to the dashboard read paths.

use std::path::Path;

use serde::Deserialize;

use crate::config::RunConfig;
use crate::error::{HarnessError, Result};
use crate::git::GitProvenance;
use crate::persist::{Db, IngestRecord, TurnRecord};
use crate::score::{Axis, AxisScore, Finding, GateReason, GateTrip, Severity, weighted_quality};

/// The complete skill-run record handed to `parish-harness ingest`.
#[derive(Debug, Deserialize)]
pub struct IngestPayload {
    /// Run config; its `label` is forced to `skill:quality-harness` on ingest so
    /// every skill run shares one `configs` row (by content hash).
    pub config: RunConfig,
    pub git: GitProvenance,
    /// The pinned rubric hash the skill scored against (pass the binary's pinned
    /// rubric sha so timeline / A-B treat skill and binary runs as comparable).
    pub rubric_sha256: String,
    /// Stable run directory name under `<artifacts>/runs/`. Must match the
    /// directory the skill wrote its `turns/NNN/frame.png` into.
    pub uuid: String,
    /// Present iff the run was gated (a hard fail). When set, the run is stored
    /// gated with a NULL quality score; `quality_score`/`axes` are ignored.
    #[serde(default)]
    pub gate: Option<GatePayload>,
    /// Weighted-mean quality. When absent on a non-gated run it is recomputed
    /// from `axes`.
    #[serde(default)]
    pub quality_score: Option<f64>,
    #[serde(default)]
    pub cost: CostPayload,
    pub turns: Vec<TurnPayload>,
    pub axes: Vec<AxisPayload>,
    #[serde(default)]
    pub findings: Vec<FindingPayload>,
}

/// Gate trip on a hard-failed run.
#[derive(Debug, Deserialize)]
pub struct GatePayload {
    /// Snake_case gate reason (`crash`, `parser_reject`, `timeout`,
    /// `json_parse_fail`, `empty_turn_burn`, `judge_critical`).
    pub reason: String,
    #[serde(default)]
    pub turn: u32,
    #[serde(default)]
    pub detail: String,
}

/// Cost / token accounting for the run (the skill's own tally).
#[derive(Debug, Default, Deserialize)]
pub struct CostPayload {
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub player_tokens: u64,
    #[serde(default)]
    pub judge_tokens: u64,
}

/// One persisted turn. Mirrors [`TurnRecord`]; paths are relative to the run's
/// artifact dir (`turns/NNN/frame.png`).
#[derive(Debug, Deserialize)]
pub struct TurnPayload {
    pub turn_index: u32,
    pub player_input: String,
    #[serde(default = "ok")]
    pub outcome: String,
    #[serde(default = "dialogue")]
    pub kind: String,
    #[serde(default)]
    pub elapsed_ms: u64,
    #[serde(default = "empty_obj")]
    pub engine_state_json: String,
    #[serde(default)]
    pub location_id: Option<u32>,
    #[serde(default)]
    pub location_name: Option<String>,
    #[serde(default)]
    pub game_clock: Option<String>,
    #[serde(default)]
    pub npcs_here_count: Option<u32>,
    pub frame_path: String,
    pub lines_path: String,
    /// Optional relative path (e.g. `turns/NNN/llm.json`) to this turn's
    /// inference log. When present the file must exist in the bundle; the
    /// dashboard serves it so a turn can be clicked to view its raw
    /// prompt/response. Absent => the turn has no captured log.
    #[serde(default)]
    pub llm_transcript_path: Option<String>,
}

fn ok() -> String {
    "ok".into()
}
fn dialogue() -> String {
    "dialogue".into()
}
fn empty_obj() -> String {
    "{}".into()
}

/// One axis score.
#[derive(Debug, Deserialize)]
pub struct AxisPayload {
    pub axis: String,
    pub score: u8,
    #[serde(default)]
    pub rationale: String,
}

/// One discrete finding. Unlike the judge's `RawFinding`, the skill supplies the
/// dedup `signature` directly (it computed it when filing the GitHub issue), so
/// the dashboard collapses skill and binary findings of the same class together.
#[derive(Debug, Deserialize)]
pub struct FindingPayload {
    pub category: String,
    #[serde(default)]
    pub turn_index: Option<u32>,
    pub severity: String,
    pub description: String,
    #[serde(default)]
    pub evidence_quote: String,
    pub signature: String,
    /// Optional GitHub issue URL the skill filed for this finding. When present
    /// it is persisted onto the finding row so the dashboard renders an `[issue]`
    /// link, matching binary runs whose filer sets the same column.
    #[serde(default)]
    pub issue_url: Option<String>,
}

/// Read, validate, and ingest a skill-run payload. Returns the new run id.
///
/// `artifacts_root` is the directory that holds `runs/<uuid>/...`; the run's
/// absolute artifact dir is stored so the dashboard's frame route resolves
/// `{artifact_dir}/turns/NNN/frame.png` exactly as for a binary run.
pub fn load_and_ingest(db: &Db, payload_path: &Path, artifacts_root: &Path) -> Result<i64> {
    let body = std::fs::read_to_string(payload_path)
        .map_err(|e| HarnessError::io(payload_path.display().to_string(), e))?;
    let payload: IngestPayload = serde_json::from_str(&body).map_err(|e| HarnessError::Json {
        context: format!("ingest payload {}", payload_path.display()),
        source: e,
    })?;

    let rec = build_record(payload, artifacts_root)?;
    db.ingest_complete_run(rec)
}

/// Map a deserialized payload onto the domain [`IngestRecord`], validating frame
/// artifacts along the way.
fn build_record(mut payload: IngestPayload, artifacts_root: &Path) -> Result<IngestRecord> {
    // Force the config label so every skill run content-hashes to one config row.
    payload.config.label = Some("skill:quality-harness".to_string());

    let artifact_dir = artifacts_root.join("runs").join(&payload.uuid);
    let artifact_dir_str = artifact_dir.to_string_lossy().into_owned();

    // Map turns, validating that each referenced frame exists and is non-empty
    // (rule #14 — never accept a blank/missing artifact as if it were content).
    let mut turns = Vec::with_capacity(payload.turns.len());
    for t in payload.turns {
        let frame_abs = artifact_dir.join(&t.frame_path);
        let meta = std::fs::metadata(&frame_abs)
            .map_err(|e| HarnessError::io(frame_abs.display().to_string(), e))?;
        if meta.len() == 0 {
            return Err(HarnessError::BlankArtifact(format!(
                "turn {} frame is empty: {}",
                t.turn_index,
                frame_abs.display()
            )));
        }
        // A referenced inference log must exist — never store a dangling path.
        if let Some(rel) = &t.llm_transcript_path {
            let abs = artifact_dir.join(rel);
            if !abs.is_file() {
                return Err(HarnessError::io(
                    abs.display().to_string(),
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("turn {} llm_transcript_path missing", t.turn_index),
                    ),
                ));
            }
        }
        turns.push(TurnRecord {
            turn_index: t.turn_index,
            player_input: t.player_input,
            outcome: t.outcome,
            kind: t.kind,
            elapsed_ms: t.elapsed_ms,
            engine_state_json: t.engine_state_json,
            location_id: t.location_id,
            location_name: t.location_name,
            game_clock: t.game_clock,
            npcs_here_count: t.npcs_here_count,
            screenshot_path: None,
            frame_path: t.frame_path,
            lines_path: t.lines_path,
            llm_transcript_path: t.llm_transcript_path,
        });
    }

    // Map axes, rejecting unknown keys so a typo can't silently drop a score.
    let mut axes = Vec::with_capacity(payload.axes.len());
    for a in &payload.axes {
        let axis = Axis::from_key(&a.axis)
            .ok_or_else(|| HarnessError::Config(format!("unknown quality axis '{}'", a.axis)))?;
        axes.push(AxisScore {
            axis,
            score: a.score.min(100),
            rationale: a.rationale.clone(),
        });
    }

    // Carry each finding's optional issue URL in a parallel vec so the sink can
    // set it on the row after insert (the domain `Finding` stays link-agnostic).
    let mut findings = Vec::with_capacity(payload.findings.len());
    let mut finding_issue_urls = Vec::with_capacity(payload.findings.len());
    for f in payload.findings {
        finding_issue_urls.push(f.issue_url);
        findings.push(Finding {
            category: f.category,
            turn_index: f.turn_index,
            severity: Severity::from_label(&f.severity),
            description: f.description,
            evidence_quote: f.evidence_quote,
            signature: f.signature,
        });
    }

    let gate = payload.gate.map(|g| GateTrip {
        reason: GateReason::from_label(&g.reason),
        turn: g.turn,
        detail: g.detail,
    });

    // Scored runs carry a quality number; recompute from axes when omitted.
    let quality_score = if gate.is_some() {
        None
    } else {
        payload.quality_score.or_else(|| weighted_quality(&axes))
    };

    Ok(IngestRecord {
        config: payload.config,
        git: payload.git,
        rubric_sha256: payload.rubric_sha256,
        artifact_dir: artifact_dir_str,
        turns,
        axes,
        findings,
        finding_issue_urls,
        gate,
        quality_score,
        cost_usd: payload.cost.cost_usd,
        player_tokens: payload.cost.player_tokens,
        judge_tokens: payload.cost.judge_tokens,
    })
}
