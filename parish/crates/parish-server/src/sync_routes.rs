//! Synchronous command/state endpoints for thin clients.
//!
//! `POST /api/command` — dispatch a player command and return the complete
//! response (text log + world state) in a single HTTP round-trip.
//!
//! `GET /api/state` — read-only world state snapshot.
//!
//! These are "infrastructure" endpoints — they have no Tauri command
//! counterpart and are intentionally excluded from `route_registry`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Extension, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use tokio::time::Instant;

use parish_core::event_bus::{EventBus as EventBusTrait, Topic};
use parish_core::input::{InputResult, classify_input_with_addressees};

use crate::routes::{admin_emails, check_admin};
use crate::routes::{
    handle_game_input, handle_system_command, is_admin_command, validate_addressed_to,
};
use crate::state::AppState;
use crate::sync_types::{
    CommandRequest, CommandResponse, Kind, Outcome, StateBundle, StateResponse,
};

const DEFAULT_TIMEOUT_MS: u64 = 60_000;
const MAX_TIMEOUT_MS: u64 = 120_000;
const MAX_TEXT_LEN: usize = 2000;

/// `POST /api/command` — synchronous player command.
pub async fn post_command(
    Extension(state): Extension<Arc<AppState>>,
    Extension(auth): Extension<crate::cf_auth::AuthContext>,
    Json(body): Json<CommandRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let text = body.text.trim().to_string();

    if text.is_empty() {
        return (
            StatusCode::OK,
            Json(CommandResponse {
                outcome: Outcome::Empty,
                kind: Kind::Empty,
                echo: text,
                lines: vec![],
                kind_detail: serde_json::Value::Null,
                travel: None,
                state: None,
                elapsed_ms: 0,
            }),
        );
    }

    if text.len() > MAX_TEXT_LEN {
        return (StatusCode::BAD_REQUEST, Json(rejected("input too long")));
    }

    if validate_addressed_to(&body.addressed_to).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(rejected("addressed_to invalid")),
        );
    }

    let timeout_ms = body
        .timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .min(MAX_TIMEOUT_MS);
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let include_state = body.include_state.unwrap_or(true);
    let include_map = body.include_map.unwrap_or(false);

    // Serialise concurrent commands on this session.
    let _lock = match state.command_lock.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            return (StatusCode::CONFLICT, Json(rejected("command_in_progress")));
        }
    };
    let _persistence_guard = state.persistence_gate.lock().await;

    // Subscribe to relevant topics *before* dispatching so no events are missed.
    let stream = state.event_bus.subscribe(&[
        Topic::TextLog,
        Topic::WorldUpdate,
        Topic::InferenceToken,
        Topic::TravelStart,
    ]);

    let classified = classify_input_with_addressees(&text, &body.addressed_to);

    // Admin gate (mirrors submit_input logic).
    if let InputResult::SystemCommand(ref cmd) = classified
        && is_admin_command(cmd)
        && check_admin(&auth.email, &text, admin_emails()).is_err()
    {
        return (StatusCode::FORBIDDEN, Json(rejected("admin_only")));
    }

    // Dispatch (same path as submit_input, but we hold the event stream).
    match classified {
        InputResult::SystemCommand(cmd) => {
            let cmd_name = format!("{cmd:?}").to_lowercase();
            let _ = handle_system_command(cmd, &state, &text).await;
            let drain = crate::drain::drain_command(stream, deadline).await;

            let elapsed_ms = start.elapsed().as_millis() as u64;
            let state_bundle = if include_state {
                Some(build_state_bundle(&state, include_map).await)
            } else {
                None
            };

            (
                StatusCode::OK,
                Json(CommandResponse {
                    outcome: if drain.timed_out {
                        Outcome::Timeout
                    } else {
                        Outcome::Ok
                    },
                    kind: Kind::System,
                    echo: text,
                    lines: drain.lines,
                    kind_detail: serde_json::json!({ "command": cmd_name }),
                    travel: None,
                    state: state_bundle,
                    elapsed_ms,
                }),
            )
        }
        InputResult::GameInput(raw) => {
            if let Err(error) = handle_game_input(raw, body.addressed_to, &state, Vec::new()).await
            {
                tracing::error!(
                    session_id = %state.session_id,
                    %error,
                    "player task journal append failed"
                );
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(rejected("task_persistence_failed")),
                );
            }
            let drain = crate::drain::drain_command(stream, deadline).await;

            let elapsed_ms = start.elapsed().as_millis() as u64;

            let (kind, travel, map_needed) = classify_drain(&drain);
            let effective_include_map = include_map || map_needed;

            let state_bundle = if include_state {
                Some(build_state_bundle(&state, effective_include_map).await)
            } else {
                None
            };
            let dialogue_quality_detail = serde_json::json!({
                "dialogue_quality": {
                    "turns": drain.dialogue_quality.len(),
                    "contract_valid": drain
                        .dialogue_quality
                        .iter()
                        .filter(|item| item.contract_valid)
                        .count(),
                    "guard_interventions": drain
                        .dialogue_quality
                        .iter()
                        .filter(|item| item.guard_intervened)
                        .count(),
                    "guard_reasons": drain
                        .dialogue_quality
                        .iter()
                        .flat_map(|item| item.guard_reasons.iter().map(String::as_str))
                        .collect::<Vec<_>>(),
                    "parse_dispositions": drain
                        .dialogue_quality
                        .iter()
                        .map(|item| item.parse_disposition.as_str())
                        .collect::<Vec<_>>(),
                    "request_profiles": drain
                        .dialogue_quality
                        .iter()
                        .map(dialogue_request_profile)
                        .collect::<Vec<_>>(),
                }
            });

            (
                StatusCode::OK,
                Json(CommandResponse {
                    outcome: if drain.timed_out {
                        Outcome::Timeout
                    } else {
                        Outcome::Ok
                    },
                    kind,
                    echo: text,
                    lines: drain.lines,
                    kind_detail: dialogue_quality_detail,
                    travel,
                    state: state_bundle,
                    elapsed_ms,
                }),
            )
        }
    }
}

/// `GET /api/state` — read-only world snapshot.
pub async fn get_state(
    Extension(state): Extension<Arc<AppState>>,
    Query(params): Query<StateParams>,
) -> Json<StateResponse> {
    let include_map = params.include_map.unwrap_or(true);
    let bundle = build_state_bundle(&state, include_map).await;
    let server_time = iso8601_now();

    Json(StateResponse {
        world: bundle.world,
        npcs_here: bundle.npcs_here,
        map: bundle.map,
        server_time,
    })
}

#[derive(Deserialize)]
pub struct StateParams {
    include_map: Option<bool>,
}

// ── helpers ───────────────────────────────────────────────────────────────────

async fn build_state_bundle(state: &Arc<AppState>, include_map: bool) -> StateBundle {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let mut snapshot = parish_core::ipc::snapshot_from_world(&world);
    snapshot.name_hints =
        parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
    let npcs = parish_core::ipc::build_npcs_here(&world, &npc_manager);
    let map = if include_map {
        let config = state.config.lock().await;
        let transport = state.transport.default_mode();
        Some(parish_core::ipc::build_map_data(
            &world,
            transport,
            config.reveal_unexplored_locations,
        ))
    } else {
        None
    };
    StateBundle {
        world: snapshot,
        npcs_here: npcs,
        map,
    }
}

fn classify_drain(
    drain: &crate::drain::DrainResult,
) -> (Kind, Option<crate::sync_types::TravelDetail>, bool) {
    if drain.travel.is_some() {
        let t = drain.travel.clone();
        return (Kind::Moved, t, true);
    }
    if drain.had_streaming {
        return (Kind::Talked, None, false);
    }
    (Kind::Looked, None, false)
}

fn rejected(reason: &str) -> CommandResponse {
    CommandResponse {
        outcome: Outcome::Rejected,
        kind: Kind::Rejected,
        echo: String::new(),
        lines: vec![],
        kind_detail: serde_json::json!({ "reason": reason }),
        travel: None,
        state: None,
        elapsed_ms: 0,
    }
}

/// Canonicalizes an `f32` generation parameter for evidence serialization.
///
/// TOML values such as `0.7` are not exactly representable as `f32`; exposing
/// their binary expansion (`0.699999988...`) makes semantically identical
/// benchmark profiles fail the promotion gate's exact-profile comparison.
fn canonical_generation_float(value: f32) -> f64 {
    (f64::from(value) * 10_000.0).round() / 10_000.0
}

fn dialogue_request_profile(item: &parish_core::ipc::DialogueQualityPayload) -> serde_json::Value {
    serde_json::json!({
        "model": item.model,
        "max_tokens": item.generation.max_tokens,
        "temperature": canonical_generation_float(item.generation.temperature),
        "frequency_penalty": item
            .generation
            .frequency_penalty
            .map(canonical_generation_float),
        "json_mode": item.generation.json_mode,
        "enable_thinking": item.generation.enable_thinking,
        "reasoning_effort": item.generation.reasoning_effort,
    })
}

fn iso8601_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO-8601 UTC without pulling in chrono.
    let (y, mo, d, h, mi, s) = seconds_to_ymdhms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn seconds_to_ymdhms(mut s: u64) -> (u64, u64, u64, u64, u64, u64) {
    let sec = s % 60;
    s /= 60;
    let min = s % 60;
    s /= 60;
    let hour = s % 24;
    s /= 24;
    // Days since Unix epoch → Gregorian date (good until year 2100).
    let mut y = 1970u64;
    loop {
        let days_in_year =
            if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) {
                366
            } else {
                365
            };
        if s < days_in_year {
            break;
        }
        s -= days_in_year;
        y += 1;
    }
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let months = [
        31u64,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 1u64;
    for &days in &months {
        if s < days {
            break;
        }
        s -= days;
        mo += 1;
    }
    (y, mo, s + 1, hour, min, sec)
}

#[cfg(test)]
mod generation_profile_tests {
    use super::{canonical_generation_float, dialogue_request_profile};

    #[test]
    fn generation_floats_have_stable_evidence_representation() {
        assert_eq!(canonical_generation_float(0.7), 0.7);
        assert_eq!(canonical_generation_float(0.35), 0.35);
        assert_eq!(canonical_generation_float(-0.125), -0.125);
    }

    #[test]
    fn request_profile_includes_reasoning_control() {
        let item = parish_core::ipc::DialogueQualityPayload {
            turn_id: 1,
            parse_disposition: "full_json".into(),
            contract_valid: true,
            guard_intervened: false,
            guard_reasons: Vec::new(),
            model: "qwen-9b".into(),
            generation: parish_core::ipc::DialogueGenerationTelemetry {
                max_tokens: 768,
                temperature: 0.7,
                frequency_penalty: Some(0.5),
                json_mode: true,
                enable_thinking: Some(true),
                reasoning_effort: Some(parish_core::config::ReasoningEffort::Max),
            },
        };

        assert_eq!(
            dialogue_request_profile(&item)["enable_thinking"],
            serde_json::json!(true)
        );
        assert_eq!(
            dialogue_request_profile(&item)["reasoning_effort"],
            serde_json::json!("max")
        );
    }
}
