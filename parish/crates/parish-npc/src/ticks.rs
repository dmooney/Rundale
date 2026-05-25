//! Tier 1, Tier 2, and Tier 3 tick functions for NPC simulation.
//!
//! Tier 1 ticks run per player interaction (full LLM inference).
//! Tier 2 ticks run every 5 game-minutes for nearby NPCs (lighter inference).
//! Tier 3 ticks run every 1 game-day for distant NPCs (batch inference).

use chrono::{DateTime, Utc};

use crate::memory::{MemoryEntry, try_promote};
use crate::types::{Tier2Event, Tier2Response, Tier3Response, Tier3Update};
use crate::{
    LanguageSettings, Npc, NpcId, NpcStreamResponse, build_tier1_context, build_tier1_system_prompt,
};
use parish_config::{NpcConfig, RelationshipLabelConfig};
use parish_types::GossipNetwork;
use parish_types::ParishError;
use parish_world::graph::WorldGraph;
use parish_world::{LocationId, WorldState};

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
}

/// A group of NPC snapshots at a single location, for Tier 2 processing.
#[derive(Debug, Clone)]
pub struct Tier2Group {
    /// Location where these NPCs are gathered.
    pub location: LocationId,
    /// Location name for prompt context.
    pub location_name: String,
    /// Snapshots of NPCs at this location.
    pub npcs: Vec<NpcSnapshot>,
}

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
    npc_names: &std::collections::HashMap<NpcId, String>,
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

/// Builds an enhanced system prompt for Tier 1 interactions using the given config.
///
/// Extends the base system prompt with relationship summaries (using real names)
/// and knowledge entries for richer, more contextual NPC dialogue.
#[cfg(test)]
pub(crate) fn build_enhanced_system_prompt(
    npc: &Npc,
    improv: bool,
    language: &LanguageSettings,
    npc_names: &std::collections::HashMap<NpcId, String>,
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
    npc_names: &std::collections::HashMap<NpcId, String>,
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

/// "Already introduced" anchor — fires only on the second and later
/// turns with a given NPC (when the NPC's name has already been
/// surfaced to the player). TODO #39 captured this failure mode:
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

/// Gossip context from the gossip network.
fn gossip_block(world: &WorldState, npc: &Npc) -> Option<String> {
    let ctx = world.gossip_network.gossip_context_string(npc.id, 2);
    if ctx.is_empty() {
        return None;
    }
    Some(format!("\n\n{ctx}"))
}

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
    _npc_names: &std::collections::HashMap<NpcId, String>,
    player_name_for_npc: Option<&str>,
    was_introduced: bool,
) -> String {
    let mut context = build_tier1_context(world);

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
    npc_names: &std::collections::HashMap<NpcId, String>,
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

/// Processes a Tier 1 NPC response using the given config, updating mood and recording a memory.
///
/// Call this after receiving and parsing the LLM response for a Tier 1
/// interaction. Updates the NPC's mood from metadata and adds a memory
/// entry recording the interaction.
///
/// Returns a list of debug event strings (e.g. mood changes, memory commits).
pub fn apply_tier1_response_with_config(
    npc: &mut Npc,
    response: &NpcStreamResponse,
    player_input: &str,
    game_time: chrono::DateTime<Utc>,
    config: &NpcConfig,
    player_name: Option<&str>,
) -> Vec<String> {
    let mut events = Vec::new();

    // Update mood from metadata
    if let Some(ref meta) = response.metadata
        && !meta.mood.is_empty()
        && meta.mood != npc.mood
    {
        events.push(format!("{} mood: {} -> {}", npc.name, npc.mood, meta.mood));
        npc.mood = meta.mood.clone();
    }

    // Record memory of the interaction, using player's name if known
    let speaker_label = player_name.unwrap_or("A newcomer");
    let content = format!(
        "{} said: '{}'. Responded: {}",
        speaker_label,
        player_input,
        truncate_for_memory(&response.dialogue, config.memory_truncation_dialogue)
    );
    events.push(format!(
        "{} remembers: {}",
        npc.name,
        truncate_for_memory(&content, config.memory_truncation_event_log)
    ));
    let mem_entry = MemoryEntry {
        timestamp: game_time,
        content,
        participants: vec![NpcId(0), npc.id], // NpcId(0) = player
        location: npc.location,
        kind: Some(crate::memory::MemoryKind::SpokeWithPlayer),
    };
    if let Some(evicted) = npc.memory.add(mem_entry) {
        let npc_name = npc.name.clone();
        let loc_name = String::new(); // location name not available here
        try_promote(&mut npc.long_term_memory, &evicted, &[npc_name], &loc_name);
    }

    events
}

/// Processes a Tier 1 NPC response, updating mood and recording a memory.
///
/// Call this after receiving and parsing the LLM response for a Tier 1
/// interaction. Updates the NPC's mood from metadata and adds a memory
/// entry recording the interaction.
///
/// Returns a list of debug event strings (e.g. mood changes, memory commits).
#[cfg(test)]
pub(crate) fn apply_tier1_response(
    npc: &mut Npc,
    response: &NpcStreamResponse,
    player_input: &str,
    game_time: chrono::DateTime<Utc>,
) -> Vec<String> {
    apply_tier1_response_with_config(
        npc,
        response,
        player_input,
        game_time,
        &NpcConfig::default(),
        None,
    )
}

/// Records witness memories for NPCs who overheard a player-NPC conversation.
///
/// When the player speaks to one NPC, other NPCs at the same location
/// witness the exchange and store it in their short-term memory. This
/// gives bystander NPCs awareness of what's been said around them.
pub fn record_witness_memories(
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    speaker_id: NpcId,
    speaker_name: &str,
    player_input: &str,
    npc_dialogue: &str,
    game_time: chrono::DateTime<chrono::Utc>,
    location: LocationId,
) -> Vec<String> {
    let mut debug_events = Vec::new();

    let content = format!(
        "Overheard: a newcomer said '{}' and {} replied '{}'",
        player_input, speaker_name, npc_dialogue,
    );

    // Collect witness IDs first to avoid borrow issues
    let witness_ids: Vec<NpcId> = npcs
        .values()
        .filter(|npc| npc.location == location && npc.id != speaker_id)
        .filter(|npc| matches!(npc.state, crate::types::NpcState::Present))
        .map(|npc| npc.id)
        .collect();

    for witness_id in witness_ids {
        let mem_entry = MemoryEntry {
            timestamp: game_time,
            content: content.clone(),
            participants: vec![NpcId(0), speaker_id, witness_id],
            location,
            kind: Some(crate::memory::MemoryKind::OverheardConversation),
        };

        if let Some(witness) = npcs.get_mut(&witness_id) {
            debug_events.push(format!(
                "{} overheard: {}",
                witness.name,
                truncate_for_memory(&content, 80),
            ));

            if let Some(evicted) = witness.memory.add(mem_entry) {
                let witness_name = witness.name.clone();
                try_promote(&mut witness.long_term_memory, &evicted, &[witness_name], "");
            }
        }
    }

    debug_events
}

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

/// Returns the top-N strongest relationships (by absolute strength) in a
/// stable order: by |strength| descending, then by NpcId ascending as a
/// tie-breaker so iteration order over the underlying HashMap doesn't leak.
fn top_relationships(npc: &Npc, n: usize) -> Vec<(NpcId, f64)> {
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
pub fn npc_snapshot_from_npc(
    npc: &Npc,
    npc_names: &std::collections::HashMap<NpcId, String>,
) -> NpcSnapshot {
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
    }
}

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

    // Cap output to bound vllm-mlx runaway risk on uncapped JSON gen.
    // Tier 2 outputs ~50-100 tokens in practice; 200 is comfortable headroom.
    // Streaming path: discards chunks (the assembled string returns from
    // generate_stream_with_format) but enables mid-flight cancellation (#9).
    let (sink_tx, mut sink_rx) =
        tokio::sync::mpsc::channel::<String>(parish_inference::TOKEN_CHANNEL_CAPACITY);
    tokio::spawn(async move { while sink_rx.recv().await.is_some() {} });

    let stream_fut =
        client.generate_stream_with_format(model, &prompt, None, sink_tx, None, Some(200), None);

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

    match raw.and_then(|s| {
        serde_json::from_str::<Tier2Response>(&s)
            .map_err(|e| ParishError::Inference(format!("Tier 2 JSON parse failed: {e}")))
    }) {
        Ok(resp) => Some(Tier2Event {
            location: group.location,
            summary: resp.summary,
            participants: participant_ids,
            mood_changes: resp.mood_changes,
            relationship_changes: resp.relationship_changes,
        }),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("cancelled mid-stream") {
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
pub fn tier2_summary_mentions_absent_npc(
    event: &Tier2Event,
    npcs: &std::collections::HashMap<NpcId, Npc>,
) -> bool {
    summary_mentions_absent_npc(&event.summary, &event.participants, npcs).is_some()
}

fn summary_mentions_absent_npc(
    summary: &str,
    participants: &[NpcId],
    npcs: &std::collections::HashMap<NpcId, Npc>,
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

/// Applies a Tier 2 event's effects to the relevant NPCs using the given config.
///
/// Updates moods, adjusts relationship strengths, and records memories
/// for all participating NPCs.
///
/// Returns debug event strings describing what happened.
pub fn apply_tier2_event_with_config(
    event: &Tier2Event,
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    game_time: chrono::DateTime<Utc>,
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
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    game_time: chrono::DateTime<Utc>,
) -> Vec<String> {
    apply_tier2_event_with_config(
        event,
        npcs,
        game_time,
        &NpcConfig::default(),
        &parish_types::events::EventBus::new(),
    )
}

/// Creates gossip from a Tier 2 event if it is notable.
///
/// Notable events are those with significant relationship changes (|delta| > 0.3)
/// or summaries longer than a trivial threshold. The first participant is treated
/// as the gossip source.
pub fn create_gossip_from_tier2_event(
    event: &Tier2Event,
    gossip_network: &mut GossipNetwork,
    game_time: chrono::DateTime<Utc>,
) {
    // Create gossip from large relationship changes
    for rc in &event.relationship_changes {
        if rc.delta.abs() > 0.3 {
            gossip_network.create(
                event.summary.clone(),
                *event.participants.first().unwrap_or(&NpcId(0)),
                game_time,
            );
            return; // One gossip item per event is enough
        }
    }

    // Create gossip from non-trivial dialogue summaries (>30 chars suggests substance)
    if event.summary.len() > 30 {
        gossip_network.create(
            event.summary.clone(),
            *event.participants.first().unwrap_or(&NpcId(0)),
            game_time,
        );
    }
}

/// Propagates gossip between NPCs during a Tier 2 group interaction.
///
/// For each pair of NPCs at the same location, attempts to propagate
/// gossip from one to the other. Returns the total count of rumors
/// transmitted across all pairs in this group.
pub fn propagate_gossip_at_location(
    participant_ids: &[NpcId],
    gossip_network: &mut GossipNetwork,
    rng: &mut impl rand::Rng,
) -> usize {
    let mut total_transmitted = 0usize;
    for i in 0..participant_ids.len() {
        for j in (i + 1)..participant_ids.len() {
            let transmitted = gossip_network.propagate(participant_ids[i], participant_ids[j], rng);
            total_transmitted += transmitted.len();
            // Also propagate in reverse direction
            let transmitted = gossip_network.propagate(participant_ids[j], participant_ids[i], rng);
            total_transmitted += transmitted.len();
        }
    }
    total_transmitted
}

// ---------------------------------------------------------------------------
// Tier 3 — batch inference for distant NPCs
// ---------------------------------------------------------------------------

/// Default batch size for Tier 3 inference (NPCs per LLM call).
pub const TIER3_BATCH_SIZE: usize = 10;

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

    let mut prompt = format!(
        "You are simulating background NPC activity in a rural Irish parish in 1820. \
        Simulate {hours} hours of activity for the people below. \
        The weather is {weather}, the season is {season}, the time is {time}.\n\n\
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

/// Creates a Tier 3 snapshot from an NPC, resolving location names from the graph.
///
/// Peer names are resolved at snapshot time via `npc_names` so the snapshot is
/// self-contained — the prompt builder does not need access to the name map.
pub fn tier3_snapshot_from_npc(
    npc: &Npc,
    graph: &WorldGraph,
    npc_names: &std::collections::HashMap<NpcId, String>,
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
}

/// Runs a Tier 3 batch simulation for distant NPCs.
///
/// Builds a single prompt summarizing all provided NPC snapshots and their states,
/// submits it to the inference queue with `Batch` priority, and parses the JSON
/// response. If there are more NPCs than `batch_size`, they are split into
/// multiple sequential queue submissions.
pub async fn tick_tier3(ctx: &Tier3Context<'_>) -> Result<Vec<Tier3Update>, ParishError> {
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
        tokio::spawn(async move { while sink_rx.recv().await.is_some() {} });

        let stream_fut = ctx.client.generate_stream_with_format(
            ctx.model,
            &prompt,
            None,
            sink_tx,
            None,
            Some(600),
            None,
        );

        let raw = match ctx.cancel.clone() {
            Some(tok) => tokio::select! {
                biased;
                () = tok.cancelled() => Err(ParishError::Inference(
                    "Tier 3 cancelled mid-stream".to_string(),
                )),
                res = stream_fut => res,
            },
            None => stream_fut.await,
        };

        match raw.and_then(|s| {
            serde_json::from_str::<Tier3Response>(&s)
                .map_err(|e| ParishError::Inference(format!("Tier 3 JSON parse failed: {e}")))
        }) {
            Ok(resp) => {
                all_updates.extend(resp.updates);
            }
            Err(e) => {
                tracing::warn!("Tier 3 batch inference failed: {}", e);
                // Continue with other batches rather than failing entirely
            }
        }
    }

    Ok(all_updates)
}

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
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    graph: &WorldGraph,
    game_time: chrono::DateTime<Utc>,
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
                    npc.location = new_loc;
                    event_bus.publish(parish_types::events::GameEvent::NpcDeparted {
                        npc_id: update.npc_id,
                        location: from,
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

/// Truncates a string to a maximum length, adding "..." if truncated.
fn truncate_for_memory(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = crate::floor_char_boundary(s, max_len.saturating_sub(3));
        let safe_boundary = boundary.min(s.len());
        format!("{}...", &s[..safe_boundary])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NpcMetadata;
    use crate::types::{MoodChange, Relationship, RelationshipChange, RelationshipKind};
    use chrono::TimeZone;
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

        let npc_names: std::collections::HashMap<NpcId, String> = std::collections::HashMap::new();
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
        // TODO #39 — first contact: anchor must NOT render so the NPC
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
        // Short-term memories are now injected unconditionally to prevent
        // NPCs from "forgetting" recent events even when keyword matching misses them.
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.memory.add(MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "Saw a stranger at the crossroads".to_string(),
            participants: vec![NpcId(1)],
            location: LocationId(1),
            kind: None,
        });
        let world = WorldState::new();

        let npc_names: std::collections::HashMap<NpcId, String> = std::collections::HashMap::new();
        let lang = LanguageSettings::english_only();
        let context = build_enhanced_context(&npc, &world, "says hello", &[], &lang, &npc_names);
        assert!(context.contains("Recent events you remember:"));
        assert!(context.contains("Saw a stranger at the crossroads"));
    }

    #[test]
    fn test_apply_tier1_response_updates_mood() {
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
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        apply_tier1_response(&mut npc, &response, "says hello", game_time);

        assert_eq!(npc.mood, "cheerful");
        assert_eq!(npc.memory.len(), 1);
    }

    #[test]
    fn test_apply_tier1_response_no_metadata() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        let response = NpcStreamResponse {
            dialogue: "Hello there!".to_string(),
            metadata: None,
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        apply_tier1_response(&mut npc, &response, "waves", game_time);

        assert_eq!(npc.mood, "calm"); // mood should not change
        assert_eq!(npc.memory.len(), 1); // memory still recorded
    }

    #[test]
    fn test_build_tier2_prompt() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
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
        assert!(prompt.contains("Evening"));
        assert!(prompt.contains("Overcast"));
        assert!(prompt.contains("summary"));
        // No more cryptic encoding
        assert!(!prompt.contains("INT["));
    }

    #[test]
    fn test_apply_tier2_event() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc1 = make_test_npc(1, "Padraig", 2);
        npc1.relationships
            .insert(NpcId(5), Relationship::new(RelationshipKind::Friend, 0.5));
        npcs.insert(NpcId(1), npc1);
        npcs.insert(NpcId(5), make_test_npc(5, "Tommy", 2));

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

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
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
        use parish_types::events::{EventBus, GameEvent};

        let config = NpcConfig::default();
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();

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
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig Darcy", 2));
        npcs.insert(NpcId(5), make_test_npc(5, "Tommy O'Brien", 2));
        // Aoife is authored elsewhere — not part of this scene.
        npcs.insert(NpcId(9), make_test_npc(9, "Aoife Brennan", 3));

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
    fn test_truncate_for_memory() {
        assert_eq!(truncate_for_memory("short", 10), "short");
        let long = "a".repeat(100);
        let truncated = truncate_for_memory(&long, 20);
        assert!(truncated.len() <= 20);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_relationship_strength_descriptions() {
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
    fn test_build_enhanced_system_prompt_with_config() {
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
    fn test_build_enhanced_context_action_line_at_end() {
        let npc = make_test_npc(1, "Padraig", 1);
        let world = WorldState::new();
        let npc_names: std::collections::HashMap<NpcId, String> = std::collections::HashMap::new();
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
    fn test_apply_tier1_response_with_config_truncation() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        let long_dialogue = "a".repeat(200);
        let response = NpcStreamResponse {
            dialogue: long_dialogue,
            metadata: None,
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        let config = NpcConfig {
            memory_truncation_dialogue: 40,
            memory_truncation_event_log: 30,
            ..NpcConfig::default()
        };
        let events = apply_tier1_response_with_config(
            &mut npc, &response, "waves", game_time, &config, None,
        );

        // The debug event log entry should be truncated to ~30 chars
        assert!(events.iter().any(|e| e.contains("remembers:")));
        assert_eq!(npc.memory.len(), 1);
    }

    #[test]
    fn test_apply_tier2_event_with_config_truncation() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));

        let long_summary = "a".repeat(200);
        let event = Tier2Event {
            location: LocationId(2),
            summary: long_summary,
            participants: vec![NpcId(1)],
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let config = NpcConfig {
            event_summary_truncation: 40,
            event_summary_debug_truncation: 20,
            ..NpcConfig::default()
        };

        let events = apply_tier2_event_with_config(
            &event,
            &mut npcs,
            game_time,
            &config,
            &parish_types::events::EventBus::new(),
        );
        assert!(!events.is_empty());

        // The stored memory content should be truncated to ~40 chars
        let mem = &npcs.get(&NpcId(1)).unwrap().memory;
        let recent = mem.recent(1);
        assert!(recent[0].content.len() <= 40);
    }

    // --- truncate_for_memory edge cases ---

    #[test]
    fn test_truncate_for_memory_empty_string() {
        assert_eq!(truncate_for_memory("", 10), "");
    }

    #[test]
    fn test_truncate_for_memory_exact_boundary() {
        assert_eq!(truncate_for_memory("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_for_memory_one_over() {
        let result = truncate_for_memory("123456", 5);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 5);
    }

    #[test]
    fn test_truncate_for_memory_max_len_zero() {
        let result = truncate_for_memory("hello", 0);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_for_memory_max_len_three() {
        // max_len=3 means only room for "..."
        let result = truncate_for_memory("hello world", 3);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_for_memory_multibyte_utf8() {
        // Ensure truncation doesn't split multi-byte characters
        let irish = "Dia dhuit, a chara. Cén chaoi a bhfuil tú?";
        let result = truncate_for_memory(irish, 15);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 18); // slightly over due to char boundary
        // Should be valid UTF-8 (no panic)
        let _ = result.chars().count();
    }

    #[test]
    fn test_truncate_for_memory_very_long_string() {
        let long = "x".repeat(10000);
        let result = truncate_for_memory(&long, 50);
        assert!(result.len() <= 50);
        assert!(result.ends_with("..."));
    }

    // --- apply_tier2_event edge cases ---

    #[test]
    fn test_apply_tier2_event_missing_npc_in_map() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));
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

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
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
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Nothing happened".to_string(),
            participants: Vec::new(),
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier2_event(&event, &mut npcs, game_time);
        assert!(events.is_empty());
        // No memories added
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 0);
    }

    #[test]
    fn test_apply_tier2_event_same_mood_no_debug_event() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc = make_test_npc(1, "Padraig", 2);
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

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
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
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));
        npcs.insert(NpcId(5), make_test_npc(5, "Tommy", 2));

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

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
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
            npcs: vec![NpcSnapshot {
                id: NpcId(1),
                name: "Padraig".to_string(),
                occupation: "Publican".to_string(),
                personality: "Warm".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: "Perceptive, wise, quick-witted.".to_string(),
                mood: "content".to_string(),
                relationship_summary: String::new(),
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
            npcs: Vec::new(),
        };

        // Empty group short-circuits before any LLM call.
        let client = parish_inference::AnyClient::simulator();
        let lang = LanguageSettings::english_only();
        let event =
            run_tier2_for_group(&client, "test", &group, "Morning", "Clear", &lang, None).await;
        assert!(event.is_none());
    }

    // --- build_tier2_prompt weather commentary ---

    #[test]
    fn test_build_tier2_prompt_rain_commentary() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "The Crossroads".to_string(),
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
                },
            ],
        };

        let lang = LanguageSettings::english_only();
        let prompt = build_tier2_prompt(&group, "Afternoon", "Heavy Rain", &lang);
        assert!(prompt.contains("commenting on the weather"));

        let prompt = build_tier2_prompt(&group, "Afternoon", "Clear", &lang);
        assert!(!prompt.contains("commenting on the weather"));
    }

    // --- apply_tier1_response same mood no change event ---

    #[test]
    fn test_apply_tier1_response_same_mood_no_change_event() {
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
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events = apply_tier1_response(&mut npc, &response, "hello", game_time);
        // No mood change event
        assert!(!events.iter().any(|e| e.contains("mood:")));
        // Memory still recorded
        assert!(events.iter().any(|e| e.contains("remembers:")));
    }

    #[test]
    fn test_apply_tier1_response_empty_mood_no_change() {
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
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events = apply_tier1_response(&mut npc, &response, "hello", game_time);
        assert_eq!(npc.mood, "calm"); // mood should not change
        assert!(!events.iter().any(|e| e.contains("mood:")));
    }

    // --- Tier 3 tests ---

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
        use crate::types::{Relationship, RelationshipKind};
        use parish_world::graph::WorldGraph;

        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc1 = make_test_npc(1, "Padraig", 2);
        npc1.relationships
            .insert(NpcId(5), Relationship::new(RelationshipKind::Friend, 0.5));
        npcs.insert(NpcId(1), npc1);
        npcs.insert(NpcId(5), make_test_npc(5, "Tommy", 2));

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

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
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
        use parish_world::graph::WorldGraph;

        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));

        let graph = WorldGraph::new(); // empty graph — no valid locations

        let updates = vec![Tier3Update {
            npc_id: NpcId(1),
            mood: "calm".to_string(),
            activity_summary: "Walked to market.".to_string(),
            new_location: Some(LocationId(999)), // nonexistent
            relationship_changes: Vec::new(),
        }];

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
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
        use parish_world::graph::WorldGraph;

        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));

        let graph = WorldGraph::new();

        let updates = vec![Tier3Update {
            npc_id: NpcId(99), // does not exist
            mood: "happy".to_string(),
            activity_summary: "Ghost NPC.".to_string(),
            new_location: None,
            relationship_changes: Vec::new(),
        }];

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
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
        let mut npc = make_test_npc(1, "Padraig", 2);
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

        let mut npc = make_test_npc(1, "Padraig", 2);
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
        let npc = make_test_npc(1, "Padraig", 2);
        let graph = WorldGraph::new();
        let snap = tier3_snapshot_from_npc(&npc, &graph, &HashMap::new());

        assert_eq!(snap.context, "");
    }

    // ── Witness memory tests ──────────────────────────────────────────

    #[test]
    fn test_witness_memory_created_for_bystander() {
        let mut npcs = HashMap::new();
        let speaker = make_test_npc(1, "Padraig", 1);
        let witness = make_test_npc(2, "Niamh", 1);
        npcs.insert(NpcId(1), speaker);
        npcs.insert(NpcId(2), witness);

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events = record_witness_memories(
            &mut npcs,
            NpcId(1),
            "Padraig",
            "Tell me about the weather",
            "Ah, it's grand today",
            game_time,
            LocationId(1),
        );

        assert_eq!(events.len(), 1);
        assert!(events[0].contains("Niamh overheard"));

        // Witness should have the memory
        let witness = npcs.get(&NpcId(2)).unwrap();
        assert_eq!(witness.memory.len(), 1);
        let mem = witness.memory.recent(1);
        assert!(mem[0].content.contains("Overheard"));
        assert!(mem[0].content.contains("Padraig"));
    }

    #[test]
    fn test_speaker_not_given_witness_memory() {
        let mut npcs = HashMap::new();
        let speaker = make_test_npc(1, "Padraig", 1);
        let witness = make_test_npc(2, "Niamh", 1);
        npcs.insert(NpcId(1), speaker);
        npcs.insert(NpcId(2), witness);

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        record_witness_memories(
            &mut npcs,
            NpcId(1),
            "Padraig",
            "Hello",
            "Dia dhuit!",
            game_time,
            LocationId(1),
        );

        // Speaker should NOT have a witness memory
        let speaker = npcs.get(&NpcId(1)).unwrap();
        assert!(speaker.memory.is_empty());
    }

    #[test]
    fn test_witness_memory_only_for_present_npcs() {
        let mut npcs = HashMap::new();
        let speaker = make_test_npc(1, "Padraig", 1);
        let witness_here = make_test_npc(2, "Niamh", 1);
        let witness_away = make_test_npc(3, "Tommy", 2); // different location
        npcs.insert(NpcId(1), speaker);
        npcs.insert(NpcId(2), witness_here);
        npcs.insert(NpcId(3), witness_away);

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events = record_witness_memories(
            &mut npcs,
            NpcId(1),
            "Padraig",
            "Hello",
            "Dia dhuit!",
            game_time,
            LocationId(1),
        );

        assert_eq!(events.len(), 1); // only Niamh
        assert!(events[0].contains("Niamh"));

        // NPC at different location should NOT have memory
        let away = npcs.get(&NpcId(3)).unwrap();
        assert!(away.memory.is_empty());
    }

    #[test]
    fn test_witness_memory_content_format() {
        let mut npcs = HashMap::new();
        let speaker = make_test_npc(1, "Padraig", 1);
        let witness = make_test_npc(2, "Niamh", 1);
        npcs.insert(NpcId(1), speaker);
        npcs.insert(NpcId(2), witness);

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        record_witness_memories(
            &mut npcs,
            NpcId(1),
            "Padraig",
            "What do you know about the landlord?",
            "That man is no friend of ours.",
            game_time,
            LocationId(1),
        );

        let witness = npcs.get(&NpcId(2)).unwrap();
        let mem = witness.memory.recent(1);
        assert!(mem[0].content.contains("landlord"));
        assert!(mem[0].content.contains("Padraig"));
        assert!(mem[0].content.contains("no friend"));
        // Participants should include player, speaker, and witness
        assert!(mem[0].participants.contains(&NpcId(0)));
        assert!(mem[0].participants.contains(&NpcId(1)));
        assert!(mem[0].participants.contains(&NpcId(2)));
    }

    #[test]
    fn test_spoke_with_npc_memory_records_partner_not_self() {
        const PADRAIG: NpcId = NpcId(1);
        const TOMMY: NpcId = NpcId(5);
        const LOCATION: LocationId = LocationId(2);

        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(PADRAIG, make_test_npc(1, "Padraig", 2));
        npcs.insert(TOMMY, make_test_npc(5, "Tommy", 2));

        let event = Tier2Event {
            location: LOCATION,
            summary: "Padraig and Tommy exchanged news".to_string(),
            participants: vec![PADRAIG, TOMMY],
            mood_changes: vec![],
            relationship_changes: vec![],
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 14, 0, 0).unwrap();
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

    // ── natural-language helpers for background sim prompts ──────────────

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
    fn test_top_relationships_sorted_by_absolute_strength() {
        // Iteration over `npc.relationships` (a HashMap) is non-deterministic.
        // top_relationships must surface the strongest |strength| values in a
        // stable order so the prompt is reproducible across runs.
        let mut npc = make_test_npc(1, "Padraig", 2);
        use crate::types::{Relationship, RelationshipKind};
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
        let mut npc = make_test_npc(1, "Padraig", 2);
        use crate::types::{Relationship, RelationshipKind};
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

    #[test]
    fn test_npc_snapshot_uses_prose_not_codes() {
        // npc_snapshot_from_npc should produce natural prose, not INT[...] codes.
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.intelligence = crate::types::Intelligence::new(4, 3, 5, 3, 3, 3);
        let names: HashMap<NpcId, String> = HashMap::new();

        let snap = npc_snapshot_from_npc(&npc, &names);

        assert!(!snap.intelligence_prose.is_empty());
        assert!(!snap.intelligence_prose.contains("INT["));
        // Specific phrasing from prompt_guidance for high verbal / high emotional.
        assert!(snap.intelligence_prose.contains("Well-spoken"));
        assert!(snap.intelligence_prose.contains("Reads people like a book"));
    }

    #[test]
    fn test_tier3_snapshot_uses_adjectives_not_codes() {
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.intelligence = crate::types::Intelligence::new(5, 3, 3, 3, 4, 3);
        let graph = WorldGraph::new();
        let names: HashMap<NpcId, String> = HashMap::new();

        let snap = tier3_snapshot_from_npc(&npc, &graph, &names);

        assert!(!snap.intelligence_adjectives.contains("INT["));
        assert_eq!(snap.intelligence_adjectives, "eloquent, wise");
    }

    #[test]
    fn test_tier2_prompt_omits_intelligence_when_average() {
        // An average NPC contributes no intelligence prose — the line should
        // still be valid without trailing whitespace.
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            npcs: vec![NpcSnapshot {
                id: NpcId(1),
                name: "Padraig".to_string(),
                occupation: "Publican".to_string(),
                personality: "Warm".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: String::new(),
                mood: "content".to_string(),
                relationship_summary: String::new(),
            }],
        };
        let lang = LanguageSettings::english_only();
        let prompt = build_tier2_prompt(&group, "Morning", "Clear", &lang);
        // Line ends with the mood and a period, no trailing spaces.
        assert!(prompt.contains("Currently content."));
        // No mention of relationship summary line.
        assert!(!prompt.contains("friendly with"));
    }
}
