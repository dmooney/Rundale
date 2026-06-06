//! Tier 1 prompt-building helpers.
//!
//! Constructs system prompts and context strings for Tier 1 (per-player-interaction)
//! NPC inference. These helpers are pure string builders — no async, no I/O.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::{LanguageSettings, Npc, NpcId, build_tier1_context, build_tier1_system_prompt};
use parish_config::{NpcConfig, RelationshipLabelConfig};
use parish_world::WorldState;

// ── relationship helpers ───────────────────────────────────────────────────

/// Returns a descriptive label for a relationship strength value using the given config thresholds.
pub fn relationship_label_with_config(
    strength: f64,
    config: &RelationshipLabelConfig,
) -> &'static str {
    match strength {
        s if s > config.very_close => "very close",
        s if s > config.friendly => "friendly",
        s if s > config.acquainted => "acquainted",
        s if s > config.cool => "cool",
        s if s > config.strained => "strained",
        _ => "hostile",
    }
}

/// Returns a descriptive label for a relationship strength value using default thresholds.
pub fn relationship_label(strength: f64) -> &'static str {
    relationship_label_with_config(strength, &RelationshipLabelConfig::default())
}

/// Returns the natural preposition that follows a relationship label.
///
/// e.g. `"friendly"` -> `"with"`, `"hostile"` -> `"toward"`.
fn relationship_preposition(label: &str) -> &'static str {
    match label {
        "very close" | "cool" | "hostile" => "to",
        "friendly" | "acquainted" | "strained" => "with",
        _ => "with",
    }
}

/// Formats a list of (peer_id, strength) relationships as a natural-language phrase.
///
/// Resolves peer ids to names via `npc_names` (falling back to "someone" for
/// unknown ids) and maps each strength to a verbal label via
/// `relationship_label_with_config`. The conventional player id `NpcId(0)` is
/// rendered as "the newcomer" to match the Tier 1 dialogue convention (see
/// `interlocutor_block`). Returns an empty string for an empty list.
///
/// Example output: `"friendly with Mary McKenna, cool to Sean Doyle"`.
pub fn format_relationships_natural(
    rels: &[(NpcId, f64)],
    npc_names: &HashMap<NpcId, String>,
    cfg: &RelationshipLabelConfig,
) -> String {
    rels.iter()
        .map(|(id, strength)| {
            let name = if id.0 == 0 {
                "the newcomer"
            } else {
                npc_names.get(id).map(|s| s.as_str()).unwrap_or("someone")
            };
            let label = relationship_label_with_config(*strength, cfg);
            let prep = relationship_preposition(label);
            format!("{label} {prep} {name}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ── system prompt builders ────────────────────────────────────────────────

/// Builds an enhanced system prompt for Tier 1 interactions using the given config.
///
/// Extends the base system prompt with relationship summaries (using real names)
/// and knowledge entries for richer, more contextual NPC dialogue.
#[cfg(test)]
pub(crate) fn build_enhanced_system_prompt(
    npc: &Npc,
    improv: bool,
    language: &LanguageSettings,
    npc_names: &HashMap<NpcId, String>,
) -> String {
    build_enhanced_system_prompt_with_config(
        npc,
        improv,
        language,
        &NpcConfig::default(),
        npc_names,
        None,
    )
}

pub fn build_enhanced_system_prompt_with_config(
    npc: &Npc,
    improv: bool,
    language: &LanguageSettings,
    config: &NpcConfig,
    npc_names: &HashMap<NpcId, String>,
    known_roster: Option<&[(NpcId, String, String)]>,
) -> String {
    let mut prompt = build_tier1_system_prompt(npc, improv, language);

    // Add known NPC roster (relationships + memory + co-located NPCs)
    // NpcId(0) is the player — shown first with a special "currently speaking with" note.
    if let Some(roster) = known_roster {
        if !roster.is_empty() {
            prompt.push_str("\n\nPEOPLE YOU KNOW:\n");
            for (target_id, name, occupation) in roster {
                if *target_id == NpcId(0) {
                    // The player — highlight them as the current interlocutor
                    prompt.push_str(&format!(
                        "- {}, {} \u{2014} this is the person you are currently speaking with\n",
                        name, occupation
                    ));
                } else if let Some(rel) = npc.relationships.get(target_id) {
                    let strength_desc =
                        relationship_label_with_config(rel.strength, &config.relationship_labels);
                    prompt.push_str(&format!(
                        "- {}, {} \u{2014} {} ({})\n",
                        name, occupation, rel.kind, strength_desc
                    ));
                } else {
                    prompt.push_str(&format!("- {}, {}\n", name, occupation));
                }
            }
            prompt.push_str(
                "If you want to mention anyone not listed above, \
                describe them by role or appearance \u{2014} never invent a name.\n",
            );
        }
    } else if !npc.relationships.is_empty() {
        // Fallback: legacy behavior for callers that don't pass a roster
        prompt.push_str("\n\nPEOPLE IN YOUR LIFE:\n");
        for (target_id, rel) in &npc.relationships {
            let name = npc_names
                .get(target_id)
                .map(|s| s.as_str())
                .unwrap_or("someone");
            let strength_desc =
                relationship_label_with_config(rel.strength, &config.relationship_labels);
            prompt.push_str(&format!("- {}: {} ({})\n", name, rel.kind, strength_desc));
        }
    }

    // Add knowledge as natural thoughts rather than bullet points
    if !npc.knowledge.is_empty() {
        prompt.push_str("\nWHAT'S ON YOUR MIND:\n");
        for item in &npc.knowledge {
            prompt.push_str(&format!("- {}\n", item));
        }
    }

    prompt
}

// ── context block builders ────────────────────────────────────────────────

/// "Already introduced" anchor — fires only on the second and later
/// turns with a given NPC (when the NPC's name has already been
/// surfaced to the player). This failure mode was captured:
/// Roisin Connolly's reply on turn 7 included "...ye share yer
/// plans with me, Roisin Connolly, of Connolly's Shop, and a keen
/// eye for opportunity?" — mid-reply self-introduction that
/// breaks immersion long after the NPC has been met.
///
/// The caller must pass `was_introduced` as captured *before*
/// `NpcManager::mark_introduced` is called for this turn, otherwise
/// the value is always true after entry and the anchor fires on
/// turn 1 too (which would suppress legitimate first-contact
/// introductions).
fn introduced_anchor_block(npc: &Npc, was_introduced: bool) -> Option<String> {
    if !was_introduced {
        return None;
    }
    let name = &npc.name;
    let occupation = &npc.occupation;
    Some(format!(
        "\n\nYou have already introduced yourself to this person — \
         they know you are {name}, the {occupation}. Do not recite \
         your full name and occupation again in this reply, and do \
         not say things like \"{name}, of <place>\" mid-reply. Speak \
         in first person as a continuing voice in the conversation."
    ))
}

/// "Where you are right now" anchor — pins the NPC to the player's
/// current location so they don't substitute a nearby canonical
/// settlement from their backstory or short-term memory.
///
/// TODO #21 surfaced this: Cormac and Nora Duffy, who work at The Mill
/// near Kilteevan, repeatedly told the player they were "here in
/// Curraghboy" because Curraghboy Village is real, neighbouring, and
/// mentioned in their family backstory. The base location label
/// (`"Your Location: ..."`) is informative — this block is directive.
fn location_anchor_block(world: &WorldState) -> String {
    let name = &world.current_location().name;
    format!(
        "\n\nWHERE YOU ARE RIGHT NOW:\n{name}.\n\
         When you say 'here', 'this place', 'this village', or 'this town' \
         you mean {name} — no other settlement. Other nearby places (mentioned \
         in your memory or backstory) exist, but you are NOT at any of them \
         right now."
    )
}

/// Interlocutor label — who the NPC is speaking with.
///
/// The anchor sentence forbids the model from addressing the player by any
/// other name that may appear in the recent-events buffer (TODO #35 — a
/// shopkeeper at a new location called the player "Nora" because the prior
/// location's NPC named Nora was still in the dialogue history).
fn interlocutor_block(player_name_for_npc: Option<&str>) -> String {
    match player_name_for_npc {
        Some(name) => format!(
            "\n\nPERSON YOU ARE SPEAKING WITH:\n{name}.\n\
             Address them by the name '{name}' only. Do not call them any \
             other name, even if a different name appears in the recent \
             conversation history."
        ),
        None => String::from(
            "\n\nPERSON YOU ARE SPEAKING WITH:\nA newcomer to the parish.\n\
             You do not yet know their name. Refer to them as 'the newcomer', \
             'stranger', 'friend', or similar — do not invent a name and do \
             not borrow a name from the recent conversation history.",
        ),
    }
}

/// Describes other NPCs present at the location with relationship context.
///
/// The anchor sentence forbids the model from speaking to or about any
/// character not in this list as if they were present (TODO #11 — Brendan
/// addressed "Nora" mid-reply while Nora was not at the location; the
/// player-side LLM mirrored this and addressed absent NPCs).
fn other_npcs_block(npc: &Npc, other_npcs: &[&Npc], config: &NpcConfig) -> Option<String> {
    if other_npcs.is_empty() {
        return Some(String::from(
            "\n\nNo one else is here. Do not address or invoke any other \
             character by name as if they were present. You may still \
             mention absent people when recalling past events, but speak \
             of them in the past tense or as elsewhere — never as if they \
             can hear you now.",
        ));
    }
    let mut block = String::from(
        "\n\nAlso present (these are the only other people you may address \
         or speak about as 'here right now'):",
    );
    for other in other_npcs {
        let relationship_note = npc
            .relationships
            .get(&other.id)
            .map(|rel| {
                let label =
                    relationship_label_with_config(rel.strength, &config.relationship_labels);
                format!(" \u{2014} {} to you, {}", rel.kind, label)
            })
            .unwrap_or_default();
        block.push_str(&format!(
            "\n- {}, the {}{}",
            other.name, other.occupation, relationship_note
        ));
    }
    Some(block)
}

/// Recent conversation history at this location.
fn conversation_block(
    world: &WorldState,
    npc: &Npc,
    player_name_for_npc: Option<&str>,
) -> Option<String> {
    let player_label = player_name_for_npc.unwrap_or("The newcomer");
    let ctx = world
        .conversation_log
        .context_string(world.player_location, npc.id, player_label, 3);
    if ctx.is_empty() {
        return None;
    }
    Some(format!("\n\nWhat's been said here:\n{ctx}"))
}

/// Continuity cue when the NPC is already in conversation.
fn continuity_block(
    world: &WorldState,
    npc: &Npc,
    player_name_for_npc: Option<&str>,
) -> Option<String> {
    if !world
        .conversation_log
        .has_recent_exchange_with(world.player_location, npc.id, 2)
    {
        return None;
    }
    let name = player_name_for_npc.unwrap_or("this newcomer");
    Some(format!(
        "\n\nYou are already in conversation with {name}. \
         Do not re-introduce yourself or greet them again."
    ))
}

/// Recent player reactions (emoji feedback).
fn reactions_block(npc: &Npc, config: &NpcConfig) -> Option<String> {
    let ctx = npc
        .reaction_log
        .context_string(config.reaction_context_count);
    if ctx.is_empty() {
        return None;
    }
    Some(format!("\n\n{ctx}"))
}

/// Recent short-term memories.
fn stm_block(npc: &Npc, now: DateTime<Utc>) -> Option<String> {
    let ctx = npc.memory.context_string_with_now(5, now);
    if ctx.is_empty() {
        return None;
    }
    Some(format!("\n\nRecent events you remember:\n{ctx}"))
}

/// Long-term memory recall (keyword-based).
fn ltm_block(npc: &Npc, player_input: &str, world: &WorldState) -> Option<String> {
    let location = world.current_location();
    let mut kw: Vec<&str> = Vec::new();
    for word in player_input.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric());
        if trimmed.len() > 4 {
            kw.push(trimmed);
        }
    }
    kw.push(&location.name);
    let ctx = npc.long_term_memory.recall_context_string(&kw, 5);
    if ctx.is_empty() {
        return None;
    }
    Some(format!("\n\n{ctx}"))
}

/// Current mood of the NPC — injected into the user-turn context rather than
/// the system prompt so that mood changes do not bust the stable system-prompt
/// prefix that the model-runtime prefix cache (vllm-mlx `--enable-prefix-cache`)
/// depends on.
fn mood_block(npc: &Npc) -> String {
    let mood = npc.mood.trim();
    if mood.is_empty() {
        String::new()
    } else if mood.ends_with('.') {
        format!("\n\nYour current mood: {mood}")
    } else {
        format!("\n\nYour current mood: {mood}.")
    }
}

/// Gossip context from the gossip network.
fn gossip_block(world: &WorldState, npc: &Npc) -> Option<String> {
    let ctx = world.gossip_network.gossip_context_string(npc.id, 2);
    if ctx.is_empty() {
        return None;
    }
    Some(format!("\n\n{ctx}"))
}

// ── composite context builder ─────────────────────────────────────────────

/// Builds an enhanced context prompt for Tier 1 interactions using the given config.
///
/// Extends the base context with the NPC's recent memories and
/// information about other NPCs present at the same location.
/// The `_language` parameter is accepted for API uniformity with the system-prompt
/// builders but is not used — the language directive belongs in the system prompt.
#[allow(clippy::too_many_arguments)]
pub fn build_enhanced_context_with_config(
    npc: &Npc,
    world: &WorldState,
    player_input: &str,
    other_npcs: &[&Npc],
    _language: &LanguageSettings,
    config: &NpcConfig,
    _npc_names: &HashMap<NpcId, String>,
    player_name_for_npc: Option<&str>,
    was_introduced: bool,
) -> String {
    let mut context = build_tier1_context(world);

    // Mood goes into the dynamic context (not the system prompt) so that mood
    // changes never bust the stable system-prompt prefix the model-runtime
    // prefix cache depends on (vllm-mlx --enable-prefix-cache).
    context.push_str(&mood_block(npc));

    context.push_str(&location_anchor_block(world));

    context.push_str(&interlocutor_block(player_name_for_npc));

    if let Some(block) = introduced_anchor_block(npc, was_introduced) {
        context.push_str(&block);
    }

    if let Some(block) = other_npcs_block(npc, other_npcs, config) {
        context.push_str(&block);
    }

    if let Some(block) = conversation_block(world, npc, player_name_for_npc) {
        context.push_str(&block);
    }

    if let Some(block) = continuity_block(world, npc, player_name_for_npc) {
        context.push_str(&block);
    }

    if let Some(block) = reactions_block(npc, config) {
        context.push_str(&block);
    }

    if let Some(block) = stm_block(npc, world.clock.now()) {
        context.push_str(&block);
    }

    if let Some(block) = ltm_block(npc, player_input, world) {
        context.push_str(&block);
    }

    if let Some(block) = gossip_block(world, npc) {
        context.push_str(&block);
    }

    context
}

/// Builds an enhanced context prompt for Tier 1 interactions.
///
/// Extends the base context with the NPC's recent memories and
/// information about other NPCs present at the same location.
#[cfg(test)]
pub(crate) fn build_enhanced_context(
    npc: &Npc,
    world: &WorldState,
    player_input: &str,
    other_npcs: &[&Npc],
    language: &LanguageSettings,
    npc_names: &HashMap<NpcId, String>,
) -> String {
    let mut context = build_enhanced_context_with_config(
        npc,
        world,
        player_input,
        other_npcs,
        language,
        &NpcConfig::default(),
        npc_names,
        None,
        false,
    );
    // Player's current input last — everything above is context for this moment
    context.push_str("\n\n");
    context.push_str(&crate::build_named_action_line(player_input, None));
    context
}

// ── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NpcMetadata;
    use parish_config::RelationshipLabelConfig;
    use parish_world::WorldState;
    use std::collections::HashMap;

    fn make_test_npc(id: u32, name: &str, location: u32) -> Npc {
        let mut npc = crate::test_helpers::make_test_npc(id, location);
        npc.name = name.to_string();
        npc.brief_description = format!("a test NPC named {}", name);
        npc.age = 40;
        npc.personality = "Friendly".to_string();
        npc
    }

    #[test]
    fn test_enhanced_system_prompt_includes_relationships() {
        use crate::types::{Relationship, RelationshipKind};
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.8));
        npc.knowledge = vec!["Knows local history".to_string()];

        let npc_names: HashMap<NpcId, String> =
            [(NpcId(2), "Brigid".to_string())].into_iter().collect();
        let lang = LanguageSettings::english_only();
        let prompt = build_enhanced_system_prompt(&npc, false, &lang, &npc_names);
        assert!(prompt.contains("PEOPLE IN YOUR LIFE:"));
        assert!(prompt.contains("very close"));
        assert!(prompt.contains("WHAT'S ON YOUR MIND:"));
        assert!(prompt.contains("Knows local history"));
    }

    #[test]
    fn test_enhanced_system_prompt_without_relationships() {
        let npc = make_test_npc(1, "Padraig", 2);
        let npc_names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();
        let prompt = build_enhanced_system_prompt(&npc, false, &lang, &npc_names);
        assert!(!prompt.contains("PEOPLE IN YOUR LIFE:"));
        assert!(!prompt.contains("WHAT'S ON YOUR MIND:"));
    }

    #[test]
    fn test_enhanced_context_with_other_npcs() {
        let npc = make_test_npc(1, "Padraig", 1);
        let other = make_test_npc(2, "Tommy", 1);
        let world = WorldState::new();

        let npc_names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();
        let context = build_enhanced_context(
            &npc,
            &world,
            "greets everyone",
            &[&other],
            &lang,
            &npc_names,
        );
        assert!(context.contains("Also present"));
        assert!(context.contains("Tommy, the Test"));
        // Name anchor (TODO #11) — block must forbid addressing absent NPCs.
        assert!(
            context.contains("these are the only other people"),
            "other_npcs_block must anchor present-only addressing:\n{context}"
        );
    }

    #[test]
    fn test_interlocutor_block_named_player_has_anchor() {
        // Name anchor (TODO #35) — when the NPC knows the player's name,
        // the block must explicitly forbid addressing them by any other
        // name from recent history.
        let block = interlocutor_block(Some("Aiden Carney"));
        assert!(block.contains("Aiden Carney"), "missing player name");
        assert!(
            block.contains("Address them by the name 'Aiden Carney' only"),
            "missing strict-name anchor:\n{block}"
        );
        assert!(
            block.contains("recent conversation history"),
            "missing history-leak guard:\n{block}"
        );
    }

    #[test]
    fn test_interlocutor_block_unintroduced_player_has_anchor() {
        // Pre-introduction (TODO #35 corollary) — the NPC must NOT borrow a
        // name from the history buffer when it doesn't yet know the player.
        let block = interlocutor_block(None);
        assert!(block.contains("A newcomer to the parish"));
        assert!(
            block.contains("do not invent a name"),
            "missing invent-name guard:\n{block}"
        );
        assert!(
            block.contains("do not borrow a name"),
            "missing borrow-from-history guard:\n{block}"
        );
    }

    #[test]
    fn test_introduced_anchor_block_fires_only_when_previously_introduced() {
        // First contact: anchor must NOT render so the NPC
        // can introduce themselves on turn 1.
        let npc = make_test_npc(1, "Padraig", 1);
        assert!(
            introduced_anchor_block(&npc, false).is_none(),
            "anchor must not render on first contact"
        );

        // Follow-up turn: anchor must render with NPC name + occupation
        // and forbid mid-reply self-recitation.
        let block =
            introduced_anchor_block(&npc, true).expect("anchor must render on subsequent turns");
        assert!(
            block.contains("Padraig"),
            "anchor missing NPC name:\n{block}"
        );
        assert!(
            block.contains("Do not recite your full name and occupation"),
            "anchor missing recitation guard:\n{block}"
        );
        assert!(
            block.contains("Padraig, of <place>"),
            "anchor missing the specific 'Name, of place' pattern guard:\n{block}"
        );
    }

    #[test]
    fn test_location_anchor_block_pins_current_location() {
        // Location anchor (TODO #21) — block must name the current
        // location and forbid substituting any other settlement.
        let world = WorldState::new();
        let block = location_anchor_block(&world);
        let name = &world.current_location().name;
        assert!(
            block.contains("WHERE YOU ARE RIGHT NOW"),
            "missing anchor header:\n{block}"
        );
        assert!(
            block.contains(name.as_str()),
            "anchor must name current location ({name}):\n{block}"
        );
        assert!(
            block.contains("no other settlement"),
            "missing no-other-settlement directive:\n{block}"
        );
        assert!(
            block.contains("you are NOT at any of them"),
            "missing not-elsewhere guard:\n{block}"
        );
    }

    #[test]
    fn test_other_npcs_block_empty_emits_solo_anchor() {
        // Solo-NPC anchor (TODO #11) — when no one else is present, the
        // builder must still emit a directive forbidding addressing absent
        // characters as if they were here.
        let npc = make_test_npc(1, "Padraig", 1);
        let cfg = NpcConfig::default();
        let block = other_npcs_block(&npc, &[], &cfg).expect("solo anchor must render");
        assert!(
            block.contains("No one else is here"),
            "missing solo-NPC anchor:\n{block}"
        );
        assert!(
            block.contains("never as if they can hear you now"),
            "missing absent-as-present guard:\n{block}"
        );
    }

    #[test]
    fn test_enhanced_context_short_term_memory_injected() {
        use crate::memory::MemoryEntry;
        use chrono::TimeZone;
        use parish_world::LocationId;
        // Short-term memories are now injected unconditionally to prevent
        // NPCs from "forgetting" recent events even when keyword matching misses them.
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.memory.add(MemoryEntry {
            timestamp: chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "Saw a stranger at the crossroads".to_string(),
            participants: vec![NpcId(1)],
            location: LocationId(1),
            kind: None,
        });
        let world = WorldState::new();

        let npc_names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();
        let context = build_enhanced_context(&npc, &world, "says hello", &[], &lang, &npc_names);
        assert!(context.contains("Recent events you remember:"));
        assert!(context.contains("Saw a stranger at the crossroads"));
    }

    #[test]
    fn test_build_enhanced_context_action_line_at_end() {
        let npc = make_test_npc(1, "Padraig", 1);
        let world = WorldState::new();
        let npc_names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();
        let context = build_enhanced_context(&npc, &world, "hello there", &[], &lang, &npc_names);
        // The newcomer's current input must be the last meaningful content
        let action_line = "The newcomer says: \"hello there\"";
        assert!(context.contains(action_line));
        assert!(
            context.rfind(action_line) > context.rfind("Your Location:"),
            "action line should come after location context"
        );
    }

    #[test]
    fn test_build_enhanced_system_prompt_with_config() {
        use crate::types::{Relationship, RelationshipKind};
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.8));

        let config = NpcConfig {
            relationship_labels: RelationshipLabelConfig {
                very_close: 0.9,
                ..RelationshipLabelConfig::default()
            },
            ..NpcConfig::default()
        };
        let npc_names: HashMap<NpcId, String> =
            [(NpcId(2), "Brigid".to_string())].into_iter().collect();
        let lang = LanguageSettings::english_only();
        let prompt =
            build_enhanced_system_prompt_with_config(&npc, false, &lang, &config, &npc_names, None);
        // 0.8 is below 0.9 threshold, so should be "friendly" not "very close"
        assert!(prompt.contains("friendly"));
        assert!(!prompt.contains("very close"));
    }

    #[test]
    fn test_relationship_strength_descriptions() {
        use crate::types::{Relationship, RelationshipKind};
        let mut npc = make_test_npc(1, "Test", 1);

        // Test all strength tiers appear in the prompt
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Family, 0.9));
        npc.relationships
            .insert(NpcId(3), Relationship::new(RelationshipKind::Enemy, -0.8));

        let npc_names: HashMap<NpcId, String> = [
            (NpcId(2), "Siobhan".to_string()),
            (NpcId(3), "Cormac".to_string()),
        ]
        .into_iter()
        .collect();
        let lang = LanguageSettings::english_only();
        let prompt = build_enhanced_system_prompt(&npc, false, &lang, &npc_names);
        assert!(prompt.contains("very close") || prompt.contains("hostile"));
    }

    #[test]
    fn test_relationship_label_with_default_config() {
        let config = RelationshipLabelConfig::default();
        assert_eq!(relationship_label_with_config(0.9, &config), "very close");
        assert_eq!(relationship_label_with_config(0.5, &config), "friendly");
        assert_eq!(relationship_label_with_config(0.1, &config), "acquainted");
        assert_eq!(relationship_label_with_config(-0.1, &config), "cool");
        assert_eq!(relationship_label_with_config(-0.5, &config), "strained");
        assert_eq!(relationship_label_with_config(-0.9, &config), "hostile");
    }

    #[test]
    fn test_relationship_label_with_custom_config() {
        let config = RelationshipLabelConfig {
            very_close: 0.9,
            friendly: 0.5,
            acquainted: 0.0,
            cool: -0.5,
            strained: -0.9,
        };
        // 0.8 is below 0.9, so "friendly" instead of "very close"
        assert_eq!(relationship_label_with_config(0.8, &config), "friendly");
        // 0.3 is below 0.5, so "acquainted" instead of "friendly"
        assert_eq!(relationship_label_with_config(0.3, &config), "acquainted");
    }

    #[test]
    fn test_relationship_label_default_wrapper() {
        assert_eq!(relationship_label(0.9), "very close");
        assert_eq!(relationship_label(-0.9), "hostile");
    }

    #[test]
    fn mood_block_empty_mood_produces_empty_string() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = String::new();
        assert_eq!(mood_block(&npc), "");
    }

    #[test]
    fn mood_block_whitespace_only_mood_produces_empty_string() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "   ".to_string();
        assert_eq!(mood_block(&npc), "");
    }

    #[test]
    fn mood_block_normal_mood_appends_period() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "calm".to_string();
        assert_eq!(mood_block(&npc), "\n\nYour current mood: calm.");
    }

    #[test]
    fn mood_block_mood_already_ending_with_period_no_double_punctuation() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "calm.".to_string();
        let result = mood_block(&npc);
        assert_eq!(result, "\n\nYour current mood: calm.");
        assert!(!result.contains("calm.."), "must not double the period");
    }

    /// Regression guard: the system prompt must be byte-stable when only the
    /// NPC mood changes (mood lives in the dynamic context, not the system
    /// prompt, so a mood change must not alter the system block).
    #[test]
    fn tier1_system_prompt_stable_across_mood_change() {
        let npc_names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();

        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "calm".to_string();
        let prompt_before = build_enhanced_system_prompt(&npc, false, &lang, &npc_names);

        npc.mood = "anxious".to_string();
        let prompt_after = build_enhanced_system_prompt(&npc, false, &lang, &npc_names);

        assert_eq!(
            prompt_before, prompt_after,
            "system prompt must be byte-identical across mood changes"
        );
    }

    #[test]
    fn test_format_relationships_natural_empty() {
        let names: HashMap<NpcId, String> = HashMap::new();
        let cfg = RelationshipLabelConfig::default();
        assert_eq!(format_relationships_natural(&[], &names, &cfg), "");
    }

    #[test]
    fn test_format_relationships_natural_known_names() {
        let names: HashMap<NpcId, String> = [
            (NpcId(2), "Mary McKenna".to_string()),
            (NpcId(3), "Sean Doyle".to_string()),
        ]
        .into_iter()
        .collect();
        let cfg = RelationshipLabelConfig::default();
        let out = format_relationships_natural(&[(NpcId(2), 0.5), (NpcId(3), -0.3)], &names, &cfg);
        assert!(out.contains("Mary McKenna"));
        assert!(out.contains("Sean Doyle"));
        // No raw NPC id
        assert!(!out.contains("NPC 2"));
        assert!(!out.contains("(0.5)"));
    }

    #[test]
    fn test_format_relationships_natural_unknown_name_fallback() {
        let names: HashMap<NpcId, String> = HashMap::new();
        let cfg = RelationshipLabelConfig::default();
        let out = format_relationships_natural(&[(NpcId(99), 0.8)], &names, &cfg);
        assert!(out.contains("someone"));
    }

    #[test]
    fn test_format_relationships_natural_player_rendered_as_newcomer() {
        // NpcId(0) is the conventional player id. The player is never in
        // npc_names (the map is built from NpcManager NPCs), so the natural
        // fallback would be "someone" — but the prompt reads better with the
        // Tier 1 convention of "the newcomer".
        let names: HashMap<NpcId, String> = HashMap::new();
        let cfg = RelationshipLabelConfig::default();
        let out = format_relationships_natural(&[(NpcId(0), 0.9)], &names, &cfg);
        assert!(out.contains("the newcomer"));
        assert!(!out.contains("someone"));
    }

    #[test]
    fn test_npc_snapshot_uses_prose_not_codes() {
        // npc_snapshot_from_npc should produce natural prose, not INT[...] codes.
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.intelligence = crate::types::Intelligence::new(4, 3, 5, 3, 3, 3);
        let names: HashMap<NpcId, String> = HashMap::new();

        let snap = super::super::tier2::npc_snapshot_from_npc(&npc, &names);

        assert!(!snap.intelligence_prose.is_empty());
        assert!(!snap.intelligence_prose.contains("INT["));
        // Specific phrasing from prompt_guidance for high verbal / high emotional.
        assert!(snap.intelligence_prose.contains("Well-spoken"));
        assert!(snap.intelligence_prose.contains("Reads people like a book"));
    }

    #[test]
    fn test_apply_tier1_response_updates_mood() {
        use crate::NpcStreamResponse;
        use chrono::TimeZone;
        let mut npc = make_test_npc(1, "Padraig", 1);
        let response = NpcStreamResponse {
            dialogue: "Hello there!".to_string(),
            metadata: Some(NpcMetadata {
                action: "speaks".to_string(),
                mood: "cheerful".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
            }),
        };
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        super::super::tier1::apply_tier1_response(&mut npc, &response, "says hello", game_time);

        assert_eq!(npc.mood, "cheerful");
        assert_eq!(npc.memory.len(), 1);
    }

    #[test]
    fn test_apply_tier1_response_no_metadata() {
        use crate::NpcStreamResponse;
        use chrono::TimeZone;
        let mut npc = make_test_npc(1, "Padraig", 1);
        let response = NpcStreamResponse {
            dialogue: "Hello there!".to_string(),
            metadata: None,
        };
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        super::super::tier1::apply_tier1_response(&mut npc, &response, "waves", game_time);

        assert_eq!(npc.mood, "calm"); // mood should not change
        assert_eq!(npc.memory.len(), 1); // memory still recorded
    }

    #[test]
    fn test_apply_tier1_response_with_config_truncation() {
        use crate::NpcStreamResponse;
        use chrono::TimeZone;
        let mut npc = make_test_npc(1, "Padraig", 1);
        let long_dialogue = "a".repeat(200);
        let response = NpcStreamResponse {
            dialogue: long_dialogue,
            metadata: None,
        };
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        let config = NpcConfig {
            memory_truncation_dialogue: 40,
            memory_truncation_event_log: 30,
            ..NpcConfig::default()
        };
        let events = super::super::tier1::apply_tier1_response_with_config(
            &mut npc, &response, "waves", game_time, &config, None,
        );

        // The debug event log entry should be truncated to ~30 chars
        assert!(events.iter().any(|e| e.contains("remembers:")));
        assert_eq!(npc.memory.len(), 1);
    }

    #[test]
    fn test_apply_tier1_response_same_mood_no_change_event() {
        use crate::NpcStreamResponse;
        use chrono::TimeZone;
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "calm".to_string();
        let response = NpcStreamResponse {
            dialogue: "Hello.".to_string(),
            metadata: Some(NpcMetadata {
                action: "speaks".to_string(),
                mood: "calm".to_string(), // same mood
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
            }),
        };
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events =
            super::super::tier1::apply_tier1_response(&mut npc, &response, "hello", game_time);
        // No mood change event
        assert!(!events.iter().any(|e| e.contains("mood:")));
        // Memory still recorded
        assert!(events.iter().any(|e| e.contains("remembers:")));
    }

    #[test]
    fn test_apply_tier1_response_empty_mood_no_change() {
        use crate::NpcStreamResponse;
        use chrono::TimeZone;
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "calm".to_string();
        let response = NpcStreamResponse {
            dialogue: "Hello.".to_string(),
            metadata: Some(NpcMetadata {
                action: "speaks".to_string(),
                mood: String::new(), // empty mood
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
            }),
        };
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events =
            super::super::tier1::apply_tier1_response(&mut npc, &response, "hello", game_time);
        assert_eq!(npc.mood, "calm"); // mood should not change
        assert!(!events.iter().any(|e| e.contains("mood:")));
    }
}
