//! Portable canonical NPC dialogue grounding and application.
//!
//! This module contains the state-independent grounding snapshot and the single
//! validated apply boundary shared by desktop and embedded runtimes. It does not
//! depend on the IPC layer, inference clients, or desktop orchestration.

use std::collections::{BTreeSet, HashSet};

use chrono::Datelike;

use crate::config::FeatureFlags;
use crate::npc::manager::NpcManager;
use crate::npc::{LanguageSettings, NpcId};
use crate::world::{LocationId, WorldState};

/// Detects if the player is introducing themselves and records the name.
///
/// Call this before `prepare_npc_conversation_turn` so the NPC prompt can
/// use the player's name. If detected, sets `world.player_name` (if not
/// already set) and teaches the speaking NPC the player's name.
pub fn detect_and_record_player_name(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    player_input: &str,
    speaker_id: NpcId,
) {
    if let Some(name) = crate::npc::detect_player_name(player_input) {
        // Don't overwrite a previously set player name
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

/// Outcome of [`apply_npc_dialogue_turn`].
///
/// Carries the debug-event strings produced by the shared per-turn pipeline
/// (steps 2 and 4) plus `display_text`, the player-visible dialogue. The model's
/// reply is committed exactly as written: `display_text` is the accepted
/// candidate's dialogue, unchanged. Every backend renders `display_text`, so
/// what the player sees matches what was stored in the conversation log and the
/// `DialogueOccurred` event.
#[derive(Debug, Clone)]
pub struct DialogueTurnOutcome {
    /// True only when the candidate met the structural response contract
    /// (complete JSON, non-empty dialogue). A false outcome has no state,
    /// memory, event, or display effects.
    pub accepted_candidate: bool,
    /// Debug-event strings from Tier-1 apply (step 2) and witness memories
    /// (step 4). The live loop discards these; headless + harness forward them.
    pub debug_events: Vec<String>,
    /// The dialogue to show the player: the accepted reply, as the model wrote
    /// it. Matches exactly what was written to the conversation log and the
    /// `DialogueOccurred` event. Empty when the candidate was rejected.
    pub display_text: String,
    /// Secondary-language hints validated against `display_text` and the active
    /// setting's curated native-language inventory (#1789).
    pub language_hints: Vec<crate::npc::LanguageHint>,
    /// Canonical task post-state when the delivered dialogue assigned a task.
    ///
    /// Callers persist this exact record before acknowledging the player turn;
    /// replay never re-runs model-output interpretation.
    pub assigned_task: Option<limerick_types::PlayerTask>,
    /// Canonical physical action from accepted metadata. Rejected response
    /// metadata is discarded before this value is derived.
    pub action: Option<String>,
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

/// Applies a parsed NPC dialogue response: the per-turn cross-cutting steps
/// every backend performs identically after a Tier-1 reply (#1172 / #1173).
///
/// Before this existed, four code paths (live `game_loop::npc_turn`, headless
/// `apply_npc_response`, and the script harness's `consume_canned_npc_response`
/// and `handle_npc_interaction_for`) each reimplemented a *different subset* of
/// these steps, so behaviour silently drifted (#1028, #1035, #1077/#1079). This
/// is the single definition; all four call it.
///
/// The model's reply is committed as written. The only check on the reply is
/// the structural response contract (`dialogue_candidate_is_complete`): a
/// candidate that is not complete JSON with non-empty dialogue fails the turn
/// with no effects. Nothing inspects, rewrites, or replaces what the model said.
///
/// The steps, in order:
/// 1. **Name detection**: `detect_and_record_player_name`, so a
///    self-introduction in `player_input` teaches the addressed speaker before
///    memory is recorded.
/// 2. **Tier-1 state update**: `apply_tier1_response_with_config` on the
///    speaker (mood, memory, language drift).
/// 3. **Conversation-exchange record**: appended to `world.conversation_log`,
///    which feeds the "What's been said here" prompt block
///    (`ticks::conversation_block`).
/// 4. **Witness memories**: co-located bystanders record an "Overheard" memory.
/// 5. **`DialogueOccurred` publish**: on `world.event_bus`, so the
///    character-log, location-log and chat-transcript subscribers record a
///    verbatim journal entry.
///
/// Operates on plain `&mut` borrows (no runtime I/O), so it needs no
/// `EventEmitter`: the only event it raises goes to the in-process `event_bus`,
/// not the UI emitter. Returns the debug-event strings produced by steps 2 and 4
/// so the caller can forward them to its own debug sink; the headless CLI and
/// the harness do, the live loop discards them (`let _ = ...`).
///
/// `player_input` is the raw player utterance used for name detection, memory,
/// witness records and the conversation log. `player_said_for_journal` is the
/// (possibly verb-stripped) line stored as `DialogueOccurred::player_said`; pass
/// the same value as `player_input` unless the caller cleans a leading verb.
///
/// Returns a [`DialogueTurnOutcome`] carrying the debug-event strings produced by
/// steps 2 and 4 **and** the player-visible `display_text`. Callers must show
/// `display_text` to the player (conversation line, `ActionResult`, headless
/// stdout) so what the player sees matches what was stored in the conversation
/// log and the `DialogueOccurred` event.
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
        language,
        flags,
    )
}

/// Captures the authored facts rendered into one NPC dialogue prompt.
///
/// These are prompt inputs only. Nothing compares a model reply against them.
pub fn dialogue_facts(world: &WorldState, speaker_id: NpcId) -> crate::npc::DialogueFacts {
    let current_date = world.clock.now().date_naive();
    // The graph stores locations in a HashMap; sort so the prompt's fact
    // lines keep one order across runs.
    let mut location_ids = world.graph.location_ids();
    location_ids.sort();
    let landmarks: Vec<crate::npc::LocationLandmarks> = location_ids
        .into_iter()
        .filter_map(|id| world.graph.get(id))
        .filter(|location| !location.landmarks.is_empty())
        .map(|location| crate::npc::LocationLandmarks {
            name: location.name.clone(),
            landmarks: location.landmarks.clone(),
        })
        .collect();
    crate::npc::DialogueFacts {
        weekday: limerick_types::time::weekday_name(current_date.weekday()).to_string(),
        day_type: limerick_types::DayType::from_date(current_date),
        landmarks,
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
    }
}

/// [`apply_npc_dialogue_turn`] for callers that retain the parser's actual
/// response disposition. Raw or recovered provider output must not be upgraded
/// to a complete structured response merely because the caller is headless or
/// test-backed: the structural contract rejects it here.
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
    language: &LanguageSettings,
    flags: &FeatureFlags,
) -> DialogueTurnOutcome {
    let mut debug_events = Vec::new();

    // The structural response contract is the one check on the reply. A
    // failed candidate has zero canonical side effects.
    if !crate::npc::dialogue_candidate_is_complete(parsed, parse_disposition) {
        return DialogueTurnOutcome {
            accepted_candidate: false,
            debug_events,
            display_text: String::new(),
            language_hints: Vec::new(),
            assigned_task: None,
            action: None,
        };
    }

    // Learn the player's name only after the candidate has met the structural
    // contract, so rejection has zero canonical side effects.
    detect_and_record_player_name(world, npc_manager, player_input, speaker_id);

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
            parsed,
            player_input,
            game_time,
            &Default::default(),
            player_name_for_mem.as_deref(),
        ));
    }

    let delivered_dialogue = parsed.dialogue.as_str();
    let language_hints = parsed
        .metadata
        .as_ref()
        .map(|metadata| {
            crate::npc::validate_language_hints(
                &metadata.language_hints,
                delivered_dialogue,
                language,
            )
        })
        .unwrap_or_default();
    let assigned_task = if flags.is_disabled(PLAYER_TASK_PROGRESSION_FLAG)
        || !is_task_request_input(player_input)
    {
        None
    } else {
        parsed
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.assigned_task.as_deref())
            .and_then(|proposal| {
                let grounding_clause =
                    grounded_task_assignment_clause(proposal, delivered_dialogue)?;
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
                let existing_ids: HashSet<limerick_types::PlayerTaskId> = world
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

    // Identity becomes known only when the delivered line explicitly
    // establishes the speaker's grounded identity (#1776/#1842). The authored
    // occupation and full roster come from the NPC manager, so a unique
    // first-name claim is never validated against model metadata. This is
    // authoritative state (it reveals the NPC's name in the notebook and UI),
    // derived from the delivered line and authored data only.
    let speaker_occupation = npc_manager
        .get(speaker_id)
        .map(|speaker| speaker.occupation.clone())
        .unwrap_or_default();
    let roster_names_occupations: Vec<(String, String)> = npc_manager
        .all_npcs()
        .map(|person| (person.name.clone(), person.occupation.clone()))
        .collect();
    if !npc_manager.is_introduced(speaker_id)
        && crate::npc::dialogue_self_identifies_speaker(
            delivered_dialogue,
            speaker_actual_name,
            &speaker_occupation,
            &roster_names_occupations,
        )
    {
        npc_manager.mark_introduced(speaker_id);
        debug_events.push(format!(
            "{} introduced themselves to the player",
            speaker_actual_name
        ));
    }

    // 3. Record the conversation exchange for scene awareness, with the
    //    NPCs present to hear it: later dialogue snapshots send each speaker
    //    only the exchanges they witnessed.
    let mut witnesses: BTreeSet<NpcId> = npc_manager.npcs_at_ids(location).into_iter().collect();
    witnesses.insert(speaker_id);
    world
        .conversation_log
        .add(crate::npc::conversation::ConversationExchange {
            timestamp: game_time,
            speaker_id,
            speaker_name: speaker_actual_name.to_string(),
            player_input: player_input.to_string(),
            npc_dialogue: delivered_dialogue.to_string(),
            location,
            witnesses,
        });

    // 4. Record witness memories for co-located bystanders.
    debug_events.extend(crate::npc::ticks::record_witness_memories(
        npc_manager.npcs_mut(),
        speaker_id,
        speaker_display_name,
        player_input,
        delivered_dialogue,
        game_time,
        location,
    ));

    // 5. Publish the full-text dialogue event. Emit even when the dialogue is
    //    empty so journal entries line up with the player's prompt, but skip
    //    when both sides are empty (no useful record) — matches the live loop's
    //    original guard.
    if !player_said_for_journal.trim().is_empty() || !delivered_dialogue.trim().is_empty() {
        world
            .event_bus
            .publish(limerick_types::events::GameEvent::DialogueOccurred {
                npc_id: speaker_id,
                location,
                summary: delivered_dialogue.to_string(),
                player_said: Some(player_said_for_journal.to_string()),
                npc_said: Some(delivered_dialogue.to_string()),
                request_id,
                timestamp: game_time,
            });
    }
    if let Some(task) = assigned_task.as_ref() {
        world
            .event_bus
            .publish(limerick_types::GameEvent::PlayerTaskAssigned {
                timestamp: task.assigned_at,
                task: task.clone(),
            });
    }

    let action = parsed
        .metadata
        .as_ref()
        .map(|metadata| metadata.action.trim())
        .filter(|action| !action.is_empty())
        .map(str::to_string);

    DialogueTurnOutcome {
        accepted_candidate: true,
        debug_events,
        display_text: delivered_dialogue.to_string(),
        language_hints,
        assigned_task,
        action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
