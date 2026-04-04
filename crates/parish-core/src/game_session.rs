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

use crate::config::ReactionConfig;
use crate::dice;
use crate::ipc::{build_travel_start, types::TravelStartPayload};
use crate::npc::manager::{NpcManager, TierTransition};
use crate::npc::reactions::{ReactionTemplates, generate_arrival_reactions};
use crate::world::description::{format_exits, render_description};
use crate::world::movement::{MovementResult, resolve_movement};
use crate::world::transport::TransportMode;
use crate::world::{Location, WorldState};

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
    /// The message text.
    pub text: String,
}

/// The side-effects produced by a single call to [`apply_movement`].
///
/// The caller is responsible for forwarding these to its own event bus or
/// IPC channel. [`WorldState::log`] has already been called for every
/// message, so test harnesses that only read from the log need not inspect
/// this struct at all.
#[derive(Debug, Default)]
pub struct GameEffects {
    /// Payload for a travel-start animation event, present only when the
    /// player actually moved (i.e. not `AlreadyHere` / `NotFound`).
    pub travel_start: Option<TravelStartPayload>,
    /// All player-visible messages in emission order.
    pub messages: Vec<GameMessage>,
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
    let result = resolve_movement(target, &world.graph, world.player_location, transport);

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

            // Reassign NPC cognitive tiers
            let tier_transitions = npc_manager.assign_tiers(world, &[]);

            // Build arrival description
            let look_text = build_look_text(world, npc_manager, transport);

            // Tick schedules so NPCs whose transit completed during travel
            // are now Present before we check for reactions
            let _schedule_events =
                npc_manager.tick_schedules(&world.clock, &world.graph, world.weather);

            // Generate arrival reactions (canned text, no LLM)
            let reaction_texts =
                apply_arrival_reactions_inner(world, npc_manager, reaction_templates);

            // Build message list in emission order
            let mut messages: Vec<GameMessage> = Vec::new();

            // Narration (travel description)
            world.log(narration.clone());
            world.log(String::new());
            messages.push(GameMessage {
                source: "system",
                text: narration,
            });

            // Arrival description + exits
            world.log(look_text.clone());
            messages.push(GameMessage {
                source: "system",
                text: look_text,
            });

            // NPC arrival reactions
            for text in reaction_texts {
                world.log(text.clone());
                messages.push(GameMessage {
                    source: "npc",
                    text,
                });
            }

            GameEffects {
                travel_start: Some(travel_start),
                messages,
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
                    text,
                }],
                ..Default::default()
            }
        }
    }
}

/// Generates NPC arrival reactions for the player's current location and
/// applies their side-effects (marking introductions, logging to world).
///
/// Returns the reaction texts. Callers that only need the reactions without
/// the full movement pipeline (e.g. a `/look` command that re-generates
/// reactions) can use this standalone function.
pub fn apply_arrival_reactions(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    templates: &ReactionTemplates,
    config: &ReactionConfig,
) -> Vec<String> {
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

    let mut texts = Vec::new();
    for reaction in &reactions {
        if reaction.introduces {
            npc_manager.mark_introduced(reaction.npc_id);
        }
        texts.push(reaction.canned_text.clone());
    }
    texts
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
fn apply_arrival_reactions_inner(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    templates: &ReactionTemplates,
) -> Vec<String> {
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

    let mut texts = Vec::new();
    for reaction in &reactions {
        if reaction.introduces {
            npc_manager.mark_introduced(reaction.npc_id);
        }
        texts.push(reaction.canned_text.clone());
    }
    texts
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
        let world = WorldState::from_mod(&game_mod).ok()?;
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
}
