//! Portable canonical NPC dialogue grounding and application.
//!
//! This module contains the state-independent grounding snapshot and the single
//! validated apply boundary shared by desktop and embedded runtimes. It does not
//! depend on the IPC layer, inference clients, or desktop orchestration.

use std::collections::HashSet;

use chrono::Datelike;

use crate::config::FeatureFlags;
use crate::npc::manager::NpcManager;
use crate::npc::{LanguageSettings, NpcId};
use crate::world::{LocationId, WorldState};

/// Records a player introduction without coupling canonical dialogue application
/// to desktop IPC handlers.
fn detect_and_record_player_name(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    player_input: &str,
    speaker_id: NpcId,
) {
    if let Some(name) = crate::npc::detect_player_name(player_input) {
        if world.player_name.is_none() {
            tracing::info!("Player introduced themselves as: {}", name);
            world.player_name = Some(name);
        }
        npc_manager.teach_player_name(speaker_id);
    }
}

/// Default-on kill switch for durable player task assignment and progression.
///
/// Disable with `/flag disable player-task-progression` to preserve action
/// narration while suppressing task state mutations and semantic task events.
pub const PLAYER_TASK_PROGRESSION_FLAG: &str = "player-task-progression";

/// Deterministic player opt-in for accepting a model-proposed task.
///
/// Restricting assignment to an explicit work/help request, affirmative
/// acceptance, or concrete first-step follow-up lets runtimes know before
/// inference which dialogue turns can mutate the durable task ledger. The
/// staged-turn admission seam calls this same classifier, so no accepted task
/// can bypass atomic journal persistence.
pub fn is_task_request_input(input: &str) -> bool {
    let normalized = input.trim().to_ascii_lowercase().replace('\u{2019}', "'");
    if [
        "don't need help",
        "do not need help",
        "can't help",
        "cannot help",
        "won't help",
        "will not help",
        "won't take the work",
        "will not take the work",
        "not take the work",
        "decline the work",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
    {
        return false;
    }

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
        "i'll take the work",
        "i will take the work",
        "i accept the work",
        "i'll do the work",
        "i will do the work",
        "what would you have me do first",
        "what should i do first",
        "what am i to do first",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}
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
    /// True only when the original candidate passed the canonical parser and
    /// semantic validator. False outcomes have no state, memory, event, or
    /// display effects.
    pub accepted_candidate: bool,
    /// Debug-event strings from Tier-1 apply (step 2) and witness memories
    /// (step 4). The live loop discards these; headless + harness forward them.
    pub debug_events: Vec<String>,
    /// The guarded, capped dialogue to show the player. Matches exactly what was
    /// written to the conversation log and the `DialogueOccurred` event. May be
    /// empty when the model returned no usable dialogue.
    pub display_text: String,
    /// Content-free names of canonical apply-seam constraints that materially
    /// affected the accepted model dialogue. A constraint remains represented
    /// when an earlier canonical guard makes its later transformation a no-op;
    /// benchmark telemetry must not depend on guard ordering.
    pub guard_reasons: Vec<String>,
    /// Secondary-language hints validated against `display_text` and the active
    /// setting's curated native-language inventory (#1789).
    pub language_hints: Vec<crate::npc::LanguageHint>,
    /// Canonical task post-state when the delivered dialogue assigned a task.
    ///
    /// Callers persist this exact record before acknowledging the player turn;
    /// replay never re-runs model-output interpretation.
    pub assigned_task: Option<parish_types::PlayerTask>,
    /// Canonical physical action from accepted metadata. Rejected response
    /// metadata is discarded before this value is derived.
    pub action: Option<String>,
}

#[cfg(all(test, feature = "desktop"))]
pub(crate) fn task_proposal_is_grounded_in_final_dialogue(
    proposal: &str,
    final_dialogue: &str,
) -> bool {
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
    apply_npc_dialogue_turn_with_disposition(
        world,
        npc_manager,
        speaker_id,
        parsed,
        crate::npc::NpcResponseParseDisposition::FullJson,
        player_input,
        player_said_for_journal,
        game_time,
        location,
        speaker_display_name,
        speaker_actual_name,
        request_id,
        grounded_person_names,
        language,
        flags,
    )
}

/// Compatibility apply entry point for synchronous callers that retain the
/// parser's actual response disposition. Raw or recovered provider output must
/// not be upgraded to a fully valid structured response merely because the
/// caller is headless or test-backed.
#[allow(clippy::too_many_arguments)]
pub fn apply_npc_dialogue_turn_with_disposition(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    speaker_id: NpcId,
    parsed: &crate::npc::NpcStreamResponse,
    parse_disposition: crate::npc::NpcResponseParseDisposition,
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
    let mut grounding = dialogue_grounding_snapshot(world, npc_manager, speaker_id);
    grounding.dialogue_obligations =
        crate::npc::derive_dialogue_obligations(player_input, &grounding.known_person_names);
    apply_npc_dialogue_turn_with_validation(
        world,
        npc_manager,
        speaker_id,
        parsed,
        parse_disposition,
        &grounding,
        crate::npc::DialogueValidationPolicy::default(),
        player_input,
        player_said_for_journal,
        game_time,
        location,
        speaker_display_name,
        speaker_actual_name,
        request_id,
        grounded_person_names,
        language,
        flags,
    )
}

/// Captures the authored facts used to validate one future dialogue result.
/// Live callers capture this before inference; trusted harness callers use the
/// compatibility wrapper above, which captures immediately before apply.
pub fn dialogue_grounding_snapshot(
    world: &WorldState,
    npc_manager: &NpcManager,
    speaker_id: NpcId,
) -> crate::npc::DialogueGroundingSnapshot {
    let current_date = world.clock.now().date_naive();
    let speaker = npc_manager.get(speaker_id);
    let mut known_person_names: Vec<String> =
        npc_manager.all_npcs().map(|npc| npc.name.clone()).collect();
    known_person_names.sort();
    if let Some(player_name) = world.player_name.as_ref()
        && !known_person_names.iter().any(|known| known == player_name)
    {
        known_person_names.push(player_name.clone());
    }
    let mut roster_names_occupations: Vec<(String, String)> = npc_manager
        .all_npcs()
        .map(|npc| (npc.name.clone(), npc.occupation.clone()))
        .collect();
    roster_names_occupations.sort();
    let mut known_location_names: Vec<String> = world
        .graph
        .location_ids()
        .into_iter()
        .filter_map(|id| world.graph.get(id).map(|location| location.name.clone()))
        .collect();
    known_location_names.sort();
    let mut work_roster: Vec<(NpcId, String, String, Option<String>)> = npc_manager
        .all_npcs()
        .map(|person| {
            let workplace = person
                .workplace
                .and_then(|id| world.graph.get(id))
                .map(|location| location.name.clone());
            (
                person.id,
                person.name.clone(),
                person.occupation.clone(),
                workplace,
            )
        })
        .collect();
    work_roster.sort_by_key(|(id, _, _, _)| id.0);
    let person_facts: Vec<crate::npc::GroundedPersonFact> = npc_manager
        .all_npcs()
        .map(|person| crate::npc::GroundedPersonFact {
            name: person.name.clone(),
            occupation: person.occupation.clone(),
            workplace: person
                .workplace
                .and_then(|id| world.graph.get(id))
                .map(|location| location.name.clone()),
            current_location: world
                .graph
                .get(person.location())
                .map(|location| location.name.clone()),
        })
        .collect();
    let location_facts: Vec<crate::npc::GroundedLocationFact> = world
        .graph
        .location_ids()
        .into_iter()
        .filter_map(|id| world.graph.get(id))
        .map(|location| {
            let mut nearby_locations: Vec<String> = location
                .connections
                .iter()
                .filter_map(|connection| world.graph.get(connection.target))
                .map(|nearby| nearby.name.clone())
                .collect();
            if let Some(anchor) = location
                .relative_to
                .as_ref()
                .and_then(|relative| world.graph.get(relative.anchor))
                .map(|anchor| anchor.name.clone())
                && !nearby_locations.contains(&anchor)
            {
                nearby_locations.push(anchor);
            }
            nearby_locations.sort();
            crate::npc::GroundedLocationFact {
                name: location.name.clone(),
                nearby_locations,
                landmarks: location.landmarks.clone(),
            }
        })
        .collect();
    let prior_player_inputs = world
        .conversation_log
        .recent_at(
            world.player_location,
            crate::npc::conversation::ConversationLog::capacity(),
        )
        .into_iter()
        .filter(|exchange| exchange.speaker_id == speaker_id)
        .map(|exchange| exchange.player_input.clone())
        .collect();
    let mut referent_context = crate::npc::DialogueReferentContext::default();
    for input in world
        .conversation_log
        .recent_at(
            world.player_location,
            crate::npc::conversation::ConversationLog::capacity(),
        )
        .into_iter()
        .map(|exchange| exchange.player_input.as_str())
    {
        referent_context.observe_player_input(
            input,
            &known_person_names,
            &known_location_names,
            world.player_name.as_deref(),
        );
    }
    crate::npc::DialogueGroundingSnapshot {
        speaker_name: speaker.map(|npc| npc.name.clone()).unwrap_or_default(),
        speaker_context: speaker.map(|npc| crate::npc::DialogueSpeakerContext {
            name: npc.name.clone(),
            occupation: npc.occupation.clone(),
            mood: npc.mood.clone(),
        }),
        canonical_mood: speaker.map(|npc| npc.mood.clone()).unwrap_or_default(),
        had_prior_exchange: world.conversation_log.has_exchange_with(speaker_id),
        time_of_day: world.clock.time_of_day(),
        known_person_names,
        roster_names_occupations,
        current_location_name: world
            .graph
            .get(world.player_location)
            .map(|location| location.name.clone())
            .unwrap_or_default(),
        known_location_names,
        player_name: world.player_name.clone(),
        work_roster: work_roster
            .into_iter()
            .map(
                |(_, name, occupation, workplace)| crate::npc::GroundedWorkFact {
                    name,
                    occupation,
                    workplace,
                },
            )
            .collect(),
        relationship_tone_hints: npc_manager.relationship_tone_hints(speaker_id),
        prior_player_inputs,
        forbidden_output_terms: world
            .dialogue_anachronisms
            .iter()
            .map(|entry| entry.term.clone())
            .collect(),
        prior_openers: Vec::new(),
        current_festival: world
            .clock
            .check_festival()
            .map(|festival| festival.to_string()),
        current_weekday: parish_types::time::weekday_name(current_date.weekday()).to_string(),
        current_day_type: parish_types::DayType::from_date(current_date),
        active_session: world
            .active_session
            .as_ref()
            .filter(|session| {
                session.date == current_date && session.location == world.player_location
            })
            .cloned(),
        remembered_objects: world
            .conversation_log
            .remembered_object_facts(speaker_id, world.player_location)
            .into_iter()
            .cloned()
            .collect(),
        person_facts,
        location_facts,
        referent_context,
        dialogue_obligations: Vec::new(),
    }
}

/// Applies one untrusted Tier-1 candidate after the single canonical validation
/// pass. Rejected candidate text and metadata are replaced before any state,
/// memory, event, or UI-facing outcome is produced.
#[allow(clippy::too_many_arguments)]
pub fn apply_npc_dialogue_turn_with_validation(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    speaker_id: NpcId,
    parsed: &crate::npc::NpcStreamResponse,
    parse_disposition: crate::npc::NpcResponseParseDisposition,
    grounding: &crate::npc::DialogueGroundingSnapshot,
    validation_policy: crate::npc::DialogueValidationPolicy,
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
    let npc_cfg = crate::config::NpcConfig::default();

    // Record whether the configured display constraint is material against the
    // complete accepted candidate, before semantic guards can shorten it. The
    // final cap still runs below, after all semantic validation. This preserves
    // the safe validation order while making quality telemetry independent of
    // overlap with the verbosity guard (#1834).
    let candidate_requires_display_cap = cap_dialogue_for_display_with_trim(
        &parsed.dialogue,
        npc_cfg.dialogue_display_max_chars,
        npc_cfg.dialogue_sentence_boundary_trim,
    )
    .as_ref()
        != parsed.dialogue;

    let validation = crate::npc::validate_dialogue_candidate(
        parsed,
        parse_disposition,
        player_input,
        grounding,
        validation_policy,
        speaker_id.0 as u64 ^ game_time.timestamp() as u64,
    );
    let accepted_candidate = validation.accepted;
    let mut canonical_response = validation.response;
    let mut guard_reasons = validation.guard_reasons;
    if !accepted_candidate {
        return DialogueTurnOutcome {
            accepted_candidate: false,
            debug_events,
            display_text: String::new(),
            guard_reasons,
            language_hints: Vec::new(),
            assigned_task: None,
            action: None,
        };
    }

    // Learn the player's name only after the candidate has passed the single
    // canonical validator, so rejection has zero canonical side effects.
    detect_and_record_player_name(world, npc_manager, player_input, speaker_id);

    // Complete the canonical text outcome before any memory, mood, task,
    // identity, event, or UI effect. This repetition guard needs the live
    // conversation log, so it belongs at the apply boundary alongside the
    // snapshot-only validator rather than in a runtime caller (#1228, #1834).
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
    let repetition_seed = speaker_id.0 as u64 ^ (game_time.timestamp() as u64);
    let deduped_dialogue = if accepted_candidate {
        crate::npc::guard_against_repetition(
            &canonical_response.dialogue,
            previous_line.as_deref(),
            npc_cfg.dialogue_repetition_threshold,
            repetition_seed,
            grounded_person_names,
        )
    } else {
        canonical_response.dialogue.clone()
    };
    if deduped_dialogue != canonical_response.dialogue {
        canonical_response.dialogue = deduped_dialogue;
        guard_reasons.push("canonical_repetition_guard".to_string());
    }

    let capped_dialogue = cap_dialogue_for_display_with_trim(
        &canonical_response.dialogue,
        npc_cfg.dialogue_display_max_chars,
        npc_cfg.dialogue_sentence_boundary_trim,
    )
    .into_owned();
    let final_dialogue_was_capped = capped_dialogue != canonical_response.dialogue;
    if final_dialogue_was_capped {
        canonical_response.dialogue = capped_dialogue;
    }
    if accepted_candidate && (candidate_requires_display_cap || final_dialogue_was_capped) {
        guard_reasons.push("display_cap".to_string());
    }

    // Recheck after repetition and display transforms. The final player-visible
    // text, not merely the originally accepted model candidate, must fulfill
    // every explicit current-turn facet before any metadata or state effect.
    if !crate::npc::dialogue_fulfills_obligations(
        &canonical_response.dialogue,
        &grounding.dialogue_obligations,
        player_input,
        &grounding.work_roster,
    ) {
        canonical_response = crate::npc::NpcStreamResponse {
            dialogue: crate::npc::dialogue_obligation_fallback(
                &grounding.dialogue_obligations,
                player_input,
                &grounding.work_roster,
            ),
            metadata: None,
        };
        if !guard_reasons
            .iter()
            .any(|reason| reason == "dialogue_obligation_guard")
        {
            guard_reasons.push("dialogue_obligation_guard".to_string());
        }
    }

    // Player-authored object attributes are durable conversation truth. They
    // are derived exclusively from the input (never candidate text/metadata)
    // and recorded only at this canonical turn boundary, so later turns and
    // save/load preserve material/colour continuity without granting model
    // prose authority (#1871).
    if let Some(fact) =
        crate::npc::extract_remembered_object_fact(player_input, speaker_id, location)
    {
        world.conversation_log.remember_object_fact(fact);
    }

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

    let capped_dialogue = &canonical_response.dialogue;
    let language_hints = canonical_response
        .metadata
        .as_ref()
        .map(|metadata| {
            crate::npc::validate_language_hints(&metadata.language_hints, capped_dialogue, language)
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
                let grounding_clause = grounded_task_assignment_clause(proposal, capped_dialogue)?;
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
    // establishes the speaker's grounded identity (#1776/#1842). The immutable
    // pre-inference snapshot supplies the authored occupation and full roster,
    // so a unique first-name claim cannot be validated against model metadata
    // or mutable post-generation state. Running after every text transform
    // prevents a removed claim from leaking through notebook/card state.
    let speaker_occupation = grounding
        .speaker_context
        .as_ref()
        .map(|speaker| speaker.occupation.as_str())
        .unwrap_or_default();
    if !npc_manager.is_introduced(speaker_id)
        && crate::npc::dialogue_self_identifies_speaker(
            capped_dialogue,
            speaker_actual_name,
            speaker_occupation,
            &grounding.roster_names_occupations,
        )
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
        capped_dialogue,
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

    let action = canonical_response
        .metadata
        .as_ref()
        .map(|metadata| metadata.action.trim())
        .filter(|action| !action.is_empty())
        .map(str::to_string);

    DialogueTurnOutcome {
        accepted_candidate: true,
        debug_events,
        display_text: capped_dialogue.to_string(),
        guard_reasons,
        language_hints,
        assigned_task,
        action,
    }
}
