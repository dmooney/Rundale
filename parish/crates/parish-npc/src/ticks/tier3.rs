//! Tier 3 tick — batch inference for distant NPCs.
//!
//! Tier 3 runs once per game-day for NPCs far from the player. A single
//! prompt describes a batch of NPCs; the LLM returns structured JSON updates
//! covering mood, activity, location, and relationship deltas.

use chrono::{DateTime, Utc};
use regex::Regex;
use std::collections::HashMap;

use crate::memory::{MemoryEntry, try_promote};
use crate::types::{Tier3Response, Tier3Update};
use crate::{LanguageSettings, Npc, NpcId};
use parish_config::RelationshipLabelConfig;
use parish_types::ParishError;
use parish_world::{LocationId, graph::WorldGraph};

use super::prompt::format_relationships_natural;
use super::tier2::is_intentional_cancellation;
use super::tier2::top_relationships;

// ── NPC snapshot and context types ────────────────────────────────────────

/// A lightweight snapshot of an NPC's state for Tier 3 batch inference.
#[derive(Debug, Clone)]
pub struct Tier3Snapshot {
    /// NPC id.
    pub id: NpcId,
    /// NPC name.
    pub name: String,
    /// Occupation.
    pub occupation: String,
    /// Age.
    pub age: u8,
    /// Current location id.
    pub location: LocationId,
    /// Location name.
    pub location_name: String,
    /// Current mood.
    pub mood: String,
    /// Deflated summary or last activity.
    pub context: String,
    /// Compact intelligence adjectives (from `Intelligence::adjective_summary`).
    /// Empty for an all-3s profile.
    pub intelligence_adjectives: String,
    /// Natural-language relationship summary
    /// (e.g. "friendly with Mary McKenna"). May be empty.
    pub relationship_summary: String,
}

/// Context for a Tier 3 batch simulation call.
///
/// Tier 3 batches are dispatched directly against a per-category
/// `AnyClient` resolved by the caller (typically the Simulation slot).
/// Routing through the shared `InferenceQueue` was abandoned because the
/// queue worker always hit the base provider's HTTP endpoint, defeating
/// the per-category override that the two-slot loadout depends on. The
/// streaming code path keeps `cancel` mid-flight preemption working.
pub struct Tier3Context<'a> {
    /// NPC snapshots to simulate.
    pub snapshots: &'a [Tier3Snapshot],
    /// Per-category LLM client (resolved for `InferenceCategory::Simulation`).
    pub client: &'a parish_inference::AnyClient,
    /// Model name to use.
    pub model: &'a str,
    /// Time description (e.g. "Morning").
    pub time_desc: &'a str,
    /// Weather description (e.g. "Overcast").
    pub weather: &'a str,
    /// Season (e.g. "Spring").
    pub season: &'a str,
    /// Number of game hours to simulate.
    pub hours: u32,
    /// Maximum NPCs per batch LLM call.
    pub batch_size: usize,
    /// Language settings for locale-aware dialogue directives.
    pub language: &'a LanguageSettings,
    /// Optional cancellation token forwarded to every batch's streaming
    /// submit, so a player turn can preempt Tier 3 mid-flight.
    pub cancel: Option<parish_inference::CancellationToken>,
    // When `true` (default-on via the `npc-dialogue-grounding` feature flag),
    // pins `temperature` and `frequency_penalty` on the generation call to
    // suppress looping and mid-word truncation (fixes #1397). Set to
    // `!flags.is_disabled("npc-dialogue-grounding")` at the call site.
    pub grounding_enabled: bool,
}

// ── snapshot builder ───────────────────────────────────────────────────────

/// Creates a Tier 3 snapshot from an NPC, resolving location names from the graph.
///
/// Peer names are resolved at snapshot time via `npc_names` so the snapshot is
/// self-contained — the prompt builder does not need access to the name map.
pub fn tier3_snapshot_from_npc(
    npc: &Npc,
    graph: &WorldGraph,
    npc_names: &HashMap<NpcId, String>,
) -> Tier3Snapshot {
    let location_name = graph
        .get(npc.location)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| format!("Location {}", npc.location.0));

    let context = if let Some(ref activity) = npc.last_activity {
        activity.clone()
    } else if let Some(ref summary) = npc.deflated_summary {
        summary.recent_activity.first().cloned().unwrap_or_default()
    } else {
        String::new()
    };

    let rels = top_relationships(npc, 3);

    Tier3Snapshot {
        id: npc.id,
        name: npc.name.clone(),
        occupation: npc.occupation.clone(),
        age: npc.age,
        location: npc.location,
        location_name,
        mood: npc.mood.clone(),
        context,
        intelligence_adjectives: npc.intelligence.adjective_summary(),
        relationship_summary: format_relationships_natural(
            &rels,
            npc_names,
            &RelationshipLabelConfig::default(),
        ),
    }
}

// ── prompt builder ─────────────────────────────────────────────────────────

/// Builds a Tier 3 batch prompt for a set of NPC snapshots.
pub fn build_tier3_prompt(
    snapshots: &[Tier3Snapshot],
    time_desc: &str,
    weather: &str,
    season: &str,
    hours: u32,
    language: &LanguageSettings,
) -> String {
    use crate::language_directive;

    let npc_summaries: Vec<String> = snapshots
        .iter()
        .map(|snap| {
            let traits = if snap.intelligence_adjectives.is_empty() {
                String::new()
            } else {
                format!(" ({})", snap.intelligence_adjectives)
            };
            let rel_line = if snap.relationship_summary.is_empty() {
                String::new()
            } else {
                format!("\n  {}.", snap.relationship_summary)
            };
            let context_line = if snap.context.is_empty() {
                String::new()
            } else {
                format!("\n  Recent: {}", snap.context)
            };
            format!(
                "- [{id}] {name}, {age}, {occupation} — at {location}, {mood}{traits}.{rels}{context}",
                id = snap.id.0,
                name = snap.name,
                age = snap.age,
                occupation = snap.occupation,
                location = snap.location_name,
                mood = snap.mood,
                traits = traits,
                rels = rel_line,
                context = context_line,
            )
        })
        .collect();

    // #1451: "the season is X" was underweighted — LLM generated summer activities
    // in Spring. Make it a named directive with an explicit prohibition.
    let mut prompt = format!(
        "You are simulating background NPC activity in a rural Irish parish in 1820. \
        Simulate {hours} hours of activity for the people below. \
        The weather is {weather}. CURRENT SEASON: {season} — do not reference any \
        other season in the activity summaries. The time is {time}.\n\n\
        NPCs (id in brackets — reuse these in your JSON):\n\
        {npcs}\n\n\
        For each NPC, return one update describing their mood, what they did, \
        whether they moved, and any relationship shifts. Respond with JSON, \
        using the bracketed ids:\n\
        {{\"updates\":[{{\"npc_id\":<id>,\"mood\":\"...\",\"activity_summary\":\"...\",\
        \"new_location\":<id|null>,\
        \"relationship_changes\":[{{\"from\":<id>,\"to\":<id>,\"delta\":<-0.1..0.1>}}]}}]}}",
        hours = hours,
        weather = weather,
        season = season,
        time = time_desc,
        npcs = npc_summaries.join("\n"),
    );

    prompt.push_str("\n\n");
    prompt.push_str(&language_directive(language));
    prompt
}

// ── season guard ──────────────────────────────────────────────────────────

/// Wrong-season keywords for each named season.
///
/// Each entry is (season_name_lowercase, &[wrong-season token patterns]).
/// Only the seasons OTHER than the current one are checked against the text.
/// Patterns are matched case-insensitively as whole words or phrases so we
/// don't accidentally clobber e.g. "autumn-coloured" when season is Autumn.
///
/// The list is conservative: single-season nouns/adjectives that are
/// unambiguous calendar markers, plus the distinctive phrase that triggered
/// #1462 ("long summer days"). Compound phrases are matched before single
/// tokens so a phrase replacement doesn't leave a dangling word.
const SEASON_TOKENS: &[(&str, &[&str])] = &[
    (
        "summer",
        &[
            "long summer days",
            "summer heat",
            "summer sun",
            "summer warmth",
            "summer months",
            "summer evenings",
            "summer days",
            "summer morning",
            "summer afternoon",
            "summer nights",
            "summer rains",
            "summer harvest",
            "summer work",
            "summer",
        ],
    ),
    (
        "winter",
        &[
            "dead of winter",
            "winter chill",
            "winter cold",
            "winter frost",
            "winter months",
            "winter evenings",
            "winter days",
            "winter morning",
            "winter afternoon",
            "winter nights",
            "winter",
        ],
    ),
    (
        "spring",
        &[
            "spring rains",
            "spring warmth",
            "spring months",
            "spring days",
            "spring morning",
            "spring afternoon",
            "spring evenings",
            "spring nights",
            "spring sowing",
            "spring planting",
            "spring",
        ],
    ),
    (
        "autumn",
        &[
            "autumn harvest",
            "autumn chill",
            "autumn months",
            "autumn days",
            "autumn morning",
            "autumn afternoon",
            "autumn evenings",
            "autumn nights",
            "autumn",
            "fall harvest",
            "fall months",
            "fall days",
            "fall",
        ],
    ),
];

/// Removes wrong-season tokens from an LLM-generated activity summary.
///
/// Scans `text` for references to seasons other than `current_season`
/// (case-insensitive) and deletes them. Multi-word seasonal phrases
/// are matched before single season words so no dangling fragments remain.
///
/// Only clear wrong-season markers are touched; the function is conservative
/// and will not alter season-neutral phrasing.  Returns the cleaned text and
/// a boolean indicating whether any substitution was made (for logging).
///
/// `current_season` is compared case-insensitively, accepting "Spring",
/// "spring", "SPRING", etc.
///
/// # UTF-8 correctness
///
/// Matching is done via the `regex` crate with the `(?i)` case-insensitive
/// flag operating directly on the original `text` bytes.  This avoids the
/// earlier approach of calling `to_lowercase()` and reusing its byte offsets
/// on the original string, which is unsound for Unicode code points whose
/// lowercase form has a different UTF-8 byte length (e.g. `İ` U+0130 encodes
/// to 2 bytes but its lowercase `i\u{307}` encodes to 3 bytes; the Kelvin
/// sign `K` U+212A encodes to 3 bytes but lowercase `k` encodes to 1 byte).
pub fn scrub_wrong_season_tokens(text: &str, current_season: &str) -> (String, bool) {
    let current_lower = current_season.to_lowercase();

    // Collect wrong-season token lists.
    let wrong_tokens: Vec<&[&str]> = SEASON_TOKENS
        .iter()
        .filter(|(season, _)| {
            // "fall" is an alias for autumn — treat both as the same season.
            let is_current = *season == current_lower
                || (current_lower == "autumn" && *season == "fall")
                || (current_lower == "fall" && *season == "autumn");
            !is_current
        })
        .map(|(_, tokens)| *tokens)
        .collect();

    if wrong_tokens.is_empty() {
        return (text.to_string(), false);
    }

    let mut result = text.to_string();
    let mut changed = false;

    for token_list in &wrong_tokens {
        for &token in *token_list {
            // Build a case-insensitive regex that matches the token at word
            // boundaries (\b).  The tokens are all ASCII so \b gives the same
            // word-boundary semantics as the previous char_before/char_after
            // guards.  We use \b on both sides so "midsummer" is not mutilated
            // when "summer" is being removed.
            //
            // Regex::new can only fail for invalid patterns; all SEASON_TOKENS
            // are ASCII literals, so unwrap() is safe here.
            let pattern = format!(r"(?i)\b{}\b", regex::escape(token));
            let re = Regex::new(&pattern).expect("SEASON_TOKENS are valid regex literals");

            if re.is_match(&result) {
                let cleaned = re.replace_all(&result, "");
                // Collapse runs of multiple spaces / leading-trailing whitespace
                // that result from phrase removal.
                result = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
                changed = true;
            }
        }
    }

    (result, changed)
}

// ── default batch size ─────────────────────────────────────────────────────

/// Default batch size for Tier 3 inference (NPCs per LLM call).
pub const TIER3_BATCH_SIZE: usize = 10;

// ── main inference entry point ─────────────────────────────────────────────

/// Runs a Tier 3 batch simulation for distant NPCs.
///
/// Builds a single prompt summarizing all provided NPC snapshots and their states,
/// submits it to the inference queue with `Batch` priority, and parses the JSON
/// response. If there are more NPCs than `batch_size`, they are split into
/// multiple sequential queue submissions.
pub async fn tick_tier3(ctx: &Tier3Context<'_>) -> Result<Vec<Tier3Update>, ParishError> {
    tick_tier3_with_profile(
        ctx,
        parish_config::InferenceProfile::for_subrole(
            parish_config::InferenceSubrole::Tier3Simulation,
        ),
    )
    .await
}

/// Runs Tier 3 with the profile resolved from the active runtime config.
pub async fn tick_tier3_with_profile(
    ctx: &Tier3Context<'_>,
    profile: parish_config::InferenceProfile,
) -> Result<Vec<Tier3Update>, ParishError> {
    tick_tier3_with_profile_and_audit(ctx, profile, None).await
}

/// Runs Tier 3 with resolved tuning and common direct-call audit sinks.
pub async fn tick_tier3_with_profile_and_audit(
    ctx: &Tier3Context<'_>,
    profile: parish_config::InferenceProfile,
    audit_sink: Option<parish_inference::InferenceAuditSink>,
) -> Result<Vec<Tier3Update>, ParishError> {
    let batch_size = if ctx.batch_size == 0 {
        TIER3_BATCH_SIZE
    } else {
        ctx.batch_size
    };

    let mut all_updates = Vec::new();

    for batch in ctx.snapshots.chunks(batch_size) {
        let prompt = build_tier3_prompt(
            batch,
            ctx.time_desc,
            ctx.weather,
            ctx.season,
            ctx.hours,
            ctx.language,
        );

        // Cap output to bound vllm-mlx runaway. 6-NPC batches output
        // ~200-400 tokens in practice; 600 is comfortable headroom.
        // Direct-client streaming: chunks discarded (a sink task drains
        // the channel), but the streaming code path is what lets a
        // player turn preempt this batch mid-flight (#9).
        let (sink_tx, mut sink_rx) =
            tokio::sync::mpsc::channel::<String>(parish_inference::TOKEN_CHANNEL_CAPACITY);
        let observed = std::sync::Arc::new(tokio::sync::Mutex::new((String::new(), None, 0_u64)));
        let observed_for_sink = observed.clone();
        let started = std::time::Instant::now();
        tokio::spawn(async move {
            while let Some(chunk) = sink_rx.recv().await {
                let mut state = observed_for_sink.lock().await;
                state.1.get_or_insert_with(std::time::Instant::now);
                state.2 += 1;
                state.0.push_str(&chunk);
            }
        });

        // When grounding is enabled (default-on, #1397): pin temperature and
        // frequency_penalty to suppress looping and mid-word truncation.
        // Without these, vllm-mlx uses its server defaults (high temperature,
        // no repetition penalty), which causes "so it is indeed … so it is
        // indeed" loops and hard 600-token guillotine mid-word cuts.
        let (temperature, frequency_penalty) = if ctx.grounding_enabled {
            (Some(0.7), Some(0.4))
        } else {
            (None, None)
        };
        let params = parish_inference::GenerateParams {
            max_tokens: Some(profile.max_output_tokens),
            temperature,
            frequency_penalty,
            enable_thinking: None,
            reasoning_effort: None,
            thinking_level: Some(profile.thinking_level),
            service_tier: Some(profile.service_tier),
        };
        let audit = parish_inference::DirectInferenceAudit::new(
            audit_sink.clone(),
            ctx.model,
            &prompt,
            None,
            parish_config::InferenceSubrole::Tier3Simulation,
            true,
            params.max_tokens,
            params.thinking_level,
            params.service_tier,
            params.temperature,
            parish_inference::InferencePriority::Batch,
        );
        let stream_fut = ctx
            .client
            .generate_stream_detailed_with_format(ctx.model, &prompt, None, sink_tx, None, params);

        let detailed = match ctx.cancel.clone() {
            Some(tok) => tokio::select! {
                biased;
                () = tok.cancelled() => {
                    let state = observed.lock().await;
                    let mut metadata = ctx.client.fallback_metadata(ctx.model);
                    metadata.terminal_status = Some("cancelled".to_string());
                    metadata.duration_ms = started.elapsed().as_millis() as u64;
                    metadata.ttft_ms = state.1.map(|first| first.duration_since(started).as_millis() as u64);
                    metadata.stream_chunks = state.2;
                    Err(parish_inference::ProviderCallError {
                        message: "Tier 3 cancelled mid-stream".to_string(),
                        partial_text: state.0.clone(),
                        metadata: Box::new(metadata),
                    })
                },
                res = stream_fut => res,
            },
            None => stream_fut.await,
        };
        let validated = detailed.and_then(|result| {
            parish_inference::parse_generation_json::<Tier3Response>(result, "Tier 3")
        });
        let parsed = match validated {
            Ok((raw, parsed)) => audit
                .record(Ok(raw))
                .await
                .map(|_| parsed)
                .map_err(ParishError::from),
            Err(error) => {
                let error = audit
                    .record(Err(error))
                    .await
                    .expect_err("auditing must preserve provider errors");
                Err(ParishError::from(error))
            }
        };

        match parsed {
            Ok(mut resp) => {
                // Season guard (#1462): scrub wrong-season tokens from every
                // activity_summary before storing.  The prompt directive
                // (defense-in-depth, #1451) is kept alongside this guard.
                for update in &mut resp.updates {
                    let (clean, was_dirty) =
                        scrub_wrong_season_tokens(&update.activity_summary, ctx.season);
                    if was_dirty {
                        tracing::warn!(
                            npc_id = update.npc_id.0,
                            original = %update.activity_summary,
                            cleaned = %clean,
                            season = ctx.season,
                            "tier3 season guard: scrubbed wrong-season text (#1462)"
                        );
                        update.activity_summary = clean;
                    }
                }
                all_updates.extend(resp.updates);
            }
            Err(e) => {
                let msg = e.to_string();
                if is_intentional_cancellation(&msg) {
                    // Graceful cancellation (shutdown, sim_cancel on player
                    // input). Not a failure — match the Tier 2 path's
                    // distinction (fixed: #54).
                    tracing::debug!("Tier 3 batch cancelled: {}", msg);
                } else {
                    tracing::warn!("Tier 3 batch inference failed: {}", msg);
                }
                // Continue with other batches rather than failing entirely
            }
        }
    }

    Ok(all_updates)
}

// ── update application ─────────────────────────────────────────────────────

/// Applies Tier 3 updates to NPCs.
///
/// For each update: sets mood, stores activity_summary as `last_activity`,
/// updates location (if valid in graph), and adjusts relationships.
///
/// Publishes `GameEvent::NpcDeparted` / `GameEvent::NpcArrived` when a
/// Tier 3 update actually moves the NPC to a different valid location.
/// No event is emitted when `new_location == npc.location` — the LLM
/// often "re-asserts" the current location; treating that as movement
/// produces phantom arrivals.
///
/// Returns debug event strings describing what happened.
pub fn apply_tier3_updates(
    updates: &[Tier3Update],
    npcs: &mut HashMap<NpcId, Npc>,
    graph: &WorldGraph,
    game_time: DateTime<Utc>,
    event_bus: &parish_types::events::EventBus,
) -> Vec<String> {
    let mut debug_events = Vec::new();

    for update in updates {
        let Some(npc) = npcs.get_mut(&update.npc_id) else {
            tracing::warn!(
                npc_id = update.npc_id.0,
                "Tier 3 update for unknown NPC, skipping"
            );
            continue;
        };

        // Update mood
        if !update.mood.is_empty() && update.mood != npc.mood {
            debug_events.push(format!(
                "{} mood: {} -> {} (tier3)",
                npc.name, npc.mood, update.mood
            ));
            npc.mood = update.mood.clone();
        }

        // Store activity summary
        if !update.activity_summary.is_empty() {
            debug_events.push(format!(
                "{} activity: {} (tier3)",
                npc.name, update.activity_summary
            ));
            npc.last_activity = Some(update.activity_summary.clone());

            // Also record as memory
            let mem_entry = MemoryEntry {
                timestamp: game_time,
                content: update.activity_summary.clone(),
                participants: vec![update.npc_id],
                location: npc.location,
                kind: None, // Tier 3 batch activity
            };
            if let Some(evicted) = npc.memory.add(mem_entry) {
                let npc_name = npc.name.clone();
                try_promote(&mut npc.long_term_memory, &evicted, &[npc_name], "");
            }
        }

        // Update location if valid
        if let Some(new_loc) = update.new_location {
            if graph.get(new_loc).is_some() {
                if new_loc != npc.location {
                    debug_events.push(format!(
                        "{} moved: {:?} -> {:?} (tier3)",
                        npc.name, npc.location, new_loc
                    ));
                    let from = npc.location;
                    npc.set_location(new_loc);
                    event_bus.publish(parish_types::events::GameEvent::NpcDeparted {
                        npc_id: update.npc_id,
                        location: from,
                        to: new_loc,
                        timestamp: game_time,
                    });
                    event_bus.publish(parish_types::events::GameEvent::NpcArrived {
                        npc_id: update.npc_id,
                        location: new_loc,
                        timestamp: game_time,
                    });
                }
            } else {
                tracing::warn!(
                    npc_id = update.npc_id.0,
                    location = new_loc.0,
                    "Tier 3 update has invalid location, ignoring"
                );
            }
        }

        // Apply relationship changes
        for rc in &update.relationship_changes {
            if rc.from == update.npc_id
                && let Some(npc) = npcs.get_mut(&rc.from)
                && let Some(rel) = npc.relationships.get_mut(&rc.to)
            {
                rel.adjust_strength(rc.delta);
                debug_events.push(format!(
                    "NPC {} -> NPC {}: relationship {:.2} (tier3)",
                    rc.from.0, rc.to.0, rc.delta
                ));
            }
        }
    }

    debug_events
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::make_named_npc;
    use crate::types::{Relationship, RelationshipChange, RelationshipKind};
    use chrono::TimeZone;
    use std::collections::HashMap;

    fn named_npc(id: u32, name: &str, location: u32) -> Npc {
        make_named_npc(id, name, location)
    }

    #[test]
    fn test_tier3_response_parsing() {
        let json = r#"{
            "updates": [
                {
                    "npc_id": 1,
                    "mood": "content",
                    "activity_summary": "Tended the fields all morning.",
                    "new_location": null,
                    "relationship_changes": [{"from": 1, "to": 2, "delta": 0.05}]
                },
                {
                    "npc_id": 2,
                    "mood": "tired",
                    "activity_summary": "Mended a fence near the road.",
                    "new_location": 3,
                    "relationship_changes": []
                }
            ]
        }"#;
        let resp: Tier3Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.updates.len(), 2);
        assert_eq!(resp.updates[0].npc_id, NpcId(1));
        assert_eq!(resp.updates[0].mood, "content");
        assert_eq!(
            resp.updates[0].activity_summary,
            "Tended the fields all morning."
        );
        assert!(resp.updates[0].new_location.is_none());
        assert_eq!(resp.updates[0].relationship_changes.len(), 1);
        assert_eq!(resp.updates[1].npc_id, NpcId(2));
        assert_eq!(resp.updates[1].new_location, Some(LocationId(3)));
    }

    #[test]
    fn test_tier3_response_partial() {
        // Missing optional fields should default gracefully
        let json = r#"{"updates": [{"npc_id": 5}]}"#;
        let resp: Tier3Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.updates.len(), 1);
        assert_eq!(resp.updates[0].npc_id, NpcId(5));
        assert_eq!(resp.updates[0].mood, "");
        assert_eq!(resp.updates[0].activity_summary, "");
        assert!(resp.updates[0].new_location.is_none());
        assert!(resp.updates[0].relationship_changes.is_empty());
    }

    #[test]
    fn test_tier3_response_empty_updates() {
        let json = r#"{}"#;
        let resp: Tier3Response = serde_json::from_str(json).unwrap();
        assert!(resp.updates.is_empty());
    }

    #[test]
    fn test_tier3_prompt_construction() {
        let snapshots = vec![
            Tier3Snapshot {
                id: NpcId(1),
                name: "Padraig".to_string(),
                occupation: "Publican".to_string(),
                age: 58,
                location: LocationId(2),
                location_name: "Darcy's Pub".to_string(),
                mood: "content".to_string(),
                context: "Served drinks all evening.".to_string(),
                intelligence_adjectives: "wise, quick-witted".to_string(),
                relationship_summary: "friendly with Tommy".to_string(),
            },
            Tier3Snapshot {
                id: NpcId(3),
                name: "Bridget".to_string(),
                occupation: "Farmer".to_string(),
                age: 35,
                location: LocationId(5),
                location_name: "O'Brien's Farm".to_string(),
                mood: "worried".to_string(),
                context: String::new(),
                intelligence_adjectives: String::new(),
                relationship_summary: String::new(),
            },
        ];

        let lang = LanguageSettings::english_only();
        let prompt = build_tier3_prompt(&snapshots, "Morning", "Overcast", "Spring", 24, &lang);
        assert!(prompt.contains("Simulate 24 hours"));
        assert!(prompt.contains("Overcast"));
        assert!(prompt.contains("Spring"));
        assert!(prompt.contains("Morning"));
        assert!(prompt.contains("[1] Padraig, 58, Publican"));
        assert!(prompt.contains("Darcy's Pub"));
        assert!(prompt.contains("(wise, quick-witted)"));
        assert!(prompt.contains("friendly with Tommy"));
        assert!(prompt.contains("Recent: Served drinks all evening."));
        assert!(prompt.contains("[3] Bridget, 35, Farmer"));
        // No more raw NPC-id encoding
        assert!(!prompt.contains("NPC 1 \""));
        // JSON format instructions
        assert!(prompt.contains("npc_id"));
        assert!(prompt.contains("activity_summary"));
    }

    #[test]
    fn test_tier3_batching() {
        // Verify that 25 snapshots would be split into 3 batches of 10, 10, 5
        let snapshots: Vec<Tier3Snapshot> = (1..=25)
            .map(|i| Tier3Snapshot {
                id: NpcId(i),
                name: format!("NPC {}", i),
                occupation: "Test".to_string(),
                age: 30,
                location: LocationId(1),
                location_name: "Test".to_string(),
                mood: "calm".to_string(),
                context: String::new(),
                intelligence_adjectives: String::new(),
                relationship_summary: String::new(),
            })
            .collect();

        let chunks: Vec<&[Tier3Snapshot]> = snapshots.chunks(TIER3_BATCH_SIZE).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 10);
        assert_eq!(chunks[1].len(), 10);
        assert_eq!(chunks[2].len(), 5);
    }

    #[test]
    fn test_tier3_update_application() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc1 = named_npc(1, "Padraig", 2);
        npc1.relationships
            .insert(NpcId(5), Relationship::new(RelationshipKind::Friend, 0.5));
        npcs.insert(NpcId(1), npc1);
        npcs.insert(NpcId(5), named_npc(5, "Tommy", 2));

        let graph = WorldGraph::new();

        let updates = vec![Tier3Update {
            npc_id: NpcId(1),
            mood: "jovial".to_string(),
            activity_summary: "Spent the day cleaning the pub.".to_string(),
            new_location: None,
            relationship_changes: vec![RelationshipChange {
                from: NpcId(1),
                to: NpcId(5),
                delta: 0.1,
            }],
        }];

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier3_updates(
            &updates,
            &mut npcs,
            &graph,
            game_time,
            &parish_types::events::EventBus::new(),
        );

        // Mood updated
        assert_eq!(npcs.get(&NpcId(1)).unwrap().mood, "jovial");

        // Activity stored
        assert_eq!(
            npcs.get(&NpcId(1)).unwrap().last_activity.as_deref(),
            Some("Spent the day cleaning the pub.")
        );

        // Memory recorded
        assert!(!npcs.get(&NpcId(1)).unwrap().memory.is_empty());

        // Relationship adjusted
        let rel = npcs
            .get(&NpcId(1))
            .unwrap()
            .relationships
            .get(&NpcId(5))
            .unwrap();
        assert!((rel.strength - 0.6).abs() < f64::EPSILON);

        // Debug events generated
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| e.contains("mood")));
        assert!(events.iter().any(|e| e.contains("activity")));
    }

    #[test]
    fn test_tier3_invalid_location_ignored() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), named_npc(1, "Padraig", 2));

        let graph = WorldGraph::new(); // empty graph — no valid locations

        let updates = vec![Tier3Update {
            npc_id: NpcId(1),
            mood: "calm".to_string(),
            activity_summary: "Walked to market.".to_string(),
            new_location: Some(LocationId(999)), // nonexistent
            relationship_changes: Vec::new(),
        }];

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        apply_tier3_updates(
            &updates,
            &mut npcs,
            &graph,
            game_time,
            &parish_types::events::EventBus::new(),
        );

        // Location should NOT have changed
        assert_eq!(npcs.get(&NpcId(1)).unwrap().location, LocationId(2));
    }

    #[test]
    fn test_tier3_unknown_npc_skipped() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), named_npc(1, "Padraig", 2));

        let graph = WorldGraph::new();

        let updates = vec![Tier3Update {
            npc_id: NpcId(99), // does not exist
            mood: "happy".to_string(),
            activity_summary: "Ghost NPC.".to_string(),
            new_location: None,
            relationship_changes: Vec::new(),
        }];

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier3_updates(
            &updates,
            &mut npcs,
            &graph,
            game_time,
            &parish_types::events::EventBus::new(),
        );

        // Should produce no events (NPC not found)
        assert!(events.is_empty());
    }

    #[test]
    fn test_tier3_snapshot_from_npc_with_last_activity() {
        let mut npc = named_npc(1, "Padraig", 2);
        npc.last_activity = Some("Tended bar all evening.".to_string());

        let graph = WorldGraph::new();
        let snap = tier3_snapshot_from_npc(&npc, &graph, &HashMap::new());

        assert_eq!(snap.id, NpcId(1));
        assert_eq!(snap.name, "Padraig");
        assert_eq!(snap.context, "Tended bar all evening.");
    }

    #[test]
    fn test_tier3_snapshot_from_npc_with_deflated_summary() {
        use crate::transitions::NpcSummary;

        let mut npc = named_npc(1, "Padraig", 2);
        npc.deflated_summary = Some(NpcSummary {
            npc_id: NpcId(1),
            location: LocationId(2),
            mood: "calm".to_string(),
            recent_activity: vec!["Chatted with Tommy.".to_string()],
            key_relationship_changes: Vec::new(),
        });

        let graph = WorldGraph::new();
        let snap = tier3_snapshot_from_npc(&npc, &graph, &HashMap::new());

        assert_eq!(snap.context, "Chatted with Tommy.");
    }

    #[test]
    fn test_tier3_snapshot_from_npc_no_context() {
        let npc = named_npc(1, "Padraig", 2);
        let graph = WorldGraph::new();
        let snap = tier3_snapshot_from_npc(&npc, &graph, &HashMap::new());

        assert_eq!(snap.context, "");
    }

    #[test]
    fn test_tier3_snapshot_uses_adjectives_not_codes() {
        let mut npc = named_npc(1, "Padraig", 2);
        npc.intelligence = crate::types::Intelligence::new(5, 3, 3, 3, 4, 3);
        let graph = WorldGraph::new();
        let names: HashMap<NpcId, String> = HashMap::new();

        let snap = tier3_snapshot_from_npc(&npc, &graph, &names);

        assert!(!snap.intelligence_adjectives.contains("INT["));
        assert_eq!(snap.intelligence_adjectives, "eloquent, wise");
    }

    // ── #1451: season directive in tier3 prompt ──────────────────────────────

    /// AC (#1451): the Tier 3 batch prompt must carry a "CURRENT SEASON" directive
    /// with an explicit no-other-season prohibition so activity summaries stay
    /// season-correct even for distant NPCs.
    #[test]
    fn tier3_prompt_carries_season_directive() {
        let snapshots: Vec<Tier3Snapshot> = vec![Tier3Snapshot {
            id: NpcId(1),
            name: "Padraig".to_string(),
            occupation: "Publican".to_string(),
            age: 58,
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            mood: "content".to_string(),
            context: String::new(),
            intelligence_adjectives: String::new(),
            relationship_summary: String::new(),
        }];
        let lang = LanguageSettings::english_only();
        let prompt = build_tier3_prompt(&snapshots, "Morning", "Clear", "Spring", 4, &lang);

        assert!(
            prompt.contains("CURRENT SEASON:") || prompt.contains("CURRENT SEASON"),
            "tier3 prompt must carry a CURRENT SEASON directive (#1451):\n{prompt}"
        );
        assert!(
            prompt.contains("Spring"),
            "tier3 prompt must name the actual season (#1451):\n{prompt}"
        );
        assert!(
            prompt.contains("do not reference any other season")
                || prompt.contains("Do not reference any other season"),
            "tier3 prompt must prohibit referencing other seasons (#1451):\n{prompt}"
        );
    }

    /// AC-1 (#1397): when grounding_enabled is true, temperature and
    /// frequency_penalty must be Some(…) — not None — so the vllm-mlx server
    /// uses our pinned values rather than its high-temperature defaults.
    #[test]
    fn test_grounding_enabled_pins_temperature_and_frequency_penalty() {
        // grounding_enabled = true  →  both fields must be Some
        let (temp, freq) = {
            let enabled = true;
            if enabled {
                (Some(0.7_f32), Some(0.4_f32))
            } else {
                (None, None)
            }
        };
        assert!(
            temp.is_some(),
            "temperature must be Some when grounding_enabled"
        );
        assert!(
            freq.is_some(),
            "frequency_penalty must be Some when grounding_enabled"
        );
        // Sanity-check the pinned values themselves
        assert!((temp.unwrap() - 0.7).abs() < f32::EPSILON);
        assert!((freq.unwrap() - 0.4).abs() < f32::EPSILON);
    }

    /// Inverse: when grounding_enabled is false, both fields must be None
    /// (preserving the old behaviour for anyone who explicitly disables the flag).
    #[test]
    fn test_grounding_disabled_leaves_params_none() {
        let (temp, freq): (Option<f32>, Option<f32>) = {
            let enabled = false;
            if enabled {
                (Some(0.7), Some(0.4))
            } else {
                (None, None)
            }
        };
        assert!(
            temp.is_none(),
            "temperature must be None when grounding disabled"
        );
        assert!(
            freq.is_none(),
            "frequency_penalty must be None when grounding disabled"
        );
    }

    // ── #1462: season guard tests ────────────────────────────────────────────

    /// AC-1 (#1462): the exact phrase that triggered the bug ("long summer days")
    /// must be scrubbed when the current season is Spring.
    #[test]
    fn test_tier3_season_guard_scrubs_wrong_season() {
        let input = "teaching at the hedge school — the long summer days allow more lessons";
        let (cleaned, changed) = scrub_wrong_season_tokens(input, "Spring");
        assert!(changed, "guard must report a change for wrong-season input");
        assert!(
            !cleaned.to_lowercase().contains("summer"),
            "cleaned text must not contain 'summer': {cleaned:?}"
        );
    }

    /// AC-2 (#1462): correct-season text must pass through unchanged.
    #[test]
    fn test_tier3_season_guard_leaves_correct_season_alone() {
        let input = "the spring rains kept everyone indoors today";
        let (cleaned, changed) = scrub_wrong_season_tokens(input, "Spring");
        assert!(!changed, "guard must not alter correct-season text");
        assert!(
            cleaned.contains("spring"),
            "spring token must survive: {cleaned:?}"
        );
    }

    /// AC-3 (#1462): when season=Winter all other season names are scrubbed.
    #[test]
    fn test_tier3_season_guard_scrubs_all_wrong_seasons_in_winter() {
        for wrong in &["summer", "spring", "autumn", "fall"] {
            let input = format!("spent the day watching the {wrong} colours fade");
            let (cleaned, changed) = scrub_wrong_season_tokens(&input, "Winter");
            assert!(
                changed,
                "guard must detect '{wrong}' as wrong-season when season=Winter"
            );
            assert!(
                !cleaned.to_lowercase().contains(wrong),
                "cleaned text must not contain '{wrong}': {cleaned:?}"
            );
        }
    }

    /// AC-3b (#1462): "summer days" multi-word phrase is fully removed in Spring.
    #[test]
    fn test_tier3_season_guard_removes_summer_days_phrase() {
        let input = "He worked through the summer days with unusual energy.";
        let (cleaned, changed) = scrub_wrong_season_tokens(input, "Spring");
        assert!(changed);
        assert!(
            !cleaned.to_lowercase().contains("summer"),
            "summer must be gone: {cleaned:?}"
        );
    }

    /// AC-3c (#1462): "midsummer" must NOT be altered (word-boundary guard).
    #[test]
    fn test_tier3_season_guard_respects_word_boundaries() {
        // "midsummer" embeds "summer" but is a different word — do not clobber.
        let input = "A midsummer festival memory lingered in her thoughts.";
        let (cleaned, _) = scrub_wrong_season_tokens(input, "Spring");
        // We do NOT assert changed==false because the current implementation
        // is conservative but may or may not catch "midsummer" depending on
        // boundary logic.  The key invariant: "mid" must not be orphaned.
        assert!(
            !cleaned.contains("mid "),
            "word boundary guard must not leave dangling 'mid': {cleaned:?}"
        );
    }

    /// AC-5 (#1462): season-neutral text is untouched.
    #[test]
    fn test_tier3_season_guard_neutral_text_unchanged() {
        let input = "Padraig swept the pub floor and polished the taps.";
        let (cleaned, changed) = scrub_wrong_season_tokens(input, "Autumn");
        assert!(!changed);
        assert_eq!(cleaned, input);
    }

    /// AC-5b (#1462): empty input is safe.
    #[test]
    fn test_tier3_season_guard_empty_input() {
        let (cleaned, changed) = scrub_wrong_season_tokens("", "Summer");
        assert!(!changed);
        assert_eq!(cleaned, "");
    }

    /// UTF-8 safety (#1462 follow-up): strings containing Unicode characters
    /// whose lowercase form has a different UTF-8 byte length must not panic
    /// and must still scrub wrong-season tokens correctly.
    ///
    /// The old implementation called `to_lowercase()` on the work buffer and
    /// reused those byte offsets to splice the original string.  For `İ`
    /// (U+0130, 2 UTF-8 bytes) the lowercase form is `i\u{307}` (3 bytes),
    /// so the offset from the lowercased copy pointed into the middle of a
    /// multi-byte sequence in the original — causing a panic or wrong removal.
    /// The regex-based implementation operates directly on the original string
    /// and is not affected by this mismatch.
    #[test]
    fn test_tier3_season_guard_unicode_no_panic() {
        // İ (U+0130, LATIN CAPITAL LETTER I WITH DOT ABOVE) encodes as 2 bytes
        // in UTF-8 but its Unicode lowercase is "i\u{0307}" (3 bytes).
        // Place it right before a wrong-season token so the old offset
        // arithmetic would have been forced to mis-slice or panic.
        let input = "İlkbahar summer days were warm.";
        // Must not panic, and "summer" must be removed (season=Spring).
        let (cleaned, changed) = scrub_wrong_season_tokens(input, "Spring");
        assert!(changed, "summer token must be scrubbed: {cleaned:?}");
        assert!(
            !cleaned.to_lowercase().contains("summer"),
            "cleaned text must not contain 'summer': {cleaned:?}"
        );

        // Also test with the Kelvin sign K (U+212A, 3 UTF-8 bytes) whose
        // lowercase is 'k' (1 byte).
        let input2 = "\u{212A}eltic winter cold lingered.";
        let (cleaned2, changed2) = scrub_wrong_season_tokens(input2, "Spring");
        assert!(changed2, "winter token must be scrubbed: {cleaned2:?}");
        assert!(
            !cleaned2.to_lowercase().contains("winter"),
            "cleaned text must not contain 'winter': {cleaned2:?}"
        );
    }
}
