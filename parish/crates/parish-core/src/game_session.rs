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
use crate::inference::AnyClient;
use crate::inference::InferenceLog;
use crate::ipc::{build_travel_start, types::TravelStartPayload};
use crate::npc::manager::{NpcManager, TierTransition};
use crate::npc::reactions::{NpcReaction, ReactionTemplates, generate_arrival_reactions};
use crate::npc::{Npc, NpcId};
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

            // Apply world state changes
            world.record_path_traversal(&path);
            world.clock.advance(minutes as i64);
            world.player_location = destination;
            world.mark_visited(destination);

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
                check_encounter(world.clock.time_of_day(), dice::DiceRoll::roll().value())
                    .map(|ev| ev.description);

            // Reassign NPC cognitive tiers
            let tier_transitions = npc_manager.assign_tiers(world, &[]);

            // Build arrival description
            let look_text = build_look_text(world, npc_manager, transport);

            // Tick schedules so NPCs whose transit completed during travel
            // are now Present before we check for reactions
            let _schedule_events =
                npc_manager.tick_schedules(&world.clock, &world.graph, world.weather);

            // Generate arrival reactions; canned text is logged to world.log.
            // Raw reactions are returned so backends with an LLM client can
            // upgrade use_llm entries via resolve_llm_greeting.
            let arrival_reactions =
                apply_arrival_reactions_inner(world, npc_manager, reaction_templates);

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
        client.generate(model, &context, Some(&system), Some(80), None),
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

    let reactions = generate_arrival_reactions(
        &npcs,
        &introduced,
        &loc_data,
        tod,
        &weather,
        templates,
        config,
        &roll_dice,
    );

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

/// Inner helper: generate reactions and apply side-effects, returning texts.
/// Generates reactions, marks introductions, logs canned text to world.log,
/// and returns the raw [`NpcReaction`] structs so backends can optionally
/// upgrade `use_llm` entries via an LLM call.
fn apply_arrival_reactions_inner(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    templates: &ReactionTemplates,
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
    let config = ReactionConfig::default();
    let roll_dice = dice::roll_n(npcs.len() * 2);

    let reactions = generate_arrival_reactions(
        &npcs,
        &introduced,
        &loc_data,
        tod,
        &weather,
        templates,
        &config,
        &roll_dice,
    );

    for reaction in &reactions {
        if reaction.introduces {
            npc_manager.mark_introduced(reaction.npc_id);
        }
        // Log canned text as the persistent record; backends may emit LLM
        // text to the frontend instead but the world log always has canned.
        world.log(reaction.canned_text.clone());
    }
    reactions
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
///   an empty placeholder in the frontend chat log before streaming begins
/// - `emit_stream_token(turn_id, source, batch)` — called with each batched
///   token chunk to be appended to the current streaming entry
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
    mut emit_text_log: impl FnMut(u64, &str),
    mut emit_stream_token: impl FnMut(u64, &str, &str),
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
                let at_workplace = npc.workplace.is_some_and(|wp| wp == current_location_id);
                let is_introduced = introduced.contains(&reaction.npc_id);
                let (system, context) =
                    build_reaction_prompt(npc, loc_name, tod, weather, is_introduced, at_workplace);
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
                            Some(100),
                            None,
                        ),
                    )
                    .await;
                    // tx is consumed by generate_stream; when it returns (success or
                    // timeout) tx is dropped, closing the channel and allowing
                    // stream_npc_tokens to finish.
                });
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
        // Find first word of location name and use it as target
        let target = loc.split_whitespace().next().unwrap_or("here");
        // Deliberately move to a place we know — just test AlreadyHere edge
        let start = world.player_location;
        let effects = apply_movement(&mut world, &mut mgr, &templates, &loc, &transport);
        // Should be AlreadyHere or Moved (depending on fuzzy match)
        // Either way: world_changed only if we moved
        assert!(!effects.messages.is_empty());
        let _ = target; // suppress unused
        let _ = start;
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
    fn apply_arrival_reactions_empty_location() {
        let Some((mut world, mut mgr, templates, _)) = setup() else {
            return;
        };
        let config = ReactionConfig::default();
        // No NPCs at start by default — should return empty
        mgr.npcs_at(world.player_location); // just for the call
        let texts = apply_arrival_reactions(&mut world, &mut mgr, &templates, &config);
        // May or may not be empty depending on game data — just verify it doesn't panic
        let _ = texts;
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
            |_turn_id, name| log_sources.push(name.to_string()),
            |_turn_id, _source, tok| token_chunks.push(tok.to_string()),
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
            |_turn_id, name| log_sources.push(name.to_string()),
            |_turn_id, _source, tok| token_chunks.push(tok.to_string()),
        )
        .await;

        assert!(log_sources.is_empty());
        assert!(token_chunks.is_empty());
    }
}
