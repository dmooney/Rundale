//! Reaction selection, threshold computation, canning, and the main generation loop.
//!
//! This module contains the core algorithm that decides which NPCs react to
//! the player's arrival and what kind of reaction each produces.

use std::collections::HashSet;

use crate::{Npc, NpcId};
use parish_config::ReactionConfig;
use parish_types::dice::DiceRoll;
use parish_world::graph::LocationData;
use parish_world::time::TimeOfDay;

use super::register::has_calculating_register;
use super::templates::{ReactionTemplates, substitute_placeholders};
use super::types::{ArrivalContext, NpcReaction, ReactionKind};

// ── Internal helpers ─────────────────────────────────────────────────────────

pub(crate) fn is_priest_occupation(occupation: &str) -> bool {
    occupation.contains("priest") || occupation.contains("clergy") || occupation.contains("curate")
}

/// Returns `true` if the mood string suggests a negative emotional state.
pub(crate) fn is_negative_mood(mood: &str) -> bool {
    let m = mood.to_lowercase();
    m.contains("angry")
        || m.contains("furious")
        || m.contains("sad")
        || m.contains("grief")
        || m.contains("irritat")
        || m.contains("frustrat")
        || m.contains("anxious")
        || m.contains("afraid")
        || m.contains("hostile")
        || m.contains("bitter")
        || m.contains("sullen")
        || m.contains("withdrawn")
}

/// Returns `true` if the NPC is currently at their workplace.
pub(crate) fn is_at_workplace(npc: &Npc, location: &LocationData) -> bool {
    npc.workplace.is_some_and(|wp| wp == location.id)
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Computes the reaction threshold for a given NPC — the probability
/// (0.0–1.0) that this NPC will react to the player's arrival.
pub fn reaction_threshold(
    npc: &Npc,
    location: &LocationData,
    time_of_day: TimeOfDay,
    config: &ReactionConfig,
) -> f64 {
    let mut threshold = config.base_chance;

    if is_at_workplace(npc, location) {
        threshold += config.workplace_bonus;
    }
    if location.indoor {
        threshold += config.indoor_bonus;
    }
    if npc.intelligence.emotional >= 4 {
        threshold += config.empathy_bonus;
    }
    if is_negative_mood(&npc.mood) {
        threshold -= config.negative_mood_penalty;
    }
    if matches!(time_of_day, TimeOfDay::Night | TimeOfDay::Midnight) {
        threshold -= config.night_penalty;
    }

    threshold.clamp(0.0, 1.0)
}

/// Selects the reaction kind for an NPC based on context and a type roll.
///
/// Returns `(kind, introduces, use_llm)`.
fn select_reaction_kind(
    at_workplace: bool,
    is_introduced: bool,
    is_priest: bool,
    type_roll: &DiceRoll,
) -> (ReactionKind, bool, bool) {
    if at_workplace && !is_introduced {
        (ReactionKind::Introduction, true, true)
    } else if at_workplace && is_introduced {
        (ReactionKind::Welcome, false, true)
    } else if !is_introduced && type_roll.check(0.25) {
        (ReactionKind::Introduction, true, true)
    } else if !is_introduced {
        (ReactionKind::Gesture, false, false)
    } else if is_priest {
        (ReactionKind::Greeting, false, type_roll.check(0.5))
    } else if type_roll.check(0.5) {
        (ReactionKind::Greeting, false, false)
    } else {
        (ReactionKind::Gesture, false, false)
    }
}

/// Caps reaction count, prioritising socially significant kinds.
fn cap_reactions_by_priority(reactions: &mut Vec<NpcReaction>, max_reactions: usize) {
    if max_reactions == 0 || reactions.len() <= max_reactions {
        return;
    }
    reactions.sort_by_key(|r| match r.kind {
        ReactionKind::Introduction => 0u8,
        ReactionKind::Welcome => 1,
        ReactionKind::Greeting => 2,
        ReactionKind::Gesture => 3,
    });
    reactions.truncate(max_reactions);
}

/// Per-NPC context used inside [`pick_canned_text`].
struct NpcArrivalCtx<'a> {
    npc: &'a Npc,
    display_name: &'a str,
    at_workplace: bool,
    occupation: &'a str,
}

/// Picks a canned text template and substitutes placeholders.
fn pick_canned_text(
    kind: &ReactionKind,
    npc_ctx: &NpcArrivalCtx<'_>,
    time_of_day: TimeOfDay,
    weather: &str,
    templates: &ReactionTemplates,
    roll: &DiceRoll,
) -> String {
    let occupation = npc_ctx.occupation;
    let raw = match kind {
        ReactionKind::Gesture => roll.pick(&templates.gestures).clone(),
        ReactionKind::Greeting => {
            if is_priest_occupation(occupation) {
                if !templates.occupation_greetings.priest.is_empty() {
                    roll.pick(&templates.occupation_greetings.priest).clone()
                } else {
                    pick_greeting_by_time(time_of_day, templates, roll)
                }
            } else {
                pick_greeting_by_time(time_of_day, templates, roll)
            }
        }
        ReactionKind::Welcome => {
            let pool = match () {
                _ if occupation.contains("publican") => &templates.welcomes.publican,
                _ if occupation.contains("shopkeeper") => &templates.welcomes.shopkeeper,
                _ if is_priest_occupation(occupation) => &templates.welcomes.priest,
                _ if occupation.contains("teacher") => &templates.welcomes.teacher,
                _ => &templates.welcomes.generic,
            };
            if pool.is_empty() {
                pick_greeting_by_time(time_of_day, templates, roll)
            } else {
                roll.pick(pool).clone()
            }
        }
        ReactionKind::Introduction => {
            let pool = if has_calculating_register(npc_ctx.npc)
                && !templates.introductions.calculating.is_empty()
            {
                &templates.introductions.calculating
            } else if npc_ctx.at_workplace {
                &templates.introductions.workplace
            } else {
                &templates.introductions.casual
            };
            if pool.is_empty() {
                "\"I'm {},\" they say.".to_string()
            } else {
                roll.pick(pool).clone()
            }
        }
    };

    substitute_placeholders(
        &raw,
        npc_ctx.npc,
        npc_ctx.display_name,
        time_of_day,
        weather,
    )
}

fn pick_greeting_by_time(
    time_of_day: TimeOfDay,
    templates: &ReactionTemplates,
    roll: &DiceRoll,
) -> String {
    let time_pool = match time_of_day {
        TimeOfDay::Dawn | TimeOfDay::Morning => &templates.greetings.morning,
        TimeOfDay::Midday | TimeOfDay::Afternoon => &templates.greetings.afternoon,
        TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight => &templates.greetings.evening,
    };

    if !templates.greetings.any.is_empty() && roll.value() < 0.3 {
        return roll.pick(&templates.greetings.any).clone();
    }

    if time_pool.is_empty() && !templates.greetings.any.is_empty() {
        roll.pick(&templates.greetings.any).clone()
    } else if time_pool.is_empty() {
        "\"Hello,\" they say.".to_string()
    } else {
        roll.pick(time_pool).clone()
    }
}

/// Generates arrival reactions for NPCs at the player's current location.
///
/// Each NPC needs **two** dice rolls in `dice` (one for reaction chance,
/// one for type/template selection). So `dice.len()` must be `≥ npcs.len() * 2`.
///
/// Returns only NPCs that actually react — silent NPCs are omitted.
pub fn generate_arrival_reactions(
    npcs: &[&Npc],
    introduced: &HashSet<NpcId>,
    ctx: &ArrivalContext<'_>,
    dice: &[DiceRoll],
) -> Vec<NpcReaction> {
    let location = ctx.location;
    let time_of_day = ctx.time_of_day;
    let weather = ctx.weather;
    let templates = ctx.templates;
    let config = ctx.config;
    let mut reactions = Vec::new();

    for (i, npc) in npcs.iter().enumerate() {
        let roll_idx = i * 2;
        if roll_idx + 1 >= dice.len() {
            break;
        }
        let chance_roll = &dice[roll_idx];
        let type_roll = &dice[roll_idx + 1];

        let threshold = reaction_threshold(npc, location, time_of_day, config);
        if !chance_roll.check(threshold) {
            continue;
        }

        let is_introduced = introduced.contains(&npc.id);
        let at_workplace = is_at_workplace(npc, location);
        let occupation = npc.occupation.to_lowercase();
        let is_priest = is_priest_occupation(&occupation);

        let (kind, introduces, use_llm) =
            select_reaction_kind(at_workplace, is_introduced, is_priest, type_roll);

        let display_name = if is_introduced || introduces {
            npc.name.clone()
        } else {
            npc.brief_description.clone()
        };

        let npc_ctx = NpcArrivalCtx {
            npc,
            display_name: &display_name,
            at_workplace,
            occupation: &occupation,
        };
        let canned_text =
            pick_canned_text(&kind, &npc_ctx, time_of_day, weather, templates, type_roll);

        reactions.push(NpcReaction {
            npc_id: npc.id,
            npc_display_name: display_name,
            kind,
            canned_text,
            introduces,
            use_llm,
        });
    }

    cap_reactions_by_priority(&mut reactions, config.max_reactions);
    reactions
}
