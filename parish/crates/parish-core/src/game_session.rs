//! Shared movement application logic for all game backends.
//!
//! Provides [`apply_movement`] and [`apply_arrival_reactions`] — free
//! functions that centralise the post-movement pipeline so that the
//! Tauri desktop backend, the axum web server, and the test harness
//! never duplicate the same logic.
//!
//! The functions mutate [`WorldState`] and [`NpcManager`] in-place
//! (calling `world.log()` for every player-visible line) and return a
//! [`GameEffects`] value describing what the caller must then broadcast
//! to its own frontend or event bus.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::config::ReactionConfig;
use crate::debug_snapshot::InferenceLogEntry;
use crate::dice;
use crate::inference::InferenceLog;
use crate::inference::{AnyClient, GenerateParams};
use crate::ipc::{build_travel_start, types::TravelStartPayload};
use crate::npc::manager::{NpcManager, TierTransition};
use crate::npc::reactions::{
    ArrivalContext, NpcReaction, ReactionTemplates, generate_arrival_reactions,
};
use crate::npc::{LanguageSettings, Npc, NpcId};
use crate::world::description::{format_exits, render_description};
use crate::world::encounter::check_encounter;
use crate::world::movement::{MovementResult, resolve_movement_with_weather};
use crate::world::time::TimeOfDay;
use crate::world::transport::TransportMode;
use crate::world::{Location, LocationId, WorldState};

/// Monotonically increasing request ID counter for reaction inference calls.
/// Starts at 100_000 to stay visually distinct from the dialogue queue IDs.
static REACTION_REQ_ID: AtomicU64 = AtomicU64::new(100_000);

/// Returns the current value of the reaction request ID counter.
///
/// Read-only accessor used by the debug panel to report how many reaction
/// inference calls have been issued this session.
pub fn reaction_req_id_peek() -> u64 {
    REACTION_REQ_ID.load(Ordering::Relaxed)
}

// ── Public types ─────────────────────────────────────────────────────────────

/// A player-visible message produced by movement resolution.
///
/// The `source` field distinguishes system narration from NPC speech so
/// each backend can style or route them appropriately.
#[derive(Debug, Clone)]
pub struct GameMessage {
    /// The message source: `"system"` for narration / descriptions,
    /// `"npc"` for NPC arrival reactions.
    pub source: &'static str,
    /// Optional semantic subtype for frontend styling (e.g. `"location"`).
    pub subtype: Option<&'static str>,
    /// The message text.
    pub text: String,
}

/// The side-effects produced by a single call to [`apply_movement`].
///
/// The caller is responsible for forwarding these to its own event bus or
/// IPC channel. [`WorldState::log`] has already been called for the canned
/// text of every reaction, so test harnesses that only read from the log need
/// not inspect `arrival_reactions` at all.
///
/// Backends with an LLM reaction client should iterate `arrival_reactions`,
/// upgrade any entry where `use_llm` is true via `resolve_llm_greeting`, and
/// emit the result. Canned text is always the safe fallback.
#[derive(Debug, Default)]
pub struct GameEffects {
    /// Payload for a travel-start animation event, present only when the
    /// player actually moved (i.e. not `AlreadyHere` / `NotFound`).
    pub travel_start: Option<TravelStartPayload>,
    /// Narration and look-description messages in emission order.
    /// Does NOT include arrival reactions — those are in `arrival_reactions`.
    pub messages: Vec<GameMessage>,
    /// Raw NPC arrival reactions. Canned text is pre-logged to `world.log()`.
    /// Backends with an LLM client may upgrade `use_llm` entries; others
    /// should emit `reaction.canned_text` directly.
    pub arrival_reactions: Vec<NpcReaction>,
    /// `true` when the world state changed (player moved).
    pub world_changed: bool,
    /// Cognitive-tier reassignments that occurred after movement.
    pub tier_transitions: Vec<TierTransition>,
}

// ── Core functions ────────────────────────────────────────────────────────────

/// Resolves a movement intent and applies all post-movement state changes.
///
/// Internally performs:
/// 1. Movement resolution via [`resolve_movement`].
/// 2. For a successful arrival:
///    - builds the travel-start payload,
///    - records edge traversals,
///    - advances the clock,
///    - updates the player's location and visited set,
///    - updates the legacy `locations` map,
///    - reassigns NPC cognitive tiers,
///    - renders the arrival description and exits,
///    - generates NPC arrival reactions (canned text, no LLM).
/// 3. For `AlreadyHere` or `NotFound`, returns an appropriate message.
///
/// Every player-visible line is appended to `world.log()` *and* included
/// in the returned [`GameEffects::messages`], so both the test harness
/// (which reads the log) and GUI backends (which emit events) are served.
pub fn apply_movement(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    reaction_templates: &ReactionTemplates,
    target: &str,
    transport: &TransportMode,
) -> GameEffects {
    let result = resolve_movement_with_weather(
        target,
        &world.graph,
        world.player_location,
        transport,
        world.weather,
    );

    match result {
        MovementResult::Arrived {
            destination,
            path,
            minutes,
            narration,
        } => {
            // Build travel-start payload *before* mutating state so the path is valid
            let travel_start = build_travel_start(&path, minutes, &world.graph);
            let origin = world.player_location;

            // Apply world state changes
            world.record_path_traversal(&path);
            world.clock.advance(minutes as i64);
            world.player_location = destination;
            world.mark_visited(destination);

            // Publish PlayerMoved on the broadcast bus so the character-log
            // writer can record the journey in player.md. Fires here (not at
            // the higher-level `handle_movement`) so the script harness —
            // which calls `apply_movement` directly — emits the event too.
            world
                .event_bus
                .publish(parish_types::events::GameEvent::PlayerMoved {
                    from: origin,
                    to: destination,
                    timestamp: world.clock.now(),
                });

            // Update legacy locations map
            if let Some(data) = world.graph.get(destination) {
                world
                    .locations
                    .entry(destination)
                    .or_insert_with(|| Location {
                        id: destination,
                        name: data.name.clone(),
                        description: data.description_template.clone(),
                        indoor: data.indoor,
                        public: data.public,
                        lat: data.lat,
                        lon: data.lon,
                    });
            }

            // Check for a travel encounter now that the clock has advanced.
            let encounter_msg =
                check_encounter(world.clock.time_of_day(), dice::DiceRoll::roll().value());

            // Reassign NPC cognitive tiers
            let tier_transitions = npc_manager.assign_tiers(world, &[]);

            // Build arrival description
            let look_text = build_look_text(world, npc_manager, transport);

            // Tick schedules so NPCs whose transit completed during travel
            // are now Present before we check for reactions
            let _schedule_events = npc_manager.tick_schedules(
                &world.clock,
                &world.graph,
                world.weather,
                &world.event_bus,
            );

            // Generate arrival reactions; canned text is logged to world.log.
            // Raw reactions are returned so backends with an LLM client can
            // upgrade use_llm entries via resolve_llm_greeting.
            let arrival_reactions = apply_arrival_reactions(
                world,
                npc_manager,
                reaction_templates,
                &ReactionConfig::default(),
            );

            // Build system message list (narration + look only — NOT reactions)
            let mut messages: Vec<GameMessage> = Vec::new();

            // Narration (travel description)
            world.log(narration.clone());
            world.log(String::new());
            messages.push(GameMessage {
                source: "system",
                subtype: None,
                text: narration,
            });

            // En-route encounter (fires ~20% of traversals, see encounter.rs)
            if let Some(text) = encounter_msg {
                world.log(text.clone());
                messages.push(GameMessage {
                    source: "system",
                    subtype: Some("encounter"),
                    text,
                });
            }

            // Arrival description + exits
            world.log(look_text.clone());
            messages.push(GameMessage {
                source: "system",
                subtype: Some("location"),
                text: look_text,
            });

            GameEffects {
                travel_start: Some(travel_start),
                messages,
                arrival_reactions,
                world_changed: true,
                tier_transitions,
            }
        }

        MovementResult::AlreadyHere => {
            let text = "Sure, you're already standing right here.".to_string();
            world.log(text.clone());
            GameEffects {
                messages: vec![GameMessage {
                    source: "system",
                    subtype: None,
                    text,
                }],
                ..Default::default()
            }
        }

        MovementResult::NotFound(name) => {
            let exits = format_exits(
                world.player_location,
                &world.graph,
                transport.speed_m_per_s,
                &transport.label,
            );
            let text = format!(
                "You haven't the faintest notion how to reach \"{}\". {}",
                name, exits
            );
            world.log(text.clone());
            GameEffects {
                messages: vec![GameMessage {
                    source: "system",
                    subtype: None,
                    text,
                }],
                ..Default::default()
            }
        }

        MovementResult::BlockedByWeather {
            weather, reason, ..
        } => {
            let text = format!("{} (The weather is {}. Best wait it out.)", reason, weather);
            world.log(text.clone());
            GameEffects {
                messages: vec![GameMessage {
                    source: "system",
                    subtype: Some("blocked-weather"),
                    text,
                }],
                ..Default::default()
            }
        }
    }
}

// ── Dialogue application ───────────────────────────────────────────────────────

/// Caps NPC dialogue to a maximum character length for display (#1224).
///
/// Appends `…` (U+2026) when the string is clipped, matching the single
/// codepoint used by `truncate_for_memory`. Passes the string through as-is
/// when it fits within the limit or when the cap is 0 (cap disabled).
///
/// Call this on `parsed.dialogue` before constructing the `ConversationLine`
/// or `DialogueOccurred` event. The in-memory representation (conversation
/// log, witness memories, tier-1 response memory) is already capped
/// independently by `memory_truncation_dialogue` in `NpcConfig`.
pub fn cap_dialogue_for_display(dialogue: &str, max_chars: usize) -> std::borrow::Cow<'_, str> {
    cap_dialogue_for_display_with_trim(dialogue, max_chars, true)
}

/// Sentence-boundary terminators a clipped reply is allowed to end on (#1400).
const SENTENCE_TERMINATORS: [char; 4] = ['.', '!', '?', '\u{2026}'];

/// Length cap with an explicit sentence-boundary-trim toggle (#1400).
///
/// When `sentence_boundary_trim` is `true`, a reply that overruns the cap is
/// rewound to the last sentence terminator (`.`, `!`, `?`, `…`, optionally
/// followed by a closing quote) within the budget before the `…` marker is
/// appended, so the player never sees a mid-word / mid-clause cut
/// ("...out and about, and…"). When no terminator exists in the budget, or the
/// toggle is `false`, it falls back to the legacy raw char-boundary clip.
pub fn cap_dialogue_for_display_with_trim(
    dialogue: &str,
    max_chars: usize,
    sentence_boundary_trim: bool,
) -> std::borrow::Cow<'_, str> {
    if max_chars == 0 || dialogue.len() <= max_chars {
        return std::borrow::Cow::Borrowed(dialogue);
    }
    // Reserve 3 bytes for the `…` codepoint (U+2026, 3-byte UTF-8).
    let raw_boundary = crate::npc::floor_char_boundary(dialogue, max_chars.saturating_sub(3));
    let raw_safe = raw_boundary.min(dialogue.len());

    if sentence_boundary_trim {
        if let Some(end) = last_sentence_boundary(&dialogue[..raw_safe]) {
            // `end` is a byte index just past a terminator (and any trailing
            // closing quote) — a clean clause end. Only use it if non-empty so
            // we never collapse a long run-on to a bare "…".
            return std::borrow::Cow::Owned(format!("{}\u{2026}", &dialogue[..end]));
        }
    }
    std::borrow::Cow::Owned(format!("{}\u{2026}", &dialogue[..raw_safe]))
}

/// Returns the byte index just past the last sentence boundary in `s`, or
/// `None` if there is no usable boundary (so the caller falls back to the raw
/// clip). A boundary is a sentence terminator optionally followed by a single
/// closing quote (`"` / `'` / `\u{201D}` / `\u{2019}`); the index is advanced
/// past that quote so the clause closes cleanly.
fn last_sentence_boundary(s: &str) -> Option<usize> {
    let bytes_len = s.len();
    let mut last: Option<usize> = None;
    for (idx, ch) in s.char_indices() {
        if SENTENCE_TERMINATORS.contains(&ch) {
            let mut end = idx + ch.len_utf8();
            // Absorb a single trailing closing quote so `"...home."` keeps the quote.
            if let Some(next) = s[end..].chars().next()
                && matches!(next, '"' | '\'' | '\u{201D}' | '\u{2019}')
            {
                end += next.len_utf8();
            }
            last = Some(end);
        }
    }
    // Reject a boundary that is the whole string (nothing was actually clipped
    // by sentence logic) or empty (would collapse to a bare ellipsis).
    match last {
        Some(end) if end > 0 && end < bytes_len => Some(end),
        // If the only boundary is at the very end of the budget window, it is
        // still a clean clause end — keep it as long as it leaves real content.
        Some(end) if end == bytes_len && end > 0 => Some(end),
        _ => None,
    }
}

/// Outcome of [`apply_npc_dialogue_turn`].
///
/// Carries the debug-event strings produced by the shared per-turn pipeline
/// (steps 2 and 4) plus `display_text` — the single, authoritative
/// player-visible dialogue after the anti-repetition guard (#1228) and the
/// display-length cap (#1224). Every backend renders `display_text`; none
/// re-derives a player line from the raw `parsed.dialogue`, which would bypass
/// both guards.
#[derive(Debug, Clone)]
pub struct DialogueTurnOutcome {
    /// Debug-event strings from Tier-1 apply (step 2) and witness memories
    /// (step 4). The live loop discards these; headless + harness forward them.
    pub debug_events: Vec<String>,
    /// The guarded, capped dialogue to show the player. Matches exactly what was
    /// written to the conversation log and the `DialogueOccurred` event. May be
    /// empty when the model returned no usable dialogue.
    pub display_text: String,
}

/// Applies a parsed NPC dialogue response — the per-turn cross-cutting steps
/// every backend performs identically after a Tier-1 reply (#1172 / #1173).
///
/// Before this existed, four code paths (live `game_loop::npc_turn`, headless
/// `apply_npc_response`, and the script harness's `consume_canned_npc_response`
/// and `handle_npc_interaction_for`) each reimplemented a *different subset* of
/// these steps, so behaviour silently drifted (#1028, #1035, #1077/#1079). This
/// is the single definition; all four call it.
///
/// The steps, in order:
/// 1. **Name detection** — `detect_and_record_player_name`, so a
///    self-introduction in `player_input` teaches the addressed speaker before
///    memory is recorded.
/// 2. **Tier-1 state update** — `apply_tier1_response_with_config` on the
///    speaker (mood, memory, language drift).
/// 3. **Conversation-exchange record** — appended to `world.conversation_log`,
///    which feeds the "What's been said here" prompt block
///    (`ticks::conversation_block`).
/// 4. **Witness memories** — co-located bystanders record an "Overheard" memory.
/// 5. **`DialogueOccurred` publish** — on `world.event_bus`, so the
///    character-log, location-log and chat-transcript subscribers record a
///    verbatim journal entry.
///
/// Operates on plain `&mut` borrows (no runtime I/O), so it needs no
/// `EventEmitter`: the only event it raises goes to the in-process `event_bus`,
/// not the UI emitter. Returns the debug-event strings produced by steps 2 and 4
/// so the caller can forward them to its own debug sink — the headless CLI and
/// the harness do; the live loop discards them (`let _ = …`).
///
/// `player_input` is the raw player utterance used for name detection, memory,
/// witness records and the conversation log. `player_said_for_journal` is the
/// (possibly verb-stripped) line stored as `DialogueOccurred::player_said`; pass
/// the same value as `player_input` unless the caller cleans a leading verb.
///
/// Returns a [`DialogueTurnOutcome`] carrying the debug-event strings produced by
/// steps 2 and 4 **and** the player-visible `display_text` — the dialogue after
/// the anti-repetition guard (#1228) and length cap (#1224). Callers must show
/// `display_text` to the player (conversation line, `ActionResult`, headless
/// stdout) so what the player sees matches what was stored in the conversation
/// log and the `DialogueOccurred` event. Building a player line from
/// `parsed.dialogue` directly would bypass both guards and re-introduce the
/// divergence #1224/#1228 closed.
#[allow(clippy::too_many_arguments)]
pub fn apply_npc_dialogue_turn(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    speaker_id: NpcId,
    parsed: &crate::npc::NpcStreamResponse,
    player_input: &str,
    player_said_for_journal: &str,
    game_time: chrono::DateTime<chrono::Utc>,
    location: LocationId,
    speaker_display_name: &str,
    speaker_actual_name: &str,
    request_id: Option<u64>,
) -> DialogueTurnOutcome {
    let mut debug_events = Vec::new();

    // 1. Learn the player's name from a self-introduction *before* recording
    //    memory, so the addressed speaker's memory uses the real name (#1028).
    crate::ipc::detect_and_record_player_name(world, npc_manager, player_input, speaker_id);

    // 2. Tier-1 state update on the speaker.
    let player_name_for_mem = if npc_manager.knows_player_name(speaker_id) {
        world.player_name.clone()
    } else {
        None
    };
    if let Some(npc) = npc_manager.get_mut(speaker_id) {
        debug_events.extend(crate::npc::ticks::apply_tier1_response_with_config(
            npc,
            parsed,
            player_input,
            game_time,
            &Default::default(),
            player_name_for_mem.as_deref(),
        ));
    }

    // Anti-repetition guard (#1228). Applied *before* the length cap so the
    // length budget is spent on de-duplicated content, not a wall of repeated
    // clauses. Collapses consecutive duplicate clauses within the new line and,
    // when the result is near-identical to this NPC's own previous line at this
    // location, substitutes a varied fallback. Deterministic and provider-
    // agnostic; runs identically for the local MLX model and any cloud provider.
    let npc_cfg = crate::config::NpcConfig::default();
    let previous_line: Option<String> = world
        .conversation_log
        .recent_at(
            location,
            crate::npc::conversation::ConversationLog::capacity(),
        )
        .into_iter()
        .rev()
        .find(|e| e.speaker_id == speaker_id)
        .map(|e| e.npc_dialogue.clone());
    // Stable per-turn seed so the fallback is deterministic for tests/replay but
    // varies across NPCs and turns.
    let repetition_seed = speaker_id.0 as u64 ^ (game_time.timestamp() as u64);
    let deduped_dialogue = crate::npc::guard_against_repetition(
        &parsed.dialogue,
        previous_line.as_deref(),
        npc_cfg.dialogue_repetition_threshold,
        repetition_seed,
    );

    // Cap the displayed dialogue to the configured limit (#1224). Applied here,
    // before the conversation log and event bus, so all player-visible paths see
    // the same capped text. The in-memory representation (witness memories,
    // tier-1 memory entry) is capped separately via `memory_truncation_dialogue`.
    let display_cap = npc_cfg.dialogue_display_max_chars;
    // Sentence-boundary trim (#1400): clip back to a clause end so a capped
    // reply never shows a mid-word/mid-clause "…". Default-on; kill-switched
    // via the `dialogue_sentence_boundary_trim` config field (runtime flag
    // `dialogue-sentence-boundary-trim`).
    let capped_dialogue = cap_dialogue_for_display_with_trim(
        &deduped_dialogue,
        display_cap,
        npc_cfg.dialogue_sentence_boundary_trim,
    );

    // 3. Record the conversation exchange for scene awareness.
    world
        .conversation_log
        .add(crate::npc::conversation::ConversationExchange {
            timestamp: game_time,
            speaker_id,
            speaker_name: speaker_actual_name.to_string(),
            player_input: player_input.to_string(),
            npc_dialogue: capped_dialogue.to_string(),
            location,
        });

    // 4. Record witness memories for co-located bystanders.
    debug_events.extend(crate::npc::ticks::record_witness_memories(
        npc_manager.npcs_mut(),
        speaker_id,
        speaker_display_name,
        player_input,
        &capped_dialogue,
        game_time,
        location,
    ));

    // 5. Publish the full-text dialogue event. Emit even when the dialogue is
    //    empty so journal entries line up with the player's prompt, but skip
    //    when both sides are empty (no useful record) — matches the live loop's
    //    original guard.
    if !player_said_for_journal.trim().is_empty() || !capped_dialogue.trim().is_empty() {
        world
            .event_bus
            .publish(parish_types::events::GameEvent::DialogueOccurred {
                npc_id: speaker_id,
                location,
                summary: capped_dialogue.to_string(),
                player_said: Some(player_said_for_journal.to_string()),
                npc_said: Some(capped_dialogue.to_string()),
                request_id,
                timestamp: game_time,
            });
    }

    DialogueTurnOutcome {
        debug_events,
        display_text: capped_dialogue.into_owned(),
    }
}

/// Rolled but not yet logged/committed travel encounter, returned from
/// [`roll_travel_encounter`] so backends can optionally enrich the text via
/// an LLM before committing.
#[derive(Debug, Clone)]
pub struct RolledEncounter {
    /// The canned encounter — safe fallback if LLM enrichment fails.
    pub canned: parish_world::wayfarers::WayfarerEncounter,
    /// Deterministic seed derived from clock + path (stable for this journey).
    pub seed: u64,
    /// Current time of day (drives pool selection + prompt context).
    pub time: TimeOfDay,
    /// Current season.
    pub season: crate::world::time::Season,
    /// Current weather.
    pub weather: parish_world::Weather,
}

/// Rolls a travel encounter without logging it.
///
/// Returns `Some(RolledEncounter)` if the dice roll triggers for this
/// journey, `None` otherwise. Backends can then either log
/// [`RolledEncounter::canned`] directly or await
/// [`enrich_travel_encounter`] to upgrade the line via an LLM call.
pub fn roll_travel_encounter(world: &WorldState, effects: &GameEffects) -> Option<RolledEncounter> {
    let ts = effects.travel_start.as_ref()?;
    let from_id = ts
        .waypoints
        .first()
        .and_then(|w| w.id.parse::<u32>().ok())
        .map(LocationId)
        .unwrap_or(world.player_location);
    let to_id = ts
        .waypoints
        .last()
        .and_then(|w| w.id.parse::<u32>().ok())
        .map(LocationId)
        .unwrap_or(world.player_location);
    let clock_minutes = world.clock.now().timestamp() / 60;
    let seed = parish_world::wayfarers::encounter_seed(clock_minutes, from_id, to_id);
    let time = world.clock.time_of_day();
    let season = world.clock.season();
    let weather = world.weather;
    let canned = parish_world::wayfarers::resolve_encounter(time, season, weather, seed)?;
    Some(RolledEncounter {
        canned,
        seed,
        time,
        season,
        weather,
    })
}

/// Upgrades a rolled encounter via an LLM call, using the canned text as a
/// few-shot seed. Falls back to the canned line on timeout, empty output,
/// or any error. Always returns a single formatted line ready to log.
pub async fn enrich_travel_encounter(
    rolled: &RolledEncounter,
    client: &AnyClient,
    model: &str,
    timeout_secs: u64,
) -> String {
    let (system, context) = parish_world::wayfarers::build_enrichment_prompt(
        &rolled.canned,
        rolled.time,
        rolled.season,
        rolled.weather,
        rolled.seed,
    );

    let timeout = Duration::from_secs(timeout_secs);
    let result = tokio::time::timeout(
        timeout,
        client.generate(
            model,
            &context,
            Some(&system),
            GenerateParams {
                max_tokens: Some(80),
                temperature: None,
                frequency_penalty: None,
            },
        ),
    )
    .await;

    match result {
        Ok(Ok(text)) => {
            let trimmed = text.trim();
            let cleaned = trimmed.split("---").next().unwrap_or(trimmed).trim();
            // Strip leading "- " / "* " if the model returned a bullet anyway.
            let cleaned = cleaned.trim_start_matches(['-', '*', ' ']).trim();
            // Strip surrounding quotes if the model added them.
            let cleaned = cleaned.trim_matches(|c: char| c == '"' || c == '\'').trim();
            // Keep only the first line — some models add follow-ups.
            let first_line = cleaned.lines().next().unwrap_or("").trim();
            if first_line.is_empty() {
                rolled.canned.text.clone()
            } else {
                first_line.to_string()
            }
        }
        _ => rolled.canned.text.clone(),
    }
}

/// Rolls a travel encounter for the just-completed journey and logs it to `world`.
///
/// Call this immediately after a successful [`apply_movement`] (i.e. when
/// `effects.world_changed` is true). Uses the path endpoints from
/// `effects.travel_start` to build a deterministic seed so the same journey
/// at the same clock time always produces the same encounter.
///
/// Gate this behind the `travel-encounters` feature flag at the call site:
/// ```ignore
/// if effects.world_changed && !flags.is_disabled("travel-encounters") {
///     apply_travel_encounter(world, &effects);
/// }
/// ```
pub fn apply_travel_encounter(world: &mut WorldState, effects: &GameEffects) {
    if let Some(rolled) = roll_travel_encounter(world, effects) {
        world.log(format!("  · {}", rolled.canned.text));
    }
}

/// Generates NPC arrival reactions for the player's current location and
/// applies their side-effects (marking introductions, logging to world).
///
/// Returns the raw [`NpcReaction`] structs. Callers that only need the
/// reactions without the full movement pipeline can use this standalone
/// function. Canned text is logged to `world.log()`; backends with an LLM
/// client may upgrade `use_llm` entries via `resolve_llm_greeting`.
pub fn apply_arrival_reactions(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    templates: &ReactionTemplates,
    config: &ReactionConfig,
) -> Vec<NpcReaction> {
    let npcs = npc_manager.npcs_at(world.player_location);
    if npcs.is_empty() {
        return Vec::new();
    }
    let loc_data = match world.current_location_data() {
        Some(d) => d.clone(),
        None => return Vec::new(),
    };
    let tod = world.clock.time_of_day();
    let weather = world.weather.to_string();
    let introduced = npc_manager.introduced_set();
    let roll_dice = dice::roll_n(npcs.len() * 2);

    let arrival_ctx = ArrivalContext {
        location: &loc_data,
        time_of_day: tod,
        weather: &weather,
        templates,
        config,
    };
    let reactions = generate_arrival_reactions(&npcs, &introduced, &arrival_ctx, &roll_dice);

    for reaction in &reactions {
        if reaction.introduces {
            npc_manager.mark_introduced(reaction.npc_id);
        }
        world.log(reaction.canned_text.clone());
    }
    reactions
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Renders the location description and exits as a single string.
fn build_look_text(
    world: &WorldState,
    npc_manager: &NpcManager,
    transport: &TransportMode,
) -> String {
    let desc = if let Some(loc_data) = world.current_location_data() {
        let tod = world.clock.time_of_day();
        let weather = world.weather.to_string();
        let npc_display: Vec<String> = npc_manager
            .npcs_at(world.player_location)
            .iter()
            .map(|n| npc_manager.display_name(n).to_string())
            .collect();
        let npc_names: Vec<&str> = npc_display.iter().map(|s| s.as_str()).collect();
        render_description(loc_data, tod, &weather, &npc_names)
    } else {
        world.current_location().description.clone()
    };

    let exits = format_exits(
        world.player_location,
        &world.graph,
        transport.speed_m_per_s,
        &transport.label,
    );

    format!("{}\n{}", desc, exits)
}

/// Streams NPC arrival reaction texts to the frontend gradually, upgrading
/// `use_llm` entries via the provided LLM client when available.
///
/// For each reaction, calls `emit_text_log` with the NPC display name to
/// create an empty placeholder entry in the frontend chat log, then pipes
/// token batches to `emit_stream_token` so the frontend stream-pump can
/// reveal them word-by-word — matching the gradual appearance of normal NPC
/// dialogue. Canned text is used when no LLM client is available or when the
/// reaction does not require an LLM.
///
/// The caller is responsible for emitting a `stream-end` event after this
/// function returns so the frontend finalises the last streaming entry.
///
/// # Parameters
/// - `reactions` — raw reactions from `GameEffects::arrival_reactions`
/// - `all_npcs` — full NPC roster (used to look up each reacting NPC's data)
/// - `current_location_id` — player's current location (for workplace check)
/// - `loc_name` — display name of the current location
/// - `tod` — current time of day
/// - `weather` — current weather string
/// - `introduced` — set of NPC IDs the player has already met
/// - `client` — LLM client, or `None` to always use canned text
/// - `model` — model name passed to the LLM
/// - `inference_log` — optional log to record each call for the debug panel
/// - `emit_text_log(turn_id, npc_name)` — called once per reaction to create
///   an empty placeholder in the frontend chat log before streaming begins.
///   The implementation MUST tie the placeholder to `turn_id` via
///   `text_log_for_stream_turn` so the UI's streaming-placeholder guard
///   recognises it and `finalizeStreamingEntry` can remove it when the turn
///   ends with no tokens (otherwise an empty bubble lingers in the chat).
/// - `emit_stream_token(turn_id, source, batch)` — called with each batched
///   token chunk to be appended to the current streaming entry
/// - `emit_stream_turn_end(turn_id)` — called exactly once after the per-NPC
///   token stream finishes (success, timeout, or empty). The UI uses this to
///   finalise the streaming entry; without it an empty-output reaction leaves
///   a blank placeholder bubble forever (#984 follow-up).
#[allow(clippy::too_many_arguments)]
// Justification: mirrors the previous resolve_reaction_texts signature; all
// arguments are necessary to build the per-NPC prompt and wire the callbacks.
pub async fn stream_reaction_texts(
    reactions: &[NpcReaction],
    all_npcs: &[Npc],
    current_location_id: LocationId,
    loc_name: &str,
    tod: TimeOfDay,
    weather: &str,
    introduced: &HashSet<NpcId>,
    client: Option<&AnyClient>,
    model: &str,
    inference_log: Option<&InferenceLog>,
    language: &LanguageSettings,
    mut emit_text_log: impl FnMut(u64, &str),
    mut emit_stream_token: impl FnMut(u64, &str, &str),
    mut emit_stream_turn_end: impl FnMut(u64),
) {
    use crate::ipc::stream_npc_tokens;
    use crate::npc::reactions::build_reaction_prompt;
    use tokio::sync::mpsc;

    let timeout_secs = ReactionConfig::default().llm_timeout_secs;

    for reaction in reactions {
        let npc = all_npcs.iter().find(|n| n.id == reaction.npc_id);
        let turn_id = REACTION_REQ_ID.fetch_add(1, Ordering::Relaxed);

        // Emit an empty placeholder so the frontend shows the NPC name immediately
        // and the stream-pump knows which entry to fill.
        emit_text_log(turn_id, &reaction.npc_display_name);

        let (tx, rx) = mpsc::channel::<String>(parish_inference::TOKEN_CHANNEL_CAPACITY);

        // Capture prompt data here (before the spawn) so we can log it afterwards.
        let mut llm_log_info: Option<(usize, String, String)> = None; // (prompt_len, system, context)

        if reaction.use_llm {
            if let (Some(c), Some(npc)) = (client, npc) {
                if c.is_simulator() {
                    // The simulator generates Markov nonsense for free-text
                    // prompts, which surfaces in the chat bubble as gibberish
                    // ("bridget from the new collection ... God help us").
                    // Use the deterministic canned line instead so reactions
                    // remain readable when offline / in headless test runs.
                    let _ = tx.try_send(reaction.canned_text.clone());
                    drop(tx);
                } else {
                    let at_workplace = npc.workplace.is_some_and(|wp| wp == current_location_id);
                    let is_introduced = introduced.contains(&reaction.npc_id);
                    let (system, context) = build_reaction_prompt(
                        npc,
                        loc_name,
                        tod,
                        weather,
                        is_introduced,
                        at_workplace,
                        language,
                    );
                    llm_log_info = Some((context.len(), system.clone(), context.clone()));

                    let c_clone = c.clone();
                    let model_str = model.to_string();
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(
                            Duration::from_secs(timeout_secs),
                            c_clone.generate_stream(
                                &model_str,
                                &context,
                                Some(&system),
                                tx,
                                GenerateParams {
                                    max_tokens: Some(100),
                                    temperature: None,
                                    frequency_penalty: None,
                                },
                            ),
                        )
                        .await;
                        // tx is consumed by generate_stream; when it returns (success or
                        // timeout) tx is dropped, closing the channel and allowing
                        // stream_npc_tokens to finish.
                    });
                }
            } else {
                // No client or NPC not found — fall back to canned text.
                // Single send on a fresh channel; try_send will not fail.
                let _ = tx.try_send(reaction.canned_text.clone());
                drop(tx);
            }
        } else {
            // Canned text path: send directly through the channel so
            // stream_npc_tokens can still pace the output word-by-word.
            let _ = tx.try_send(reaction.canned_text.clone());
            drop(tx);
        }

        let npc_name = reaction.npc_display_name.clone();
        let started = Instant::now();
        let accumulated = stream_npc_tokens(rx, |batch| {
            emit_stream_token(turn_id, &npc_name, batch);
        })
        .await;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        // Finalise this NPC's streaming entry so the UI removes the empty
        // placeholder if no tokens arrived (LLM timeout / empty output) or
        // marks the populated entry as no-longer-streaming otherwise.
        emit_stream_turn_end(turn_id);

        if let (Some((prompt_len, system_prompt, prompt_text)), Some(log)) =
            (llm_log_info, inference_log)
        {
            let entry = InferenceLogEntry {
                request_id: turn_id,
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                model: model.to_string(),
                streaming: true,
                duration_ms: elapsed_ms,
                prompt_len,
                response_len: accumulated.len(),
                error: None,
                system_prompt: Some(system_prompt),
                prompt_text,
                response_text: accumulated,
                max_tokens: Some(100),
                ttft_ms: None,
                output_tokens: None,
                temperature: None,
                priority: crate::inference::InferencePriority::Interactive,
            };
            let mut log_guard = log.lock().await;
            log_guard.push(entry);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_mod::{GameMod, find_default_mod};
    use crate::world::transport::TransportMode;

    fn setup() -> Option<(WorldState, NpcManager, ReactionTemplates, TransportMode)> {
        let mod_dir = find_default_mod()?;
        let game_mod = GameMod::load(&mod_dir).ok()?;
        let world = crate::game_mod::world_state_from_mod(&game_mod).ok()?;
        let npc_manager = NpcManager::load_from_file(&mod_dir.join("npcs.json")).ok()?;
        let templates = game_mod.reactions.clone();
        let transport = TransportMode::walking();
        Some((world, npc_manager, templates, transport))
    }

    #[test]
    fn apply_movement_already_here() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let loc = world.current_location().name.clone();
        let effects = apply_movement(&mut world, &mut mgr, &templates, &loc, &transport);
        assert!(!effects.messages.is_empty());
    }

    #[test]
    fn apply_movement_not_found_produces_message() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            "xyzzy-no-such-place",
            &transport,
        );
        assert!(!effects.world_changed);
        assert!(effects.travel_start.is_none());
        assert_eq!(effects.messages.len(), 1);
        assert!(effects.messages[0].text.contains("faintest notion"));
    }

    #[test]
    fn apply_movement_arrives_sets_world_changed() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let start = world.player_location;
        // Find a reachable neighbor
        let neighbor = world.graph.neighbors(start).into_iter().next();
        let Some((neighbor_id, _)) = neighbor else {
            return;
        };
        let neighbor_name = world
            .graph
            .get(neighbor_id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let effects = apply_movement(&mut world, &mut mgr, &templates, &neighbor_name, &transport);
        assert!(effects.world_changed);
        assert!(effects.travel_start.is_some());
        assert_eq!(world.player_location, neighbor_id);
        // Log should contain narration + look text
        assert!(world.text_log.len() >= 2);
    }

    #[test]
    fn apply_arrival_reactions_does_not_panic() {
        let Some((mut world, mut mgr, templates, _)) = setup() else {
            return;
        };
        let config = ReactionConfig::default();
        apply_arrival_reactions(&mut world, &mut mgr, &templates, &config);
    }

    /// Verifies that stream_reaction_texts calls emit_text_log once per reaction
    /// and emits the complete canned text as one or more token chunks when no
    /// LLM client is provided.
    #[tokio::test]
    async fn stream_reaction_texts_canned_streams_gradually() {
        use crate::npc::reactions::{NpcReaction, ReactionKind};

        let reaction = NpcReaction {
            npc_id: NpcId(999),
            npc_display_name: "Ciarán".to_string(),
            kind: ReactionKind::Greeting,
            canned_text: "Hello there!".to_string(),
            introduces: false,
            use_llm: false,
        };

        let mut log_sources: Vec<String> = Vec::new();
        let mut token_chunks: Vec<String> = Vec::new();
        let mut turn_ends: Vec<u64> = Vec::new();

        let lang = crate::npc::LanguageSettings::english_only();
        stream_reaction_texts(
            &[reaction],
            &[],
            LocationId(0),
            "Galway",
            crate::world::time::TimeOfDay::Morning,
            "clear",
            &std::collections::HashSet::new(),
            None,
            "",
            None,
            &lang,
            |_turn_id, name| log_sources.push(name.to_string()),
            |_turn_id, _source, tok| token_chunks.push(tok.to_string()),
            |turn_id| turn_ends.push(turn_id),
        )
        .await;

        assert_eq!(
            log_sources,
            vec!["Ciarán"],
            "emit_text_log called with NPC name"
        );
        assert!(
            !token_chunks.is_empty(),
            "at least one token chunk must be emitted"
        );
        assert_eq!(
            token_chunks.join(""),
            "Hello there!",
            "concatenated chunks equal the canned text"
        );
        assert_eq!(
            turn_ends.len(),
            1,
            "exactly one stream-turn-end per reaction"
        );
    }

    /// Regression: when the reaction client is the offline Markov simulator,
    /// the LLM path must be skipped so the chat stream never shows Markov
    /// gibberish ("bridget from the new collection... God help us").
    #[tokio::test]
    async fn stream_reaction_texts_skips_llm_when_client_is_simulator() {
        use crate::npc::Npc;
        use crate::npc::reactions::{NpcReaction, ReactionKind};
        use parish_inference::{AnyClient, simulator::SimulatorClient};
        use std::sync::Arc;

        let reaction = NpcReaction {
            npc_id: NpcId(42),
            npc_display_name: "Bridie".to_string(),
            kind: ReactionKind::Greeting,
            canned_text: "Welcome, stranger.".to_string(),
            introduces: false,
            use_llm: true,
        };
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(42);
        npc.name = "Bridie".to_string();

        let client = AnyClient::Simulator(Arc::new(SimulatorClient::new()));
        let mut token_chunks: Vec<String> = Vec::new();

        let lang = crate::npc::LanguageSettings::english_only();
        stream_reaction_texts(
            &[reaction],
            &[npc],
            LocationId(0),
            "Kilteevan",
            crate::world::time::TimeOfDay::Morning,
            "clear",
            &std::collections::HashSet::new(),
            Some(&client),
            "sim",
            None,
            &lang,
            |_, _| {},
            |_turn_id, _source, tok| token_chunks.push(tok.to_string()),
            |_turn_id| {},
        )
        .await;

        let combined = token_chunks.join("");
        assert_eq!(
            combined, "Welcome, stranger.",
            "simulator client must yield canned text, not Markov nonsense"
        );
    }

    /// Helper: find a location in the default mod that has at least one NPC
    /// whose `Present` state puts them there right now.
    fn find_location_with_present_npc(world: &WorldState, mgr: &NpcManager) -> Option<LocationId> {
        world
            .graph
            .location_ids()
            .into_iter()
            .find(|&loc_id| !mgr.npcs_at(loc_id).is_empty())
    }

    /// Regression: calling `apply_arrival_reactions` as a standalone entry
    /// point at a location that has NPCs present must return a non-empty
    /// reaction list AND append canned text to `world.text_log`.
    #[test]
    fn apply_arrival_reactions_standalone_produces_reactions() {
        let Some((mut world, mut mgr, templates, _)) = setup() else {
            return;
        };
        let Some(loc_with_npc) = find_location_with_present_npc(&world, &mgr) else {
            // Default mod should always have at least one NPC somewhere — if
            // not, we don't have a test fixture for this scenario.
            return;
        };

        // Teleport the player directly to the NPC's location — do NOT call
        // apply_movement so we isolate the standalone reaction-application
        // path.
        world.player_location = loc_with_npc;
        let log_len_before = world.text_log.len();

        // Force base_chance = 1.0 so every present NPC reacts regardless of
        // dice rolls; the test is about the pipeline, not the probability model.
        let config = ReactionConfig {
            base_chance: 1.0,
            ..Default::default()
        };
        let reactions = apply_arrival_reactions(&mut world, &mut mgr, &templates, &config);

        assert!(
            !reactions.is_empty(),
            "apply_arrival_reactions at a location with NPCs should yield at least one reaction"
        );
        // Canned text must be logged to the world log.
        assert!(
            world.text_log.len() > log_len_before,
            "apply_arrival_reactions should append canned text to world.text_log"
        );
        // Each reaction should have non-empty canned text.
        for reaction in &reactions {
            assert!(
                !reaction.canned_text.is_empty(),
                "reaction canned_text should not be empty"
            );
        }
    }

    /// Regression: the first call to `apply_arrival_reactions` for an
    /// unknown NPC should mark them introduced so that subsequent display
    /// uses their real name.
    #[test]
    fn apply_arrival_reactions_marks_introductions() {
        let Some((mut world, mut mgr, templates, _)) = setup() else {
            return;
        };
        let Some(loc_with_npc) = find_location_with_present_npc(&world, &mgr) else {
            return;
        };

        world.player_location = loc_with_npc;
        let config = ReactionConfig::default();
        let reactions = apply_arrival_reactions(&mut world, &mut mgr, &templates, &config);

        // For every reaction that says it introduces the NPC, the manager
        // must report that NPC as introduced afterward.
        for reaction in &reactions {
            if reaction.introduces {
                assert!(
                    mgr.is_introduced(reaction.npc_id),
                    "NPC {:?} should be marked introduced after its introducing reaction",
                    reaction.npc_id
                );
            }
        }
    }

    /// Regression: `apply_movement` should reassign NPC cognitive tiers.
    /// Moving into the same location as an NPC should promote them closer
    /// to Tier 1 (distance 0 = Tier 1).
    #[test]
    fn apply_movement_reassigns_tiers_on_arrival() {
        use crate::npc::types::CogTier;

        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        // Baseline tier assignment at starting position.
        mgr.assign_tiers(&world, &[]);

        // Find a neighbor that has a Present NPC we can move toward.
        let neighbors: Vec<LocationId> = world
            .graph
            .neighbors(world.player_location)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let target = neighbors
            .into_iter()
            .find(|&id| !mgr.npcs_at(id).is_empty());
        let Some(target) = target else {
            // No immediate neighbor has an NPC; this mod layout is not
            // tractable for this specific test.
            return;
        };
        let target_name = world
            .graph
            .get(target)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let npc_at_target = mgr
            .npcs_at(target)
            .first()
            .map(|n| n.id)
            .expect("npcs_at target should not be empty");

        // Move there.
        let effects = apply_movement(&mut world, &mut mgr, &templates, &target_name, &transport);
        assert!(effects.world_changed);
        assert_eq!(world.player_location, target);

        // The target-location NPC must now be in Tier 1 (distance 0).
        let tier = mgr.tier_of(npc_at_target).unwrap_or(CogTier::Tier4);
        assert_eq!(
            tier,
            CogTier::Tier1,
            "NPC at the player's location must be promoted to Tier 1"
        );
    }

    // ── Additional coverage ──────────────────────────────────────────────────

    #[test]
    fn reaction_req_id_monotonic() {
        let first = reaction_req_id_peek();
        // The counter starts at 100_000 and only grows; any subsequent read
        // must be >= the first read.
        let second = reaction_req_id_peek();
        assert!(second >= first);
        assert!(first >= 100_000);
    }

    #[test]
    fn apply_movement_not_found_log_contains_exits() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            "definitely-not-a-place-0xdeadbeef",
            &transport,
        );
        // The not-found message should also have been logged to world.log.
        assert!(
            world
                .text_log
                .iter()
                .any(|line| line.contains("faintest notion")),
            "not-found message must be appended to text_log"
        );
        // Effects carry the same message.
        assert_eq!(effects.messages.len(), 1);
        assert!(!effects.world_changed);
    }

    /// Regression: location descriptions and travel narration emitted after a
    /// successful move must carry `source: "system"`, never an NPC name. If
    /// the source ever drifts to an NPC name, the frontend renders the line
    /// as a dialogue bubble (#chat-mistagging report from 10-turn demo).
    #[test]
    fn apply_movement_arrival_messages_are_system_sourced() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let neighbor = world
            .graph
            .neighbors(world.player_location)
            .into_iter()
            .next();
        let Some((neighbor_id, _)) = neighbor else {
            return;
        };
        let neighbor_name = world
            .graph
            .get(neighbor_id)
            .map(|d| d.name.clone())
            .unwrap_or_default();

        let effects = apply_movement(&mut world, &mut mgr, &templates, &neighbor_name, &transport);

        assert!(effects.world_changed);
        assert!(
            !effects.messages.is_empty(),
            "arrival should produce at least one player-visible message"
        );
        for msg in &effects.messages {
            assert_eq!(
                msg.source, "system",
                "post-move message had non-system source {:?}: {}",
                msg.source, msg.text
            );
        }
    }

    #[test]
    fn apply_movement_records_edge_traversal_and_visit() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let start = world.player_location;
        let neighbor = world.graph.neighbors(start).into_iter().next();
        let Some((neighbor_id, _)) = neighbor else {
            return;
        };
        let neighbor_name = world
            .graph
            .get(neighbor_id)
            .map(|d| d.name.clone())
            .unwrap_or_default();

        assert!(!world.visited_locations.contains(&neighbor_id));
        let clock_before = world.clock.now();

        let effects = apply_movement(&mut world, &mut mgr, &templates, &neighbor_name, &transport);

        // World mutations: visited, clock advanced, edge traversal recorded.
        assert!(effects.world_changed);
        assert!(world.visited_locations.contains(&neighbor_id));
        assert!(world.clock.now() > clock_before);

        // Edge traversal is canonical (min, max).
        let key = if start < neighbor_id {
            (start, neighbor_id)
        } else {
            (neighbor_id, start)
        };
        assert_eq!(world.edge_traversals.get(&key).copied(), Some(1));
    }

    #[test]
    fn apply_movement_already_here_explicit() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let exact_name = world.current_location().name.clone();
        let start = world.player_location;
        let text_log_before = world.text_log.len();

        let effects = apply_movement(&mut world, &mut mgr, &templates, &exact_name, &transport);

        // Player location should not change, but the harness currently resolves the
        // *same* name via fuzzy match to the same location — accept either the
        // `AlreadyHere` short-circuit or the `Arrived`-to-self pipeline.
        assert_eq!(world.player_location, start);
        // Either way, at least one line is appended to the log.
        assert!(world.text_log.len() > text_log_before);
        // And at least one user-visible message is emitted.
        assert!(!effects.messages.is_empty());
    }

    #[test]
    fn apply_arrival_reactions_returns_empty_when_no_location_data() {
        // WorldState::new() has a legacy `locations` map but no graph data for
        // the current location — the fast-path should return an empty vec.
        let mut world = WorldState::new();
        let mut mgr = NpcManager::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        let reactions = apply_arrival_reactions(&mut world, &mut mgr, &templates, &config);
        assert!(reactions.is_empty());
    }

    #[tokio::test]
    async fn stream_reaction_texts_empty_list_emits_nothing() {
        let mut log_sources: Vec<String> = Vec::new();
        let mut token_chunks: Vec<String> = Vec::new();
        let mut turn_ends: Vec<u64> = Vec::new();

        let lang = crate::npc::LanguageSettings::english_only();
        stream_reaction_texts(
            &[],
            &[],
            LocationId(0),
            "Galway",
            crate::world::time::TimeOfDay::Morning,
            "clear",
            &std::collections::HashSet::new(),
            None,
            "",
            None,
            &lang,
            |_turn_id, name| log_sources.push(name.to_string()),
            |_turn_id, _source, tok| token_chunks.push(tok.to_string()),
            |turn_id| turn_ends.push(turn_id),
        )
        .await;

        assert!(log_sources.is_empty());
        assert!(token_chunks.is_empty());
        assert!(turn_ends.is_empty());
    }

    /// Regression for the "blank NPC reply" bug: every per-NPC reaction MUST
    /// emit a `stream-turn-end` (callback `emit_stream_turn_end`) after its
    /// token stream finishes — including when the LLM produces zero tokens.
    /// Without this, the frontend's stream-manager never finalises the empty
    /// placeholder bubble and the chat shows a permanent blank entry.
    #[tokio::test]
    async fn stream_reaction_texts_emits_stream_turn_end_for_each_reaction() {
        use crate::npc::reactions::{NpcReaction, ReactionKind};

        // Two reactions with empty canned text — simulates the LLM-disabled
        // path producing nothing visible (worst case for the UI cleanup hook).
        let reactions = vec![
            NpcReaction {
                npc_id: NpcId(1),
                npc_display_name: "Aoife".to_string(),
                kind: ReactionKind::Greeting,
                canned_text: String::new(),
                introduces: false,
                use_llm: false,
            },
            NpcReaction {
                npc_id: NpcId(2),
                npc_display_name: "Brian".to_string(),
                kind: ReactionKind::Greeting,
                canned_text: String::new(),
                introduces: false,
                use_llm: false,
            },
        ];

        let mut placeholder_turn_ids: Vec<u64> = Vec::new();
        let mut turn_end_ids: Vec<u64> = Vec::new();

        let lang = crate::npc::LanguageSettings::english_only();
        stream_reaction_texts(
            &reactions,
            &[],
            LocationId(0),
            "Galway",
            crate::world::time::TimeOfDay::Morning,
            "clear",
            &std::collections::HashSet::new(),
            None,
            "",
            None,
            &lang,
            |turn_id, _name| placeholder_turn_ids.push(turn_id),
            |_turn_id, _source, _tok| { /* no tokens emitted for empty canned */ },
            |turn_id| turn_end_ids.push(turn_id),
        )
        .await;

        // Each reaction must produce exactly one placeholder AND one turn-end,
        // with matching turn_ids in the same order. The UI relies on this 1:1
        // pairing to clean up empty bubbles.
        assert_eq!(
            placeholder_turn_ids.len(),
            2,
            "expected one text-log placeholder per reaction"
        );
        assert_eq!(
            turn_end_ids.len(),
            2,
            "expected one stream-turn-end per reaction (blank-reply regression)"
        );
        assert_eq!(
            placeholder_turn_ids, turn_end_ids,
            "placeholder turn_ids must match stream-turn-end turn_ids 1:1"
        );
    }

    // ── #1224 — cap_dialogue_for_display (AC-6, AC-7) ────────────────────────

    /// AC-7 (fix-1224-1225): dialogue shorter than the cap passes through unchanged.
    #[test]
    fn cap_dialogue_for_display_short_dialogue_unchanged() {
        let short = "Dia dhuit, a chara. What brings ye here?";
        let result = crate::game_session::cap_dialogue_for_display(short, 800);
        assert_eq!(
            result.as_ref(),
            short,
            "short dialogue must not be modified"
        );
    }

    /// AC-6 (fix-1224-1225): dialogue longer than the cap is truncated with `…`.
    #[test]
    fn cap_dialogue_for_display_long_dialogue_truncated() {
        let long = "a".repeat(1000);
        let result = crate::game_session::cap_dialogue_for_display(&long, 800);
        assert!(
            result.len() <= 800,
            "capped dialogue must not exceed max_chars bytes (got {})",
            result.len()
        );
        assert!(
            result.ends_with('…'),
            "truncated dialogue must end with single-codepoint ellipsis"
        );
        assert!(
            !result.ends_with("..."),
            "must use U+2026 ellipsis, not three ASCII dots"
        );
    }

    /// cap is disabled when max_chars is 0.
    #[test]
    fn cap_dialogue_for_display_zero_cap_is_passthrough() {
        let long = "a".repeat(5000);
        let result = crate::game_session::cap_dialogue_for_display(&long, 0);
        assert_eq!(result.len(), 5000, "zero cap must not truncate");
    }

    // ── #1400 — sentence-boundary-aware display cap ──────────────────────────

    /// AC-1 (#1400): a reply that overruns the cap is trimmed back to a sentence
    /// boundary — the result ends on a terminator + `…`, never mid-word or on a
    /// dangling conjunction/comma.
    #[test]
    fn cap_dialogue_sentence_trim_ends_on_clause_boundary() {
        // Two clean sentences, then a long dangling clause that would overrun a
        // small cap mid-word ("...out and about, and …").
        let dialogue = "'Tis a grand morning indeed. The fields are bright with \
            dew and birdsong. 'Tis a fine day to be out and about, and the road \
            north past the low fields is as pleasant a walk as any in the parish";
        // Cap chosen so the raw clip lands inside the third, dangling clause.
        let cap = 95;
        let result =
            crate::game_session::cap_dialogue_for_display_with_trim(dialogue, cap, true);
        assert!(result.ends_with('…'), "must end with ellipsis: {result:?}");
        // Strip the trailing ellipsis and assert the preceding char is a clean
        // sentence terminator (or a quote closing one), never a comma/letter.
        let body = result.trim_end_matches('\u{2026}');
        let last = body.chars().last().unwrap();
        assert!(
            matches!(last, '.' | '!' | '?' | '"' | '\'' | '\u{201D}' | '\u{2019}'),
            "clipped reply must end on a clause boundary, got {last:?}: {result:?}"
        );
        assert!(
            !body.trim_end().ends_with("and") && !body.trim_end().ends_with(','),
            "must not end on a dangling conjunction/comma: {result:?}"
        );
        // It kept at least the first sentence.
        assert!(result.contains("grand morning indeed."));
    }

    /// AC-2 (#1400): a reply already under the cap and ending on a sentence
    /// boundary passes through unchanged (no spurious trimming).
    #[test]
    fn cap_dialogue_sentence_trim_under_cap_unchanged() {
        let s = "Welcome. I run the pub here. What'll ye have?";
        let result = crate::game_session::cap_dialogue_for_display_with_trim(s, 800, true);
        assert_eq!(result.as_ref(), s, "under-cap dialogue must be unchanged");
    }

    /// AC-3 (#1400): a single giant run-on word with no boundary in the budget
    /// falls back to the raw char-boundary clip — never panics, stays ≤ cap,
    /// valid UTF-8.
    #[test]
    fn cap_dialogue_sentence_trim_no_boundary_falls_back() {
        let run_on = "x".repeat(2000);
        let result = crate::game_session::cap_dialogue_for_display_with_trim(&run_on, 800, true);
        assert!(result.len() <= 800, "fallback clip must stay within cap");
        assert!(result.ends_with('…'));
        // Valid UTF-8 (no panic on char iteration).
        let _ = result.chars().count();
    }

    /// AC-3 corollary: multibyte content with no sentence boundary still clips
    /// safely on a char boundary.
    #[test]
    fn cap_dialogue_sentence_trim_multibyte_no_boundary() {
        let irish = "Dia dhuit a chara cén chaoi a bhfuil tú ".repeat(40);
        let result = crate::game_session::cap_dialogue_for_display_with_trim(&irish, 200, true);
        assert!(result.len() <= 200);
        assert!(result.ends_with('…'));
        let _ = result.chars().count(); // must be valid UTF-8
    }

    /// AC-4 (#1400): kill-switch — with sentence-boundary trim disabled, the cap
    /// reverts to the legacy raw char-boundary clip (byte-for-byte identical to
    /// `cap_dialogue_for_display` before this fix).
    #[test]
    fn cap_dialogue_sentence_trim_killswitch_matches_legacy_clip() {
        let dialogue = "'Tis a grand morning indeed. The fields are bright with \
            dew and birdsong. 'Tis a fine day to be out and about, and the road";
        let cap = 95;
        let trimmed_off =
            crate::game_session::cap_dialogue_for_display_with_trim(dialogue, cap, false);
        // Legacy raw clip: floor char boundary at cap-3, then "…".
        let raw_boundary = crate::npc::floor_char_boundary(dialogue, cap - 3);
        let expected = format!("{}\u{2026}", &dialogue[..raw_boundary]);
        assert_eq!(
            trimmed_off.as_ref(),
            expected,
            "kill-switched cap must match the legacy raw char-boundary clip"
        );
    }

    /// `apply_npc_dialogue_turn` stores capped dialogue in the DialogueOccurred event.
    #[test]
    fn apply_npc_dialogue_turn_caps_dialogue_in_event() {
        use crate::npc::manager::NpcManager;
        use crate::npc::{NpcId, NpcStreamResponse};
        use chrono::TimeZone;
        use parish_types::events::GameEvent;
        use parish_world::{LocationId, WorldState};

        let mut world = WorldState::new();
        // Subscribe before calling apply so we receive the event.
        let mut rx = world.event_bus.subscribe();
        let mut npc_manager = NpcManager::new();

        // Synthesise an NPC at the player's start location.
        let npc = crate::npc::Npc {
            id: NpcId(1),
            location: world.player_location,
            state: crate::npc::types::NpcState::Present,
            ..crate::npc::Npc::new_test_npc()
        };
        npc_manager.add_npc(npc);

        let long_dialogue = "word ".repeat(300); // ~1500 chars, well over 800
        let parsed = NpcStreamResponse {
            dialogue: long_dialogue.clone(),
            metadata: None,
        };
        let game_time = chrono::Utc
            .with_ymd_and_hms(1820, 3, 20, 17, 30, 0)
            .unwrap();

        crate::game_session::apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(1),
            &parsed,
            "tell me a story",
            "tell me a story",
            game_time,
            LocationId(1),
            "Padraig",
            "Padraig O'Brien",
            None,
        );

        // The DialogueOccurred event published to the bus must carry the
        // capped dialogue, not the full 1500-char original.
        let default_cap = crate::config::NpcConfig::default().dialogue_display_max_chars;
        // try_recv pulls events without async: the broadcast tx just sent one.
        let event = rx
            .try_recv()
            .expect("DialogueOccurred event must be published to the bus");

        if let GameEvent::DialogueOccurred { npc_said, .. } = event {
            let said = npc_said.as_deref().unwrap_or("");
            assert!(
                said.len() <= default_cap,
                "AC-6: npc_said in event ({} bytes) must not exceed cap ({} bytes)",
                said.len(),
                default_cap
            );
        } else {
            panic!("Expected DialogueOccurred event, got something else");
        }
    }
}
