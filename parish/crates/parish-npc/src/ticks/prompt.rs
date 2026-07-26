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
        None, // no location grounding for test convenience wrapper
    )
}

pub fn build_enhanced_system_prompt_with_config(
    npc: &Npc,
    improv: bool,
    language: &LanguageSettings,
    config: &NpcConfig,
    npc_names: &HashMap<NpcId, String>,
    known_roster: Option<&[(NpcId, String, String)]>,
    // Real location names from the world graph. When `Some`, injects a
    // `PLACES IN THIS PARISH` block with an anti-sycophancy instruction
    // so the NPC declines to confirm places or people not on the lists
    // (fixes #1394). Pass `None` for test/legacy callers; the block is
    // silently omitted.
    location_names: Option<&[String]>,
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
                    prompt.push_str(&format!(
                        "- {}, {} \u{2014} real parish person\n",
                        name, occupation
                    ));
                }
            }
            prompt.push_str(
                "Each entry above gives the person's pronouns (where known) and \
                age \u{2014} use their stated pronouns and never guess gender from \
                a name. Entries marked as real parish people are real names in \
                the parish, but do not claim close acquaintance unless your \
                relationships or memories say so. If you \
                want to mention anyone not listed above, describe them by role or \
                appearance \u{2014} never invent a name, and never refer to a \
                person (he/she/they/her/him) who has not been mentioned.\n",
            );
            prompt.push_str(
                "WORK REFERRALS: When asked who might offer a particular kind of \
                work, compare the request with the authored occupation and workplace \
                shown for each person. Prefer an exact relevant trade or workplace. \
                Friendship is not evidence that someone employs farm hands, runs a \
                shop, or practises another trade. Never invent duties, a business, or \
                a workplace for an unrelated person; if no grounded match exists, say \
                plainly that you do not know whom to ask.\n",
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

    // Location grounding block (#1394): inject real place names and an
    // anti-sycophancy instruction so the NPC refuses to confirm invented
    // locations or people rather than agreeing with the player's false premises.
    if let Some(places) = location_names.filter(|p| !p.is_empty()) {
        prompt.push_str("\n\nPLACES IN THIS PARISH:\n");
        for place in places {
            prompt.push_str(&format!("- {place}\n"));
        }
        prompt.push_str(
            "These are the only real places in this parish. \
            If someone mentions a place or person you do not recognise \
            from the lists above, say you know of no such place or person \
            \u{2014} do not confirm, describe, or invent details about \
            anything not listed. Politely correct or deflect instead.\n",
        );
        // #1401: the open-mention guard above is not enough — small models
        // still play along when a fabricated person is embedded in the
        // question as a *presupposition* ("is old Festus, the cooper, still at
        // his shop by the bridge?"). Name the presupposition case explicitly
        // and require honest non-recognition for any unknown named person.
        prompt.push_str(
            "Watch for a PRESUPPOSED name: if someone speaks of a specific \
            person by name as though you both know them \u{2014} asking after \
            their health, their trade, or their doings \u{2014} and that name \
            is NOT in the people you know above, do not go along with it. Say \
            plainly that you know no one by that name in these parts (\"I know \
            no such person\", \"never heard of him\"). Never confirm they are \
            still about, describe them, or invent their trade or whereabouts. \
            An invented name asked of you as fact is still invented \u{2014} \
            answer with honest non-recognition, not a friendly yarn.\n",
        );
        // #1420: the SAME presupposition trap applies to PLACES. A player asks
        // after "the abbey in town" or "Father Pendleton's chapel" as settled
        // fact; the small model confirms it exists and even gives directions
        // ("right in the heart of Kilteevan"). A named building or settlement
        // stated as fact is just as invented as a named person. Cover it
        // explicitly and forbid confirming existence OR giving a location.
        prompt.push_str(
            "Watch equally for a PRESUPPOSED place: if someone speaks of a \
            specific building or settlement by name (an abbey, a chapel, a mill, \
            a named inn, a named townland) as though it plainly exists \u{2014} \
            asking where it is or how to reach it \u{2014} and it is NOT in the \
            PLACES IN THIS PARISH list above, do not go along with it. Say \
            plainly there is no such place hereabouts (\"there's no abbey in \
            these parts\", \"I know of no such place\"). NEVER confirm it exists, \
            NEVER say it is in or near any real place, and NEVER give directions \
            to it. A place named to you as fact is still invented unless it is on \
            the list. When in doubt, declare non-recognition rather than invent a \
            location \u{2014} a wrong yarn is worse than an honest \"I don't know \
            it.\"\n",
        );
        // #1504: distinguish acquaintance questions from identity challenges.
        // "Do you know X?" is an ACQUAINTANCE question — answer whether you
        // know the named person. It is NOT asking whether you ARE that person.
        // Never respond with your own name or identity ("I'm but Seamus") to
        // "Do you know X?" — that answers a question that was never asked.
        // Only assert your own identity when directly asked "Are you X?" or
        // "Who are you?".
        prompt.push_str(
            "ACQUAINTANCE vs IDENTITY: \"Do you know X?\" is an ACQUAINTANCE \
            question \u{2014} answer it directly: \"Aye, I know them\" or \
            \"I know no one by that name.\" Do NOT respond with your own name or \
            clarify who you are in answer to an acquaintance question \u{2014} \
            that addresses a question that was never asked. Reserve first-person \
            identity assertions (\"I'm ...\", \"My name is ...\") for when someone \
            directly asks \"Are you X?\" or \"Who are you?\"\n",
        );
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
        block.push_str("- \"");
        block.push_str(excerpt);
        block.push_str("\"\n");
    }
    Some(block)
}

/// Cross-NPC crutch-phrase suppression block (#1422).
///
/// The per-NPC [`prior_phrases_block`] only catches an NPC recycling *its own*
/// lines. Small models (Qwen 14B) instead reach for an identical opener *frame*
/// across consecutive *different* NPCs in a session ("ye've come to the right
/// place" from three NPCs in a row). This injects recent lines from OTHER
/// speakers at the location so the model is told to vary the frame rather than
/// echo a neighbour. Only renders when another NPC has spoken here recently.
fn cross_npc_phrases_block(world: &WorldState, npc: &Npc) -> Option<String> {
    let lines = world
        .conversation_log
        .other_npcs_recent_lines(world.player_location, npc.id, 4);
    if lines.is_empty() {
        return None;
    }
    let mut block = String::from(
        "\n\nOTHER FOLK HERE JUST SAID THESE \u{2014} do NOT echo their opener \
         or frame; reach for plainly different wording of your own:\n",
    );
    for line in &lines {
        let excerpt: &str = if line.len() > 120 {
            let mut end = 120;
            while end > 0 && !line.is_char_boundary(end) {
                end -= 1;
            }
            &line[..end]
        } else {
            line
        };
        block.push_str("- \"");
        block.push_str(excerpt);
        block.push_str("\"\n");
    }
    block.push_str(
        "In particular, never greet with \"ye've come to the right place\" or any \
         near-copy of a frame already used above \u{2014} a parish where everyone \
         says the same line rings false.",
    );
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
///
/// Frames the history as BACKGROUND AWARENESS rather than a script to quote —
/// small models otherwise recite another NPC's exact prior line or narrate
/// the previous exchange (#1447).
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
    Some(format!(
        "\n\nBACKGROUND AWARENESS — what has recently been said nearby \
         (use only as context; do NOT quote it word-for-word, do NOT narrate \
         who said what, and do NOT repeat any line verbatim in your reply):\n{ctx}"
    ))
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
    identity_known: bool,
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
    let identity_cue = if identity_known {
        " Do not re-introduce yourself or greet them again."
    } else {
        " Do not greet them again. They still do not know your name; if they \
         ask who you are, answer with your actual name."
    };
    Some(format!(
        "\n\nYou are already in conversation with {name}.{identity_cue}{no_reask}"
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

/// NPC's own current activity — injected with an explicit ownership label
/// so models cannot attribute the NPC's errand or task to the player (#1448).
fn last_activity_block(npc: &Npc) -> Option<String> {
    let activity = npc.last_activity.as_deref()?;
    if activity.is_empty() {
        return None;
    }
    Some(format!(
        "\n\nYOUR current activity (this is what YOU have been doing — \
         it is NOT something the player did or asked you about): {activity}"
    ))
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
    // #1421: a bare "Your current mood: X" label was inconsistently honoured —
    // the small model would invert it (a `sharp` widow giving "a warm welcome",
    // an `alert` shopkeeper reading cheerful) because the cultural-warmth
    // guideline in the system prompt out-pulls a soft label. Frame the mood as
    // an OVERRIDE the model must obey even on a first greeting, so the
    // formulaic-warm register cannot wash it out.
    format!(
        "\n\nYOUR CURRENT MOOD: {mood}. This mood OVERRIDES any default-friendly \
         or welcoming register \u{2014} let it shape THIS reply, including your \
         very first greeting. {tone_directive} Do not paper over this mood with a \
         warm welcome or cheerful opener if the mood does not call for one."
    )
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

/// Builds the final, live-turn contract appended after transcript and memory
/// context.
///
/// Important behavioral constraints near the start of a long context can lose
/// to recent dialogue or examples on smaller models. This compact block repeats
/// only facts that apply to the current exchange: canonical starting mood,
/// first-contact state, and answer-first handling for explicit questions.
pub fn live_turn_contract_block(
    npc: &Npc,
    had_prior_exchange: bool,
    identity_known: bool,
    player_input: &str,
) -> String {
    let mood = npc.mood.trim().trim_end_matches('.');
    let input = player_input.trim().to_lowercase();
    let mut block = String::from("\n\nLIVE TURN CONTRACT:\n");
    if !mood.is_empty() {
        block.push_str(&format!(
            "- CANONICAL STARTING MOOD: {mood}. {tone} This authored state, not \
             the mood label you later emit in JSON, governs the spoken reply.\n",
            tone = mood_tone_directive(mood)
        ));
    }

    if !had_prior_exchange {
        block.push_str(
            "- FIRST CONTACT: this is your first exchange with this person. Do \
             not claim to have seen them around, spoken before, met previously, \
             welcomed them back, or otherwise imply prior familiarity.\n",
        );
    }

    // An NPC can remain anonymous after an earlier exchange. A later direct
    // name question must still produce a spoken identity rather than relying on
    // the first-contact branch (#1776).
    if !identity_known
        && (input.contains("your name")
            || input.contains("who are you")
            || input.contains("who're you")
            || input.contains("might i ask your name"))
    {
        block.push_str(
            "- IDENTITY: the player directly asked who you are. Say your \
             actual name aloud in the dialogue; identity is not revealed by \
             metadata or by the mere fact that this exchange occurred.\n",
        );
    }

    let direct_question = player_input.contains('?')
        || [
            "who ",
            "what ",
            "when ",
            "where ",
            "why ",
            "how ",
            "have you ",
            "have ye ",
            "did you ",
            "did ye ",
            "is it ",
            "is that ",
            "are you ",
            "are ye ",
            "can you ",
            "can ye ",
            "could you ",
            "could ye ",
        ]
        .iter()
        .any(|prefix| input.starts_with(prefix));
    if direct_question {
        block.push_str(
            "- ANSWER FIRST: answer the player's explicit question in the first \
             sentence. If they ask for firsthand evidence, give one concrete \
             observation or plainly admit that you did not see it / do not know. \
             Do not merely repeat the premise, hide behind vague talk, or turn \
             the same question back on the player.\n",
        );
    }

    if input.contains("work")
        || input.contains("job")
        || input.contains("hire")
        || input.contains("pair of hands")
        || input.contains("another hand")
    {
        block.push_str(
            "- WORK REQUEST: any person you recommend must have an authored \
             occupation or workplace that actually matches the requested work. \
             Do not assign farm, shop, forge, school, or other duties to someone \
             whose roster entry says otherwise.\n",
        );
    }

    block
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

    if let Some(block) = continuity_block(
        world,
        npc,
        player_name_for_npc,
        quality_continuity,
        was_introduced,
    ) {
        context.push_str(&block);
    }

    // Anti-phrase-recycling block (#1387): inject the NPC's own recent lines
    // as a "do not repeat" list so the model cannot recycle verbatim phrases
    // from turns that fall outside the short conversation-history window.
    if quality_continuity && let Some(block) = prior_phrases_block(world, npc) {
        context.push_str(&block);
    }

    // Cross-NPC crutch-phrase suppression (#1422): catch a frame shared across
    // *different* NPCs in a session, which the per-NPC guard above cannot see.
    if quality_continuity && let Some(block) = cross_npc_phrases_block(world, npc) {
        context.push_str(&block);
    }

    if let Some(block) = reactions_block(npc, config) {
        context.push_str(&block);
    }

    // NPC's own current activity — labeled explicitly to prevent models from
    // attributing the NPC's errand to the player (#1448).
    if let Some(block) = last_activity_block(npc) {
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

    /// AC-8 (#1422): the cross-NPC crutch block must list a recent line from a
    /// DIFFERENT NPC at the location and tell the model not to echo the frame.
    #[test]
    fn cross_npc_phrases_block_suppresses_shared_frame() {
        use parish_types::conversation::ConversationExchange;
        let npc = make_test_npc(1, "Padraig", 1);
        let mut world = WorldState::new();
        let loc = world.player_location;
        // A different NPC (id 2) already used the crutch frame here.
        world.conversation_log.add(ConversationExchange {
            timestamp: world.clock.now(),
            speaker_id: NpcId(2),
            speaker_name: "Peig".to_string(),
            player_input: "hello".to_string(),
            npc_dialogue: "Ye've come to the right place for a spot of gossip.".to_string(),
            location: loc,
        });

        let block = cross_npc_phrases_block(&world, &npc)
            .expect("block must render when another NPC spoke here");
        assert!(
            block.contains("OTHER FOLK HERE JUST SAID"),
            "block must head the other-folk list:\n{block}"
        );
        assert!(
            block.contains("Ye've come to the right place"),
            "block must surface the neighbour's recent line:\n{block}"
        );
        assert!(
            block.contains("ye've come to the right place") && block.contains("near-copy"),
            "block must forbid echoing the shared frame:\n{block}"
        );
    }

    /// AC-8 negative: the NPC's own lines (id matches) must NOT appear in the
    /// cross-NPC block — that is the per-NPC guard's job, not this one's.
    #[test]
    fn cross_npc_phrases_block_excludes_self() {
        use parish_types::conversation::ConversationExchange;
        let npc = make_test_npc(1, "Padraig", 1);
        let mut world = WorldState::new();
        let loc = world.player_location;
        world.conversation_log.add(ConversationExchange {
            timestamp: world.clock.now(),
            speaker_id: NpcId(1),
            speaker_name: "Padraig".to_string(),
            player_input: "hello".to_string(),
            npc_dialogue: "Only my own line here.".to_string(),
            location: loc,
        });
        assert!(
            cross_npc_phrases_block(&world, &npc).is_none(),
            "block must not render when only the NPC's own lines exist"
        );
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
        // The word "stranger" may still appear in a prohibitive clause
        // (e.g. "do NOT address them as 'stranger'") — the assertion targets
        // only the positive recommendations after "Refer to them as".
        let block = interlocutor_block(None, true);
        let refer_to_section = block.split("Refer to them as").nth(1).unwrap_or("");
        assert!(
            !refer_to_section.contains("stranger"),
            "familiar interlocutor block must not list 'stranger' as a valid address:\n{block}"
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
        let prompt = build_enhanced_system_prompt_with_config(
            &npc, false, &lang, &config, &npc_names, None, None,
        );
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

    // ── #1401: presupposed fabricated-person non-recognition ─────────────────

    /// AC-6 (#1401): when location grounding is on, the system prompt must
    /// contain an explicit directive covering a *presupposed* unknown person,
    /// instructing the NPC to express non-recognition rather than confirm.
    #[test]
    fn grounding_block_covers_presupposed_unknown_person() {
        let npc = make_test_npc(1, "Padraig", 2);
        let config = NpcConfig::default();
        let names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();
        let places = vec!["Darcy's Pub".to_string(), "The Mill".to_string()];

        let prompt = build_enhanced_system_prompt_with_config(
            &npc,
            false,
            &lang,
            &config,
            &names,
            None,
            Some(&places),
        );
        assert!(
            prompt.contains("PRESUPPOSED name"),
            "grounding block must name the presupposition case:\n{prompt}"
        );
        assert!(
            prompt.contains("know no such person") || prompt.contains("never heard of him"),
            "grounding block must instruct honest non-recognition:\n{prompt}"
        );
        assert!(
            prompt.contains("Never confirm they are still about"),
            "grounding block must forbid confirming an invented person:\n{prompt}"
        );
    }

    /// AC-1/AC-2 (#1420): the grounding block must ALSO cover a *presupposed*
    /// unknown place — an abbey/chapel/townland asserted as fact — and forbid
    /// confirming it exists or giving directions to it.
    #[test]
    fn grounding_block_covers_presupposed_unknown_place() {
        let npc = make_test_npc(1, "Padraig", 2);
        let config = NpcConfig::default();
        let names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();
        let places = vec!["Darcy's Pub".to_string(), "The Mill".to_string()];

        let prompt = build_enhanced_system_prompt_with_config(
            &npc,
            false,
            &lang,
            &config,
            &names,
            None,
            Some(&places),
        );
        assert!(
            prompt.contains("PRESUPPOSED place"),
            "grounding block must name the presupposed-place case:\n{prompt}"
        );
        assert!(
            prompt.contains("no such place"),
            "grounding block must instruct non-recognition of an unknown place:\n{prompt}"
        );
        assert!(
            prompt.contains("NEVER give directions"),
            "grounding block must forbid giving directions to an invented place:\n{prompt}"
        );
    }

    /// AC-7 (#1401): kill-switch — with grounding disabled (`location_names`
    /// is `None`), neither the PLACES block nor the presupposition directive
    /// is emitted.
    #[test]
    fn grounding_block_absent_when_location_names_none() {
        let npc = make_test_npc(1, "Padraig", 2);
        let config = NpcConfig::default();
        let names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();

        let prompt = build_enhanced_system_prompt_with_config(
            &npc, false, &lang, &config, &names, None, None,
        );
        assert!(
            !prompt.contains("PLACES IN THIS PARISH"),
            "no grounding block when location_names is None:\n{prompt}"
        );
        assert!(
            !prompt.contains("PRESUPPOSED name"),
            "no presupposition directive when grounding disabled:\n{prompt}"
        );
        assert!(
            !prompt.contains("PRESUPPOSED place"),
            "no place-presupposition directive when grounding disabled:\n{prompt}"
        );
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
            result.starts_with("\n\nYOUR CURRENT MOOD: calm."),
            "mood block must start with label: {result}"
        );
        // Directive sentence follows the label.
        assert!(
            result.len() > "\n\nYOUR CURRENT MOOD: calm.".len(),
            "mood block must include tone directive after label: {result}"
        );
    }

    /// AC-6 (#1421): the mood block must frame the mood as an OVERRIDE of the
    /// default-friendly register so the small model stops washing it out with a
    /// formulaic warm greeting.
    #[test]
    fn mood_block_frames_mood_as_override_of_friendly_register() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "sharp".to_string();
        let result = mood_block(&npc);
        assert!(
            result.contains("OVERRIDES"),
            "mood block must declare the mood overrides the default register: {result}"
        );
        assert!(
            result.to_lowercase().contains("first greeting")
                || result.to_lowercase().contains("warm welcome")
                || result.to_lowercase().contains("cheerful opener"),
            "mood block must explicitly forbid papering over with a warm/cheerful opener: {result}"
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
                assigned_task: None,
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
                assigned_task: None,
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
                assigned_task: None,
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
        let block = continuity_block(&world, &npc, None, true, true);
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

        let anonymous = continuity_block(&world, &npc, None, true, false).unwrap();
        assert!(!anonymous.contains("Do not re-introduce yourself"));
        assert!(anonymous.contains("still do not know your name"));
        assert!(anonymous.contains("answer with your actual name"));
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
        let block = continuity_block(&world, &npc, None, false, true);
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
        // The PERSON YOU ARE SPEAKING WITH block must not offer "stranger"
        // as a valid address. The word may still appear in a prohibitive
        // clause ("do NOT address them as 'stranger'"); we therefore check
        // only the positive recommendations after "Refer to them as".
        let interlocutor_start = context
            .find("PERSON YOU ARE SPEAKING WITH")
            .expect("interlocutor block must be present");
        let interlocutor_end = context[interlocutor_start..]
            .find("\n\n")
            .map(|off| interlocutor_start + off)
            .unwrap_or(context.len());
        let interlocutor_section = &context[interlocutor_start..interlocutor_end];
        let refer_to_section = interlocutor_section
            .split("Refer to them as")
            .nth(1)
            .unwrap_or("");
        assert!(
            !refer_to_section.contains("stranger"),
            "interlocutor block must not list 'stranger' as valid address after {FAMILIARITY_EXCHANGE_THRESHOLD} exchanges:\n{interlocutor_section}"
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

    // ── Part-B (#1422): enhanced system prompt carries single-question cap ────

    /// The enhanced system prompt (built on top of the Tier 1 base prompt) must
    /// carry the "AT MOST ONE question per reply" brevity instruction (#1422
    /// Part B). `build_enhanced_system_prompt_with_config` delegates to
    /// `build_tier1_system_prompt` for its base, so the cap is inherited;
    /// this test is the explicit regression guard.
    #[test]
    fn enhanced_system_prompt_carries_single_question_cap() {
        let npc = make_test_npc(1, "Padraig", 1);
        let config = NpcConfig::default();
        let names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();

        let prompt = build_enhanced_system_prompt_with_config(
            &npc, false, &lang, &config, &names, None, None,
        );
        assert!(
            prompt.contains("AT MOST ONE question") || prompt.contains("at most one question"),
            "enhanced system prompt must carry the single-question brevity cap (#1422 Part B):\n{prompt}"
        );
    }

    /// Companion: the `dialogue_anti_repetition` config field defaults to true
    /// so the cross-NPC opener dedup ships enabled by default (#1422).
    #[test]
    fn npc_config_dialogue_anti_repetition_defaults_true() {
        let cfg = NpcConfig::default();
        assert!(
            cfg.dialogue_anti_repetition,
            "dialogue_anti_repetition must default to true (#1422)"
        );
    }

    // ── #1447: conversation-history paraphrase framing ───────────────────────

    /// AC (#1447): conversation_block must carry the paraphrase directive —
    /// "BACKGROUND AWARENESS" header plus explicit no-verbatim-quote instruction.
    #[test]
    fn conversation_block_carries_paraphrase_directive() {
        use parish_types::conversation::ConversationExchange;

        let npc = make_test_npc(1, "Padraig", 1);
        let mut world = WorldState::new();
        let loc = world.player_location;
        world.conversation_log.add(ConversationExchange {
            timestamp: world.clock.now(),
            speaker_id: NpcId(1),
            speaker_name: "Padraig".to_string(),
            player_input: "hello".to_string(),
            npc_dialogue: "Fine day, so it is.".to_string(),
            location: loc,
        });

        let block = conversation_block(&world, &npc, None)
            .expect("block must render when an exchange exists");
        assert!(
            block.contains("BACKGROUND AWARENESS"),
            "conversation block must use BACKGROUND AWARENESS framing (#1447):\n{block}"
        );
        assert!(
            block.contains("do NOT quote it word-for-word") || block.contains("do not quote"),
            "conversation block must forbid verbatim quoting (#1447):\n{block}"
        );
        assert!(
            block.contains("do NOT narrate") || block.contains("do not narrate"),
            "conversation block must forbid narrating who said what (#1447):\n{block}"
        );
    }

    // ── #1448: NPC own-activity label ───────────────────────────────────────

    /// AC (#1448): last_activity_block must label the activity as the NPC's OWN
    /// and must NOT suggest the player did or asked about it.
    #[test]
    fn last_activity_block_labels_activity_as_npcs_own() {
        let mut npc = make_test_npc(1, "Peig", 1);
        npc.last_activity = Some("Walked to Connolly's shop for thread.".to_string());

        let block = last_activity_block(&npc).expect("block must render when last_activity is set");
        assert!(
            block.contains("YOUR current activity") || block.contains("YOUR own"),
            "block must label the activity as the NPC's own (#1448):\n{block}"
        );
        assert!(
            block.contains("NOT something the player did") || block.contains("this is what YOU"),
            "block must clarify the activity is not the player's (#1448):\n{block}"
        );
        assert!(
            block.contains("Connolly's shop"),
            "block must include the actual activity text:\n{block}"
        );
    }

    /// AC (#1448): last_activity_block must return None when last_activity is not set.
    #[test]
    fn last_activity_block_absent_when_no_activity() {
        let npc = make_test_npc(1, "Peig", 1);
        assert!(
            last_activity_block(&npc).is_none(),
            "last_activity_block must be None when last_activity is not set"
        );
    }

    /// AC (#1448): the labeled activity block must appear in the assembled context.
    #[test]
    fn context_includes_own_activity_label_when_last_activity_set() {
        let mut npc = make_test_npc(1, "Peig", 1);
        npc.last_activity = Some("Tending the cow in the byre.".to_string());
        let world = WorldState::new();
        let config = NpcConfig::default();
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
            context.contains("YOUR current activity"),
            "assembled context must label the NPC's own activity (#1448):\n{context}"
        );
        assert!(
            context.contains("Tending the cow"),
            "assembled context must include the activity text (#1448):\n{context}"
        );
    }

    /// AC-1 (#1504): when grounding is enabled, the system prompt must contain
    /// the acquaintance-vs-identity directive that prevents the NPC from
    /// answering "do you know X?" with a self-identification assertion.
    #[test]
    fn grounding_block_distinguishes_acquaintance_from_identity_questions() {
        let npc = make_test_npc(1, "Seamus", 2);
        let config = NpcConfig::default();
        let names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();
        let places = vec!["Darcy's Pub".to_string(), "The Mill".to_string()];

        let prompt = build_enhanced_system_prompt_with_config(
            &npc,
            false,
            &lang,
            &config,
            &names,
            None,
            Some(&places),
        );
        assert!(
            prompt.contains("ACQUAINTANCE vs IDENTITY") || prompt.contains("acquaintance question"),
            "grounding block must contain acquaintance-vs-identity directive (#1504):\n{prompt}"
        );
        assert!(
            prompt.contains("Do you know X?") || prompt.contains("do you know"),
            "directive must reference the 'do you know' pattern (#1504):\n{prompt}"
        );
        assert!(
            prompt.contains("identity") || prompt.contains("\"Are you X?\""),
            "directive must distinguish identity questions (#1504):\n{prompt}"
        );
    }

    /// AC-2 (#1504): the acquaintance-vs-identity directive must NOT appear
    /// when grounding is disabled (location_names is None).
    #[test]
    fn acquaintance_vs_identity_directive_absent_when_grounding_disabled() {
        let npc = make_test_npc(1, "Seamus", 2);
        let config = NpcConfig::default();
        let names: HashMap<NpcId, String> = HashMap::new();
        let lang = LanguageSettings::english_only();

        let prompt = build_enhanced_system_prompt_with_config(
            &npc, false, &lang, &config, &names, None, None,
        );
        assert!(
            !prompt.contains("ACQUAINTANCE vs IDENTITY"),
            "acquaintance-vs-identity directive must be absent when grounding is disabled:\n{prompt}"
        );
    }

    #[test]
    fn live_turn_contract_grounds_first_contact_mood_and_direct_answer() {
        let mut npc = make_test_npc(1, "Tommy O'Brien", 1);
        npc.mood = "reflective".to_string();
        let block = live_turn_contract_block(
            &npc,
            false,
            false,
            "Have you seen something here yourself, or is that only an old tale?",
        );

        assert!(block.contains("CANONICAL STARTING MOOD: reflective"));
        assert!(block.contains("FIRST CONTACT"));
        assert!(block.contains("ANSWER FIRST"));
        assert!(block.contains("one concrete observation"));
        assert!(block.contains("Do not merely repeat the premise"));
    }

    #[test]
    fn live_turn_contract_requires_spoken_identity_when_name_is_asked() {
        let npc = make_test_npc(1, "Peig Hannigan", 1);
        let block =
            live_turn_contract_block(&npc, false, false, "Good morning. Might I ask your name?");
        assert!(block.contains("Say your actual name aloud"));
        assert!(block.contains("not revealed by metadata"));
    }

    #[test]
    fn later_name_question_still_requires_spoken_identity() {
        let npc = make_test_npc(1, "Peig Hannigan", 1);
        let block =
            live_turn_contract_block(&npc, true, false, "We spoke before, but who are you?");
        assert!(!block.contains("FIRST CONTACT"));
        assert!(block.contains("Say your actual name aloud"));
    }

    #[test]
    fn known_identity_is_not_forced_to_repeat_on_name_question() {
        let npc = make_test_npc(1, "Peig Hannigan", 1);
        let block = live_turn_contract_block(&npc, true, true, "We spoke before, but who are you?");
        assert!(!block.contains("Say your actual name aloud"));
    }

    #[test]
    fn enhanced_prompt_requires_occupation_grounded_work_referrals() {
        let npc = make_test_npc(1, "Peig Hannigan", 1);
        let config = NpcConfig::default();
        let names: HashMap<NpcId, String> = HashMap::new();
        let roster = vec![
            (
                NpcId(2),
                "Siobhan Murphy".to_string(),
                "she/her, 45, Farmer; workplace: Murphy's Farm".to_string(),
            ),
            (
                NpcId(7),
                "Mick Flanagan".to_string(),
                "he/him, 65, Retired Constable".to_string(),
            ),
        ];
        let prompt = build_enhanced_system_prompt_with_config(
            &npc,
            false,
            &LanguageSettings::english_only(),
            &config,
            &names,
            Some(&roster),
            None,
        );
        assert!(prompt.contains("WORK REFERRALS"));
        assert!(prompt.contains("authored occupation and workplace"));
        assert!(prompt.contains("Never invent duties"));
    }
}
