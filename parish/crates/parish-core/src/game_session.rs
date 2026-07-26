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

use crate::config::{FeatureFlags, ReactionConfig};
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

/// Feature-flag name (default **off**) that enables spontaneous NPC arrival
/// greetings.
///
/// When the player moves into a populated location, [`apply_arrival_reactions`]
/// normally generates greet / welcome / introduce / nod lines. With this flag
/// unset (the default, since `FeatureFlags::is_enabled` returns `false` for
/// unknown flags) those spontaneous greetings are suppressed: NPCs only speak
/// when the player addresses them. Enable with `/flag enable npc-arrival-greetings`
/// to restore the lively-arrival behavior.
///
/// Muting greetings can leave NPCs anonymous until they actually say their name
/// in dialogue; merely beginning a conversation does not reveal identity
/// (#1776). The background social simulation (Tier 2 gossip, mood drift,
/// schedules) is likewise untouched; only the visible arrival lines are gated.
pub const NPC_ARRIVAL_GREETINGS_FLAG: &str = "npc-arrival-greetings";

/// Default-on kill switch for durable player task assignment and progression.
///
/// Disable with `/flag disable player-task-progression` to preserve action
/// narration while suppressing task state mutations and semantic task events.
pub const PLAYER_TASK_PROGRESSION_FLAG: &str = "player-task-progression";

/// Deterministic player opt-in for accepting a model-proposed task.
///
/// Restricting assignment to an explicit work/help request lets runtimes know
/// before inference which dialogue turns can mutate the durable task ledger,
/// so those turns can be staged atomically without delaying every ordinary
/// conversation.
pub fn is_task_request_input(input: &str) -> bool {
    let normalized = input.trim().to_ascii_lowercase();
    [
        "any work",
        "have work",
        "work for me",
        "need help",
        "can i help",
        "how can i help",
        "what can i do",
        "what needs doing",
        "anything needs doing",
        "anything need doing",
        "where should i begin",
        "where do i begin",
        "give me a task",
        "have a task",
        "any tasks",
        "any jobs",
        "have a job",
        "chores",
        "errand",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
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

/// Authoritative result of applying a physical player action.
#[derive(Debug, Clone)]
pub struct PlayerActionOutcome {
    /// Existing second-person narration preserved for every runtime.
    pub narration: String,
    /// Canonical post-mutation task record when this action started one
    /// unambiguous same-location assignment.
    pub progressed_task: Option<parish_types::PlayerTask>,
}

// ── Core functions ────────────────────────────────────────────────────────────

/// Applies a physical action through the shared cross-runtime seam.
///
/// Action narration is always returned. When [`PLAYER_TASK_PROGRESSION_FLAG`]
/// is enabled (the default), one uniquely matching same-location assignment may
/// transition from `Assigned` to `InProgress`; actions never infer completion.
/// A successful transition publishes exactly one semantic
/// [`GameEvent::PlayerTaskProgressed`](parish_types::GameEvent::PlayerTaskProgressed)
/// carrying the authoritative post-mutation task record.
pub fn apply_player_action(
    world: &mut WorldState,
    raw_action: &str,
    flags: &FeatureFlags,
) -> Option<PlayerActionOutcome> {
    let narration = player_action_narration(raw_action)?;
    let progressed_task = if flags.is_disabled(PLAYER_TASK_PROGRESSION_FLAG) {
        None
    } else {
        let location = world.player_location;
        let timestamp = world.clock.now();
        world
            .player_progress
            .advance_assigned_task(raw_action, location, timestamp)
            .and_then(|task_id| world.player_progress.task(task_id).cloned())
    };

    if let Some(task) = progressed_task.as_ref() {
        let action = task.last_matching_action.clone().unwrap_or_default();
        world
            .event_bus
            .publish(parish_types::GameEvent::PlayerTaskProgressed {
                task: task.clone(),
                previous_status: parish_types::TaskStatus::Assigned,
                action,
                timestamp: task.started_at.unwrap_or_else(|| world.clock.now()),
            });
    }

    Some(PlayerActionOutcome {
        narration,
        progressed_task,
    })
}

fn player_action_narration(raw_action: &str) -> Option<String> {
    // Preserve the existing narration contract: strip a first-person "I ",
    // lowercase the action's first character, and normalize one trailing full
    // stop. This keeps #1780's visible result unchanged while progression is
    // added underneath it.
    let trimmed = raw_action.trim().trim_end_matches('.');
    let action = trimmed
        .get(..2)
        .filter(|prefix| prefix.eq_ignore_ascii_case("i "))
        .map_or(trimmed, |_| trimmed[2..].trim_start());
    let mut chars = action.chars();
    let first = chars.next()?;
    let normalized = first.to_lowercase().collect::<String>() + chars.as_str();
    if normalized.is_empty() {
        return None;
    }
    Some(format!("You {normalized}."))
}

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
    flags: &FeatureFlags,
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

            // Seed a player-arrival rumour into the gossip network so NPCs can
            // later learn and spread word of the stranger's movements.
            //
            // Gated behind `player-action-gossip` (default-on via `is_disabled`).
            // The player's synthetic NpcId is NpcId(0); see the guard comment in
            // `create_gossip_from_tier2_event` which explicitly avoids that id as
            // a default for NPC-sourced gossip.
            if !flags.is_disabled("player-action-gossip") {
                let loc_name = world
                    .graph
                    .get(destination)
                    .map(|d| d.name.as_str())
                    .unwrap_or("somewhere");
                let content = format!("A stranger arrived at {loc_name}");
                let gossip_id =
                    world
                        .gossip_network
                        .create(content.clone(), NpcId(0), world.clock.now());
                tracing::debug!(
                    gossip_id,
                    location = %loc_name,
                    "[gossip] player arrival seeded: {content}"
                );
                world
                    .event_bus
                    .publish(parish_types::events::GameEvent::GossipSpread {
                        source: NpcId(0),
                        location: destination,
                        content,
                        timestamp: world.clock.now(),
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
            //
            // Gated behind `npc-arrival-greetings` (default-off via `is_enabled`,
            // so unknown == suppressed). When off, the arrival is silent: no
            // greeting is generated, logged, or streamed, and no NPC is
            // introduced via the arrival path — introductions still happen on
            // first conversation (see `ipc::handlers`). The background social
            // simulation is untouched (this gates only the visible greeting).
            let arrival_reactions = if flags.is_enabled(NPC_ARRIVAL_GREETINGS_FLAG) {
                apply_arrival_reactions(
                    world,
                    npc_manager,
                    reaction_templates,
                    &ReactionConfig::default(),
                )
            } else {
                Vec::new()
            };

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

    if sentence_boundary_trim && let Some(end) = last_sentence_boundary(&dialogue[..raw_safe]) {
        // `end` is a byte index just past a terminator (and any trailing
        // closing quote) — a clean clause end. Only used when non-empty so
        // we never collapse a long run-on to a bare "…".
        return std::borrow::Cow::Owned(format!("{}\u{2026}", &dialogue[..end]));
    }
    std::borrow::Cow::Owned(format!("{}\u{2026}", &dialogue[..raw_safe]))
}

/// Returns the byte index just past the last sentence boundary in `s`, or
/// `None` if there is no usable boundary (so the caller falls back to the raw
/// clip). A boundary is a sentence terminator optionally followed by a single
/// closing quote (`"` / `'` / `\u{201D}` / `\u{2019}`); the index is advanced
/// past that quote so the clause closes cleanly.
fn last_sentence_boundary(s: &str) -> Option<usize> {
    // Scan backward so we short-circuit at the first terminator we find
    // (which is the last one in forward order) rather than walking the whole
    // string to track a running `last` pointer.
    for (idx, ch) in s.char_indices().rev() {
        if SENTENCE_TERMINATORS.contains(&ch) {
            let mut end = idx + ch.len_utf8();
            // Absorb a single trailing closing quote so `"...home."` keeps the quote.
            if let Some(next) = s[end..].chars().next()
                && matches!(next, '"' | '\'' | '\u{201D}' | '\u{2019}')
            {
                end += next.len_utf8();
            }
            // Reject empty (would collapse to a bare ellipsis) or at the very
            // start.
            if end == 0 {
                return None;
            }
            // A boundary at exactly `bytes_len` is still a clean clause end —
            // keep it as long as it leaves real content.
            return Some(end);
        }
    }
    None
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
    /// Secondary-language hints validated against `display_text` and the active
    /// setting's curated native-language inventory (#1789).
    pub language_hints: Vec<crate::npc::LanguageHint>,
    /// Canonical task post-state when the delivered dialogue assigned a task.
    ///
    /// Callers persist this exact record before acknowledging the player turn;
    /// replay never re-runs model-output interpretation.
    pub assigned_task: Option<parish_types::PlayerTask>,
}

#[cfg(test)]
fn task_proposal_is_grounded_in_final_dialogue(proposal: &str, final_dialogue: &str) -> bool {
    grounded_task_assignment_clause(proposal, final_dialogue).is_some()
}

fn grounded_task_assignment_clause<'a>(proposal: &str, final_dialogue: &'a str) -> Option<&'a str> {
    let proposal_verb = positive_task_directive_verb(proposal)?;
    let proposal_tokens = task_grounding_tokens(proposal);
    if proposal_tokens.len() < 2 {
        return None;
    }
    let required_overlap = 2.max(proposal_tokens.len().div_ceil(2));

    let direct_clause = final_dialogue
        .split_inclusive(['.', '!', '?', ';', '\n', '\u{2014}'])
        .find(|clause| {
            let clause = clause.trim();
            let Some(dialogue_verb) = clause_assignment_verb(clause) else {
                return false;
            };
            let matched_verb = if dialogue_verb == proposal_verb || dialogue_verb == "help" {
                dialogue_verb
            } else if nested_start_gerund_verb(clause) == Some(proposal_verb) {
                proposal_verb
            } else {
                return false;
            };
            let mut dialogue_tokens = task_grounding_tokens(clause);
            // The assignment grammar already proves that an inflected form
            // such as "breaking" names the proposal's work verb. Include its
            // canonical form in the lexical overlap instead of requiring a
            // second copy of the imperative "break" in the spoken clause.
            dialogue_tokens.insert(matched_verb.to_string());
            proposal_tokens.intersection(&dialogue_tokens).count() >= required_overlap
        });
    direct_clause.or_else(|| {
        final_dialogue
            .split_inclusive(['.', '!', '?', ';', '\n'])
            .find(|clause| {
                let clause = clause.trim();
                if implied_need_assignment_verb(clause) != Some(proposal_verb) {
                    return false;
                }
                let mut dialogue_tokens = task_grounding_tokens(clause);
                dialogue_tokens.insert(proposal_verb.to_string());
                proposal_tokens.intersection(&dialogue_tokens).count() >= required_overlap
            })
    })
}

fn nested_start_gerund_verb(clause: &str) -> Option<&'static str> {
    let lower = clause
        .trim()
        .trim_end_matches(['.', '!', '?', ';', '\n', '\u{2014}'])
        .to_lowercase()
        .replace('\u{2019}', "'");
    let body = [" and start ", " then start "]
        .iter()
        .find_map(|separator| lower.split_once(separator).map(|(_, body)| body))?;
    let body = body.strip_prefix("by ").unwrap_or(body);
    leading_work_gerund(body)
}

fn implied_need_assignment_verb(clause: &str) -> Option<&'static str> {
    let trimmed = clause.trim();
    if trimmed.is_empty()
        || trimmed.starts_with(['"', '\'', '\u{2018}', '\u{201C}'])
        || assignment_language_is_negative_or_avoidant(&trimmed.to_lowercase())
    {
        return None;
    }
    let lower = trimmed
        .trim_end_matches(['.', '!', '?', ';', '\n'])
        .to_lowercase()
        .replace('\u{2019}', "'");
    let (_, need_body) = lower.split_once(" needs ")?;
    let verb = leading_work_gerund(need_body)?;
    [
        "\u{2014} start there",
        "- start there",
        "\u{2014} begin there",
        "- begin there",
        ", start there",
        ", begin there",
    ]
    .iter()
    .any(|directive| need_body.contains(directive))
    .then_some(verb)
}

fn positive_task_directive_verb(value: &str) -> Option<&'static str> {
    let lower = value.trim().to_lowercase();
    if assignment_language_is_negative_or_avoidant(&lower) {
        return None;
    }
    leading_work_verb(lower.trim_end_matches(['.', '!', ';']))
}

fn clause_assignment_verb(clause: &str) -> Option<&'static str> {
    let trimmed = clause.trim();
    if trimmed.is_empty()
        || trimmed.starts_with(['"', '\'', '\u{2018}', '\u{201C}'])
        || assignment_language_is_negative_or_avoidant(&trimmed.to_lowercase())
    {
        return None;
    }

    let is_question = trimmed.ends_with('?');
    let lower = trimmed
        .trim_end_matches(['.', '!', '?', ';', '\n', '\u{2014}'])
        .trim()
        .to_lowercase()
        .replace('\u{2019}', "'");
    let clause_start = lower
        .strip_prefix("first,")
        .or_else(|| lower.strip_prefix("first "))
        .unwrap_or(&lower)
        .trim_start();

    let request_prefixes = [
        "could you ",
        "could ye ",
        "would you ",
        "would ye ",
        "can you ",
        "can ye ",
        "will you ",
        "will ye ",
        "i need you to ",
        "i need ye to ",
        "i'd have you ",
        "i'd have ye ",
    ];
    if let Some(body) = request_prefixes
        .iter()
        .find_map(|prefix| clause_start.strip_prefix(prefix))
    {
        return requested_work_verb(body);
    }

    if let Some(body) = clause_start.strip_prefix("please ") {
        return requested_work_verb(body);
    }

    let best_start_prefixes = [
        "you'd best start with ",
        "you'd best start by ",
        "ye'd best start with ",
        "ye'd best start by ",
    ];
    if let Some(body) = best_start_prefixes
        .iter()
        .find_map(|prefix| clause_start.strip_prefix(prefix))
    {
        return leading_work_gerund(body);
    }

    // A bare imperative is a direct assignment, but a bare question such as
    // "Dig over the potato patch?" is merely checking/repeating a proposal.
    if is_question {
        return None;
    }
    if let Some(body) = clause_start.strip_prefix("start by ") {
        return leading_work_gerund(body);
    }
    leading_work_verb(clause_start)
}

fn requested_work_verb(value: &str) -> Option<&'static str> {
    let value = value.trim_start();
    let value = value.strip_prefix("please ").unwrap_or(value);
    if let Some(body) = value.strip_prefix("mind ") {
        return leading_work_gerund(body);
    }
    if let Some(body) = value.strip_prefix("start by ") {
        return leading_work_gerund(body);
    }
    leading_work_verb(value)
}

fn leading_work_verb(value: &str) -> Option<&'static str> {
    let value = value.trim_start();
    if value.starts_with("see to ") {
        return Some("see_to");
    }
    if value.starts_with("take care of ") {
        return Some("take_care_of");
    }
    if value.starts_with("help with ") {
        return Some("help");
    }

    let first_word = value
        .split(|character: char| !character.is_alphanumeric())
        .next()
        .unwrap_or_default();
    Some(match first_word {
        "break" => "break",
        "bring" => "bring",
        "carry" => "carry",
        "clean" => "clean",
        "clear" => "clear",
        "collect" => "collect",
        "cut" => "cut",
        "dig" => "dig",
        "draw" => "draw",
        "feed" => "feed",
        "fetch" => "fetch",
        "fill" => "fill",
        "gather" => "gather",
        "harvest" => "harvest",
        "hoe" => "hoe",
        "mend" => "mend",
        "milk" => "milk",
        "plant" => "plant",
        "rake" => "rake",
        "repair" => "repair",
        "sow" => "sow",
        "stack" => "stack",
        "sweep" => "sweep",
        "tend" => "tend",
        "turn" => "turn",
        "weed" => "weed",
        _ => return None,
    })
}

fn leading_work_gerund(value: &str) -> Option<&'static str> {
    let value = value.trim_start();
    if value.starts_with("seeing to ") {
        return Some("see_to");
    }
    if value.starts_with("taking care of ") {
        return Some("take_care_of");
    }

    let first_word = value
        .split(|character: char| !character.is_alphanumeric())
        .next()
        .unwrap_or_default();
    Some(match first_word {
        "breaking" => "break",
        "bringing" => "bring",
        "carrying" => "carry",
        "cleaning" => "clean",
        "clearing" => "clear",
        "collecting" => "collect",
        "cutting" => "cut",
        "digging" => "dig",
        "drawing" => "draw",
        "feeding" => "feed",
        "fetching" => "fetch",
        "filling" => "fill",
        "gathering" => "gather",
        "harvesting" => "harvest",
        "hoeing" => "hoe",
        "mending" => "mend",
        "milking" => "milk",
        "planting" => "plant",
        "raking" => "rake",
        "repairing" => "repair",
        "sowing" => "sow",
        "stacking" => "stack",
        "sweeping" => "sweep",
        "tending" => "tend",
        "turning" => "turn",
        "weeding" => "weed",
        _ => return None,
    })
}

fn assignment_language_is_negative_or_avoidant(value: &str) -> bool {
    let lower = value.to_lowercase().replace('\u{2019}', "'");
    let words: HashSet<&str> = lower
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();

    words.iter().any(|word| {
        matches!(
            *word,
            "no" | "not"
                | "never"
                | "nothing"
                | "cannot"
                | "dont"
                | "cant"
                | "wont"
                | "neednt"
                | "avoid"
                | "avoids"
                | "avoided"
                | "avoiding"
                | "remember"
                | "remembers"
                | "remembered"
                | "remind"
                | "reminds"
                | "reminded"
                | "report"
                | "reports"
                | "reported"
                | "recall"
                | "recalls"
                | "recalled"
                | "quote"
                | "quotes"
                | "quoted"
                | "finished"
                | "completed"
                | "done"
                | "dug"
                | "weeded"
                | "repaired"
                | "mended"
                | "cleared"
                | "carried"
                | "fetched"
                | "harvested"
                | "planted"
                | "sowed"
                | "stacked"
                | "swept"
                | "tended"
                | "cleaned"
                | "collected"
                | "remembering"
                | "reminding"
                | "reporting"
                | "recalling"
                | "quoting"
        )
    }) || [
        "no work",
        "don't ",
        "do not ",
        "can't ",
        "cannot ",
        "won't ",
        "will not ",
        "needn't ",
        "instead of",
        "rather than",
        "move away",
        "stay away",
        "keep away",
        "clear out",
        "break the news",
        "break the silence",
        "break the ice",
        "clear the air",
        "bring the matter up",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
        || (words.contains("leave") && words.contains("alone"))
        || words.contains("already")
}

fn task_proposal_names_remote_location(
    world: &WorldState,
    proposal: &str,
    authoritative_location: LocationId,
) -> bool {
    let normalized_proposal = normalized_phrase(proposal);
    world.graph.location_ids().into_iter().any(|location_id| {
        if location_id == authoritative_location {
            return false;
        }
        let Some(location) = world.graph.get(location_id) else {
            return false;
        };

        phrase_is_contained(&normalized_proposal, &normalized_phrase(&location.name))
            || location.aliases.iter().any(|alias| {
                let normalized_alias = normalized_phrase(alias);
                if normalized_alias.split_whitespace().count() >= 2 {
                    return phrase_is_contained(&normalized_proposal, &normalized_alias);
                }
                ["at", "in", "inside", "near", "outside", "by", "to", "from"]
                    .iter()
                    .any(|preposition| {
                        phrase_is_contained(
                            &normalized_proposal,
                            &format!("{preposition} {normalized_alias}"),
                        ) || phrase_is_contained(
                            &normalized_proposal,
                            &format!("{preposition} the {normalized_alias}"),
                        )
                    })
                    || phrase_is_contained(&normalized_proposal, &format!("the {normalized_alias}"))
            })
    })
}

fn normalized_phrase(value: &str) -> String {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn phrase_is_contained(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && format!(" {haystack} ").contains(&format!(" {needle} "))
}

fn task_grounding_tokens(value: &str) -> HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|token| token.chars().count() >= 3)
        .filter(|token| {
            !matches!(
                token.as_str(),
                "and"
                    | "for"
                    | "from"
                    | "help"
                    | "into"
                    | "over"
                    | "start"
                    | "the"
                    | "then"
                    | "there"
                    | "this"
                    | "with"
                    | "work"
                    | "you"
                    | "your"
            )
        })
        .collect()
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
    grounded_person_names: &[String],
    language: &LanguageSettings,
    flags: &FeatureFlags,
) -> DialogueTurnOutcome {
    let mut debug_events = Vec::new();

    // 1. Learn the player's name from a self-introduction *before* recording
    //    memory, so the addressed speaker's memory uses the real name (#1028).
    crate::ipc::detect_and_record_player_name(world, npc_manager, player_input, speaker_id);

    // Canonical semantic guards run before memory/state application so every
    // runtime records the same grounded text. These checks use authored state
    // and the canonical conversation log rather than trusting model metadata.
    let had_prior_exchange = world.conversation_log.has_exchange_with(speaker_id);
    let canonical_mood = npc_manager
        .get(speaker_id)
        .map(|npc| npc.mood.clone())
        .unwrap_or_default();
    let mut work_roster_with_ids: Vec<(NpcId, String, String, Option<String>)> = npc_manager
        .all_npcs()
        .map(|person| {
            let workplace = person
                .workplace
                .and_then(|location_id| world.graph.get(location_id))
                .map(|location| location.name.clone());
            (
                person.id,
                person.name.clone(),
                person.occupation.clone(),
                workplace,
            )
        })
        .collect();
    work_roster_with_ids.sort_by_key(|(id, _, _, _)| id.0);
    let work_roster: Vec<(String, String, Option<String>)> = work_roster_with_ids
        .into_iter()
        .map(|(_, name, occupation, workplace)| (name, occupation, workplace))
        .collect();

    let mut canonical_response = parsed.clone();
    canonical_response.dialogue =
        crate::npc::guard_mood_register(&canonical_response.dialogue, &canonical_mood);
    canonical_response.dialogue = crate::npc::guard_unfounded_first_contact_familiarity(
        &canonical_response.dialogue,
        had_prior_exchange,
    );
    canonical_response.dialogue =
        crate::npc::guard_direct_evidence_evasion(&canonical_response.dialogue, player_input);
    canonical_response.dialogue = crate::npc::guard_work_recommendation(
        &canonical_response.dialogue,
        player_input,
        &work_roster,
    );

    // 2. Tier-1 state update on the speaker.
    let player_name_for_mem = if npc_manager.knows_player_name(speaker_id) {
        world.player_name.clone()
    } else {
        None
    };
    if let Some(npc) = npc_manager.get_mut(speaker_id) {
        debug_events.extend(crate::npc::ticks::apply_tier1_response_with_config(
            npc,
            &canonical_response,
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
        &canonical_response.dialogue,
        previous_line.as_deref(),
        npc_cfg.dialogue_repetition_threshold,
        repetition_seed,
        grounded_person_names,
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
    let language_hints = canonical_response
        .metadata
        .as_ref()
        .map(|metadata| {
            crate::npc::validate_language_hints(
                &metadata.language_hints,
                &capped_dialogue,
                language,
            )
        })
        .unwrap_or_default();
    let assigned_task = if flags.is_disabled(PLAYER_TASK_PROGRESSION_FLAG)
        || !is_task_request_input(player_input)
    {
        None
    } else {
        canonical_response
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.assigned_task.as_deref())
            .and_then(|proposal| {
                let grounding_clause = grounded_task_assignment_clause(proposal, &capped_dialogue)?;
                let authoritative_location = npc_manager.get(speaker_id)?.location();
                if authoritative_location != world.player_location
                    || task_proposal_names_remote_location(world, proposal, authoritative_location)
                    || task_proposal_names_remote_location(
                        world,
                        grounding_clause,
                        authoritative_location,
                    )
                {
                    return None;
                }
                let existing_ids: HashSet<parish_types::PlayerTaskId> = world
                    .player_progress
                    .tasks()
                    .iter()
                    .map(|task| task.id)
                    .collect();
                let task_id = world
                    .player_progress
                    .assign_task(proposal, speaker_id, authoritative_location, game_time)
                    .ok()?;
                (!existing_ids.contains(&task_id))
                    .then(|| world.player_progress.task(task_id).cloned())
                    .flatten()
            })
    };

    // Identity becomes known only when the final delivered line explicitly
    // establishes the speaker's authored full name (#1776). Doing this after
    // every text guard prevents a name removed by post-processing from leaking
    // through the notebook/card state.
    if !npc_manager.is_introduced(speaker_id)
        && crate::npc::dialogue_self_identifies_speaker(&capped_dialogue, speaker_actual_name)
    {
        npc_manager.mark_introduced(speaker_id);
        debug_events.push(format!(
            "{} introduced themselves to the player",
            speaker_actual_name
        ));
    }

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
    if let Some(task) = assigned_task.as_ref() {
        world
            .event_bus
            .publish(parish_types::GameEvent::PlayerTaskAssigned {
                timestamp: task.assigned_at,
                task: task.clone(),
            });
    }

    DialogueTurnOutcome {
        debug_events,
        display_text: capped_dialogue.into_owned(),
        language_hints,
        assigned_task,
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
/// - `emit_text_log(turn_id, npc_name, subtype)` — called once per reaction
///   to create an empty placeholder in the frontend chat log before streaming
///   begins. `subtype` is `Some("action")` for non-verbal reactions (e.g.
///   `ReactionKind::Gesture`) and `None` for verbal ones. The implementation
///   MUST tie the placeholder to `turn_id` via `text_log_for_stream_turn` (or
///   `text_log_for_stream_turn_typed` when subtype is `Some`) so the UI's
///   streaming-placeholder guard recognises it and `finalizeStreamingEntry` can
///   remove it when the turn ends with no tokens (otherwise an empty bubble
///   lingers in the chat).
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
    mut emit_text_log: impl FnMut(u64, &str, Option<&'static str>),
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

        // Derive the frontend subtype from the reaction kind: non-verbal reactions
        // (Gesture and any future non-verbal kind) carry `subtype: "action"` so
        // the UI renders them as italicised narration rather than a speech bubble.
        let reaction_subtype: Option<&'static str> =
            if reaction.kind == crate::npc::reactions::ReactionKind::Gesture {
                Some("action")
            } else {
                None
            };

        // Emit an empty placeholder so the frontend shows the NPC name immediately
        // and the stream-pump knows which entry to fill.
        emit_text_log(turn_id, &reaction.npc_display_name, reaction_subtype);

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
    use crate::npc::reactions::reaction_threshold;
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
        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            &loc,
            &transport,
            &FeatureFlags::default(),
        );
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
            &FeatureFlags::default(),
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
        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            &neighbor_name,
            &transport,
            &FeatureFlags::default(),
        );
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
            |_turn_id, name, _subtype| log_sources.push(name.to_string()),
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
            |_, _, _| {},
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

    /// Helper for the `npc-arrival-greetings` gate tests: find a one-hop arrival
    /// into a populated location where at least one present NPC is *guaranteed*
    /// to react (its `reaction_threshold` clamps to 1.0 at an indoor workplace),
    /// so the assertion is deterministic and not at the mercy of the dice.
    ///
    /// Returns `(origin, destination, destination_name)` — set the player at
    /// `origin` and `apply_movement` to `destination_name`. `None` if the default
    /// mod / current time-of-day offers no guaranteed greeter (test no-ops).
    fn find_one_hop_to_guaranteed_greeter(
        world: &WorldState,
        mgr: &NpcManager,
    ) -> Option<(LocationId, LocationId, String)> {
        let config = ReactionConfig::default();
        let tod = world.clock.time_of_day();
        for dest in world.graph.location_ids() {
            let present = mgr.npcs_at(dest);
            if present.is_empty() {
                continue;
            }
            let Some(dest_data) = world.graph.get(dest) else {
                continue;
            };
            let guaranteed = present
                .iter()
                .copied()
                .any(|npc| reaction_threshold(npc, dest_data, tod, &config) >= 1.0);
            if !guaranteed {
                continue;
            }
            if let Some((origin, _)) = world.graph.neighbors(dest).into_iter().next() {
                return Some((origin, dest, dest_data.name.clone()));
            }
        }
        None
    }

    /// With the `npc-arrival-greetings` flag at its default (off), arriving at a
    /// populated location must produce NO arrival greetings — even at a location
    /// where an NPC would otherwise be guaranteed to react. This is the core
    /// suppression proof and also catches an inverted gate condition (an inverted
    /// gate would yield the guaranteed reaction here and fail the assert).
    #[test]
    fn apply_movement_suppresses_arrival_greetings_when_flag_off() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let (origin, _dest, dest_name) = find_one_hop_to_guaranteed_greeter(&world, &mgr)
            .expect("default mod must provide a one-hop arrival with a guaranteed greeter");
        world.player_location = origin;

        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            &dest_name,
            &transport,
            &FeatureFlags::default(),
        );

        assert!(
            effects.world_changed,
            "precondition: the player should have arrived at the populated destination"
        );
        assert!(
            effects.arrival_reactions.is_empty(),
            "arrival greetings must be suppressed when npc-arrival-greetings is off (default)"
        );
    }

    /// Enabling `npc-arrival-greetings` restores arrival greetings: arriving at a
    /// location with a guaranteed-reactor NPC yields at least one reaction. This
    /// pins the gate to the exact flag constant — checking a different flag would
    /// leave this empty and fail.
    #[test]
    fn apply_movement_emits_arrival_greetings_when_flag_enabled() {
        let Some((mut world, mut mgr, templates, transport)) = setup() else {
            return;
        };
        let (origin, _dest, dest_name) = find_one_hop_to_guaranteed_greeter(&world, &mgr)
            .expect("default mod must provide a one-hop arrival with a guaranteed greeter");
        world.player_location = origin;

        let mut flags = FeatureFlags::default();
        flags.enable(NPC_ARRIVAL_GREETINGS_FLAG);

        let effects = apply_movement(
            &mut world, &mut mgr, &templates, &dest_name, &transport, &flags,
        );

        assert!(effects.world_changed);
        assert!(
            !effects.arrival_reactions.is_empty(),
            "a guaranteed-reactor NPC must greet on arrival when npc-arrival-greetings is enabled"
        );
    }

    /// Pin the flag name — the live `/flag enable npc-arrival-greetings` command,
    /// docs, and fixture all depend on this exact string.
    #[test]
    fn npc_arrival_greetings_flag_name_is_stable() {
        assert_eq!(NPC_ARRIVAL_GREETINGS_FLAG, "npc-arrival-greetings");
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
        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            &target_name,
            &transport,
            &FeatureFlags::default(),
        );
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
            &FeatureFlags::default(),
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

        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            &neighbor_name,
            &transport,
            &FeatureFlags::default(),
        );

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

        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            &neighbor_name,
            &transport,
            &FeatureFlags::default(),
        );

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

        let effects = apply_movement(
            &mut world,
            &mut mgr,
            &templates,
            &exact_name,
            &transport,
            &FeatureFlags::default(),
        );

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
            |_turn_id, name, _subtype| log_sources.push(name.to_string()),
            |_turn_id, _source, tok| token_chunks.push(tok.to_string()),
            |turn_id| turn_ends.push(turn_id),
        )
        .await;

        assert!(log_sources.is_empty());
        assert!(token_chunks.is_empty());
        assert!(turn_ends.is_empty());
    }

    /// Item 2 (#1431): a `Gesture` reaction must emit `subtype: Some("action")` so
    /// the frontend can render it as italicised narration rather than a speech bubble.
    /// A `Greeting` reaction must emit `subtype: None` (verbal — rendered as a bubble).
    #[tokio::test]
    async fn stream_reaction_texts_gesture_emits_action_subtype() {
        use crate::npc::reactions::{NpcReaction, ReactionKind};

        let gesture = NpcReaction {
            npc_id: NpcId(1),
            npc_display_name: "Siobhan".to_string(),
            kind: ReactionKind::Gesture,
            canned_text: "looks up briefly".to_string(),
            introduces: false,
            use_llm: false,
        };
        let greeting = NpcReaction {
            npc_id: NpcId(2),
            npc_display_name: "Cormac".to_string(),
            kind: ReactionKind::Greeting,
            canned_text: "Good day to ye".to_string(),
            introduces: false,
            use_llm: false,
        };

        let mut subtypes: Vec<Option<&'static str>> = Vec::new();

        let lang = crate::npc::LanguageSettings::english_only();
        stream_reaction_texts(
            &[gesture, greeting],
            &[],
            LocationId(0),
            "Kilteevan",
            crate::world::time::TimeOfDay::Morning,
            "clear",
            &std::collections::HashSet::new(),
            None,
            "",
            None,
            &lang,
            |_turn_id, _name, subtype| subtypes.push(subtype),
            |_turn_id, _source, _tok| {},
            |_turn_id| {},
        )
        .await;

        assert_eq!(subtypes.len(), 2, "one subtype call per reaction");
        assert_eq!(
            subtypes[0],
            Some("action"),
            "Gesture reaction must carry subtype 'action'"
        );
        assert_eq!(
            subtypes[1], None,
            "Greeting reaction must carry no subtype (verbal)"
        );
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
            |turn_id, _name, _subtype| placeholder_turn_ids.push(turn_id),
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
        let result = crate::game_session::cap_dialogue_for_display_with_trim(dialogue, cap, true);
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
        let mut npc = crate::npc::Npc::new_test_npc();
        npc.id = NpcId(1);
        npc.set_location(world.player_location);
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
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
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

    #[test]
    fn dialogue_turn_reveals_identity_only_after_spoken_full_name() {
        use crate::npc::manager::NpcManager;
        use crate::npc::{LanguageSettings, NpcId, NpcStreamResponse};
        use chrono::TimeZone;
        use parish_world::WorldState;

        let mut world = WorldState::new();
        let mut npc_manager = NpcManager::new();
        let mut npc = crate::npc::Npc::new_test_npc();
        npc.id = NpcId(22);
        npc.name = "Peig Hannigan".to_string();
        npc.brief_description = "an elderly widow".to_string();
        npc.set_location(world.player_location);
        npc_manager.add_npc(npc);
        let location = world.player_location;
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap();

        let unnamed = NpcStreamResponse {
            dialogue: "Good morning. What brings ye here?".to_string(),
            metadata: None,
        };
        crate::game_session::apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(22),
            &unnamed,
            "Might I ask your name?",
            "Might I ask your name?",
            game_time,
            location,
            "an elderly widow",
            "Peig Hannigan",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );
        assert!(
            !npc_manager.is_introduced(NpcId(22)),
            "an exchange with no spoken identity must not reveal the NPC"
        );

        let named = NpcStreamResponse {
            dialogue: "Peig Hannigan's the name. What brings ye here?".to_string(),
            metadata: None,
        };
        crate::game_session::apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(22),
            &named,
            "I did ask your name.",
            "I did ask your name.",
            game_time + chrono::Duration::minutes(10),
            location,
            "an elderly widow",
            "Peig Hannigan",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );
        assert!(
            npc_manager.is_introduced(NpcId(22)),
            "the canonical delivered self-identification must reveal the NPC"
        );
    }

    #[test]
    fn dialogue_turn_validates_hints_against_final_delivered_text() {
        use crate::npc::manager::NpcManager;
        use crate::npc::{LanguageHint, LanguageSettings, NpcId, NpcMetadata, NpcStreamResponse};
        use chrono::TimeZone;
        use parish_world::WorldState;

        let mut world = WorldState::new();
        let mut npc_manager = NpcManager::new();
        let mut npc = crate::npc::Npc::new_test_npc();
        npc.set_location(world.player_location);
        npc_manager.add_npc(npc);
        let location = world.player_location;
        let parsed = NpcStreamResponse {
            dialogue: "Dia dhuit. Listen for the whispers on the road.".to_string(),
            metadata: Some(NpcMetadata {
                action: String::new(),
                mood: "calm".to_string(),
                internal_thought: None,
                language_hints: vec![
                    LanguageHint {
                        word: "whispers".to_string(),
                        pronunciation: "WISP-urs".to_string(),
                        meaning: Some("murmurs".to_string()),
                    },
                    LanguageHint {
                        word: "Dia dhuit".to_string(),
                        pronunciation: "DEE-ah GHWIT".to_string(),
                        meaning: Some("hello".to_string()),
                    },
                ],
                mentioned_people: Vec::new(),
                assigned_task: None,
            }),
        };
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap();
        let outcome = crate::game_session::apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(1),
            &parsed,
            "Good day.",
            "Good day.",
            game_time,
            location,
            "Padraig",
            "Padraig O'Brien",
            None,
            &[],
            &LanguageSettings::new("en-IE", Some("ga-IE".to_string())),
            &FeatureFlags::default(),
        );
        assert_eq!(outcome.language_hints.len(), 1);
        assert_eq!(outcome.language_hints[0].word, "Dia dhuit");
    }

    #[test]
    fn task_proposal_requires_concrete_overlap_and_spoken_assignment() {
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Dig over the potato patch.",
            "First, help with the potato patch — break the clods and plant seed in the open rows."
        ));
        assert!(!task_proposal_is_grounded_in_final_dialogue(
            "Dig over the potato patch.",
            "The potato patch has been hard work this spring."
        ));
        assert!(!task_proposal_is_grounded_in_final_dialogue(
            "Dig over the potato patch.",
            "There is no work for ye in the potato patch today."
        ));
        assert!(!task_proposal_is_grounded_in_final_dialogue(
            "Mend the west wall.",
            "First, help with the potato patch."
        ));
        assert!(!task_proposal_is_grounded_in_final_dialogue(
            "Ask Siobhan at the farm.",
            "You can ask Siobhan at the farm about digging the potato patch."
        ));
        assert!(!task_proposal_is_grounded_in_final_dialogue(
            "Help with the potato patch.",
            "Liam will help with the potato patch."
        ));
        assert!(!task_proposal_is_grounded_in_final_dialogue(
            "See Liam about the potato patch.",
            "See, Liam has already dug over the potato patch."
        ));
        assert!(!task_proposal_is_grounded_in_final_dialogue(
            "Take seed to the potato patch.",
            "Take my advice: leave the potato patch alone."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "See to the broken west gate.",
            "First, see to the broken west gate."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Take care of the potato patch.",
            "Take care of the potato patch before sundown."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Mend the west wall.",
            "Could ye mend the west wall?"
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Weed the potato patch.",
            "Would you weed the potato patch?"
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Carry the turf.",
            "Please carry the turf."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Mend the west wall.",
            "Could ye please mend the west wall?"
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Mend the west wall.",
            "Would ye mind mending the west wall?"
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Fetch water from the well.",
            "Plainly, then—Plainly, then—Start by fetching water from the well for the sick woman."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Break stones from the road and carry them to the side.",
            "Good morning. Ye'd best start with breaking stones from the road — the ford needs clearing. Carry the clods to the side of the path."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Mend the west wall.",
            "You’d best start by mending the west wall."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Turn over the soil in the potato patch.",
            "Aye. First, fetch the spade and start turning over the soil in the potato patch."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Turn the potato patch.",
            "'Tis a fine day for work. The potato patch needs turning — start there. Break up the clods and loosen the soil."
        ));
        assert!(!task_proposal_is_grounded_in_final_dialogue(
            "Mend the west wall.",
            "Ye'd best start with sweeping beside the west wall."
        ));
        assert!(task_proposal_is_grounded_in_final_dialogue(
            "Break the stone clods.",
            "Please break the stone clods."
        ));
        for non_assignment in [
            "I need you to remember that Liam already dug over the potato patch.",
            "I need you to report that Liam repaired the potato patch.",
            "Help me remember that Liam dug over the potato patch.",
            "Please carry word that Liam repaired the potato patch.",
            "He said I need you to dig over the potato patch.",
            "Start by leaving the potato patch alone.",
            "I need you to move away from the potato patch.",
            "I need you to clear out the potato patch.",
            "\u{201c}Dig over the potato patch,\u{201d} Liam told me yesterday.",
            "Dig over the potato patch?",
            "Help me count the rows in the potato patch.",
            "Please break the news about the potato patch.",
            "Please break the silence beside the potato patch.",
            "Please break the ice beside the potato patch.",
            "Please clear the air about the potato patch.",
            "Please bring the matter up about the potato patch.",
            "Start by remembering that Liam dug over the potato patch.",
            "Ye'd best start with remembering that Liam dug over the potato patch.",
            "Ye'd best not start with digging over the potato patch.",
            "Liam said ye'd best start with digging over the potato patch.",
            "Please dig no potato patch.",
            "\u{201c}The potato patch needs digging — start there,\u{201d} Liam told me.",
        ] {
            assert!(
                !task_proposal_is_grounded_in_final_dialogue(
                    "Dig over the potato patch.",
                    non_assignment,
                ),
                "{non_assignment:?} must not be treated as a direct assignment"
            );
        }
        for negative_proposal in [
            "Leave the potato patch alone.",
            "Avoid the potato patch.",
            "Do not dig over the potato patch.",
            "Dig no potato patch.",
            "Move away from the potato patch.",
        ] {
            assert!(
                !task_proposal_is_grounded_in_final_dialogue(
                    negative_proposal,
                    "Please dig over the potato patch.",
                ),
                "{negative_proposal:?} must not become a durable task"
            );
        }
    }

    #[test]
    fn canonical_dialogue_seam_assigns_grounded_potato_task_and_publishes_payload() {
        use crate::npc::manager::NpcManager;
        use crate::npc::{NpcId, NpcMetadata, NpcStreamResponse};
        use chrono::{Duration as ChronoDuration, TimeZone};
        use parish_types::{GameEvent, TaskStatus};
        use parish_world::WorldState;

        let mut world = WorldState::new();
        let mut rx = world.event_bus.subscribe();
        let location = world.player_location;
        let mut npc_manager = NpcManager::new();
        let mut npc = crate::npc::Npc::new_test_npc();
        npc.id = NpcId(7);
        npc.name = "Siobhan Murphy".to_string();
        npc.set_location(location);
        npc_manager.add_npc(npc);
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let response = NpcStreamResponse {
            dialogue: "First, help with the potato patch — break the clods and plant seed in the open rows.".to_string(),
            metadata: Some(NpcMetadata {
                action: "points toward the field".to_string(),
                mood: "busy".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                assigned_task: Some("Dig over the potato patch.".to_string()),
            }),
        };

        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &response,
            "Where should I begin the work?",
            "Where should I begin the work?",
            game_time,
            location,
            "a farmer",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );
        let mut repeated_response = NpcStreamResponse {
            dialogue: "Begin with the potato patch; dig the clods and plant seed in the open rows."
                .to_string(),
            ..response.clone()
        };
        repeated_response
            .metadata
            .as_mut()
            .expect("metadata")
            .assigned_task = Some("  Dig over the potato patch  ".to_string());
        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &repeated_response,
            "I'll make a start there.",
            "I'll make a start there.",
            game_time + ChronoDuration::minutes(10),
            location,
            "a farmer",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );

        let task = world
            .player_progress
            .active_tasks()
            .next()
            .expect("grounded spoken assignment must create a task");
        assert_eq!(task.description, "Dig over the potato patch.");
        assert_eq!(task.assigned_by, NpcId(7));
        assert_eq!(task.location, location);
        assert_eq!(task.assigned_at, game_time);
        assert_eq!(task.status, TaskStatus::Assigned);
        assert_eq!(
            world.player_progress.len(),
            1,
            "a repeated response proposing the same active task must be idempotent"
        );

        let events: Vec<GameEvent> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let assignment_events: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                GameEvent::PlayerTaskAssigned { task, timestamp } => Some((task, timestamp)),
                _ => None,
            })
            .collect();
        assert_eq!(
            assignment_events.len(),
            1,
            "an idempotent repeat must not publish a duplicate assignment event"
        );
        let assigned = assignment_events[0];
        assert_eq!(assigned.0, task);
        assert_eq!(*assigned.1, game_time);
    }

    #[test]
    fn canonical_dialogue_seam_accepts_live_shaped_start_by_gerund_assignment() {
        use crate::npc::manager::NpcManager;
        use crate::npc::{NpcId, NpcMetadata, NpcStreamResponse};
        use chrono::TimeZone;
        use parish_types::GameEvent;
        use parish_world::WorldState;

        let mut world = WorldState::new();
        let location = world.player_location;
        let mut rx = world.event_bus.subscribe();
        let mut npc_manager = NpcManager::new();
        let mut npc = crate::npc::Npc::new_test_npc();
        npc.id = NpcId(7);
        npc.set_location(location);
        npc_manager.add_npc(npc);
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let response = NpcStreamResponse {
            dialogue: "Plainly, then—Plainly, then—Start by fetching water from the well for the sick woman.".to_string(),
            metadata: Some(NpcMetadata {
                action: "hands over a pail".to_string(),
                mood: "concerned".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                assigned_task: Some("Fetch water from the well.".to_string()),
            }),
        };

        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &response,
            "How can I help?",
            "How can I help?",
            game_time,
            location,
            "a farmer",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );

        let task = world
            .player_progress
            .active_tasks()
            .next()
            .expect("the grounded start-by-gerund request must create a task");
        assert_eq!(task.description, "Fetch water from the well.");
        assert_eq!(
            std::iter::from_fn(|| rx.try_recv().ok())
                .filter(|event| matches!(event, GameEvent::PlayerTaskAssigned { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn canonical_dialogue_seam_accepts_live_shaped_best_start_with_assignment() {
        use crate::npc::manager::NpcManager;
        use crate::npc::{NpcId, NpcMetadata, NpcStreamResponse};
        use chrono::TimeZone;
        use parish_types::GameEvent;
        use parish_world::WorldState;

        let mut world = WorldState::new();
        let location = world.player_location;
        let mut rx = world.event_bus.subscribe();
        let mut npc_manager = NpcManager::new();
        let mut npc = crate::npc::Npc::new_test_npc();
        npc.id = NpcId(7);
        npc.set_location(location);
        npc_manager.add_npc(npc);
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let response = NpcStreamResponse {
            dialogue: "Good morning. Ye'd best start with breaking stones from the road — the ford needs clearing. Carry the clods to the side of the path.".to_string(),
            metadata: Some(NpcMetadata {
                action: "points towards the ford".to_string(),
                mood: "practical".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                assigned_task: Some(
                    "Break stones from the road and carry them to the side.".to_string(),
                ),
            }),
        };

        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &response,
            "Is there work for me?",
            "Is there work for me?",
            game_time,
            location,
            "a farmer",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );

        let task = world
            .player_progress
            .active_tasks()
            .next()
            .expect("the grounded best-start-with request must create a task");
        assert_eq!(
            task.description,
            "Break stones from the road and carry them to the side."
        );
        assert_eq!(
            std::iter::from_fn(|| rx.try_recv().ok())
                .filter(|event| matches!(event, GameEvent::PlayerTaskAssigned { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn canonical_dialogue_seam_rejects_unspoken_or_disabled_task_metadata() {
        use crate::npc::manager::NpcManager;
        use crate::npc::{NpcId, NpcMetadata, NpcStreamResponse};
        use chrono::{Duration, TimeZone};
        use parish_types::GameEvent;
        use parish_world::WorldState;

        let mut world = WorldState::new();
        let location = world.player_location;
        let mut rx = world.event_bus.subscribe();
        let mut npc_manager = NpcManager::new();
        let mut npc = crate::npc::Npc::new_test_npc();
        npc.id = NpcId(7);
        npc.set_location(location);
        npc_manager.add_npc(npc);
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let response = |dialogue: &str| NpcStreamResponse {
            dialogue: dialogue.to_string(),
            metadata: Some(NpcMetadata {
                action: String::new(),
                mood: "busy".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                assigned_task: Some("Dig over the potato patch.".to_string()),
            }),
        };

        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &response("There is no work for ye in the potato patch today."),
            "Have ye work for another pair of hands?",
            "Have ye work for another pair of hands?",
            game_time,
            location,
            "a farmer",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );
        assert!(world.player_progress.is_empty());

        let advice = NpcStreamResponse {
            dialogue: "You can ask Siobhan at the farm about digging the potato patch.".to_string(),
            metadata: Some(NpcMetadata {
                action: String::new(),
                mood: "busy".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: vec!["Siobhan".to_string()],
                assigned_task: Some("Ask Siobhan at the farm.".to_string()),
            }),
        };
        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &advice,
            "Thank you for the advice.",
            "Thank you for the advice.",
            game_time + Duration::minutes(5),
            location,
            "a farmer",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );
        assert!(
            world.player_progress.is_empty(),
            "advice or a referral must not become a durable player task"
        );
        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &response("Liam will help with the potato patch."),
            "What of Liam?",
            "What of Liam?",
            game_time + Duration::minutes(7),
            location,
            "a farmer",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );
        assert!(
            world.player_progress.is_empty(),
            "third-person work descriptions must not become player assignments"
        );
        for (minutes, dialogue) in [
            (
                8,
                "I need you to remember that Liam already dug over the potato patch.",
            ),
            (
                9,
                "\u{201c}Dig over the potato patch,\u{201d} Liam told me yesterday.",
            ),
            (10, "Start by leaving the potato patch alone."),
        ] {
            apply_npc_dialogue_turn(
                &mut world,
                &mut npc_manager,
                NpcId(7),
                &response(dialogue),
                "What work is there?",
                "What work is there?",
                game_time + Duration::minutes(minutes),
                location,
                "a farmer",
                "Siobhan Murphy",
                None,
                &[],
                &LanguageSettings::english_only(),
                &FeatureFlags::default(),
            );
            assert!(
                world.player_progress.is_empty(),
                "{dialogue:?} must not mutate the task ledger"
            );
        }

        let mut disabled = FeatureFlags::default();
        disabled.disable(PLAYER_TASK_PROGRESSION_FLAG);
        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &response("First, help with the potato patch — break the clods and plant seed."),
            "Where should I begin?",
            "Where should I begin?",
            game_time + Duration::minutes(15),
            location,
            "a farmer",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &disabled,
        );
        assert!(world.player_progress.is_empty());
        assert!(
            std::iter::from_fn(|| rx.try_recv().ok())
                .all(|event| !matches!(event, GameEvent::PlayerTaskAssigned { .. })),
            "rejected and kill-switched metadata must not publish task events"
        );
    }

    #[test]
    fn canonical_dialogue_seam_rejects_assignment_at_a_known_remote_location() {
        use crate::npc::manager::NpcManager;
        use crate::npc::{NpcId, NpcMetadata, NpcStreamResponse};
        use chrono::TimeZone;
        use parish_types::GameEvent;
        use parish_world::WorldState;
        use parish_world::graph::WorldGraph;

        let mut world = WorldState::new();
        world.graph = WorldGraph::load_from_str(
            r#"{
                "locations": [
                    {
                        "id": 1,
                        "name": "Darcy's Pub",
                        "description_template": "A public house.",
                        "indoor": true,
                        "public": true,
                        "connections": [{
                            "target": 2,
                            "path_description": "the road to the church"
                        }],
                        "lat": 53.0,
                        "lon": -8.0,
                        "aliases": ["pub", "the pub"]
                    },
                    {
                        "id": 2,
                        "name": "St. Brigid's Church",
                        "description_template": "A stone church.",
                        "indoor": false,
                        "public": true,
                        "connections": [{
                            "target": 1,
                            "path_description": "the road to the pub"
                        }],
                        "lat": 53.1,
                        "lon": -8.1,
                        "aliases": ["church", "the church", "chapel"]
                    }
                ]
            }"#,
        )
        .unwrap();
        world.player_location = LocationId(1);
        let mut rx = world.event_bus.subscribe();

        let mut npc_manager = NpcManager::new();
        let mut npc = crate::npc::Npc::new_test_npc();
        npc.id = NpcId(7);
        npc.set_location(LocationId(1));
        npc_manager.add_npc(npc);
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let response = NpcStreamResponse {
            dialogue: "Could ye mend the chapel wall?".to_string(),
            metadata: Some(NpcMetadata {
                action: "points down the road".to_string(),
                mood: "busy".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                assigned_task: Some("Mend the chapel wall.".to_string()),
            }),
        };

        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &response,
            "Have ye work for me?",
            "Have ye work for me?",
            game_time,
            LocationId(1),
            "a publican",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );

        assert!(
            world.player_progress.is_empty(),
            "a remote-location task must not be assigned at the current location"
        );
        assert_eq!(
            std::iter::from_fn(|| rx.try_recv().ok())
                .filter(|event| matches!(event, GameEvent::PlayerTaskAssigned { .. }))
                .count(),
            0,
            "a rejected remote assignment must not publish a task event"
        );

        let best_start_remote_response = NpcStreamResponse {
            dialogue: "Ye'd best start with mending the chapel wall.".to_string(),
            metadata: Some(NpcMetadata {
                action: "points down the road".to_string(),
                mood: "busy".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                assigned_task: Some("Mend the chapel wall.".to_string()),
            }),
        };
        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &best_start_remote_response,
            "Anything else?",
            "Anything else?",
            game_time + chrono::Duration::minutes(2),
            LocationId(1),
            "a publican",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );
        assert!(
            world.player_progress.is_empty(),
            "the best-start frame must retain the remote-location guard"
        );
        assert_eq!(
            std::iter::from_fn(|| rx.try_recv().ok())
                .filter(|event| matches!(event, GameEvent::PlayerTaskAssigned { .. }))
                .count(),
            0,
            "the rejected best-start remote assignment must not publish an event"
        );

        let local_response = NpcStreamResponse {
            dialogue: "The church roof can wait. Please sweep the pub floor.".to_string(),
            metadata: Some(NpcMetadata {
                action: "offers a broom".to_string(),
                mood: "busy".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                assigned_task: Some("Sweep the pub floor.".to_string()),
            }),
        };
        apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            NpcId(7),
            &local_response,
            "What needs doing here?",
            "What needs doing here?",
            game_time + chrono::Duration::minutes(5),
            LocationId(1),
            "a publican",
            "Siobhan Murphy",
            None,
            &[],
            &LanguageSettings::english_only(),
            &FeatureFlags::default(),
        );

        let local_task = world
            .player_progress
            .active_tasks()
            .next()
            .expect("unrelated remote context must not suppress a grounded local task");
        assert_eq!(local_task.description, "Sweep the pub floor.");
        assert_eq!(
            std::iter::from_fn(|| rx.try_recv().ok())
                .filter(|event| matches!(event, GameEvent::PlayerTaskAssigned { .. }))
                .count(),
            1,
            "the local grounding clause must publish one task event"
        );
    }

    #[test]
    fn shared_player_action_progresses_once_but_never_completes() {
        use chrono::TimeZone;
        use parish_types::{GameEvent, NpcId, TaskStatus};
        use parish_world::WorldState;

        let mut world = WorldState::new();
        let location = world.player_location;
        let assigned_at = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let task_id = world
            .player_progress
            .assign_task(
                "Dig over the potato patch.",
                NpcId(7),
                location,
                assigned_at,
            )
            .unwrap();
        let mut rx = world.event_bus.subscribe();

        let outcome = apply_player_action(
            &mut world,
            "I set to work in the potato patch, breaking clods and planting seed.",
            &FeatureFlags::default(),
        )
        .expect("nonblank action");
        assert_eq!(
            outcome.narration,
            "You set to work in the potato patch, breaking clods and planting seed."
        );
        let task = world.player_progress.task(task_id).unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.completed_at, None);

        let event = rx.try_recv().expect("task progression event");
        match event {
            GameEvent::PlayerTaskProgressed {
                task,
                previous_status,
                action,
                ..
            } => {
                assert_eq!(task.status, TaskStatus::InProgress);
                assert_eq!(previous_status, TaskStatus::Assigned);
                assert_eq!(
                    action,
                    "I set to work in the potato patch, breaking clods and planting seed."
                );
            }
            other => panic!("expected PlayerTaskProgressed, got {other:?}"),
        }
        assert!(
            matches!(
                rx.try_recv(),
                Err(tokio::sync::broadcast::error::TryRecvError::Empty)
            ),
            "one action must publish exactly one task-progression event"
        );
    }

    #[test]
    fn shared_player_action_ignores_unrelated_and_kill_switched_actions() {
        use chrono::TimeZone;
        use parish_types::{NpcId, TaskStatus};
        use parish_world::WorldState;

        let seed_world = || {
            let mut world = WorldState::new();
            let location = world.player_location;
            let assigned_at = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
            let task_id = world
                .player_progress
                .assign_task(
                    "Dig over the potato patch.",
                    NpcId(7),
                    location,
                    assigned_at,
                )
                .unwrap();
            (world, task_id)
        };

        let (mut unrelated_world, unrelated_id) = seed_world();
        let mut unrelated_rx = unrelated_world.event_bus.subscribe();
        let outcome = apply_player_action(
            &mut unrelated_world,
            "I mend the gate by the road.",
            &FeatureFlags::default(),
        )
        .unwrap();
        assert!(outcome.progressed_task.is_none());
        assert_eq!(
            unrelated_world
                .player_progress
                .task(unrelated_id)
                .unwrap()
                .status,
            TaskStatus::Assigned
        );
        assert!(unrelated_rx.try_recv().is_err());

        let (mut disabled_world, disabled_id) = seed_world();
        let mut disabled_rx = disabled_world.event_bus.subscribe();
        let mut disabled = FeatureFlags::default();
        disabled.disable(PLAYER_TASK_PROGRESSION_FLAG);
        let outcome = apply_player_action(
            &mut disabled_world,
            "I set to work in the potato patch, breaking clods.",
            &disabled,
        )
        .unwrap();
        assert!(outcome.progressed_task.is_none());
        assert_eq!(
            disabled_world
                .player_progress
                .task(disabled_id)
                .unwrap()
                .status,
            TaskStatus::Assigned
        );
        assert!(disabled_rx.try_recv().is_err());
    }
}
