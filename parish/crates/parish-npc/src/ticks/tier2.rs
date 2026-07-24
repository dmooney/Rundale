//! Tier 2 tick — background group inference for nearby NPCs.
//!
//! Tier 2 runs every 5 game-minutes for NPCs at the same location (not
//! necessarily the player's). Inference is lighter than Tier 1 and runs in a
//! background task so it does not block player turns.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::memory::{MemoryEntry, try_promote};
use crate::types::{Tier2Event, Tier2Response};
use crate::{LanguageSettings, Npc, NpcId};
use parish_config::{NpcConfig, RelationshipLabelConfig};
use parish_types::{DayType, ParishError, Season};
use parish_world::LocationId;

use super::prompt::format_relationships_natural;
use super::truncate::truncate_for_memory;

// ── NPC snapshot types ─────────────────────────────────────────────────────

/// A lightweight snapshot of an NPC's state for Tier 2 inference.
///
/// Contains only the data needed to build Tier 2 prompts, allowing
/// the inference to run in a background task without borrowing from
/// the NpcManager.
#[derive(Debug, Clone)]
pub struct NpcSnapshot {
    /// NPC id.
    pub id: NpcId,
    /// NPC name.
    pub name: String,
    /// Occupation.
    pub occupation: String,
    /// Personality summary.
    pub personality: String,
    /// Narration pronouns (e.g. `he/him`, `she/her`, `they/them`) so Tier 2
    /// narration doesn't mis-gender authored NPCs (#1026).
    pub pronouns: String,
    /// Natural-language description of how this NPC thinks and speaks
    /// (from `Intelligence::prompt_guidance`). Empty for an all-3s profile.
    pub intelligence_prose: String,
    /// Current mood.
    pub mood: String,
    /// Natural-language relationship summary
    /// (e.g. "friendly with Mary McKenna, cool toward Sean Doyle"). May be empty.
    pub relationship_summary: String,
    /// Authored schedule activity for the NPC at their canonical current
    /// location and time. `None` when no matching schedule entry exists.
    pub current_activity: Option<String>,
}

/// A group of NPC snapshots at a single location, for Tier 2 processing.
#[derive(Debug, Clone)]
pub struct Tier2Group {
    /// Location where these NPCs are gathered.
    pub location: LocationId,
    /// Location name for prompt context.
    pub location_name: String,
    /// Every other canonical location name in the loaded world. Used to
    /// reject summaries that silently relocate participants.
    pub other_location_names: Vec<String>,
    /// Snapshots of NPCs at this location.
    pub npcs: Vec<NpcSnapshot>,
}

// ── relationship helper (shared with Tier 3 snapshots) ────────────────────

/// Returns the top-N strongest relationships (by absolute strength) in a
/// stable order: by |strength| descending, then by NpcId ascending as a
/// tie-breaker so iteration order over the underlying HashMap doesn't leak.
pub(super) fn top_relationships(npc: &Npc, n: usize) -> Vec<(NpcId, f64)> {
    let mut rels: Vec<(NpcId, f64)> = npc
        .relationships
        .iter()
        .map(|(id, rel)| (*id, rel.strength))
        .collect();
    rels.sort_by(|(a_id, a_s), (b_id, b_s)| {
        b_s.abs()
            .partial_cmp(&a_s.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a_id.0.cmp(&b_id.0))
    });
    rels.truncate(n);
    rels
}

/// Creates an `NpcSnapshot` from a live NPC for Tier 2 background inference.
///
/// The snapshot is a lightweight owned copy that can be passed to a background
/// task without holding a lock on the `NpcManager`. Peer names are resolved
/// at snapshot time so the snapshot is self-contained — the prompt builder
/// does not need access to the name map.
pub fn npc_snapshot_from_npc(npc: &Npc, npc_names: &HashMap<NpcId, String>) -> NpcSnapshot {
    let rels = top_relationships(npc, 3);

    NpcSnapshot {
        id: npc.id,
        name: npc.name.clone(),
        occupation: npc.occupation.clone(),
        personality: npc.personality.clone(),
        pronouns: npc.pronouns.clone(),
        intelligence_prose: npc.intelligence.prompt_guidance(),
        mood: npc.mood.clone(),
        relationship_summary: format_relationships_natural(
            &rels,
            npc_names,
            &RelationshipLabelConfig::default(),
        ),
        current_activity: None,
    }
}

/// Creates a Tier-2 snapshot grounded in the NPC's authored activity at the
/// canonical current location and game-time context.
///
/// A schedule entry is included only when its location still matches
/// `npc.location`; weather diversions and cuaird visits can legitimately put an
/// NPC somewhere other than the raw schedule destination.
pub fn npc_snapshot_from_npc_at(
    npc: &Npc,
    npc_names: &HashMap<NpcId, String>,
    hour: u8,
    season: Season,
    day_type: DayType,
) -> NpcSnapshot {
    let mut snapshot = npc_snapshot_from_npc(npc, npc_names);
    snapshot.current_activity = npc
        .schedule_entry(hour, season, day_type)
        .filter(|entry| entry.location == npc.location)
        .map(|entry| entry.activity.trim())
        .filter(|activity| !activity.is_empty())
        .map(str::to_owned);
    snapshot
}

// ── inference helpers ──────────────────────────────────────────────────────

/// Strict-JSON reminder appended to a Tier 2 prompt on retry.
///
/// Regression note (fixed: #27): the 1.5B simulation-tier model occasionally emits
/// malformed JSON (unquoted keys, trailing prose, markdown fences),
/// which surfaces as `"Tier 2 JSON parse failed: ..."` and silently
/// drops the location's off-screen update for that tick. The retry
/// re-invokes inference with this reminder appended so the model
/// has a second, sharper push toward strict JSON.
pub(crate) const TIER2_STRICT_JSON_REMINDER: &str = "\n\n\
    IMPORTANT — your previous reply was not valid JSON. Return STRICT \
    JSON ONLY. No markdown fences, no commentary before or after the \
    object, no trailing prose. All keys must be double-quoted strings. \
    The envelope shape is exactly:\n\
    {\"summary\": \"...\", \"mood_changes\": [], \"relationship_changes\": []}";

/// Returns true when the error string represents a JSON parse
/// failure produced by [`run_tier2_for_group`]. Used to gate the
/// one-shot retry — non-parse failures (cancellation, transport,
/// timeout) should not retry.
pub(crate) fn is_tier2_json_parse_failure(msg: &str) -> bool {
    msg.contains("Tier 2 JSON parse failed")
}

/// Cumulative Tier 2 JSON parse failure count since process start
/// (see #29). Surfaced in `parish_core::debug_snapshot::InferenceDebug`
/// so an operator can trend silent off-screen sim drops across a demo
/// run. Per-location detail still lives in `parish_npc::ticks` WARN
/// logs — the counter is a coarse trend signal, not a replacement.
static TIER2_PARSE_FAILURES_TOTAL: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Returns the cumulative Tier 2 JSON parse failure count since process
/// start. Used by `build_debug_snapshot` to populate
/// `InferenceDebug::tier2_parse_failures_total`.
pub fn tier2_parse_failures_total() -> u64 {
    TIER2_PARSE_FAILURES_TOTAL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Records a Tier 2 JSON parse failure. Internal — callers reach
/// the counter through the WARN log path that already classifies
/// the error.
fn record_tier2_parse_failure() {
    TIER2_PARSE_FAILURES_TOTAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Returns true when an inference error string represents a graceful
/// cancellation (shutdown, `sim_cancel` on player input, demo turn cap)
/// rather than a real failure. Both Tier 2 and Tier 3 paths construct
/// their cancellation errors as `"Tier {N} cancelled mid-stream"`, so
/// the substring `"cancelled mid-stream"` is the discriminator. This
/// keeps intentional cancellations off the WARN path that the Tier 2
/// and Tier 3 failure classifiers reserve for real errors.
pub(super) fn is_intentional_cancellation(msg: &str) -> bool {
    msg.contains("cancelled mid-stream")
}

/// Runs a single Tier 2 inference attempt: streams via `client`, sinks
/// tokens to discard, races against `cancel`, and parses the assembled
/// string into a [`Tier2Response`]. Errors are returned for the caller
/// to classify (parse failure → retry; cancellation → bail; transport
/// → bail with diagnostic).
async fn try_tier2_inference(
    client: &parish_inference::AnyClient,
    model: &str,
    prompt: &str,
    response_format: Option<parish_inference::ResponseFormat>,
    cancel: Option<parish_inference::CancellationToken>,
) -> Result<Tier2Response, ParishError> {
    // Cap output to bound vllm-mlx runaway risk on uncapped JSON gen.
    // Tier 2 outputs ~50-100 tokens in practice; 200 is comfortable headroom.
    // Streaming path: discards chunks (the assembled string returns from
    // generate_stream_with_format) but enables mid-flight cancellation (#9).
    let (sink_tx, mut sink_rx) =
        tokio::sync::mpsc::channel::<String>(parish_inference::TOKEN_CHANNEL_CAPACITY);
    tokio::spawn(async move { while sink_rx.recv().await.is_some() {} });

    let stream_fut = client.generate_stream_with_format(
        model,
        prompt,
        None,
        sink_tx,
        response_format,
        parish_inference::GenerateParams {
            max_tokens: Some(200),
            temperature: None,
            frequency_penalty: None,
        },
    );

    let raw = match cancel {
        Some(tok) => tokio::select! {
            biased;
            () = tok.cancelled() => Err(ParishError::Inference(
                "Tier 2 cancelled mid-stream".to_string(),
            )),
            res = stream_fut => res,
        },
        None => stream_fut.await,
    };

    raw.and_then(|s| {
        serde_json::from_str::<Tier2Response>(&s)
            .map_err(|e| ParishError::Inference(format!("Tier 2 JSON parse failed: {e}")))
    })
}

// ── prompt builder ─────────────────────────────────────────────────────────

/// Builds the system prompt for a Tier 2 interaction between NPCs at a location.
pub fn build_tier2_prompt(
    group: &Tier2Group,
    time_desc: &str,
    weather: &str,
    language: &LanguageSettings,
) -> String {
    use crate::language_directive;

    let npc_descriptions: Vec<String> = group
        .npcs
        .iter()
        .map(|snap| {
            let mut line = format!(
                "- [{id}] {name} ({pronouns}), {occupation}. Currently {mood}.",
                id = snap.id.0,
                name = snap.name,
                pronouns = snap.pronouns,
                occupation = snap.occupation,
                mood = snap.mood,
            );
            if !snap.intelligence_prose.is_empty() {
                line.push(' ');
                line.push_str(&snap.intelligence_prose);
            }
            if !snap.relationship_summary.is_empty() {
                line.push(' ');
                line.push_str(&snap.relationship_summary);
                line.push('.');
            }
            if let Some(activity) = &snap.current_activity {
                line.push_str(" Authored activity at this exact location: ");
                line.push_str(activity);
                line.push('.');
            } else {
                line.push_str(
                    " No authored activity is available here; keep their action generic and \
                     compatible with this location.",
                );
            }
            line
        })
        .collect();

    let weather_commentary = match weather {
        "Light Rain" | "Heavy Rain" | "Storm" => " People are commenting on the weather.",
        _ => "",
    };

    let mut prompt = format!(
        "You are simulating background interactions between characters in a small \
        Irish parish in 1820.\n\n\
        Location: {location}\n\
        CANONICAL LOCATION — every listed character is physically at {location}. \
        Do not move them elsewhere in the summary, do not say they are \"at home\", \
        and do not mention a different mill, forge, shop, farm, road, or village. \
        Their actions must fit this exact location and the authored current activity \
        shown for them below; occupation alone is not evidence that they are at work.\n\
        Time: {time}\n\
        Weather: {weather}.{weather_commentary}\n\n\
        Dramatis personae (id in brackets — reuse these in your JSON):\n\
        {characters}\n\n\
        Write one short sentence (max 20 words) describing what these characters are \
        doing right now. Refer to each character with the pronouns shown in \
        parentheses. Only name characters listed in the dramatis personae above — to \
        refer to anyone absent, use a generic descriptor (\"the smith\", \"a \
        neighbour\"), never a proper name. Most exchanges are uneventful — leave \
        mood_changes and relationship_changes as empty arrays unless a character's \
        mood has clearly shifted or a relationship has meaningfully strengthened or \
        strained.\n\n\
        Respond with a JSON object, using the bracketed ids. Default shape (use this \
        when nothing notable changes):\n\
        {{\"summary\": \"...\", \"mood_changes\": [], \"relationship_changes\": []}}\n\n\
        Only when something actually changes, include entries:\n\
          mood_changes:        {{\"npc_id\": <id>, \"new_mood\": \"<mood>\"}}\n\
          relationship_changes: {{\"from\": <id>, \"to\": <id>, \"delta\": <-0.1 to 0.1>}}",
        location = group.location_name,
        time = time_desc,
        weather = weather,
        characters = npc_descriptions.join("\n"),
    );

    prompt.push_str("\n\n");
    prompt.push_str(&language_directive(language));
    prompt
}

// ── event predicates ───────────────────────────────────────────────────────

fn mentions_other_location(summary: &str, location_name: &str) -> bool {
    let summary = summary.to_lowercase();
    let location = location_name.trim().to_lowercase();
    if location.is_empty() {
        return false;
    }
    if summary.contains(&location) {
        return true;
    }

    let without_article = location
        .strip_prefix("the ")
        .or_else(|| location.strip_prefix("a "))
        .unwrap_or(&location);
    without_article != location
        && [
            "at ", "by ", "in ", "near ", "beside ", "outside ", "inside ",
        ]
        .iter()
        .any(|prefix| summary.contains(&format!("{prefix}{without_article}")))
}

/// Returns a concise reason when a Tier-2 summary contradicts its canonical
/// group location.
///
/// The check is deliberately narrow: it rejects explicit names of other
/// loaded locations and the generic relocation phrase "at home". Ordinary
/// dialogue about a person or object is left untouched.
pub fn tier2_summary_location_conflict(group: &Tier2Group, summary: &str) -> Option<String> {
    let lower = summary.to_lowercase();
    if lower.contains(" at home")
        || lower.starts_with("at home")
        || lower.contains(" in their home")
        || lower.contains(" in his home")
        || lower.contains(" in her home")
    {
        return Some("summary relocates a participant home".to_string());
    }

    let current = group.location_name.trim();
    group
        .other_location_names
        .iter()
        .filter(|name| !name.trim().eq_ignore_ascii_case(current))
        .find(|name| mentions_other_location(summary, name))
        .map(|name| format!("summary names other location '{name}'"))
}

fn tier2_grounding_error(group: &Tier2Group, summary: &str) -> Option<ParishError> {
    tier2_summary_location_conflict(group, summary)
        .map(|reason| ParishError::Inference(format!("Tier 2 location grounding failed: {reason}")))
}

fn is_tier2_grounding_failure(msg: &str) -> bool {
    msg.contains("Tier 2 location grounding failed")
}

/// Returns the name of an NPC mentioned in `summary` who is not one of the
/// scene `participants`, if any. Used to drop Tier 2 narrative beats that
/// hallucinate absent characters into a location (#1027).
///
/// Matches on the full authored name (case-insensitive substring). Full
/// names are distinctive enough to avoid the first-name collisions that a
/// shorter token match would hit. The result is deterministic — when several
/// absent NPCs are named, the lexicographically first is returned — so the
/// warning and any test assertion are stable across HashMap iteration order.
/// Whether a Tier 2 event's summary names an NPC outside its participant list.
///
/// Callers use this to gate side effects that propagate the summary text —
/// e.g. skipping `create_gossip_from_tier2_event` so a hallucinated name can't
/// spread through the gossip network (#1027), mirroring the in-function guard
/// that suppresses the `NpcInteraction` publish and the memory write.
pub fn tier2_summary_mentions_absent_npc(event: &Tier2Event, npcs: &HashMap<NpcId, Npc>) -> bool {
    summary_mentions_absent_npc(&event.summary, &event.participants, npcs).is_some()
}

fn summary_mentions_absent_npc(
    summary: &str,
    participants: &[NpcId],
    npcs: &HashMap<NpcId, Npc>,
) -> Option<String> {
    let haystack = summary.to_lowercase();
    let mut absent: Vec<String> = npcs
        .iter()
        .filter(|(id, _)| !participants.contains(id))
        .filter_map(|(_, npc)| {
            let name = npc.name.trim();
            if name.is_empty() {
                return None;
            }
            haystack
                .contains(&name.to_lowercase())
                .then(|| name.to_string())
        })
        .collect();
    absent.sort();
    absent.into_iter().next()
}

// ── main inference entry point ─────────────────────────────────────────────

/// Runs Tier 2 inference for a group of NPCs at a location.
///
/// Calls the provided `client` directly — the caller resolves the right
/// per-category client (typically `InferenceCategory::Simulation`) via
/// `GameConfig::resolve_category_client` before invoking this. This was
/// formerly routed through the shared `InferenceQueue`, which always sent
/// to the *base* provider's HTTP endpoint regardless of the per-category
/// override, breaking the two-slot Apple Silicon loadout where
/// Simulation is supposed to hit the small slot on `:8001` while
/// Dialogue holds the big slot on `:8000`. Direct-client dispatch is
/// the same pattern `emit_npc_reactions` already uses.
///
/// `cancel` enables mid-flight preemption when a player turn arrives:
/// the streaming future races against the token via `tokio::select!`
/// and drops on cancel, closing the underlying HTTP/SSE connection so
/// the simulation slot frees up for the next request. Pass `None` when
/// no preemption is needed.
pub async fn run_tier2_for_group(
    client: &parish_inference::AnyClient,
    model: &str,
    group: &Tier2Group,
    time_desc: &str,
    weather: &str,
    language: &LanguageSettings,
    cancel: Option<parish_inference::CancellationToken>,
) -> Option<Tier2Event> {
    if group.npcs.len() < 2 {
        // Solo NPC: generate a simple template event, no inference needed
        if let Some(snap) = group.npcs.first() {
            return Some(Tier2Event {
                location: group.location,
                summary: format!(
                    "{} goes about their business at {}.",
                    snap.name, group.location_name
                ),
                participants: vec![snap.id],
                mood_changes: Vec::new(),
                relationship_changes: Vec::new(),
            });
        }
        return None;
    }

    let prompt = build_tier2_prompt(group, time_desc, weather, language);
    let participant_ids: Vec<NpcId> = group.npcs.iter().map(|s| s.id).collect();

    let mut last_err: ParishError =
        match try_tier2_inference(client, model, &prompt, None, cancel.clone()).await {
            Ok(resp) => {
                if let Some(error) = tier2_grounding_error(group, &resp.summary) {
                    error
                } else {
                    return Some(Tier2Event {
                        location: group.location,
                        summary: resp.summary,
                        participants: participant_ids,
                        mood_changes: resp.mood_changes,
                        relationship_changes: resp.relationship_changes,
                    });
                }
            }
            Err(e) => e,
        };

    // Retry exactly once on malformed JSON (see #27) or a canonical-location
    // conflict (#1785). Cancellation and transport errors fall through to the
    // diagnostic block below.
    let msg = last_err.to_string();
    let parse_failure = is_tier2_json_parse_failure(&msg);
    let grounding_failure = is_tier2_grounding_failure(&msg);
    if !is_intentional_cancellation(&msg) && (parse_failure || grounding_failure) {
        if parse_failure {
            record_tier2_parse_failure(); // see #29
        }
        tracing::debug!(
            "Tier 2 response rejected at {}, retrying once: {}",
            group.location_name,
            msg
        );
        let grounding_reminder = format!(
            "\n\nCANONICAL LOCATION CORRECTION — every participant is at {}. \
             Describe only actions possible there and consistent with the authored \
             activities above. Do not say anyone is at home and do not name another place.",
            group.location_name
        );
        let retry_prompt = if parse_failure {
            format!("{}{}", prompt, TIER2_STRICT_JSON_REMINDER)
        } else {
            format!("{prompt}{grounding_reminder}")
        };
        // TD-033: the retry's whole premise is the small model emitting
        // malformed JSON, so pull the strongest lever the `_with_format`
        // variant exists for — set a provider-side JSON response format so
        // capable backends constrain output to JSON. Anthropic / Simulator
        // ignore it and fall back to plain streaming, matching prior behavior.
        match try_tier2_inference(
            client,
            model,
            &retry_prompt,
            Some(parish_inference::ResponseFormat::JsonObject),
            cancel,
        )
        .await
        {
            Ok(resp) => {
                if let Some(error) = tier2_grounding_error(group, &resp.summary) {
                    last_err = error;
                } else {
                    tracing::debug!("Tier 2 retry succeeded at {}", group.location_name);
                    return Some(Tier2Event {
                        location: group.location,
                        summary: resp.summary,
                        participants: participant_ids,
                        mood_changes: resp.mood_changes,
                        relationship_changes: resp.relationship_changes,
                    });
                }
            }
            Err(e) => {
                // Retry also failed — count again if it was another parse
                // failure (see #29). Cancellation between attempts will
                // fall through to the diagnostic block without counting.
                if is_tier2_json_parse_failure(&e.to_string()) {
                    record_tier2_parse_failure();
                }
                last_err = e;
            }
        }
    }

    {
        let msg = last_err.to_string();
        if is_intentional_cancellation(&msg) {
            // Graceful cancellation (shutdown, demo turn cap). Not a failure.
            tracing::debug!("Tier 2 cancelled at {}: {}", group.location_name, msg);
        } else {
            tracing::error!(
                "Tier 2 inference failed at {}: {}",
                group.location_name,
                msg
            );
        }
        None
    }
}

// ── event application ──────────────────────────────────────────────────────

/// Applies a Tier 2 event's effects to the relevant NPCs using the given config.
///
/// Updates moods, adjusts relationship strengths, and records memories
/// for all participating NPCs.
///
/// Returns debug event strings describing what happened.
pub fn apply_tier2_event_with_config(
    event: &Tier2Event,
    npcs: &mut HashMap<NpcId, Npc>,
    game_time: DateTime<Utc>,
    config: &NpcConfig,
    event_bus: &parish_types::events::EventBus,
) -> Vec<String> {
    let mut debug_events = Vec::new();

    // #1027: the Tier 2 LLM sometimes pulls absent characters into a scene
    // (from gossip / relationship context / its training prior). If the
    // summary names an NPC who isn't a participant, treat the whole narrative
    // beat as untrusted — don't publish it, don't commit it to memory, and
    // (via `tier2_summary_mentions_absent_npc`) don't let the caller gossip it.
    // The mechanical deltas below are still applied, but only for actual
    // participants, since Tier 2 cognition requires co-location.
    let absent_npc = summary_mentions_absent_npc(&event.summary, &event.participants, npcs);

    // Publish the narrative beat before mutating state so the bus carries
    // the story before downstream subscribers see deltas.
    if !event.summary.trim().is_empty() {
        if let Some(absent) = &absent_npc {
            tracing::warn!(
                location = event.location.0,
                absent_npc = %absent,
                "Tier 2 summary named an NPC absent from the scene; dropping interaction beat"
            );
            debug_events.push(format!(
                "Tier 2 interaction dropped: summary named absent NPC '{absent}'"
            ));
        } else {
            event_bus.publish(parish_types::events::GameEvent::NpcInteraction {
                participants: event.participants.clone(),
                location: event.location,
                summary: event.summary.clone(),
                timestamp: game_time,
            });
        }
    }

    // Apply mood changes — only for scene participants. Tier 2 cognition is a
    // co-located group activity, so a delta for a non-participant id is an LLM
    // hallucination; applying it would also mis-file the mood entry under the
    // scene's `event.location` rather than that NPC's real location (#1027).
    for mc in &event.mood_changes {
        if !event.participants.contains(&mc.npc_id) {
            continue;
        }
        if let Some(npc) = npcs.get_mut(&mc.npc_id)
            && npc.mood != mc.new_mood
        {
            debug_events.push(format!(
                "{} mood: {} -> {}",
                npc.name, npc.mood, mc.new_mood
            ));
            npc.mood = mc.new_mood.clone();
            event_bus.publish(parish_types::events::GameEvent::MoodChanged {
                npc_id: mc.npc_id,
                new_mood: mc.new_mood.clone(),
                location: event.location,
                timestamp: game_time,
            });
        }
    }

    // Apply relationship changes — only between two scene participants, for the
    // same co-location reason as mood changes above (#1027).
    for rc in &event.relationship_changes {
        if rc.delta == 0.0 {
            continue;
        }
        if !event.participants.contains(&rc.from) || !event.participants.contains(&rc.to) {
            continue;
        }
        if let Some(npc) = npcs.get_mut(&rc.from)
            && let Some(rel) = npc.relationships.get_mut(&rc.to)
        {
            rel.adjust_strength(rc.delta);
            event_bus.publish(parish_types::events::GameEvent::RelationshipChanged {
                npc_a: rc.from,
                npc_b: rc.to,
                delta: rc.delta,
                timestamp: game_time,
            });
        }
    }

    // Record memory for all participants — but skip a hallucinated summary so
    // the absent NPC's name can't re-enter model context via memory prompts
    // (#1027). Mechanical deltas above already landed.
    if absent_npc.is_none() {
        let memory_content = truncate_for_memory(&event.summary, config.event_summary_truncation);
        // Log the memory commit for all participants
        for &pid in &event.participants {
            if let Some(npc) = npcs.get(&pid) {
                debug_events.push(format!(
                    "{} remembers: {}",
                    npc.name,
                    truncate_for_memory(&event.summary, config.event_summary_debug_truncation)
                ));
            }
        }
        for &participant_id in &event.participants {
            if let Some(npc) = npcs.get_mut(&participant_id) {
                // Record the first *other* participant as the conversation partner.
                // For two-NPC conversations this is unambiguous; for larger groups
                // we store the first other participant as a representative.
                let partner = event
                    .participants
                    .iter()
                    .copied()
                    .find(|&p| p != participant_id);
                let mem_entry = MemoryEntry {
                    timestamp: game_time,
                    content: memory_content.clone(),
                    participants: event.participants.clone(),
                    location: event.location,
                    kind: partner.map(crate::memory::MemoryKind::SpokeWithNpc),
                };
                if let Some(evicted) = npc.memory.add(mem_entry) {
                    let npc_name = npc.name.clone();
                    try_promote(&mut npc.long_term_memory, &evicted, &[npc_name], "");
                }
            }
        }
    }

    debug_events
}

/// Applies a Tier 2 event's effects to the relevant NPCs.
///
/// Updates moods, adjusts relationship strengths, and records memories
/// for all participating NPCs.
///
/// Returns debug event strings describing what happened.
#[cfg(test)]
pub(crate) fn apply_tier2_event(
    event: &Tier2Event,
    npcs: &mut HashMap<NpcId, Npc>,
    game_time: DateTime<Utc>,
) -> Vec<String> {
    apply_tier2_event_with_config(
        event,
        npcs,
        game_time,
        &NpcConfig::default(),
        &parish_types::events::EventBus::new(),
    )
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{make_named_npc, make_scheduled_npc, make_test_npc};
    use crate::types::{MoodChange, Relationship, RelationshipChange, RelationshipKind};
    use chrono::TimeZone;
    use parish_types::events::{EventBus, GameEvent};

    fn named_npc(id: u32, name: &str, location: u32) -> Npc {
        make_named_npc(id, name, location)
    }

    /// Regression test (fixed: #27) — JSON parse failures discriminate cleanly from other
    /// error shapes so the retry only fires for the intended failure
    /// mode.
    #[test]
    fn test_is_tier2_json_parse_failure_discriminator() {
        // Positive cases — the exact string `run_tier2_for_group`
        // constructs via `format!("Tier 2 JSON parse failed: {e}")`.
        assert!(is_tier2_json_parse_failure(
            "Tier 2 JSON parse failed: key must be a string at line 2 column 3"
        ));
        assert!(is_tier2_json_parse_failure(
            "inference error: Tier 2 JSON parse failed: expected value"
        ));

        // Negative cases — every other error path must NOT trigger the
        // retry.
        assert!(!is_tier2_json_parse_failure("Tier 2 cancelled mid-stream"));
        assert!(!is_tier2_json_parse_failure(
            "Tier 3 JSON parse failed: ..." // different tier
        ));
        assert!(!is_tier2_json_parse_failure("connection refused"));
        assert!(!is_tier2_json_parse_failure("timeout after 600s"));
        assert!(!is_tier2_json_parse_failure(""));
    }

    #[test]
    fn test_tier2_strict_json_reminder_carries_required_anchors() {
        // The reminder must explicitly disable common 1.5B failure
        // modes the demo audit observed: unquoted keys, markdown
        // fences, prose around the JSON object.
        let r = TIER2_STRICT_JSON_REMINDER;
        assert!(r.contains("STRICT JSON ONLY"), "missing strict-only:\n{r}");
        assert!(
            r.contains("No markdown fences"),
            "missing fence guard:\n{r}"
        );
        assert!(
            r.contains("double-quoted strings"),
            "missing quoted-key guard:\n{r}"
        );
        // The exact envelope shape must appear so the model has a
        // concrete target.
        assert!(
            r.contains(r#"{"summary": "...", "mood_changes": [], "relationship_changes": []}"#),
            "missing envelope template:\n{r}"
        );
    }

    /// Regression test (fixed: #54) — Tier 3 cancellation discriminator. The Tier 2 path
    /// has long distinguished "cancelled mid-stream" (graceful preempt)
    /// from real failures; this test pins the shared helper used by
    /// both tiers so neither regresses to WARN-on-cancel.
    #[test]
    fn test_is_intentional_cancellation_recognises_cancel_messages() {
        assert!(is_intentional_cancellation(
            "inference error: Tier 3 cancelled mid-stream"
        ));
        assert!(is_intentional_cancellation(
            "inference error: Tier 2 cancelled mid-stream"
        ));
        assert!(is_intentional_cancellation("Tier 3 cancelled mid-stream"));
    }

    #[test]
    fn test_is_intentional_cancellation_rejects_real_failures() {
        assert!(!is_intentional_cancellation(
            "Tier 3 JSON parse failed: expected value at line 2 column 3"
        ));
        assert!(!is_intentional_cancellation(
            "connection refused (os error 61)"
        ));
        assert!(!is_intentional_cancellation("timeout after 600s"));
        assert!(!is_intentional_cancellation(""));
    }

    #[test]
    fn test_build_tier2_prompt() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            other_location_names: vec!["The Mill".to_string()],
            npcs: vec![
                NpcSnapshot {
                    id: NpcId(1),
                    name: "Padraig".to_string(),
                    occupation: "Publican".to_string(),
                    personality: "Warm".to_string(),
                    pronouns: "he/him".to_string(),
                    intelligence_prose: "Perceptive, wise, quick-witted.".to_string(),
                    mood: "content".to_string(),
                    relationship_summary: "friendly with Tommy".to_string(),
                    current_activity: Some("tending bar".to_string()),
                },
                NpcSnapshot {
                    id: NpcId(5),
                    name: "Tommy".to_string(),
                    occupation: "Retired Farmer".to_string(),
                    personality: "Storyteller".to_string(),
                    pronouns: "they/them".to_string(),
                    intelligence_prose: "Well-spoken and brilliantly creative.".to_string(),
                    mood: "reflective".to_string(),
                    relationship_summary: String::new(),
                    current_activity: Some("sharing a story".to_string()),
                },
            ],
        };

        let lang = LanguageSettings::english_only();
        let prompt = build_tier2_prompt(&group, "Evening", "Overcast", &lang);
        assert!(prompt.contains("Darcy's Pub"));
        assert!(prompt.contains("Dramatis personae"));
        // #1026: each dramatis-personae line carries the NPC's pronouns.
        assert!(prompt.contains("[1] Padraig (he/him), Publican"));
        assert!(prompt.contains("[5] Tommy (they/them), Retired Farmer"));
        // #1027: the prompt forbids naming characters outside the roster.
        assert!(prompt.contains("Only name characters listed in the dramatis personae"));
        assert!(prompt.contains("Currently content"));
        assert!(prompt.contains("Perceptive, wise"));
        assert!(prompt.contains("friendly with Tommy"));
        assert!(prompt.contains("CANONICAL LOCATION"));
        assert!(prompt.contains("Authored activity at this exact location: tending bar"));
        assert!(prompt.contains("occupation alone is not evidence that they are at work"));
        assert!(prompt.contains("Evening"));
        assert!(prompt.contains("Overcast"));
        assert!(prompt.contains("summary"));
        // No more cryptic encoding
        assert!(!prompt.contains("INT["));

        assert!(
            tier2_summary_location_conflict(
                &group,
                "At Darcy's Pub, Padraig tends bar while Tommy shares a story."
            )
            .is_none()
        );
        assert!(
            tier2_summary_location_conflict(
                &group,
                "Padraig pours a drink while Tommy waits by The Mill."
            )
            .is_some()
        );
        assert!(tier2_summary_location_conflict(&group, "Tommy rests at home.").is_some());
    }

    #[test]
    fn tier2_snapshot_uses_only_activity_at_actual_location() {
        let mut npc = make_scheduled_npc(1, 1, 2);
        let names = HashMap::from([(npc.id, npc.name.clone())]);

        npc.location = LocationId(2);
        let at_work = npc_snapshot_from_npc_at(&npc, &names, 10, Season::Spring, DayType::Weekday);
        assert_eq!(at_work.current_activity.as_deref(), Some("working"));

        npc.location = LocationId(3);
        let diverted = npc_snapshot_from_npc_at(&npc, &names, 10, Season::Spring, DayType::Weekday);
        assert_eq!(diverted.current_activity, None);
    }

    #[test]
    fn test_apply_tier2_event() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc1 = named_npc(1, "Padraig", 2);
        npc1.relationships
            .insert(NpcId(5), Relationship::new(RelationshipKind::Friend, 0.5));
        npcs.insert(NpcId(1), npc1);
        npcs.insert(NpcId(5), named_npc(5, "Tommy", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Padraig and Tommy shared stories over a pint".to_string(),
            participants: vec![NpcId(1), NpcId(5)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(1),
                new_mood: "jovial".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: NpcId(1),
                to: NpcId(5),
                delta: 0.1,
            }],
        };

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        apply_tier2_event(&event, &mut npcs, game_time);

        // Check mood updated
        assert_eq!(npcs.get(&NpcId(1)).unwrap().mood, "jovial");

        // Check relationship adjusted
        let rel = npcs
            .get(&NpcId(1))
            .unwrap()
            .relationships
            .get(&NpcId(5))
            .unwrap();
        assert!((rel.strength - 0.6).abs() < f64::EPSILON);

        // Check memories recorded for both
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 1);
        assert_eq!(npcs.get(&NpcId(5)).unwrap().memory.len(), 1);
    }

    #[test]
    fn tier2_drops_summary_naming_absent_npc() {
        let config = NpcConfig::default();
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();

        fn count_interactions(rx: &mut tokio::sync::broadcast::Receiver<GameEvent>) -> usize {
            let mut n = 0;
            while let Ok(ev) = rx.try_recv() {
                if matches!(ev, GameEvent::NpcInteraction { .. }) {
                    n += 1;
                }
            }
            n
        }

        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), named_npc(1, "Padraig Darcy", 2));
        npcs.insert(NpcId(5), named_npc(5, "Tommy O'Brien", 2));
        // Aoife is authored elsewhere — not part of this scene.
        npcs.insert(NpcId(9), named_npc(9, "Aoife Brennan", 3));

        // Summary names absent Aoife, plus a mood delta for absent Aoife.
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let hallucinated = Tier2Event {
            location: LocationId(2),
            summary: "Padraig pours a pint while Aoife Brennan chats nearby".to_string(),
            participants: vec![NpcId(1), NpcId(5)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(9),
                new_mood: "furious".to_string(),
            }],
            relationship_changes: vec![],
        };
        assert!(
            tier2_summary_mentions_absent_npc(&hallucinated, &npcs),
            "predicate must flag the hallucinated summary",
        );
        apply_tier2_event_with_config(&hallucinated, &mut npcs, game_time, &config, &bus);
        assert_eq!(
            count_interactions(&mut rx),
            0,
            "summary naming an absent NPC must not publish an NpcInteraction",
        );
        // The hallucinated summary is not committed to participant memory.
        assert_eq!(
            npcs.get(&NpcId(1)).unwrap().memory.len(),
            0,
            "a hallucinated summary must not enter memory",
        );
        // A mood delta for a non-participant is filtered out, not applied.
        assert_eq!(
            npcs.get(&NpcId(9)).unwrap().mood,
            "calm",
            "mood deltas for non-participants must be dropped",
        );

        // A clean summary naming only participants publishes normally and is
        // remembered.
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let clean = Tier2Event {
            location: LocationId(2),
            summary: "Padraig and Tommy swap stories over a pint".to_string(),
            participants: vec![NpcId(1), NpcId(5)],
            mood_changes: vec![],
            relationship_changes: vec![],
        };
        assert!(
            !tier2_summary_mentions_absent_npc(&clean, &npcs),
            "predicate must pass a clean summary",
        );
        apply_tier2_event_with_config(&clean, &mut npcs, game_time, &config, &bus);
        assert_eq!(
            count_interactions(&mut rx),
            1,
            "a clean summary should publish exactly one NpcInteraction",
        );
        assert_eq!(
            npcs.get(&NpcId(1)).unwrap().memory.len(),
            1,
            "a clean summary should be committed to participant memory",
        );
    }

    #[test]
    fn test_apply_tier2_event_with_config_truncation() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), named_npc(1, "Padraig", 2));

        let long_summary = "a".repeat(200);
        let event = Tier2Event {
            location: LocationId(2),
            summary: long_summary,
            participants: vec![NpcId(1)],
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
        };

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let config = NpcConfig {
            event_summary_truncation: 40,
            event_summary_debug_truncation: 20,
            ..NpcConfig::default()
        };

        let events =
            apply_tier2_event_with_config(&event, &mut npcs, game_time, &config, &EventBus::new());
        assert!(!events.is_empty());

        // The stored memory content should be truncated to ~40 chars
        let mem = &npcs.get(&NpcId(1)).unwrap().memory;
        let recent = mem.recent(1);
        assert!(recent[0].content.len() <= 40);
    }

    #[test]
    fn test_apply_tier2_event_missing_npc_in_map() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), named_npc(1, "Padraig", 2));
        // NpcId(99) is NOT in the map

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Something happened".to_string(),
            participants: vec![NpcId(1), NpcId(99)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(99),
                new_mood: "happy".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: NpcId(99),
                to: NpcId(1),
                delta: 0.1,
            }],
        };

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        // Should not panic — missing NPCs are silently skipped
        let events = apply_tier2_event(&event, &mut npcs, game_time);
        // Padraig still gets a memory
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 1);
        // Some events generated for the NPC that exists
        assert!(!events.is_empty());
    }

    #[test]
    fn test_apply_tier2_event_empty_participants() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), named_npc(1, "Padraig", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Nothing happened".to_string(),
            participants: Vec::new(),
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
        };

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier2_event(&event, &mut npcs, game_time);
        assert!(events.is_empty());
        // No memories added
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 0);
    }

    #[test]
    fn test_apply_tier2_event_same_mood_no_debug_event() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc = named_npc(1, "Padraig", 2);
        npc.mood = "calm".to_string();
        npcs.insert(NpcId(1), npc);

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Padraig sits quietly".to_string(),
            participants: vec![NpcId(1)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(1),
                new_mood: "calm".to_string(), // same as current
            }],
            relationship_changes: Vec::new(),
        };

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier2_event(&event, &mut npcs, game_time);
        // No mood change event since mood didn't actually change
        assert!(!events.iter().any(|e| e.contains("mood:")));
        // But memory event should still be there
        assert!(events.iter().any(|e| e.contains("remembers:")));
    }

    #[test]
    fn test_apply_tier2_event_relationship_not_found() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        // Padraig has no relationship with Tommy
        npcs.insert(NpcId(1), named_npc(1, "Padraig", 2));
        npcs.insert(NpcId(5), named_npc(5, "Tommy", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "They chat".to_string(),
            participants: vec![NpcId(1), NpcId(5)],
            mood_changes: Vec::new(),
            relationship_changes: vec![RelationshipChange {
                from: NpcId(1),
                to: NpcId(5),
                delta: 0.1,
            }],
        };

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        // Should not panic — missing relationship is silently skipped
        let _events = apply_tier2_event(&event, &mut npcs, game_time);
        // Both still get memories
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 1);
        assert_eq!(npcs.get(&NpcId(5)).unwrap().memory.len(), 1);
    }

    // --- run_tier2_for_group solo NPC ---

    #[tokio::test]
    async fn test_run_tier2_solo_npc_template() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            other_location_names: vec!["The Mill".to_string()],
            npcs: vec![NpcSnapshot {
                id: NpcId(1),
                name: "Padraig".to_string(),
                occupation: "Publican".to_string(),
                personality: "Warm".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: "Perceptive, wise, quick-witted.".to_string(),
                mood: "content".to_string(),
                relationship_summary: String::new(),
                current_activity: Some("tending bar".to_string()),
            }],
        };

        // Solo NPC short-circuits before any LLM call — the simulator client
        // satisfies the type and never gets called.
        let client = parish_inference::AnyClient::simulator();
        let lang = LanguageSettings::english_only();
        let event =
            run_tier2_for_group(&client, "test", &group, "Morning", "Clear", &lang, None).await;
        assert!(event.is_some());
        let event = event.unwrap();
        assert!(event.summary.contains("Padraig"));
        assert!(event.summary.contains("Darcy's Pub"));
        assert_eq!(event.participants, vec![NpcId(1)]);
        assert!(event.mood_changes.is_empty());
        assert!(event.relationship_changes.is_empty());
    }

    #[tokio::test]
    async fn test_run_tier2_empty_group_returns_none() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            other_location_names: Vec::new(),
            npcs: Vec::new(),
        };

        // Empty group short-circuits before any LLM call.
        let client = parish_inference::AnyClient::simulator();
        let lang = LanguageSettings::english_only();
        let event =
            run_tier2_for_group(&client, "test", &group, "Morning", "Clear", &lang, None).await;
        assert!(event.is_none());
    }

    /// TD-033: the Tier 2 first attempt sends no `response_format` (None) while
    /// the retry — whose premise is the small model emitting malformed JSON —
    /// must set a provider-side JSON `response_format` so JSON mode activates.
    /// Drives `try_tier2_inference` directly against a recording mock and
    /// asserts the per-attempt format flags.
    #[tokio::test]
    async fn test_tier2_retry_sets_json_response_format() {
        let (client, mock) = parish_inference::AnyClient::mock();

        // First attempt: production passes `None` (preserves prior behavior).
        let _ = try_tier2_inference(&client, "test-model", "first prompt", None, None).await;
        // Retry: production passes a JSON response format.
        let _ = try_tier2_inference(
            &client,
            "test-model",
            "retry prompt",
            Some(parish_inference::ResponseFormat::JsonObject),
            None,
        )
        .await;

        let log = mock.response_format_log();
        assert_eq!(
            log,
            vec![false, true],
            "first attempt sends no response_format, retry sends a JSON response_format (TD-033)"
        );
    }

    // --- build_tier2_prompt weather commentary ---

    #[test]
    fn test_build_tier2_prompt_rain_commentary() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "The Crossroads".to_string(),
            other_location_names: vec!["The Mill".to_string(), "The Forge".to_string()],
            npcs: vec![
                NpcSnapshot {
                    id: NpcId(1),
                    name: "Padraig".to_string(),
                    occupation: "Publican".to_string(),
                    personality: "Warm".to_string(),
                    pronouns: "he/him".to_string(),
                    intelligence_prose: String::new(),
                    mood: "calm".to_string(),
                    relationship_summary: String::new(),
                    current_activity: Some("waiting at the crossroads".to_string()),
                },
                NpcSnapshot {
                    id: NpcId(2),
                    name: "Tommy".to_string(),
                    occupation: "Farmer".to_string(),
                    personality: "Gruff".to_string(),
                    pronouns: "he/him".to_string(),
                    intelligence_prose: "Plain-spoken.".to_string(),
                    mood: "tired".to_string(),
                    relationship_summary: String::new(),
                    current_activity: Some("resting by the wall".to_string()),
                },
            ],
        };

        let lang = LanguageSettings::english_only();
        let prompt = build_tier2_prompt(&group, "Afternoon", "Heavy Rain", &lang);
        assert!(prompt.contains("commenting on the weather"));

        let prompt = build_tier2_prompt(&group, "Afternoon", "Clear", &lang);
        assert!(!prompt.contains("commenting on the weather"));
    }

    #[test]
    fn test_tier2_prompt_omits_intelligence_when_average() {
        // An average NPC contributes no intelligence prose — the line should
        // still be valid without trailing whitespace.
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            other_location_names: Vec::new(),
            npcs: vec![NpcSnapshot {
                id: NpcId(1),
                name: "Padraig".to_string(),
                occupation: "Publican".to_string(),
                personality: "Warm".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: String::new(),
                mood: "content".to_string(),
                relationship_summary: String::new(),
                current_activity: None,
            }],
        };
        let lang = LanguageSettings::english_only();
        let prompt = build_tier2_prompt(&group, "Morning", "Clear", &lang);
        // Line ends with the mood and a period, no trailing spaces.
        assert!(prompt.contains("Currently content."));
        // No mention of relationship summary line.
        assert!(!prompt.contains("friendly with"));
    }

    #[test]
    fn test_spoke_with_npc_memory_records_partner_not_self() {
        const PADRAIG: NpcId = NpcId(1);
        const TOMMY: NpcId = NpcId(5);
        const LOCATION: LocationId = LocationId(2);

        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(PADRAIG, named_npc(1, "Padraig", 2));
        npcs.insert(TOMMY, named_npc(5, "Tommy", 2));

        let event = Tier2Event {
            location: LOCATION,
            summary: "Padraig and Tommy exchanged news".to_string(),
            participants: vec![PADRAIG, TOMMY],
            mood_changes: vec![],
            relationship_changes: vec![],
        };

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 14, 0, 0).unwrap();
        apply_tier2_event(&event, &mut npcs, game_time);

        let padraig_mem = npcs.get(&PADRAIG).unwrap().memory.recent(1);
        let tommy_mem = npcs.get(&TOMMY).unwrap().memory.recent(1);

        assert_eq!(
            padraig_mem[0].kind,
            Some(crate::memory::MemoryKind::SpokeWithNpc(TOMMY)),
            "Padraig's memory should reference Tommy, not himself"
        );
        assert_eq!(
            tommy_mem[0].kind,
            Some(crate::memory::MemoryKind::SpokeWithNpc(PADRAIG)),
            "Tommy's memory should reference Padraig, not himself"
        );
    }

    #[test]
    fn test_top_relationships_sorted_by_absolute_strength() {
        // Iteration over `npc.relationships` (a HashMap) is non-deterministic.
        // top_relationships must surface the strongest |strength| values in a
        // stable order so the prompt is reproducible across runs.
        let mut npc = make_test_npc(1, 2);
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.3));
        npc.relationships
            .insert(NpcId(3), Relationship::new(RelationshipKind::Enemy, -0.9));
        npc.relationships
            .insert(NpcId(4), Relationship::new(RelationshipKind::Neighbor, 0.1));
        npc.relationships
            .insert(NpcId(5), Relationship::new(RelationshipKind::Friend, 0.6));

        let top = top_relationships(&npc, 3);
        assert_eq!(top.len(), 3);
        // |0.9| > |0.6| > |0.3| > |0.1| — bottom one (NpcId 4) is dropped.
        assert_eq!(top[0].0, NpcId(3));
        assert_eq!(top[1].0, NpcId(5));
        assert_eq!(top[2].0, NpcId(2));
    }

    #[test]
    fn test_top_relationships_tiebreaks_by_id() {
        let mut npc = make_test_npc(1, 2);
        // Three relationships at identical |strength| — order must still be
        // deterministic (ascending NpcId).
        npc.relationships
            .insert(NpcId(7), Relationship::new(RelationshipKind::Friend, 0.5));
        npc.relationships
            .insert(NpcId(3), Relationship::new(RelationshipKind::Friend, -0.5));
        npc.relationships
            .insert(NpcId(5), Relationship::new(RelationshipKind::Friend, 0.5));

        let top = top_relationships(&npc, 3);
        assert_eq!(top[0].0, NpcId(3));
        assert_eq!(top[1].0, NpcId(5));
        assert_eq!(top[2].0, NpcId(7));
    }
}
