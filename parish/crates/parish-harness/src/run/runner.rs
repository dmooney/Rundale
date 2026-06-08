//! The run loop — boot/attach the backend, play N turns capturing state +
//! artifacts with per-turn gating, then run the end-of-run judge pass, score,
//! and persist.

use std::path::{Path, PathBuf};
use std::time::Duration;

use uuid::Uuid;

use crate::actor::{Judge, Observation, Player, RunTranscript, TurnTranscript, summarize_state};
use crate::client::{self, GameClient, HttpGameClient};
use crate::config::RunConfig;
use crate::error::{HarnessError, Result};
use crate::frame;
use crate::git::GitProvenance;
use crate::persist::{Db, RunSummary, TurnRecord};
use crate::run::turn::{write_run_artifact, write_turn_artifacts};
use crate::score::{GateState, GateTrip, TurnEvent, weighted_quality};
use crate::score::gate::GateReason;

/// Everything needed to execute one run.
pub struct RunParams {
    pub config: RunConfig,
    pub base_url: String,
    pub turns: u32,
    pub command_timeout_ms: u64,
    pub ready_timeout: Duration,
    pub artifact_root: PathBuf,
    pub player: Box<dyn Player>,
    pub judge: Box<dyn Judge>,
}

/// Run a full session and return the persisted summary. `db` is borrowed so the
/// caller (and tests) can inspect rows afterward.
pub async fn execute_run(db: &Db, params: RunParams) -> Result<RunSummary> {
    let RunParams {
        config,
        base_url,
        turns,
        command_timeout_ms,
        ready_timeout,
        artifact_root,
        player,
        judge,
    } = params;

    let git = GitProvenance::capture();
    let config_id = db.upsert_config(&config)?;

    let run_uuid = Uuid::new_v4();
    let run_dir = artifact_root.join("runs").join(run_uuid.to_string());
    std::fs::create_dir_all(&run_dir)
        .map_err(|e| HarnessError::io(run_dir.display().to_string(), e))?;
    let run_dir_str = run_dir.display().to_string();

    let rubric_sha = judge.rubric_sha256().to_string();
    let run_id = db.start_run(config_id, &git, &rubric_sha, &run_dir_str)?;

    // Snapshot the resolved config alongside the artifacts.
    write_run_artifact(&run_dir, "config.json", &config.canonical_json())?;

    let client = HttpGameClient::new(&base_url);

    // ── Boot / attach ─────────────────────────────────────────────────────
    let mut gate = GateState::new(config.gate.clone());
    if let Err(e) = client::wait_ready(&client, ready_timeout).await {
        // No reachable backend — a Crash gate, recorded at turn 0.
        let trip = crash_trip(&mut gate, 0, &format!("backend not ready: {e}"));
        db.finish_run_gated(run_id, &trip)?;
        return db.run_summary(run_id);
    }
    if let Err(e) = client.new_game().await {
        if is_crash(&e) {
            let trip = crash_trip(&mut gate, 0, &format!("new_game failed: {e}"));
            db.finish_run_gated(run_id, &trip)?;
            return db.run_summary(run_id);
        }
        return Err(e);
    }

    // Apply feature flags, then BYOK model overrides.
    for flag in &config.flags {
        client.apply_flag(&flag.name, flag.on).await?;
    }
    for (category, model) in &config.engine_models {
        if let Err(e) = client.apply_byok(category, model).await {
            // The headless server lacks /api/submit-byok; a 404 is a soft skip,
            // anything else is a real failure.
            match &e {
                HarnessError::HttpStatus { status: 404, .. } => {
                    tracing::warn!(category, "backend has no BYOK endpoint; skipping override");
                }
                _ => return Err(e),
            }
        }
    }

    // ── Turn loop ─────────────────────────────────────────────────────────
    let mut transcript = RunTranscript {
        persona: config.player.persona.clone(),
        turns: Vec::new(),
    };
    let mut last_narrative = String::new();

    for turn in 0..turns {
        // Pre-command state for the observation.
        let pre_state = match client.engine_state().await {
            Ok(s) => s,
            Err(e) if is_crash(&e) => {
                let trip = crash_trip(&mut gate, turn, &format!("engine_state failed: {e}"));
                db.finish_run_gated(run_id, &trip)?;
                return db.run_summary(run_id);
            }
            Err(e) => return Err(e),
        };

        let mv = {
            let obs = Observation {
                turn_index: turn,
                state: &pre_state,
                narrative: &last_narrative,
                persona: &config.player.persona,
                strategy: &config.player.strategy,
            };
            player.choose_action(&obs).await?
        };

        // Submit; classify failures into gate events.
        let resp = match client
            .submit_command(&mv.text, &mv.addressed_to, command_timeout_ms)
            .await
        {
            Ok(r) => r,
            Err(HarnessError::Json { context, .. }) => {
                let trip = gate
                    .observe(turn, &TurnEvent::JsonParseFail(context.clone()))
                    .unwrap_or(GateTrip {
                        reason: GateReason::JsonParseFail,
                        turn,
                        detail: context,
                    });
                db.finish_run_gated(run_id, &trip)?;
                return db.run_summary(run_id);
            }
            Err(e) if is_crash(&e) => {
                let trip = crash_trip(&mut gate, turn, &format!("submit failed: {e}"));
                db.finish_run_gated(run_id, &trip)?;
                return db.run_summary(run_id);
            }
            Err(e) => return Err(e),
        };

        // Post-command state for the frame, signature, and persistence.
        let post_state = match client.engine_state().await {
            Ok(s) => s,
            Err(e) if is_crash(&e) => {
                let trip = crash_trip(&mut gate, turn, &format!("engine_state failed: {e}"));
                db.finish_run_gated(run_id, &trip)?;
                return db.run_summary(run_id);
            }
            Err(e) => return Err(e),
        };

        let narrative = resp.narrative();
        let outcome = resp.outcome();
        let outcome_str = resp.outcome.clone();
        let kind = resp.kind.clone();
        let state_sig = summarize_state(&post_state);

        // Render + persist artifacts.
        let stframe = frame::render(&post_state, &narrative, turn)?;
        let lines_json = serde_json::to_string(&resp.lines).map_err(|e| HarnessError::Json {
            context: "serialize turn lines".into(),
            source: e,
        })?;
        let arts = write_turn_artifacts(&run_dir, turn, &stframe, &lines_json)?;
        let engine_state_json =
            serde_json::to_string(&post_state).map_err(|e| HarnessError::Json {
                context: "serialize engine state".into(),
                source: e,
            })?;

        db.record_turn(
            run_id,
            &TurnRecord {
                turn_index: turn,
                player_input: mv.text.clone(),
                outcome: outcome_str.clone(),
                kind: kind.clone(),
                elapsed_ms: resp.elapsed_ms,
                engine_state_json,
                location_id: Some(post_state.active_scene.location_id),
                location_name: Some(post_state.active_scene.location_name.clone()),
                game_clock: Some(format!(
                    "{:02}:{:02}",
                    post_state.clock.hour, post_state.clock.minute
                )),
                npcs_here_count: Some(post_state.npcs.here.len() as u32),
                screenshot_path: None,
                frame_path: arts.frame_path.clone(),
                lines_path: arts.lines_path.clone(),
                llm_transcript_path: None,
            },
        )?;

        transcript.turns.push(TurnTranscript {
            turn_index: turn,
            player_input: mv.text,
            narrative: narrative.clone(),
            outcome: outcome_str,
            kind,
            state_summary: state_sig.clone(),
        });
        let _ = arts.svg_path; // written; not separately tracked in the DB yet.

        // Per-turn gate evaluation.
        if let Some(trip) = gate.observe(
            turn,
            &TurnEvent::Response {
                outcome,
                state_sig: Some(state_sig),
            },
        ) {
            db.finish_run_gated(run_id, &trip)?;
            write_transcript(&run_dir, &transcript)?;
            return db.run_summary(run_id);
        }

        last_narrative = narrative;
    }

    // ── End-of-run judge pass ───────────────────────────────────────────────
    write_transcript(&run_dir, &transcript)?;
    let verdict = judge.judge_run(&transcript).await?;
    let verdict_json = render_verdict_json(&verdict);
    write_run_artifact(&run_dir, "verdict.json", &verdict_json)?;

    // Persist findings regardless of gating.
    for f in &verdict.findings {
        db.insert_finding(run_id, f)?;
    }

    if verdict.has_critical() {
        let trip = GateTrip {
            reason: GateReason::JudgeCritical,
            turn: transcript.turns.len() as u32,
            detail: "judge emitted a critical finding".into(),
        };
        db.finish_run_gated(run_id, &trip)?;
    } else {
        let quality = weighted_quality(&verdict.axes);
        db.finish_run_scored(run_id, quality, &verdict.axes)?;
    }

    db.run_summary(run_id)
}

/// Build a Crash gate trip (and update gate state) for an operational failure
/// that should be recorded as a gate rather than surfaced to the operator.
fn crash_trip(gate: &mut GateState, turn: u32, detail: &str) -> GateTrip {
    gate.observe(turn, &TurnEvent::Crash(detail.to_string()))
        .unwrap_or_else(|| GateTrip {
            reason: GateReason::Crash,
            turn,
            detail: detail.to_string(),
        })
}

/// Whether an error is a backend crash (transport / 5xx) vs an operational
/// harness failure.
fn is_crash(e: &HarnessError) -> bool {
    matches!(
        e,
        HarnessError::Transport { .. } | HarnessError::HttpStatus { .. }
    )
}

fn write_transcript(run_dir: &Path, transcript: &RunTranscript) -> Result<()> {
    let json = serde_json::to_string_pretty(transcript).map_err(|e| HarnessError::Json {
        context: "serialize transcript".into(),
        source: e,
    })?;
    write_run_artifact(run_dir, "transcript.json", &json)
}

/// Render the verdict (axes + findings) to JSON for the artifact bundle.
fn render_verdict_json(verdict: &crate::actor::JudgeVerdict) -> String {
    let axes: serde_json::Map<String, serde_json::Value> = verdict
        .axes
        .iter()
        .map(|a| {
            (
                a.axis.key().to_string(),
                serde_json::json!({"score": a.score, "rationale": a.rationale}),
            )
        })
        .collect();
    let findings: Vec<serde_json::Value> = verdict
        .findings
        .iter()
        .map(|f| {
            serde_json::json!({
                "category": f.category,
                "turn_index": f.turn_index,
                "severity": f.severity.label(),
                "description": f.description,
                "evidence_quote": f.evidence_quote,
                "signature": f.signature,
            })
        })
        .collect();
    serde_json::json!({"axes": axes, "findings": findings}).to_string()
}
