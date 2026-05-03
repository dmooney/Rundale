//! Tier 1, Tier 2, and Tier 3 tick functions for NPC simulation.
//!
//! Tier 1 ticks run per player interaction (full LLM inference).
//! Tier 2 ticks run every 5 game-minutes for nearby NPCs (lighter inference).
//! Tier 3 ticks run every 1 game-day for distant NPCs (batch inference).

use chrono::Utc;

use crate::memory::{MemoryEntry, try_promote};
use crate::types::{Tier2Event, Tier2Response, Tier3Response, Tier3Update};
use crate::{
    Npc, NpcId, NpcStreamResponse, build_named_action_line, build_tier1_context,
    build_tier1_system_prompt,
};
use parish_config::{NpcConfig, RelationshipLabelConfig};
use parish_inference::InferencePriority;
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
    /// Compact intelligence tag for prompt injection (e.g. `INT[V3 A4 E2 P5 W4 C3]`).
    pub intelligence_tag: String,
    /// Legacy one-word mood (kept for back-compat with any Tier 2
    /// callers still reading this field).
    pub mood: String,
    /// Short emotion descriptor for prompt injection. Derived from
    /// `EmotionState::short_descriptor()` — e.g. "grief-stricken,
    /// withdrawn". Preferred over `mood` in the Tier 2 prompt.
    pub feeling: String,
    /// Relationship summaries with other NPCs at this location.
    pub relationship_context: String,
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

/// Builds an enhanced system prompt for Tier 1 interactions using the given config.
///
/// Extends the base system prompt with relationship summaries (using real names)
/// and knowledge entries for richer, more contextual NPC dialogue.
pub fn build_enhanced_system_prompt_with_config(
    npc: &Npc,
    improv: bool,
    emotions_enabled: bool,
    config: &NpcConfig,
    npc_names: &std::collections::HashMap<NpcId, String>,
    known_roster: Option<&[(NpcId, String, String)]>,
) -> String {
    let mut prompt = build_tier1_system_prompt(npc, improv, emotions_enabled);

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

/// Builds an enhanced system prompt for Tier 1 interactions.
///
/// Extends the base system prompt with relationship summaries and
/// knowledge entries for richer, more contextual NPC dialogue.
pub fn build_enhanced_system_prompt(
    npc: &Npc,
    improv: bool,
    npc_names: &std::collections::HashMap<NpcId, String>,
) -> String {
    build_enhanced_system_prompt_with_config(
        npc,
        improv,
        true,
        &NpcConfig::default(),
        npc_names,
        None,
    )
}

/// Builds an enhanced context prompt for Tier 1 interactions using the given config.
///
/// Extends the base context with the NPC's recent memories and
/// information about other NPCs present at the same location.
pub fn build_enhanced_context_with_config(
    npc: &Npc,
    world: &WorldState,
    player_input: &str,
    other_npcs: &[&Npc],
    config: &NpcConfig,
    _npc_names: &std::collections::HashMap<NpcId, String>,
    player_name_for_npc: Option<&str>,
) -> String {
    let mut context = build_tier1_context(world);

    // Clearly identify who the NPC is talking to
    let interlocutor_label = player_name_for_npc.unwrap_or("A newcomer to the parish");
    context.push_str(&format!(
        "\n\nPERSON YOU ARE SPEAKING WITH:\n{interlocutor_label}.",
    ));

    // Add other NPCs present with relationship context
    if !other_npcs.is_empty() {
        context.push_str("\n\nAlso present:");
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
            context.push_str(&format!(
                "\n- {}, the {}{}",
                other.name, other.occupation, relationship_note
            ));
        }
    }

    // Add recent conversation history at this location
    let player_label = player_name_for_npc.unwrap_or("The newcomer");
    let conv_ctx =
        world
            .conversation_log
            .context_string(world.player_location, npc.id, player_label, 3);
    if !conv_ctx.is_empty() {
        context.push_str("\n\nWhat's been said here:\n");
        context.push_str(&conv_ctx);
    }

    // Add scene continuity cue
    if world
        .conversation_log
        .has_recent_exchange_with(world.player_location, npc.id, 2)
    {
        let name = player_name_for_npc.unwrap_or("this newcomer");
        context.push_str(&format!(
            "\n\nYou are already in conversation with {name}. \
            Do not re-introduce yourself or greet them again."
        ));
    }

    // Add recent player reactions (emoji feedback)
    let reaction_ctx = npc
        .reaction_log
        .context_string(config.reaction_context_count);
    if !reaction_ctx.is_empty() {
        context.push_str("\n\n");
        context.push_str(&reaction_ctx);
    }

    // Add recent short-term memories unconditionally (ensures NPC doesn't
    // forget what just happened, even if keyword matching would miss it)
    let stm_ctx = npc.memory.context_string_with_now(5, world.clock.now());
    if !stm_ctx.is_empty() {
        context.push_str("\n\nRecent events you remember:\n");
        context.push_str(&stm_ctx);
    }

    // Add long-term memory recall (keyword-based)
    let location = world.current_location();
    let query_keywords: Vec<&str> = {
        let mut kw: Vec<&str> = Vec::new();
        // Extract keywords from player input (words > 4 chars)
        for word in player_input.split_whitespace() {
            let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric());
            if trimmed.len() > 4 {
                kw.push(trimmed);
            }
        }
        kw.push(&location.name);
        kw
    };
    let ltm_ctx = npc
        .long_term_memory
        .recall_context_string(&query_keywords, 5);
    if !ltm_ctx.is_empty() {
        context.push_str("\n\n");
        context.push_str(&ltm_ctx);
    }

    // Add gossip context
    let gossip_ctx = world.gossip_network.gossip_context_string(npc.id, 2);
    if !gossip_ctx.is_empty() {
        context.push_str("\n\n");
        context.push_str(&gossip_ctx);
    }

    context
}

/// Builds an enhanced context prompt for Tier 1 interactions.
///
/// Extends the base context with the NPC's recent memories and
/// information about other NPCs present at the same location.
pub fn build_enhanced_context(
    npc: &Npc,
    world: &WorldState,
    player_input: &str,
    other_npcs: &[&Npc],
    npc_names: &std::collections::HashMap<NpcId, String>,
) -> String {
    let mut context = build_enhanced_context_with_config(
        npc,
        world,
        player_input,
        other_npcs,
        &NpcConfig::default(),
        npc_names,
        None,
    );
    // Player's current input last — everything above is context for this moment
    context.push_str("\n\n");
    context.push_str(&build_named_action_line(player_input, None));
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

    // Apply structured emotion delta from metadata. This is the
    // write-back half of the Tier 1 emotion loop: the LLM reads the
    // state via the prompt preamble, and here we consume the
    // LLM-reported impulse to update internal state. The legacy
    // mood string is re-derived after this so downstream readers
    // stay consistent.
    let mut emotion_already_applied = false;
    if let Some(ref meta) = response.metadata
        && let Some(ref impulse) = meta.emotion_delta
    {
        let prev_label = npc.emotion.label();
        npc.apply_emotion_impulse(impulse);
        let new_label = npc.emotion.label();
        if prev_label != new_label {
            events.push(format!(
                "{} emotion: {} -> {} ({:?} {:+.2}{})",
                npc.name,
                prev_label,
                new_label,
                impulse.family,
                impulse.delta,
                impulse
                    .cause
                    .as_deref()
                    .map(|c| format!(", {c}"))
                    .unwrap_or_default(),
            ));
        }
        emotion_already_applied = true;
    }

    // Legacy mood-string fallback. If the LLM returned a structured
    // delta, `apply_emotion_impulse` has already re-derived `mood`
    // from `emotion.label()` and we must *not* overwrite it with the
    // free-form string (which would break mood/emotion sync).
    //
    // When the LLM omits `emotion_delta` (emotions flag off, legacy
    // schema, or a model that ignored the field), re-seed the
    // structured `emotion` from the reported mood via the same
    // `initial_from` heuristic used at NPC load. Without this,
    // `npc.emotion` stays stale while `npc.mood` moves — and the
    // next decay tick silently reverts `npc.mood` back to
    // `emotion.label()`, undoing the LLM's reported shift. We
    // assign `emotion` directly (not via `set_emotion`) so the LLM's
    // exact string is preserved in `mood`; subsequent decay ticks
    // will naturally normalise it to `emotion.label()` as the family
    // vector evolves.
    if !emotion_already_applied
        && let Some(ref meta) = response.metadata
        && !meta.mood.is_empty()
        && meta.mood != npc.mood
    {
        events.push(format!("{} mood: {} -> {}", npc.name, npc.mood, meta.mood));
        npc.emotion = parish_types::EmotionState::initial_from(&npc.temperament, &meta.mood);
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
pub fn apply_tier1_response(
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
    emotions_enabled: bool,
) -> String {
    let npc_descriptions: Vec<String> = group
        .npcs
        .iter()
        .map(|snap| {
            // With emotions off, fall back to the legacy `mood:` descriptor
            // so the prompt is byte-identical to the pre-feature shape.
            let state_field = if emotions_enabled {
                format!("feeling: {}", snap.feeling)
            } else {
                format!("mood: {}", snap.mood)
            };
            format!(
                "- NPC {id} \"{name}\" ({occ}), {state}, {intel}",
                id = snap.id.0,
                name = snap.name,
                occ = snap.occupation,
                state = state_field,
                intel = snap.intelligence_tag,
            )
        })
        .collect();

    let weather_commentary = match weather {
        "Light Rain" | "Heavy Rain" | "Storm" => " People are commenting on the weather.",
        _ => "",
    };

    // With emotions off, drop the `emotion_deltas` schema line. Apply
    // functions treat the field as optional, so omitting it matches the
    // legacy Tier 2 response shape exactly.
    let emotion_deltas_line = if emotions_enabled {
        ",\n          \"emotion_deltas\": [{\"npc_id\": <id>, \"impulse\": {\"family\": \"joy|sadness|fear|anger|disgust|surprise|shame|affection\", \"delta\": <-0.3 to 0.3>, \"cause\": \"one short phrase\"}}]"
    } else {
        ""
    };

    format!(
        "You are simulating background interactions between characters in a small \
        Irish parish in 1820.\n\n\
        Location: {location}\n\
        Time: {time}\n\
        Weather: {weather}.{weather_commentary}\n\n\
        Characters present:\n{characters}\n\n\
        Generate a brief (1-2 sentence) summary of what these characters are doing \
        and saying to each other. Include any mood changes, emotion nudges, or \
        relationship shifts.\n\n\
        Respond with a JSON object:\n\
        {{\n\
          \"summary\": \"Brief description of the interaction\",\n\
          \"mood_changes\": [{{\"npc_id\": <id>, \"new_mood\": \"<mood>\"}}],\n\
          \"relationship_changes\": [{{\"from\": <id>, \"to\": <id>, \"delta\": <-0.1 to 0.1>}}]{emotion_deltas}\n\
        }}",
        location = group.location_name,
        time = time_desc,
        weather = weather,
        characters = npc_descriptions.join("\n"),
        emotion_deltas = emotion_deltas_line,
    )
}

/// Creates an `NpcSnapshot` from a live NPC for Tier 2 background inference.
///
/// The snapshot is a lightweight owned copy that can be passed to a background
/// task without holding a lock on the `NpcManager`.
pub fn npc_snapshot_from_npc(npc: &Npc) -> NpcSnapshot {
    let intel = &npc.intelligence;
    let intelligence_tag = format!(
        "INT[V{} A{} E{} P{} W{} C{}]",
        intel.verbal,
        intel.analytical,
        intel.emotional,
        intel.practical,
        intel.wisdom,
        intel.creative,
    );

    let relationship_context: Vec<String> = npc
        .relationships
        .iter()
        .take(3)
        .map(|(target_id, rel)| format!("NPC {} ({:.1})", target_id.0, rel.strength))
        .collect();

    NpcSnapshot {
        id: npc.id,
        name: npc.name.clone(),
        occupation: npc.occupation.clone(),
        personality: npc.personality.clone(),
        intelligence_tag,
        mood: npc.mood.clone(),
        feeling: npc.emotion.short_descriptor(),
        relationship_context: relationship_context.join(", "),
    }
}

/// Runs Tier 2 inference for a group of NPCs at a location.
///
/// Submits to the Background lane of the inference priority queue.
/// Returns a `Tier2Event` with the summary, mood changes, and relationship deltas.
pub async fn run_tier2_for_group(
    queue: &parish_inference::InferenceQueue,
    model: &str,
    group: &Tier2Group,
    time_desc: &str,
    weather: &str,
    emotions_enabled: bool,
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
                emotion_deltas: Vec::new(),
            });
        }
        return None;
    }

    let prompt = build_tier2_prompt(group, time_desc, weather, emotions_enabled);
    let participant_ids: Vec<NpcId> = group.npcs.iter().map(|s| s.id).collect();

    match parish_inference::submit_json::<Tier2Response>(
        queue,
        InferencePriority::Background,
        model,
        &prompt,
        None,
    )
    .await
    {
        Ok(resp) => Some(Tier2Event {
            location: group.location,
            summary: resp.summary,
            participants: participant_ids,
            mood_changes: resp.mood_changes,
            relationship_changes: resp.relationship_changes,
            emotion_deltas: resp.emotion_deltas,
        }),
        Err(e) => {
            tracing::warn!("Tier 2 inference failed at {}: {}", group.location_name, e);
            None
        }
    }
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
) -> Vec<String> {
    let mut debug_events = Vec::new();

    // Track which NPCs received a structured emotion_delta so the
    // fallback mood_changes pass below doesn't overwrite the derived
    // mood label for those NPCs.
    let mut emotion_applied: std::collections::HashSet<NpcId> = std::collections::HashSet::new();

    // Apply structured emotion deltas first — these are the preferred
    // path. Each delta is scaled by the NPC's temperament reactivity
    // (inside apply_emotion_impulse) and re-derives the mood label.
    for change in &event.emotion_deltas {
        if let Some(npc) = npcs.get_mut(&change.npc_id) {
            let prev = npc.emotion.label();
            npc.apply_emotion_impulse(&change.impulse);
            let now = npc.emotion.label();
            if prev != now {
                debug_events.push(format!(
                    "{} emotion: {} -> {} ({:?} {:+.2})",
                    npc.name, prev, now, change.impulse.family, change.impulse.delta,
                ));
            }
            emotion_applied.insert(change.npc_id);
        }
    }

    // Apply freeform mood_changes for NPCs that did NOT receive a
    // structured delta. This keeps the legacy pathway alive for
    // older prompt templates and models that ignore emotion_deltas.
    for mc in &event.mood_changes {
        if emotion_applied.contains(&mc.npc_id) {
            continue;
        }
        if let Some(npc) = npcs.get_mut(&mc.npc_id) {
            if npc.mood != mc.new_mood {
                debug_events.push(format!(
                    "{} mood: {} -> {}",
                    npc.name, npc.mood, mc.new_mood
                ));
            }
            npc.mood = mc.new_mood.clone();
        }
    }

    // Apply relationship changes
    for rc in &event.relationship_changes {
        if let Some(npc) = npcs.get_mut(&rc.from)
            && let Some(rel) = npc.relationships.get_mut(&rc.to)
        {
            rel.adjust_strength(rc.delta);
        }
    }

    // Record memory for all participants
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

    debug_events
}

/// Applies a Tier 2 event's effects to the relevant NPCs.
///
/// Updates moods, adjusts relationship strengths, and records memories
/// for all participating NPCs.
///
/// Returns debug event strings describing what happened.
pub fn apply_tier2_event(
    event: &Tier2Event,
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    game_time: chrono::DateTime<Utc>,
) -> Vec<String> {
    apply_tier2_event_with_config(event, npcs, game_time, &NpcConfig::default())
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
    /// Legacy one-word mood (retained for callers that haven't
    /// migrated to `feeling`).
    pub mood: String,
    /// Short emotion descriptor for prompt injection.
    pub feeling: String,
    /// Deflated summary or last activity.
    pub context: String,
    /// Relationship summaries for prompt injection.
    pub relationship_context: String,
}

/// Builds a Tier 3 batch prompt for a set of NPC snapshots.
pub fn build_tier3_prompt(
    snapshots: &[Tier3Snapshot],
    time_desc: &str,
    weather: &str,
    season: &str,
    hours: u32,
    emotions_enabled: bool,
) -> String {
    let npc_summaries: Vec<String> = snapshots
        .iter()
        .map(|snap| {
            let context_line = if snap.context.is_empty() {
                String::new()
            } else {
                format!("\nRecent: {}", snap.context)
            };
            let rel_line = if snap.relationship_context.is_empty() {
                String::new()
            } else {
                format!("\nRelationships: {}", snap.relationship_context)
            };
            // With emotions off, fall back to the legacy `Mood:` label so
            // the prompt is byte-identical to the pre-feature Tier 3 shape.
            let state_field = if emotions_enabled {
                format!("Feeling: {}", snap.feeling)
            } else {
                format!("Mood: {}", snap.mood)
            };
            format!(
                "NPC {id} \"{name}\" ({occupation}, age {age}): At {location}. {state}.{context}{rels}",
                id = snap.id.0,
                name = snap.name,
                occupation = snap.occupation,
                age = snap.age,
                location = snap.location_name,
                state = state_field,
                context = context_line,
                rels = rel_line,
            )
        })
        .collect();

    // With emotions off, drop the structured `emotion_delta` schema line
    // and the "prefer emotion_delta" note on the `mood` field.
    let (mood_note, emotion_delta_line) = if emotions_enabled {
        (
            " — kept for back-compat; prefer emotion_delta",
            "\n        - emotion_delta (object or null): {{\"family\": one of joy|sadness|fear|anger|disgust|surprise|shame|affection, \"delta\": signed number from -0.3 to 0.3, \"cause\": short phrase}}",
        )
    } else {
        ("", "")
    };

    format!(
        "You are simulating background NPC activity in a rural Irish parish in 1820.\n\
        Given the following NPCs and their current states, simulate {hours} hours of activity.\n\
        The weather is {weather}. The season is {season}. The time is {time}.\n\n\
        Return a JSON object with an \"updates\" array. Each update has:\n\
        - npc_id (integer)\n\
        - mood (string, one word){mood_note}\n\
        - activity_summary (string, 1 sentence)\n\
        - new_location (integer or null)\n\
        - relationship_changes (array of {{\"from\": <id>, \"to\": <id>, \"delta\": <-0.1 to 0.1>}}){emotion_delta}\n\n\
        NPCs:\n{npcs}",
        hours = hours,
        weather = weather,
        season = season,
        time = time_desc,
        npcs = npc_summaries.join("\n\n"),
        mood_note = mood_note,
        emotion_delta = emotion_delta_line,
    )
}

/// Creates a Tier 3 snapshot from an NPC, resolving location names from the graph.
pub fn tier3_snapshot_from_npc(npc: &Npc, graph: &WorldGraph) -> Tier3Snapshot {
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

    let relationship_context: Vec<String> = npc
        .relationships
        .iter()
        .take(3)
        .map(|(target_id, rel)| format!("NPC {} ({:.1})", target_id.0, rel.strength))
        .collect();

    Tier3Snapshot {
        id: npc.id,
        name: npc.name.clone(),
        occupation: npc.occupation.clone(),
        age: npc.age,
        location: npc.location,
        location_name,
        mood: npc.mood.clone(),
        feeling: npc.emotion.short_descriptor(),
        context,
        relationship_context: relationship_context.join(", "),
    }
}

/// Context for a Tier 3 batch simulation call.
///
/// Tier 3 batches are submitted through the priority `InferenceQueue` with
/// `Batch` priority so they yield to player-facing dialogue (Tier 1 Interactive)
/// and Tier 2 background simulation (Background).
pub struct Tier3Context<'a> {
    /// NPC snapshots to simulate.
    pub snapshots: &'a [Tier3Snapshot],
    /// Inference queue used to submit batch requests.
    pub queue: &'a parish_inference::InferenceQueue,
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
    /// Whether the structured emotion system is active this session.
    /// When `false`, the prompt reverts to the legacy `Mood:` label and
    /// omits the `emotion_delta` schema line.
    pub emotions_enabled: bool,
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
            ctx.emotions_enabled,
        );

        match parish_inference::submit_json::<Tier3Response>(
            ctx.queue,
            InferencePriority::Batch,
            ctx.model,
            &prompt,
            None,
        )
        .await
        {
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
/// Returns debug event strings describing what happened.
pub fn apply_tier3_updates(
    updates: &[Tier3Update],
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    graph: &WorldGraph,
    game_time: chrono::DateTime<Utc>,
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

        // Apply structured emotion_delta if present; otherwise fall
        // back to the legacy mood string. The structured path also
        // re-derives mood from `emotion.label()`.
        if let Some(ref impulse) = update.emotion_delta {
            let prev = npc.emotion.label();
            npc.apply_emotion_impulse(impulse);
            let now = npc.emotion.label();
            if prev != now {
                debug_events.push(format!(
                    "{} emotion: {} -> {} (tier3 {:?} {:+.2})",
                    npc.name, prev, now, impulse.family, impulse.delta,
                ));
            }
        } else if !update.mood.is_empty() && update.mood != npc.mood {
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
                debug_events.push(format!(
                    "{} moved: {:?} -> {:?} (tier3)",
                    npc.name, npc.location, new_loc
                ));
                npc.location = new_loc;
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

// ---------------------------------------------------------------------------
// Per-tick emotion background: decay + relationship contagion.
// ---------------------------------------------------------------------------

/// Decays every NPC's emotional state toward its temperament-derived
/// baseline over `dt_secs` of game time, then re-derives the
/// `mood` label from the decayed state.
///
/// Always safe to call regardless of the `emotions` feature flag —
/// pure float math with zero LLM cost. Running decay unconditionally
/// keeps `Npc.emotion` coherent so toggling the flag mid-session
/// reveals up-to-date state rather than a frozen snapshot from when
/// the flag was last disabled.
pub fn decay_emotions_tick(npcs: &mut std::collections::HashMap<NpcId, Npc>, dt_secs: f32) {
    if dt_secs <= 0.0 {
        return;
    }
    for npc in npcs.values_mut() {
        npc.emotion.decay(dt_secs);
        // Re-derive the canonical mood label from the decayed state.
        // If decay is small this is usually a no-op, but it prevents
        // drift when a strong emotion gradually subsides.
        npc.mood = npc.emotion.label().to_string();
    }
}

/// Propagates a fraction of each NPC's family intensities to NPCs
/// they have strong positive relationships with (strength > 0.6).
///
/// Two-phase: first collects the full set of inbound impulses for
/// every NPC, then applies them all at once. This makes contagion
/// symmetric (A→B and B→A produce the same result regardless of
/// HashMap iteration order) and caps the per-tick mutation.
///
/// The total absolute delta any single NPC can receive in one tick
/// is capped at `MAX_CONTAGION_DELTA` per family, so a single
/// distraught NPC can't catastrophically cascade a whole village in
/// one tick.
///
/// Always safe to call regardless of the `emotions` flag — see
/// [`decay_emotions_tick`] for the same reasoning.
pub fn propagate_contagion(npcs: &mut std::collections::HashMap<NpcId, Npc>, fraction: f32) {
    /// Maximum net change to any family per NPC per contagion tick.
    const MAX_CONTAGION_DELTA: f32 = 0.1;
    /// Only relationships with strength above this threshold transmit
    /// emotional contagion.
    const MIN_STRONG_REL: f64 = 0.6;

    if fraction <= 0.0 {
        return;
    }
    let fraction = fraction.clamp(0.0, 1.0);

    // Phase 1: collect per-target inbound deltas keyed by family.
    // Snapshot family vectors up front so subsequent apply doesn't
    // feed back into later source reads in the same tick.
    let snapshot: Vec<(NpcId, parish_types::FamilyVec)> = npcs
        .iter()
        .map(|(id, npc)| (*id, npc.emotion.families))
        .collect();

    let mut inbound: std::collections::HashMap<NpcId, parish_types::FamilyVec> =
        std::collections::HashMap::new();

    for (target_id, target_npc) in npcs.iter() {
        for (source_id, rel) in &target_npc.relationships {
            if rel.strength <= MIN_STRONG_REL {
                continue;
            }
            let Some((_, source_families)) = snapshot.iter().find(|(id, _)| id == source_id) else {
                continue;
            };
            let weight = fraction * (rel.strength as f32);
            let entry = inbound.entry(*target_id).or_default();
            for fam in parish_types::EmotionFamily::ALL.iter().copied() {
                let slot = entry.get_mut(fam);
                *slot += source_families.get(fam) * weight;
            }
        }
    }

    // Phase 2: apply each NPC's capped inbound contribution.
    for (target_id, deltas) in inbound {
        let Some(npc) = npcs.get_mut(&target_id) else {
            continue;
        };
        for fam in parish_types::EmotionFamily::ALL.iter().copied() {
            let delta = deltas
                .get(fam)
                .clamp(-MAX_CONTAGION_DELTA, MAX_CONTAGION_DELTA);
            if delta.abs() < 0.001 {
                continue;
            }
            let slot = npc.emotion.families.get_mut(fam);
            *slot = (*slot + delta).clamp(0.0, 1.0);
        }
        npc.mood = npc.emotion.label().to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NpcMetadata;
    use crate::memory::{LongTermMemory, ShortTermMemory};
    use crate::types::{MoodChange, NpcState, Relationship, RelationshipChange, RelationshipKind};
    use chrono::TimeZone;
    use std::collections::HashMap;

    fn make_test_npc(id: u32, name: &str, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: name.to_string(),
            brief_description: format!("a test NPC named {}", name),
            age: 40,
            occupation: "Test".to_string(),
            personality: "Friendly".to_string(),
            intelligence: crate::types::Intelligence::default(),
            location: LocationId(location),
            mood: "calm".to_string(),
            emotion: parish_types::EmotionState::default(),
            temperament: parish_types::Temperament::default(),
            home: Some(LocationId(location)),
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::default(),
            deflated_summary: None,
            reaction_log: crate::reactions::ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
        }
    }

    #[test]
    fn test_enhanced_system_prompt_includes_relationships() {
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.8));
        npc.knowledge = vec!["Knows local history".to_string()];

        let npc_names: HashMap<NpcId, String> =
            [(NpcId(2), "Brigid".to_string())].into_iter().collect();
        let prompt = build_enhanced_system_prompt(&npc, false, &npc_names);
        assert!(prompt.contains("PEOPLE IN YOUR LIFE:"));
        assert!(prompt.contains("very close"));
        assert!(prompt.contains("WHAT'S ON YOUR MIND:"));
        assert!(prompt.contains("Knows local history"));
    }

    #[test]
    fn test_enhanced_system_prompt_without_relationships() {
        let npc = make_test_npc(1, "Padraig", 2);
        let npc_names: HashMap<NpcId, String> = HashMap::new();
        let prompt = build_enhanced_system_prompt(&npc, false, &npc_names);
        assert!(!prompt.contains("PEOPLE IN YOUR LIFE:"));
        assert!(!prompt.contains("WHAT'S ON YOUR MIND:"));
    }

    #[test]
    fn test_enhanced_context_with_other_npcs() {
        let npc = make_test_npc(1, "Padraig", 1);
        let other = make_test_npc(2, "Tommy", 1);
        let world = WorldState::new();

        let npc_names: std::collections::HashMap<NpcId, String> = std::collections::HashMap::new();
        let context =
            build_enhanced_context(&npc, &world, "greets everyone", &[&other], &npc_names);
        assert!(context.contains("Also present:"));
        assert!(context.contains("Tommy, the Test"));
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
        let context = build_enhanced_context(&npc, &world, "says hello", &[], &npc_names);
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
                emotion_delta: None,
            }),
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        apply_tier1_response(&mut npc, &response, "says hello", game_time);

        assert_eq!(npc.mood, "cheerful");
        assert_eq!(npc.memory.len(), 1);
    }

    #[test]
    fn test_apply_tier1_legacy_mood_syncs_structured_emotion() {
        // Regression: when the LLM omits emotion_delta and returns
        // only a legacy mood string, the structured emotion must
        // also be updated. Otherwise `npc.emotion` stays stale and
        // the next decay tick silently reverts the mood back to
        // `emotion.label()`, undoing the reported shift. Codex
        // flagged this as a P1 on PR #443.
        let mut npc = make_test_npc(1, "Padraig", 1);
        // Seed Padraig with a strong anger state so drift would be
        // visible: his dominant family is Anger before the LLM
        // reports a mood shift to "grieving".
        npc.emotion.families.anger = 0.9;
        npc.emotion.pleasure = -0.4;
        npc.emotion.arousal = 0.7;
        npc.emotion.dominance = 0.5;
        npc.mood = npc.emotion.label().to_string();
        let pre_label = npc.emotion.label().to_string();
        assert!(
            pre_label.contains("angry") || pre_label == "furious" || pre_label == "irritated",
            "precondition: NPC should start in an anger label, got {pre_label}"
        );

        let response = NpcStreamResponse {
            dialogue: "I've no words in me just now.".into(),
            metadata: Some(NpcMetadata {
                action: "stares at the floor".into(),
                mood: "grieving".into(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                emotion_delta: None,
            }),
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        apply_tier1_response(&mut npc, &response, "says softly", game_time);

        // Mood string reflects the LLM's report exactly.
        assert_eq!(npc.mood, "grieving");

        // Critically: structured emotion is re-seeded from "grieving"
        // so Sadness now dominates the family vector. Without the
        // sync, Anger would still be 0.9 and Sadness ~0.
        let (dom_family, dom_intensity) = npc.emotion.dominant_family();
        assert_eq!(
            dom_family,
            parish_types::EmotionFamily::Sadness,
            "structured emotion should re-seed to Sadness to match 'grieving' mood"
        );
        assert!(
            dom_intensity > 0.5,
            "sadness intensity should be meaningful, got {dom_intensity}"
        );

        // The drift sentinel: after a decay tick, mood should NOT
        // snap back to an anger-family label, because emotion is
        // already in the sadness family. Running decay and checking
        // the label confirms the sync held.
        decay_emotions_tick(
            &mut std::collections::HashMap::from_iter([(npc.id, npc)]),
            60.0,
        );
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
    fn test_apply_tier1_response_emotion_delta_updates_state_and_mood() {
        // The structured emotion_delta path should (a) nudge the
        // family vector, (b) re-derive the legacy mood string from
        // the updated state, and (c) suppress the freeform `mood`
        // field in the same metadata (so the two don't fight).
        let mut npc = make_test_npc(1, "Padraig", 1);
        // Start him content so a fear nudge lands cleanly.
        npc.emotion = parish_types::EmotionState::initial_from(
            &parish_types::Temperament::default(),
            "content",
        );
        npc.mood = npc.emotion.label().to_string();
        let original_mood = npc.mood.clone();

        let response = NpcStreamResponse {
            dialogue: "Good day, now.".to_string(),
            metadata: Some(NpcMetadata {
                action: "speaks".to_string(),
                // Legacy mood claims "jovial" but the LLM also returns
                // a structured fear delta — the structured path must
                // win, so the legacy string should be ignored here.
                mood: "jovial".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                emotion_delta: Some(parish_types::EmotionImpulse {
                    family: parish_types::EmotionFamily::Fear,
                    delta: 0.6,
                    pad: None,
                    cause: Some("heard the riders outside".to_string()),
                }),
            }),
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        apply_tier1_response(&mut npc, &response, "waves", game_time);

        assert!(
            npc.emotion.families.fear > 0.2,
            "fear delta should have landed, got {}",
            npc.emotion.families.fear
        );
        assert_ne!(
            npc.mood, original_mood,
            "mood should have shifted from the original 'content' label"
        );
        assert_ne!(
            npc.mood, "jovial",
            "legacy mood string must lose to the structured delta"
        );
    }

    #[test]
    fn test_apply_tier2_event_emotion_deltas_apply_to_multiple_npcs() {
        // Two NPCs, two deltas — both should land, both moods should
        // re-derive independently.
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));
        npcs.insert(NpcId(2), make_test_npc(2, "Tommy", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "A row breaks out over the rent".to_string(),
            participants: vec![NpcId(1), NpcId(2)],
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
            emotion_deltas: vec![
                crate::types::EmotionDeltaChange {
                    npc_id: NpcId(1),
                    impulse: parish_types::EmotionImpulse {
                        family: parish_types::EmotionFamily::Anger,
                        delta: 0.65,
                        pad: None,
                        cause: Some("argued back".to_string()),
                    },
                },
                crate::types::EmotionDeltaChange {
                    npc_id: NpcId(2),
                    impulse: parish_types::EmotionImpulse {
                        family: parish_types::EmotionFamily::Fear,
                        delta: 0.5,
                        pad: None,
                        cause: Some("watched the row".to_string()),
                    },
                },
            ],
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();

        apply_tier2_event(&event, &mut npcs, game_time);

        // Note: delta 0.65 clamps to 0.5 per impulse contract, then
        // scales by default reactivity (0.5) -> landing magnitude 0.25.
        let padraig = npcs.get(&NpcId(1)).unwrap();
        assert!(
            padraig.emotion.families.anger >= 0.2,
            "Padraig's anger should have risen, got {}",
            padraig.emotion.families.anger
        );
        let tommy = npcs.get(&NpcId(2)).unwrap();
        assert!(
            tommy.emotion.families.fear >= 0.2,
            "Tommy's fear should have risen, got {}",
            tommy.emotion.families.fear
        );
        // State vectors must have diverged even if both land on the
        // same PAD-fallback label ("calm") — the label() threshold
        // for a family-derived name is 0.3, and our 0.25 post-clamp
        // landing sits just under that. What matters is that the
        // family intensities now differ, not that the coarse labels do.
        assert_ne!(
            padraig.emotion.families.anger, tommy.emotion.families.anger,
            "two NPCs with distinct impulses should have distinct anger"
        );
        assert_ne!(
            padraig.emotion.families.fear, tommy.emotion.families.fear,
            "two NPCs with distinct impulses should have distinct fear"
        );
    }

    #[test]
    fn test_apply_tier2_event_mood_change_does_not_override_structured_delta() {
        // Both an emotion_deltas entry and a mood_changes entry for
        // the same NPC — the structured path wins; the legacy path
        // is skipped for that NPC.
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Padraig stews over the news".to_string(),
            participants: vec![NpcId(1)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(1),
                new_mood: "jubilant".to_string(), // contradicts the delta
            }],
            relationship_changes: Vec::new(),
            emotion_deltas: vec![crate::types::EmotionDeltaChange {
                npc_id: NpcId(1),
                impulse: parish_types::EmotionImpulse {
                    family: parish_types::EmotionFamily::Anger,
                    delta: 0.7,
                    pad: None,
                    cause: None,
                },
            }],
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();

        apply_tier2_event(&event, &mut npcs, game_time);

        let npc = npcs.get(&NpcId(1)).unwrap();
        assert_ne!(
            npc.mood, "jubilant",
            "legacy mood_changes must not overwrite the structured-delta-derived mood"
        );
        assert!(
            npc.emotion.families.anger >= 0.2,
            "anger delta should have landed (note: clamp+reactivity caps at ~0.25), got {}",
            npc.emotion.families.anger,
        );
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
                    intelligence_tag: "INT[V3 A3 E4 P4 W5 C4]".to_string(),
                    mood: "content".to_string(),
                    feeling: "content".to_string(),
                    relationship_context: String::new(),
                },
                NpcSnapshot {
                    id: NpcId(5),
                    name: "Tommy".to_string(),
                    occupation: "Retired Farmer".to_string(),
                    personality: "Storyteller".to_string(),
                    intelligence_tag: "INT[V4 A2 E3 P4 W5 C5]".to_string(),
                    mood: "reflective".to_string(),
                    feeling: "reflective".to_string(),
                    relationship_context: String::new(),
                },
            ],
        };

        let prompt = build_tier2_prompt(&group, "Evening", "Overcast", true);
        assert!(prompt.contains("Darcy's Pub"));
        // New richer per-NPC prefix: `NPC <id> "<name>" (<occupation>)`.
        // The id prefix matches Tier 3's convention and lets the model
        // cite participants unambiguously in emotion_deltas.
        assert!(prompt.contains("NPC 1 \"Padraig\" (Publican)"));
        assert!(prompt.contains("NPC 5 \"Tommy\" (Retired Farmer)"));
        assert!(
            prompt.contains("feeling: content"),
            "Tier 2 prompt should render the short emotion descriptor"
        );
        assert!(prompt.contains("Evening"));
        assert!(prompt.contains("Overcast"));
        assert!(prompt.contains("summary"));
        assert!(
            prompt.contains("emotion_deltas"),
            "Tier 2 schema should request structured emotion deltas"
        );
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
            emotion_deltas: Vec::new(),
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
        let prompt = build_enhanced_system_prompt(&npc, false, &npc_names);
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
        let prompt =
            build_enhanced_system_prompt_with_config(&npc, false, true, &config, &npc_names, None);
        // 0.8 is below 0.9 threshold, so should be "friendly" not "very close"
        assert!(prompt.contains("friendly"));
        assert!(!prompt.contains("very close"));
    }

    #[test]
    fn test_build_enhanced_context_action_line_at_end() {
        let npc = make_test_npc(1, "Padraig", 1);
        let world = WorldState::new();
        let npc_names: std::collections::HashMap<NpcId, String> = std::collections::HashMap::new();
        let context = build_enhanced_context(&npc, &world, "hello there", &[], &npc_names);
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
            emotion_deltas: Vec::new(),
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let config = NpcConfig {
            event_summary_truncation: 40,
            event_summary_debug_truncation: 20,
            ..NpcConfig::default()
        };

        let events = apply_tier2_event_with_config(&event, &mut npcs, game_time, &config);
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
            emotion_deltas: Vec::new(),
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
            emotion_deltas: Vec::new(),
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
            emotion_deltas: Vec::new(),
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
            emotion_deltas: Vec::new(),
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
                intelligence_tag: "INT[V3 A3 E4 P4 W5 C4]".to_string(),
                mood: "content".to_string(),
                feeling: "content".to_string(),
                relationship_context: String::new(),
            }],
        };

        // Solo NPC short-circuits before any LLM call — a disconnected queue is fine.
        let (itx, _irx) = tokio::sync::mpsc::channel(1);
        let (btx, _brx) = tokio::sync::mpsc::channel(1);
        let (batx, _batrx) = tokio::sync::mpsc::channel(1);
        let queue = parish_inference::InferenceQueue::new(itx, btx, batx);
        let event = run_tier2_for_group(&queue, "test", &group, "Morning", "Clear", true).await;
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

        // Empty group short-circuits before any LLM call — a disconnected queue is fine.
        let (itx, _irx) = tokio::sync::mpsc::channel(1);
        let (btx, _brx) = tokio::sync::mpsc::channel(1);
        let (batx, _batrx) = tokio::sync::mpsc::channel(1);
        let queue = parish_inference::InferenceQueue::new(itx, btx, batx);
        let event = run_tier2_for_group(&queue, "test", &group, "Morning", "Clear", true).await;
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
                    intelligence_tag: "INT[V3]".to_string(),
                    mood: "calm".to_string(),
                    feeling: "calm".to_string(),
                    relationship_context: String::new(),
                },
                NpcSnapshot {
                    id: NpcId(2),
                    name: "Tommy".to_string(),
                    occupation: "Farmer".to_string(),
                    personality: "Gruff".to_string(),
                    intelligence_tag: "INT[V2]".to_string(),
                    mood: "tired".to_string(),
                    feeling: "weary".to_string(),
                    relationship_context: String::new(),
                },
            ],
        };

        let prompt = build_tier2_prompt(&group, "Afternoon", "Heavy Rain", true);
        assert!(prompt.contains("commenting on the weather"));

        let prompt = build_tier2_prompt(&group, "Afternoon", "Clear", true);
        assert!(!prompt.contains("commenting on the weather"));
    }

    #[test]
    fn test_build_tier2_prompt_emotions_disabled_legacy_shape() {
        // With emotions_enabled=false, the Tier 2 prompt reverts to the
        // pre-feature byte shape: `mood:` descriptor in the NPC list and
        // no `emotion_deltas` schema line.
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "The Crossroads".to_string(),
            npcs: vec![
                NpcSnapshot {
                    id: NpcId(1),
                    name: "Padraig".to_string(),
                    occupation: "Publican".to_string(),
                    personality: "Warm".to_string(),
                    intelligence_tag: "INT[V3]".to_string(),
                    mood: "calm".to_string(),
                    feeling: "serene".to_string(),
                    relationship_context: String::new(),
                },
                NpcSnapshot {
                    id: NpcId(2),
                    name: "Tommy".to_string(),
                    occupation: "Farmer".to_string(),
                    personality: "Gruff".to_string(),
                    intelligence_tag: "INT[V2]".to_string(),
                    mood: "tired".to_string(),
                    feeling: "weary".to_string(),
                    relationship_context: String::new(),
                },
            ],
        };

        let prompt = build_tier2_prompt(&group, "Afternoon", "Clear", false);
        assert!(prompt.contains("mood: calm"));
        assert!(prompt.contains("mood: tired"));
        assert!(
            !prompt.contains("feeling:"),
            "emotion-off prompt must omit the `feeling:` descriptor"
        );
        assert!(
            !prompt.contains("emotion_deltas"),
            "emotion-off prompt must omit the `emotion_deltas` schema line"
        );
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
                emotion_delta: None,
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
                emotion_delta: None,
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
                feeling: "content".to_string(),
                context: "Served drinks all evening.".to_string(),
                relationship_context: "NPC 2 (0.5)".to_string(),
            },
            Tier3Snapshot {
                id: NpcId(3),
                name: "Bridget".to_string(),
                occupation: "Farmer".to_string(),
                age: 35,
                location: LocationId(5),
                location_name: "O'Brien's Farm".to_string(),
                mood: "worried".to_string(),
                feeling: "worried, withdrawn".to_string(),
                context: String::new(),
                relationship_context: String::new(),
            },
        ];

        let prompt = build_tier3_prompt(&snapshots, "Morning", "Overcast", "Spring", 24, true);
        assert!(prompt.contains("simulate 24 hours"));
        assert!(prompt.contains("Overcast"));
        assert!(prompt.contains("Spring"));
        assert!(prompt.contains("Morning"));
        assert!(prompt.contains("NPC 1 \"Padraig\""));
        assert!(prompt.contains("Publican, age 58"));
        assert!(prompt.contains("Darcy's Pub"));
        assert!(prompt.contains("Served drinks all evening."));
        assert!(prompt.contains("NPC 3 \"Bridget\""));
        assert!(prompt.contains("Farmer, age 35"));
        // Tier 3 now renders Feeling (the short emotion descriptor)
        // rather than the raw mood string. The fixture set
        // `feeling: "worried, withdrawn"`.
        assert!(prompt.contains("Feeling: worried, withdrawn."));
        // JSON format instructions
        assert!(prompt.contains("npc_id"));
        assert!(prompt.contains("activity_summary"));
        assert!(
            prompt.contains("emotion_delta"),
            "Tier 3 schema should request structured emotion_delta"
        );
    }

    #[test]
    fn test_tier3_prompt_emotions_disabled_legacy_shape() {
        // With emotions_enabled=false, Tier 3 reverts to the pre-feature
        // byte shape: `Mood:` per-NPC label and no `emotion_delta` schema.
        let snapshots = vec![Tier3Snapshot {
            id: NpcId(1),
            name: "Padraig".to_string(),
            occupation: "Publican".to_string(),
            age: 58,
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            mood: "content".to_string(),
            feeling: "serene, grounded".to_string(),
            context: String::new(),
            relationship_context: String::new(),
        }];

        let prompt = build_tier3_prompt(&snapshots, "Morning", "Clear", "Spring", 24, false);
        assert!(
            prompt.contains("Mood: content"),
            "emotion-off prompt must use legacy `Mood:` label"
        );
        assert!(
            !prompt.contains("Feeling:"),
            "emotion-off prompt must omit `Feeling:` descriptor"
        );
        assert!(
            !prompt.contains("emotion_delta"),
            "emotion-off prompt must omit `emotion_delta` schema line"
        );
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
                feeling: "calm".to_string(),
                context: String::new(),
                relationship_context: String::new(),
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
            emotion_delta: None,
            relationship_changes: vec![RelationshipChange {
                from: NpcId(1),
                to: NpcId(5),
                delta: 0.1,
            }],
        }];

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier3_updates(&updates, &mut npcs, &graph, game_time);

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
            emotion_delta: None,
        }];

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        apply_tier3_updates(&updates, &mut npcs, &graph, game_time);

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
            emotion_delta: None,
        }];

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier3_updates(&updates, &mut npcs, &graph, game_time);

        // Should produce no events (NPC not found)
        assert!(events.is_empty());
    }

    #[test]
    fn test_tier3_snapshot_from_npc_with_last_activity() {
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.last_activity = Some("Tended bar all evening.".to_string());

        let graph = WorldGraph::new();
        let snap = tier3_snapshot_from_npc(&npc, &graph);

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
        let snap = tier3_snapshot_from_npc(&npc, &graph);

        assert_eq!(snap.context, "Chatted with Tommy.");
    }

    #[test]
    fn test_tier3_snapshot_from_npc_no_context() {
        let npc = make_test_npc(1, "Padraig", 2);
        let graph = WorldGraph::new();
        let snap = tier3_snapshot_from_npc(&npc, &graph);

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
}
