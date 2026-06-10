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
/// Regression (fixed: #21): Cormac and Nora Duffy, who work at The Mill
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
/// other name that may appear in the recent-events buffer (fixed: #35 — a
/// shopkeeper at a new location called the player "Nora" because the prior
/// location's NPC named Nora was still in the dialogue history).
///
/// When `familiar` is true (the player has had enough prior exchanges with
/// this NPC that "stranger" is socially inappropriate), the unknown-name
/// vocabulary is narrowed to "the newcomer" / "friend" — dropping "stranger"
/// (#1388).
fn interlocutor_block(player_name_for_npc: Option<&str>, familiar: bool) -> String {
    match player_name_for_npc {
        Some(name) => format!(
            "\n\nPERSON YOU ARE SPEAKING WITH:\n{name}.\n\
             Address them by the name '{name}' only. Do not call them any \
             other name, even if a different name appears in the recent \
             conversation history."
        ),
        None if familiar => String::from(
            "\n\nPERSON YOU ARE SPEAKING WITH:\nA person you have already spoken with.\n\
             You do not yet know their name, but you have met before — do NOT \
             address them as 'stranger'. Refer to them as 'the newcomer', \
             'friend', 'mo chara', or by a brief description. Do not invent a \
             name and do not borrow a name from the recent conversation history.",
        ),
        None => String::from(
            "\n\nPERSON YOU ARE SPEAKING WITH:\nA newcomer to the parish.\n\
             You do not yet know their name. Refer to them as 'the newcomer', \
             'stranger', 'friend', or similar — do not invent a name and do \
             not borrow a name from the recent conversation history.",
        ),
    }
}

/// Anti-phrase-recycling block — injects the NPC's own recent dialogue lines
/// as a "do not repeat" list so the model cannot recycle verbatim phrases from
/// turns that fall outside the short conversation-history window (#1387).
///
/// Only renders when there are prior NPC lines at this location.
fn prior_phrases_block(world: &WorldState, npc: &Npc) -> Option<String> {
    let lines = world
        .conversation_log
        .npc_prior_lines(world.player_location, npc.id, 6);
    if lines.is_empty() {
        return None;
    }
    let mut block = String::from(
        "\n\nDO NOT REPEAT THESE PHRASES — you have already used them in \
         this conversation. Using them again would be verbatim repetition:\n",
    );
    for line in &lines {
        // Truncate very long prior lines to avoid bloating the prompt.
        let excerpt: &str = if line.len() > 120 {
            // Safe UTF-8 truncation: find the last char boundary at or before 120.
            let mut end = 120;
            while end > 0 && !line.is_char_boundary(end) {
                end -= 1;
            }
            &line[..end]
        } else {
            line
        };
        block.push_str(&format!("- \"{excerpt}\"\n"));
    }
    Some(block)
}

/// Describes other NPCs present at the location with relationship context.
///
/// The anchor sentence forbids the model from speaking to or about any
/// character not in this list as if they were present (fixed: #11 — Brendan
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
///
/// Extended for #1388: when `add_no_reask` is true (the `dialogue-quality-
/// continuity` flag is on), appends an explicit directive instructing the NPC
/// not to re-ask questions whose answers already appear in the conversation
/// history block shown above.
fn continuity_block(
    world: &WorldState,
    npc: &Npc,
    player_name_for_npc: Option<&str>,
    add_no_reask: bool,
) -> Option<String> {
    if !world
        .conversation_log
        .has_recent_exchange_with(world.player_location, npc.id, 2)
    {
        return None;
    }
    let name = player_name_for_npc.unwrap_or("this newcomer");
    let no_reask = if add_no_reask {
        " Treat any facts or answers already established in the conversation \
         history above as settled — do not re-ask questions whose answers have \
         already been given."
    } else {
        ""
    };
    Some(format!(
        "\n\nYou are already in conversation with {name}. \
         Do not re-introduce yourself or greet them again.{no_reask}"
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
///
/// Includes an explicit tone directive so small models act on the mood rather
/// than treating it as an inert label (fixes #1373: sharp/alert/busy NPCs were
/// speaking cheerful-warm because the bare label lost to the cultural-warmth
/// directive in the system prompt).
fn mood_block(npc: &Npc) -> String {
    let mood = npc.mood.trim().trim_end_matches('.');
    if mood.is_empty() {
        return String::new();
    }
    let tone_directive = mood_tone_directive(mood);
    format!("\n\nYour current mood: {mood}. {tone_directive}")
}

/// Returns a tone directive sentence for the given mood word.
///
/// Maps mood categories to an explicit behavioral instruction so small models
/// honour the mood rather than defaulting to the cheerful-warm register that
/// the cultural guideline ("Show warmth") would otherwise produce.
fn mood_tone_directive(mood: &str) -> &'static str {
    let m = mood.to_lowercase();

    // Negative / tense
    if m.contains("sharp") || m.contains("curt") || m.contains("caustic") || m.contains("acerbic") {
        return "Speak curtly and directly — no pleasantries, no warm welcome.";
    }
    if m.contains("irritat")
        || m.contains("frustrat")
        || m.contains("annoyed")
        || m.contains("grumpy")
    {
        return "Let your irritation show — short replies, clipped tone.";
    }
    if m.contains("angry") || m.contains("furious") || m.contains("irate") {
        return "Your anger colours every word — brusque, sharp-edged.";
    }
    if m.contains("bitter") || m.contains("resentful") || m.contains("sour") {
        return "Your bitterness shows — guarded and cynical in tone.";
    }
    if m.contains("suspicious") || m.contains("wary") || m.contains("distrustful") {
        return "Keep your guard up — watchful, measured, trust nothing freely.";
    }
    if m.contains("anxious") || m.contains("nervous") || m.contains("worried") {
        return "Your unease comes through — halting, glancing, not quite settled.";
    }
    if m.contains("sad") || m.contains("grief") || m.contains("mournful") || m.contains("sorrowful")
    {
        return "Grief weighs on your words — slow, subdued, heavy.";
    }
    if m.contains("melanchol") || m.contains("wistful") {
        return "A quiet sadness colours your tone — reflective, not bright.";
    }

    // Busy / distracted
    if m.contains("busy") || m.contains("distracted") || m.contains("preoccupied") {
        return "You are pressed for time — brief, to the point, no lingering.";
    }
    if m.contains("restless") || m.contains("agitated") {
        return "Your restlessness shows — short attention, quick to move on.";
    }
    if m.contains("tired") || m.contains("weary") || m.contains("exhausted") {
        return "Fatigue dulls your words — slow, spare, no energy for warmth.";
    }

    // Alert / watchful
    if m.contains("alert") || m.contains("watchful") || m.contains("vigilant") {
        return "You are on edge — attentive, scanning, your words chosen carefully.";
    }

    // Neutral cognitive
    if m.contains("contemplat")
        || m.contains("thoughtful")
        || m.contains("reflective")
        || m.contains("ponder")
    {
        return "You are in your own thoughts — measured, unhurried, somewhat inward.";
    }
    if m.contains("calm") || m.contains("serene") || m.contains("tranquil") {
        return "Speak evenly and unhurried — a steady, settled manner.";
    }
    if m.contains("stoic") || m.contains("guarded") || m.contains("reserved") {
        return "Say only what needs saying — no excess, no effusion.";
    }
    if m.contains("determined") || m.contains("resolute") {
        return "Your resolve is clear — purposeful, direct, focused.";
    }
    if m.contains("calculating") {
        return "Weigh your words — careful, deliberate, giving little away.";
    }

    // Positive (but still specific)
    if m.contains("cheerful") || m.contains("jovial") || m.contains("merry") {
        return "Let your good spirits show — warm and easy, quick to smile.";
    }
    if m.contains("eager") || m.contains("excited") || m.contains("enthus") {
        return "Your enthusiasm is genuine — bright tone, leaning in.";
    }
    if m.contains("curious") || m.contains("intrigued") {
        return "Your curiosity is alive — lean in, ask with genuine interest.";
    }
    if m.contains("passionate") || m.contains("fervent") {
        return "Your passion comes through — animated, heartfelt.";
    }
    if m.contains("warm") || m.contains("friendly") || m.contains("welcoming") {
        return "Be genuinely warm — open, unhurried, glad of the company.";
    }
    if m.contains("content") || m.contains("satisfied") {
        return "You are at ease — pleasant but not excitable, simply present.";
    }

    // Fallback: no strong directive
    "Let your mood colour your register naturally."
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

/// Inputs to [`build_enhanced_context_with_config`].
///
/// Groups the Tier 1 context builder's parameters into one struct so call sites
/// make ownership and the optional anchors explicit, and so the function no
/// longer needs `#[allow(clippy::too_many_arguments)]` (TD-029). The `language`
/// and `npc_names` fields are accepted for API uniformity with the system-prompt
/// builders but are not used by the context builder — the language directive
/// belongs in the system prompt.
pub struct Tier1ContextParams<'a> {
    pub npc: &'a Npc,
    pub world: &'a WorldState,
    pub player_input: &'a str,
    pub other_npcs: &'a [&'a Npc],
    pub language: &'a LanguageSettings,
    pub config: &'a NpcConfig,
    pub npc_names: &'a HashMap<NpcId, String>,
    pub player_name_for_npc: Option<&'a str>,
    pub was_introduced: bool,
}

/// Minimum number of prior NPC–player exchanges at this location before
/// the NPC is considered "familiar" with the player and "stranger" is dropped
/// from the interlocutor address vocabulary (#1388).
const FAMILIARITY_EXCHANGE_THRESHOLD: usize = 2;

/// Builds an enhanced context prompt for Tier 1 interactions using the given config.
///
/// Extends the base context with the NPC's recent memories and
/// information about other NPCs present at the same location.
pub fn build_enhanced_context_with_config(params: Tier1ContextParams<'_>) -> String {
    let Tier1ContextParams {
        npc,
        world,
        player_input,
        other_npcs,
        language: _language,
        config,
        npc_names: _npc_names,
        player_name_for_npc,
        was_introduced,
    } = params;

    let quality_continuity = config.dialogue_quality_continuity;

    // Familiarity: has the NPC spoken with this player enough times that
    // "stranger" is no longer an appropriate address? (#1388)
    let familiar = quality_continuity
        && world
            .conversation_log
            .exchange_count_with(world.player_location, npc.id)
            >= FAMILIARITY_EXCHANGE_THRESHOLD;

    let mut context = build_tier1_context(world);

    // Mood goes into the dynamic context (not the system prompt) so that mood
    // changes never bust the stable system-prompt prefix the model-runtime
    // prefix cache depends on (vllm-mlx --enable-prefix-cache).
    context.push_str(&mood_block(npc));

    context.push_str(&location_anchor_block(world));

    context.push_str(&interlocutor_block(player_name_for_npc, familiar));

    if let Some(block) = introduced_anchor_block(npc, was_introduced) {
        context.push_str(&block);
    }

    if let Some(block) = other_npcs_block(npc, other_npcs, config) {
        context.push_str(&block);
    }

    if let Some(block) = conversation_block(world, npc, player_name_for_npc) {
        context.push_str(&block);
    }

    if let Some(block) = continuity_block(world, npc, player_name_for_npc, quality_continuity) {
        context.push_str(&block);
    }

    // Anti-phrase-recycling block (#1387): inject the NPC's own recent lines
    // as a "do not repeat" list so the model cannot recycle verbatim phrases
    // from turns that fall outside the short conversation-history window.
    if quality_continuity
        && let Some(block) = prior_phrases_block(world, npc)
    {
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
    let mut context = build_enhanced_context_with_config(Tier1ContextParams {
        npc,
        world,
        player_input,
        other_npcs,
        language,
        config: &NpcConfig::default(),
        npc_names,
        player_name_for_npc: None,
        was_introduced: false,
    });
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
    use crate::test_helpers::make_named_npc as make_test_npc;
    use parish_config::RelationshipLabelConfig;
    use parish_world::WorldState;
    use std::collections::HashMap;

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
        // Name anchor (fixed: #11) — block must forbid addressing absent NPCs.
        assert!(
            context.contains("these are the only other people"),
            "other_npcs_block must anchor present-only addressing:\n{context}"
        );
    }

    #[test]
    fn tier1_context_params_struct_matches_positional_behaviour() {
        // TD-029: build_enhanced_context_with_config now takes a params struct.
        // Assert the struct path produces the documented blocks, and that the
        // `language`/`npc_names` fields (accepted for uniformity but unused by
        // the context builder) do not affect the output — proving the refactor
        // preserved behaviour byte-for-byte.
        let npc = make_test_npc(1, "Padraig", 1);
        let other = make_test_npc(2, "Tommy", 1);
        let world = WorldState::new();
        let config = NpcConfig::default();

        let names_a: HashMap<NpcId, String> = HashMap::new();
        let names_b: HashMap<NpcId, String> =
            [(NpcId(2), "Tommy".to_string())].into_iter().collect();
        let others = [&other];

        let ctx_a = build_enhanced_context_with_config(Tier1ContextParams {
            npc: &npc,
            world: &world,
            player_input: "greets everyone",
            other_npcs: &others,
            language: &LanguageSettings::english_only(),
            config: &config,
            npc_names: &names_a,
            player_name_for_npc: Some("Aiden Carney"),
            was_introduced: true,
        });

        // Varying the unused fields must not change the produced context.
        let ctx_b = build_enhanced_context_with_config(Tier1ContextParams {
            npc: &npc,
            world: &world,
            player_input: "greets everyone",
            other_npcs: &others,
            language: &LanguageSettings::new("irish".to_string(), Some("english".to_string())),
            config: &config,
            npc_names: &names_b,
            player_name_for_npc: Some("Aiden Carney"),
            was_introduced: true,
        });
        assert_eq!(
            ctx_a, ctx_b,
            "language/npc_names are accepted-but-unused; they must not alter the context"
        );

        // Documented blocks still present via the struct path.
        assert!(ctx_a.contains("Also present"));
        assert!(ctx_a.contains("Tommy, the Test"));
        assert!(ctx_a.contains("Aiden Carney"));
    }

    #[test]
    fn test_interlocutor_block_named_player_has_anchor() {
        // Name anchor (fixed: #35) — when the NPC knows the player's name,
        // the block must explicitly forbid addressing them by any other
        // name from recent history.
        let block = interlocutor_block(Some("Aiden Carney"), false);
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
        // Pre-introduction (fixed: #35 corollary) — the NPC must NOT borrow a
        // name from the history buffer when it doesn't yet know the player.
        let block = interlocutor_block(None, false);
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
    fn test_interlocutor_block_familiar_drops_stranger() {
        // AC-4 (#1388): after sufficient prior exchanges, "stranger" must not
        // appear as a valid address option in the interlocutor block.
        let block = interlocutor_block(None, true);
        assert!(
            !block.contains("stranger"),
            "familiar interlocutor block must not offer 'stranger' as address:\n{block}"
        );
        assert!(
            block.contains("do not") || block.contains("NOT"),
            "familiar block must contain a prohibition on 'stranger':\n{block}"
        );
        assert!(
            block.contains("friend") || block.contains("newcomer"),
            "familiar block must name an alternative address:\n{block}"
        );
    }

    #[test]
    fn test_interlocutor_block_unfamiliar_permits_stranger() {
        // Regression guard: on the first encounter (familiar=false), the
        // original vocabulary including "stranger" must still be offered.
        let block = interlocutor_block(None, false);
        assert!(
            block.contains("stranger"),
            "unfamiliar block must still offer 'stranger':\n{block}"
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
        // Location anchor (fixed: #21) — block must name the current
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
        // Solo-NPC anchor (fixed: #11) — when no one else is present, the
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
    fn mood_block_normal_mood_includes_label_and_directive() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "calm".to_string();
        let result = mood_block(&npc);
        assert!(
            result.starts_with("\n\nYour current mood: calm."),
            "mood block must start with label: {result}"
        );
        // Directive sentence follows the label.
        assert!(
            result.len() > "\n\nYour current mood: calm.".len(),
            "mood block must include tone directive after label: {result}"
        );
    }

    #[test]
    fn mood_block_sharp_mood_has_curt_directive() {
        // Regression: sharp NPCs were speaking cheerful-warm (#1373).
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "sharp".to_string();
        let result = mood_block(&npc);
        assert!(result.contains("sharp"), "mood label must appear: {result}");
        assert!(
            result.to_lowercase().contains("curt")
                || result.to_lowercase().contains("pleasantries")
                || result.to_lowercase().contains("direct"),
            "sharp mood must include a curt/direct directive: {result}"
        );
    }

    #[test]
    fn mood_block_busy_mood_has_brevity_directive() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "busy".to_string();
        let result = mood_block(&npc);
        assert!(
            result.to_lowercase().contains("brief")
                || result.to_lowercase().contains("time")
                || result.to_lowercase().contains("point"),
            "busy mood must include a brevity directive: {result}"
        );
    }

    #[test]
    fn mood_block_alert_mood_has_watchful_directive() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "alert".to_string();
        let result = mood_block(&npc);
        assert!(
            result.to_lowercase().contains("edge")
                || result.to_lowercase().contains("watchful")
                || result.to_lowercase().contains("attentive")
                || result.to_lowercase().contains("careful"),
            "alert mood must include a watchful/tense directive: {result}"
        );
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

    // ── AC-1: prior-phrases anti-recycling block (#1387) ────────────────────

    /// Build a WorldState with one conversation exchange for `npc_id` at
    /// the default location.
    fn world_with_prior_exchange(npc_id: u32, npc_name: &str, prior_line: &str) -> WorldState {
        use chrono::TimeZone;
        use parish_types::conversation::ConversationExchange;

        let mut world = WorldState::new();
        world.conversation_log.add(ConversationExchange {
            timestamp: chrono::Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap(),
            speaker_id: NpcId(npc_id),
            speaker_name: npc_name.to_string(),
            player_input: "Good morning".to_string(),
            npc_dialogue: prior_line.to_string(),
            location: world.player_location,
        });
        world
    }

    #[test]
    fn prior_phrases_block_present_when_npc_has_prior_lines() {
        // AC-1 (#1387): context must contain a "do not repeat" block when the
        // NPC has spoken before at this location.
        let npc = make_test_npc(1, "Padraig", 1);
        let world = world_with_prior_exchange(1, "Padraig", "Ye've a keen eye for introductions.");
        let block = prior_phrases_block(&world, &npc);
        assert!(
            block.is_some(),
            "prior_phrases_block must render when NPC has prior lines"
        );
        let text = block.unwrap();
        assert!(
            text.contains("DO NOT REPEAT THESE PHRASES"),
            "block must contain the anti-repeat header:\n{text}"
        );
        assert!(
            text.contains("keen eye for introductions"),
            "block must quote the prior line:\n{text}"
        );
    }

    #[test]
    fn prior_phrases_block_absent_on_first_encounter() {
        // Regression guard: no prior lines → block must not render.
        let npc = make_test_npc(1, "Padraig", 1);
        let world = WorldState::new();
        assert!(
            prior_phrases_block(&world, &npc).is_none(),
            "prior_phrases_block must be None on first encounter"
        );
    }

    #[test]
    fn context_includes_prior_phrases_block_when_quality_continuity_enabled() {
        // AC-1 integrated: build_enhanced_context_with_config must include
        // the anti-recycling block when quality_continuity is true.
        let npc = make_test_npc(1, "Padraig", 1);
        let world =
            world_with_prior_exchange(1, "Padraig", "Mayhap ye'll find yer trade keeps ye busy.");
        let config = NpcConfig {
            dialogue_quality_continuity: true,
            ..NpcConfig::default()
        };
        let names: HashMap<NpcId, String> = HashMap::new();
        let context = build_enhanced_context_with_config(Tier1ContextParams {
            npc: &npc,
            world: &world,
            player_input: "hello",
            other_npcs: &[],
            language: &LanguageSettings::english_only(),
            config: &config,
            npc_names: &names,
            player_name_for_npc: None,
            was_introduced: false,
        });
        assert!(
            context.contains("DO NOT REPEAT THESE PHRASES"),
            "context must include anti-recycling block:\n{context}"
        );
        assert!(
            context.contains("busy"),
            "context must quote the prior NPC line:\n{context}"
        );
    }

    #[test]
    fn context_omits_prior_phrases_block_when_quality_continuity_disabled() {
        // Kill-switch: with quality_continuity=false, the block must not appear.
        let npc = make_test_npc(1, "Padraig", 1);
        let world =
            world_with_prior_exchange(1, "Padraig", "Mayhap ye'll find yer trade keeps ye busy.");
        let config = NpcConfig {
            dialogue_quality_continuity: false,
            ..NpcConfig::default()
        };
        let names: HashMap<NpcId, String> = HashMap::new();
        let context = build_enhanced_context_with_config(Tier1ContextParams {
            npc: &npc,
            world: &world,
            player_input: "hello",
            other_npcs: &[],
            language: &LanguageSettings::english_only(),
            config: &config,
            npc_names: &names,
            player_name_for_npc: None,
            was_introduced: false,
        });
        assert!(
            !context.contains("DO NOT REPEAT THESE PHRASES"),
            "kill-switched context must NOT include anti-recycling block:\n{context}"
        );
    }

    // ── AC-3: no-reask continuity directive (#1388) ──────────────────────────

    #[test]
    fn continuity_block_includes_no_reask_directive_when_enabled() {
        // AC-3 (#1388): continuity block must contain a "do not re-ask" directive
        // when quality_continuity is true and a prior exchange exists.
        use chrono::TimeZone;
        use parish_types::conversation::ConversationExchange;

        let npc = make_test_npc(1, "Padraig", 1);
        let mut world = WorldState::new();
        // Add two exchanges so has_recent_exchange_with(n=2) fires.
        for h in [8u32, 9u32] {
            world.conversation_log.add(ConversationExchange {
                timestamp: chrono::Utc.with_ymd_and_hms(1820, 3, 20, h, 0, 0).unwrap(),
                speaker_id: NpcId(1),
                speaker_name: "Padraig".to_string(),
                player_input: "What do you do?".to_string(),
                npc_dialogue: "I run the pub.".to_string(),
                location: world.player_location,
            });
        }
        let block = continuity_block(&world, &npc, None, true);
        assert!(
            block.is_some(),
            "continuity_block must render when NPC has prior exchanges"
        );
        let text = block.unwrap();
        assert!(
            text.contains("already established")
                || text.contains("do not re-ask")
                || text.contains("settled"),
            "continuity block must contain no-reask directive when add_no_reask=true:\n{text}"
        );
    }

    #[test]
    fn continuity_block_omits_no_reask_directive_when_disabled() {
        // Kill-switch: with add_no_reask=false, the no-reask clause must not appear.
        use chrono::TimeZone;
        use parish_types::conversation::ConversationExchange;

        let npc = make_test_npc(1, "Padraig", 1);
        let mut world = WorldState::new();
        for h in [8u32, 9u32] {
            world.conversation_log.add(ConversationExchange {
                timestamp: chrono::Utc.with_ymd_and_hms(1820, 3, 20, h, 0, 0).unwrap(),
                speaker_id: NpcId(1),
                speaker_name: "Padraig".to_string(),
                player_input: "What do you do?".to_string(),
                npc_dialogue: "I run the pub.".to_string(),
                location: world.player_location,
            });
        }
        let block = continuity_block(&world, &npc, None, false);
        let text = block.unwrap_or_default();
        assert!(
            !text.contains("already established") && !text.contains("do not re-ask"),
            "disabled continuity block must NOT contain no-reask clause:\n{text}"
        );
    }

    // ── AC-4: familiarity-aware interlocutor address (#1388) ─────────────────

    #[test]
    fn context_drops_stranger_after_familiarity_threshold() {
        // AC-4 (#1388): once FAMILIARITY_EXCHANGE_THRESHOLD exchanges exist,
        // "stranger" must not appear in the interlocutor block.
        use chrono::TimeZone;
        use parish_types::conversation::ConversationExchange;

        let npc = make_test_npc(1, "Padraig", 1);
        let mut world = WorldState::new();
        // Add FAMILIARITY_EXCHANGE_THRESHOLD exchanges.
        for h in 0..FAMILIARITY_EXCHANGE_THRESHOLD {
            world.conversation_log.add(ConversationExchange {
                timestamp: chrono::Utc
                    .with_ymd_and_hms(1820, 3, 20, 8 + h as u32, 0, 0)
                    .unwrap(),
                speaker_id: NpcId(1),
                speaker_name: "Padraig".to_string(),
                player_input: "Hello again".to_string(),
                npc_dialogue: "Welcome back.".to_string(),
                location: world.player_location,
            });
        }
        let config = NpcConfig {
            dialogue_quality_continuity: true,
            ..NpcConfig::default()
        };
        let names: HashMap<NpcId, String> = HashMap::new();
        let context = build_enhanced_context_with_config(Tier1ContextParams {
            npc: &npc,
            world: &world,
            player_input: "hello",
            other_npcs: &[],
            language: &LanguageSettings::english_only(),
            config: &config,
            npc_names: &names,
            player_name_for_npc: None, // NPC does NOT know player's name
            was_introduced: false,
        });
        // The PERSON YOU ARE SPEAKING WITH block must not offer "stranger".
        // We locate that block by its header.
        let interlocutor_start = context
            .find("PERSON YOU ARE SPEAKING WITH")
            .expect("interlocutor block must be present");
        let interlocutor_end = context[interlocutor_start..]
            .find("\n\n")
            .map(|off| interlocutor_start + off)
            .unwrap_or(context.len());
        let interlocutor_section = &context[interlocutor_start..interlocutor_end];
        assert!(
            !interlocutor_section.contains("stranger"),
            "interlocutor block must not offer 'stranger' after {FAMILIARITY_EXCHANGE_THRESHOLD} exchanges:\n{interlocutor_section}"
        );
    }

    #[test]
    fn context_permits_stranger_before_familiarity_threshold() {
        // Regression guard: below the threshold, "stranger" must still be
        // an available address option.
        let npc = make_test_npc(1, "Padraig", 1);
        let world = WorldState::new(); // no prior exchanges
        let config = NpcConfig {
            dialogue_quality_continuity: true,
            ..NpcConfig::default()
        };
        let names: HashMap<NpcId, String> = HashMap::new();
        let context = build_enhanced_context_with_config(Tier1ContextParams {
            npc: &npc,
            world: &world,
            player_input: "hello",
            other_npcs: &[],
            language: &LanguageSettings::english_only(),
            config: &config,
            npc_names: &names,
            player_name_for_npc: None,
            was_introduced: false,
        });
        assert!(
            context.contains("stranger"),
            "context must still offer 'stranger' before familiarity threshold:\n{context}"
        );
    }

    // ── AC-6: feature-flag name present in code path ─────────────────────────

    #[test]
    fn npc_config_dialogue_quality_continuity_defaults_true() {
        // AC-6: the kill-switch defaults on so the fix ships enabled.
        let cfg = NpcConfig::default();
        assert!(
            cfg.dialogue_quality_continuity,
            "dialogue_quality_continuity must default to true"
        );
    }
}
