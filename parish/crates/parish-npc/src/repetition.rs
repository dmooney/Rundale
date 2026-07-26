//! Deterministic anti-repetition guard (#1228) and post-generation dialogue
//! guards (#1459, #1460, #1472) for degenerate model output.

// ── #1228 — anti-repetition guard ──────────────────────────────────────────────
//
// Small / quantized models (e.g. the local MLX Qwen2.5-14B-4bit in #1228)
// occasionally enter a degenerate sampling loop and emit the same clause dozens
// of times inside one reply, or echo their own previous line near-verbatim on
// the next turn. The #1224 length cap clips the *tail* but the surviving prefix
// is still a wall of duplicate clauses, and it does nothing across turns. These
// helpers are the deterministic, model-agnostic backstop: they need no live
// inference and run identically for every provider.

use std::sync::OnceLock;

use crate::types::RelationshipKind;
use parish_types::time::TimeOfDay;

/// Maximum number of spoken sentences in one NPC reply.
///
/// The Tier-1 prompt interpolates this same value so the generation contract
/// and deterministic post-generation guard cannot drift apart.
pub(crate) const MAX_DIALOGUE_SENTENCES: usize = 3;

/// Normalizes a dialogue line for repetition comparison.
///
/// Lower-cases, collapses internal whitespace to single spaces, and trims
/// surrounding whitespace and trailing sentence punctuation. Two lines that
/// differ only in case, spacing, or trailing `.`/`!`/`?`/`…` normalize equal.
fn normalize_for_repetition(s: &str) -> String {
    let lowered = s.to_lowercase();
    let collapsed = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '.' | '!' | '?' | '…' | ',' | ';'))
        .to_string()
}

fn text_seed(s: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in s.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Returns whether `period_index` terminates a known abbreviated honorific
/// followed by an uppercase name token.
///
/// `HONORIFIC_PAIRS` is the shared source of truth for abbreviations. The
/// uppercase look-ahead keeps ordinary lowercase prose such as `"fr. declan"`
/// on the normal sentence-boundary path instead of treating every known token
/// as an unconditional exception.
fn period_belongs_to_honorific(s: &str, period_index: usize) -> bool {
    let before = &s[..period_index];
    let token_start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_alphabetic())
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    let token = &before[token_start..];
    let is_abbreviated_honorific = HONORIFIC_PAIRS
        .iter()
        .any(|&(_, short)| token.eq_ignore_ascii_case(short));

    is_abbreviated_honorific
        && s[period_index + '.'.len_utf8()..]
            .chars()
            .find(|ch| !ch.is_whitespace())
            .is_some_and(char::is_uppercase)
}

/// Splits a dialogue body into sentence-ish units on terminal punctuation,
/// keeping the delimiter with each piece. Periods in known abbreviated
/// honorifics are not terminal when the next non-space character begins an
/// uppercase name. Used by [`collapse_repeated_sentences`].
fn split_sentences(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, ch) in s.char_indices() {
        let is_boundary = match ch {
            '.' => !period_belongs_to_honorific(s, index),
            '!' | '?' | '…' => true,
            _ => false,
        };
        if is_boundary {
            let end = index + ch.len_utf8();
            out.push(s[start..end].to_string());
            start = end;
        }
    }
    if !s[start..].trim().is_empty() {
        out.push(s[start..].to_string());
    }
    out
}

/// Collapses consecutive duplicate sentences/clauses within a single dialogue
/// string (#1228, intra-line repetition).
///
/// When a sentence normalizes equal to the one immediately before it, the
/// duplicate is dropped. A run of N identical clauses collapses to a single
/// instance. Non-repeating dialogue is returned unchanged (modulo whitespace
/// re-joining). This is the primary defense against the degenerate
/// "Speak yer mind, and we'll see what be in it, m'friend." loop in #1228.
pub fn collapse_repeated_sentences(dialogue: &str) -> String {
    let sentences = split_sentences(dialogue);
    if sentences.len() < 2 {
        return dialogue.to_string();
    }
    let mut kept: Vec<&str> = Vec::with_capacity(sentences.len());
    let mut last_norm: Option<String> = None;
    for sentence in &sentences {
        let norm = normalize_for_repetition(sentence);
        // Skip empty fragments and consecutive duplicates.
        if norm.is_empty() {
            continue;
        }
        if last_norm.as_deref() == Some(norm.as_str()) {
            continue;
        }
        kept.push(sentence.trim());
        last_norm = Some(norm);
    }
    if kept.is_empty() {
        return dialogue.trim().to_string();
    }
    kept.join(" ")
}

/// Word-level Jaccard similarity of two normalized dialogue lines, in `[0.0, 1.0]`.
///
/// `1.0` means the two lines share the exact same set of words (ignoring order,
/// case, spacing, and trailing punctuation); `0.0` means no shared words. Used as
/// the deterministic near-identity signal for the cross-turn guard.
fn dialogue_similarity(a: &str, b: &str) -> f32 {
    let norm_a = normalize_for_repetition(a);
    let norm_b = normalize_for_repetition(b);
    if norm_a.is_empty() && norm_b.is_empty() {
        return 1.0;
    }
    let set_a: std::collections::HashSet<&str> =
        norm_a.split(' ').filter(|w| !w.is_empty()).collect();
    let set_b: std::collections::HashSet<&str> =
        norm_b.split(' ').filter(|w| !w.is_empty()).collect();
    if set_a.is_empty() || set_b.is_empty() {
        return if norm_a == norm_b { 1.0 } else { 0.0 };
    }
    let intersection = set_a.intersection(&set_b).count() as f32;
    let union = set_a.union(&set_b).count() as f32;
    intersection / union
}

/// Reports whether two dialogue lines are near-identical (#1228, cross-turn).
///
/// `threshold` is the minimum word-level Jaccard similarity (see
/// [`dialogue_similarity`]) at or above which two lines count as near-identical.
/// Exact normalized equality always counts. A threshold of `1.0` requires the
/// same word set; lower thresholds catch lines that merely reshuffle/echo the
/// same content.
pub fn is_near_identical(a: &str, b: &str, threshold: f32) -> bool {
    if normalize_for_repetition(a) == normalize_for_repetition(b) {
        return true;
    }
    dialogue_similarity(a, b) >= threshold
}

/// Deterministic, varied fallback line used when an NPC would otherwise repeat
/// its own previous line verbatim (#1228).
///
/// Picks from a small pool keyed by `seed` so the substituted line is stable for
/// a given turn but varies across NPCs/turns. Period-neutral and content-free, so
/// it is safe for any mod and any time of day. These are intentionally short — a
/// brief acknowledgement reads better than re-emitting a degenerate wall of text.
pub fn varied_repetition_fallback(seed: u64) -> &'static str {
    const POOL: [&str; 6] = [
        "Aye, as I said.",
        "Sure, ye have the right of it.",
        "Mm. There's little more to add to that.",
        "Well now, I'll not say the same twice.",
        "Aye, that's the way of it.",
        "I've said my piece on that, so.",
    ];
    POOL[(seed as usize) % POOL.len()]
}

/// Removes duplicate occurrences of known farewell tokens within a single
/// reply (#1387: double-farewell regression on Qwen2.5-14B).
///
/// Farewell phrases like "Slán abhaile" or "Slán leat" can appear twice in
/// one reply when they are separated by intervening text —
/// `collapse_repeated_sentences` only removes *consecutive* duplicates, so
/// "...Slán abhaile to ye. Safe journey. Slán abhaile" passes through
/// without modification. This function strips every occurrence after the first
/// for each known farewell token.
///
/// Matching is case-insensitive.
pub fn dedup_farewell_tokens(dialogue: &str) -> String {
    // The tokens from ALLOWED_FAREWELL_PHRASES that are worth deduplicating.
    // "Slán leat" and "Slán abhaile" are the ones observed doubling; the
    // English phrases are already handled by the general sentence-collapse.
    const FAREWELL_TOKENS: &[&str] = &[
        "Slán abhaile",
        "Slán leat",
        "sláinte",
        "Go raibh maith agat",
        "Céad míle fáilte",
        "slán abhaile",
        "slán leat",
    ];

    let mut result = dialogue.to_string();
    for token in FAREWELL_TOKENS {
        let lower_token = token.to_lowercase();
        let lower_result = result.to_lowercase();
        // Find first occurrence
        if let Some(first_pos) = lower_result.find(&lower_token) {
            // Find second occurrence
            if let Some(rel_pos) = lower_result[first_pos + token.len()..].find(&lower_token) {
                let second_pos = first_pos + token.len() + rel_pos;
                // Remove the second occurrence (and any leading punctuation/space before it).
                // Walk back to eat a preceding comma, period, or space.
                let trim_start = {
                    let bytes = result.as_bytes();
                    let mut start = second_pos;
                    while start > 0 && matches!(bytes[start - 1], b' ' | b'.' | b',' | b';' | b'!')
                    {
                        start -= 1;
                    }
                    // Leave one space if we ate back into the preceding sentence.
                    if start < second_pos && start > 0 {
                        start + 1
                    } else {
                        start
                    }
                };
                result = format!(
                    "{}{}",
                    result[..trim_start].trim_end(),
                    // Preserve anything after the second token.
                    &result[second_pos + token.len()..]
                )
                .trim()
                .to_string();
            }
        }
    }
    result
}

/// Splits `dialogue` into its opener (first sentence) and the remainder in a
/// single pass, avoiding the double [`split_sentences`] call that `extract_opener`
/// and `remainder_after_opener` previously incurred when called together on the
/// hot dialogue path.
///
/// Returns `(normalized_opener, remainder)` where:
/// - `normalized_opener` is the first sentence lowercased, whitespace-collapsed,
///   and trailing-punctuation-stripped (identical to what [`extract_opener`] used
///   to return independently).
/// - `remainder` is everything after the first sentence, leading whitespace
///   trimmed (empty `&str` when there is no second sentence).
fn split_opener_remainder(dialogue: &str) -> (String, &str) {
    let sentences = split_sentences(dialogue);
    // A leading title abbreviation is not a standalone opener sentence.
    // Treat "Fr. Declan …" (and equivalent common titles) as one unit so the
    // cross-turn opener deduper cannot strip only the honorific (#1228).
    let opener_sentence_count = sentences
        .first()
        .map(|first| normalize_for_repetition(first))
        .filter(|first| {
            matches!(
                first.as_str(),
                "fr" | "mr" | "mrs" | "ms" | "dr" | "rev" | "sr" | "st"
            )
        })
        .map_or(1, |_| 2)
        .min(sentences.len());
    let opener_len: usize = sentences
        .iter()
        .take(opener_sentence_count)
        .map(String::len)
        .sum();
    let normalized = match opener_len {
        0 => normalize_for_repetition(dialogue),
        len => normalize_for_repetition(&dialogue[..len]),
    };
    let remainder = if sentences.len() <= opener_sentence_count {
        ""
    } else {
        dialogue[opener_len..].trim_start()
    };
    (normalized, remainder)
}

/// Cross-NPC opener de-duplication guard (#1422).
///
/// When multiple NPCs reply in a single multi-NPC turn, small models often open
/// with the same stock phrase (e.g. "Ye've come to the right place …") because
/// they share the same system-prompt template. This function prevents the
/// duplication from reaching the player by stripping the repeated opener
/// sentence from `reply` when it matches one already used in this turn.
///
/// # Parameters
///
/// - `prior_openers`: normalized opener strings already emitted by earlier NPCs
///   in this turn (see [`extract_normalized_opener`]). Pass `&[]` on the first
///   NPC — the function returns `reply` unchanged.
/// - `reply`: the freshly post-processed dialogue for the current NPC.
///
/// # Behaviour
///
/// 1. Extracts and normalizes the opener of `reply` (first sentence, lowercased,
///    punctuation-stripped, ~6 words).
/// 2. Computes word-level Jaccard similarity against every prior opener.
/// 3. If any prior opener is near-identical (≥ 0.75 Jaccard, or normalized-equal),
///    the opener sentence is dropped from `reply`; the substantive remainder is
///    returned. If the remainder is empty after dropping (the reply WAS only the
///    stock opener), the original reply is returned unchanged — dropping it
///    entirely would produce a blank line, which is worse.
/// 4. If no collision is detected, `reply` is returned unchanged.
///
/// This function is pure, deterministic, and model-agnostic — it requires no
/// inference and runs identically for every provider.
pub fn dedupe_cross_npc_openers(prior_openers: &[String], reply: &str) -> String {
    if prior_openers.is_empty() || reply.trim().is_empty() {
        return reply.to_string();
    }
    // Single split: derive both the normalized opener and the remainder in one
    // pass instead of calling split_sentences twice (#1422 review).
    let (my_opener, rest) = split_opener_remainder(reply);
    if my_opener.is_empty() {
        return reply.to_string();
    }
    // `prior_openers` entries are already normalized at insertion
    // (via `extract_normalized_opener` → `normalize_for_repetition`), so
    // comparing `prev` directly against `my_opener` is correct and avoids a
    // redundant normalize call (#1422 review).
    let is_duplicate = prior_openers
        .iter()
        .any(|prev| prev.as_str() == my_opener || dialogue_similarity(&my_opener, prev) >= 0.75);
    if !is_duplicate {
        return reply.to_string();
    }
    if rest.is_empty() {
        // The entire reply is the duplicated opener — returning blank would be
        // worse, so keep the original.
        return reply.to_string();
    }
    rest.to_string()
}

/// Returns the normalized opener of `reply` for collection into the
/// `prior_openers` list used by [`dedupe_cross_npc_openers`].
///
/// Call this **after** any other post-processing guards so the stored opener
/// reflects what was actually shown to the player.
pub fn extract_normalized_opener(reply: &str) -> String {
    split_opener_remainder(reply).0
}

/// Applies the full anti-repetition guard to a freshly generated NPC line
/// (#1228, #1387). This is the single entry point the shared dialogue path calls.
///
/// Steps, in order:
/// 1. Collapse consecutive duplicate clauses *within* the new line
///    ([`collapse_repeated_sentences`]).
/// 2. Remove duplicate farewell tokens that are not consecutive
///    ([`dedup_farewell_tokens`], #1387).
/// 3. If the collapsed line is near-identical to the NPC's own `previous_line`
///    ([`is_near_identical`] at `threshold`), and the overlap is NOT explained
///    by a shared grounded full person name from `grounded_person_names`,
///    substitute a deterministic varied fallback ([`varied_repetition_fallback`]
///    keyed by `seed`).
///
/// `previous_line` is `None` when the NPC has no prior line at this location.
/// A `threshold` of `0.0` disables the cross-turn check (intra-line collapse
/// still runs, since runaway loops are always undesirable).
///
/// `grounded_person_names` is the NPC's known-roster list (e.g.
/// `setup.known_person_names`). Pass `&[]` to disable the grounded-name bypass.
/// When any full name (≥2 tokens, e.g. "Una Malone") from this list appears
/// (case-insensitive substring) in BOTH `new_line` AND `previous_line`, the high
/// word-similarity is explained by topic continuity — the guard does NOT
/// substitute a fallback. Only full names are considered: single-token first
/// names are too common to serve as a conclusive grounding anchor.
pub fn guard_against_repetition(
    new_line: &str,
    previous_line: Option<&str>,
    threshold: f32,
    seed: u64,
    grounded_person_names: &[String],
) -> String {
    let collapsed = collapse_repeated_sentences(new_line);
    let deduped = dedup_farewell_tokens(&collapsed);
    if threshold <= 0.0 {
        return deduped;
    }
    if !deduped.trim().is_empty()
        && previous_line
            .filter(|prev| !prev.trim().is_empty())
            .map(|prev| is_near_identical(&deduped, prev, threshold))
            .unwrap_or(false)
    {
        // Skip the fallback substitution when the high similarity between new_line and
        // previous_line is explained by both lines discussing the same GROUNDED person
        // (i.e. a real roster member whose full name appears in both lines).  Without
        // this check, a correct follow-up about an in-roster person triggers a false
        // positive: both lines legitimately share words because they're both on-topic.
        // Only full names (≥2 tokens, e.g. "Una Malone") are used as the grounding
        // anchor — single-token first names are too common to be conclusive.
        let grounded_topic_match = !grounded_person_names.is_empty()
            && grounded_person_names.iter().any(|name| {
                let tokens: Vec<&str> = name.split_whitespace().collect();
                if tokens.len() < 2 {
                    return false; // first-name-only: too ambiguous
                }
                // The full name must appear as a substring in BOTH lines.
                let name_lower = name.to_lowercase();
                let deduped_lower = deduped.to_lowercase();
                let prev_lower = previous_line.unwrap_or("").to_lowercase();
                deduped_lower.contains(&name_lower) && prev_lower.contains(&name_lower)
            });
        if grounded_topic_match {
            tracing::debug!(
                "repetition guard skipped: near-identical lines share a grounded full \
                 person name — not degenerate repetition (guard-false-positive-known-npc)"
            );
            return deduped;
        }
        return varied_repetition_fallback(seed).to_string();
    }
    deduped
}

// ── #1459 — person-confirmation guard ─────────────────────────────────────────
//
// The Qwen2.5-14B-4bit model ignores the "PEOPLE YOU KNOW" prompt directive
// when a fabricated person is embedded as a presupposition ("do you know Cormac
// Sweeney?") — it confirms the invented name and invents whereabouts. Prompt
// directives and frequency_penalty alone do not fix this. This guard runs on the
// finalized dialogue string AFTER inference: if the reply affirmatively confirms
// or locates a person whose full name does not appear in the known-roster, it
// replaces the dialogue with a stock non-recognition decline. This is a
// deterministic backstop that does not call the LLM again.
//
// Detection strategy (conservative):
//   1. Collect all consecutive-capitalised word bigrams and trigrams that look
//      like person names (First Last or First Middle Last) from the player input.
//   2. For each candidate name NOT in known_person_names (roster names), check
//      whether the dialogue AFFIRMS rather than denies it:
//      - Contains the name AND contains an affirmation marker ("aye", "he is",
//        "she is", "I know", "a good man", "at the", locative phrases, etc.)
//      - Does NOT contain a denial marker ("know no", "no such", "never heard",
//        "don't know", "do not know", "no one by that name", etc.)
//   3. If the fabricated-person affirmation pattern fires, replace the whole
//      dialogue with a period-appropriate non-recognition phrase.

/// Returns a stock non-recognition decline for an unknown named person.
/// Cycles through a small pool to avoid repeated identical responses.
fn non_recognition_decline(seed: u64) -> &'static str {
    const DECLINES: &[&str] = &[
        "That name is not known to me hereabouts.",
        "I cannot place that name among the folk here.",
        "No parish face comes to mind for that name.",
        "That is not a name I have heard in these parts.",
        "I cannot put a parish face to that name.",
    ];
    DECLINES[(seed as usize) % DECLINES.len()]
}

/// Minimal speaker context used by deterministic dialogue polish guards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueSpeakerContext {
    /// Speaker's display/roster name.
    pub name: String,
    /// Speaker's current occupation or role.
    pub occupation: String,
    /// Speaker's current mood label.
    pub mood: String,
}

impl DialogueSpeakerContext {
    fn is_shopkeeper(&self) -> bool {
        self.occupation.to_lowercase().contains("shopkeeper")
    }
}

fn shopkeeper_non_recognition_decline(seed: u64) -> &'static str {
    const DECLINES: &[&str] = &[
        "I keep a sharp account at this counter, and that is not a name I can place.",
        "No account in this shop comes to mind for that name.",
        "I know the names that cross my counter, and that one has not passed it.",
        "If that name trades in this parish, it has not crossed my counter.",
        "No order, debt, or bag of goods brings that name to mind.",
    ];
    DECLINES[(seed as usize) % DECLINES.len()]
}

fn non_recognition_decline_for_speaker(
    seed: u64,
    speaker: Option<&DialogueSpeakerContext>,
) -> &'static str {
    if speaker
        .map(DialogueSpeakerContext::is_shopkeeper)
        .unwrap_or(false)
    {
        return shopkeeper_non_recognition_decline(seed);
    }

    non_recognition_decline(seed)
}

/// Denial markers used by both the affirmation check and the pronoun-referent
/// check. If any of these appear in the dialogue the NPC is already declining —
/// the guard must not fire.
const DENIAL_MARKERS: &[&str] = &[
    "know no",
    "no such",
    "never heard",
    "don't know",
    "do not know",
    "no one by that name",
    "not known to me",
    "cannot say i've",
    "never met",
    "no knowledge of",
    "not familiar",
    "not acquainted",
    "stranger to me",
    "wrong parish",
];

/// Returns `true` when `dialogue` contains an affirmation of the given `name`
/// but no denial. The check is intentionally conservative — it only fires when
/// there is a clear locating or confirming phrase near the name.
fn dialogue_affirms_name(dialogue: &str, name: &str) -> bool {
    let lower = dialogue.to_lowercase();
    let name_lower = name.to_lowercase();

    // Must contain the name at all.
    if !lower.contains(&name_lower) {
        return false;
    }

    // Denial markers: if any are present, the NPC is already declining.
    for marker in DENIAL_MARKERS {
        if lower.contains(marker) {
            return false;
        }
    }

    // Affirmation markers: the NPC is confirming/locating the person.
    // These are phrases that collocate with a named person confirmation.
    const AFFIRMATION_MARKERS: &[&str] = &[
        // Existential / locative
        " is at ",
        " is in ",
        " is over ",
        " lives at ",
        " lives in ",
        " lives near ",
        " stays at ",
        " works at ",
        " works in ",
        " works near ",
        " does work at",
        " does work in",
        " do work at",
        " can be found",
        " you'll find",
        " you can find",
        " he's at ",
        " she's at ",
        " he is at ",
        " she is at ",
        " he works",
        " she works",
        " his shop",
        " her shop",
        // Social affirmation — general confirmation phrases
        "aye, i know",
        "aye, he",
        "aye, she",
        "oh aye,",
        "ye heard right",
        "you heard right",
        "heard right",
        "that's right",
        "that is right",
        "aye, that's",
        "aye, that is",
        "a good man",
        "a fine man",
        "a fine woman",
        "good woman",
        "i've heard of",
        "i have heard of",
        "heard the talk of",
        "heard tell of",
        "i know him",
        "i know her",
        "know him well",
        "know her well",
        "met him",
        "met her",
        "spoken with him",
        "spoken with her",
        // Possessive / role confirmation
        "he's the ",
        "she's the ",
        "he's a ",
        "she's a ",
        "he is a ",
        "she is a ",
        "he is the ",
        "she is the ",
        // General "he/she is [description]" confirmation after name appears
        "is a ",
        "is the ",
    ];
    for marker in AFFIRMATION_MARKERS {
        if lower.contains(marker) {
            return true;
        }
    }

    false
}

/// Returns `true` when `dialogue` contains a pronoun-based affirmation of an
/// absent referent (e.g. "he's off to the mill", "she's at the church") but no
/// denial. Used by the pronoun follow-up guard (#1470) to catch turns where the
/// player's input had no name — only a pronoun or continuation — but the NPC
/// confirms a previously-established fabricated referent via pronoun.
///
/// This is intentionally broader than [`dialogue_affirms_name`]: it does not
/// require the name to appear in the dialogue at all. It fires purely on the
/// presence of pronoun-locative / pronoun-confirmatory phrases in the absence of
/// denial markers.
fn dialogue_affirms_via_pronoun(dialogue: &str) -> bool {
    let lower = dialogue.to_lowercase();

    // If the NPC is already declining, don't suppress.
    for marker in DENIAL_MARKERS {
        if lower.contains(marker) {
            return false;
        }
    }

    // Pronoun affirmation markers — these indicate the NPC is confirming or
    // locating a person referenced only by pronoun or appositive ("the lad",
    // "the man"). Broader than [`dialogue_affirms_name`] affirmation markers.
    const PRONOUN_AFFIRMATION_MARKERS: &[&str] = &[
        "he's off to",
        "she's off to",
        "he is off to",
        "she is off to",
        "he's out",
        "she's out",
        "he is out",
        "she is out",
        "he's at ",
        "she's at ",
        "he is at ",
        "she is at ",
        "he's in ",
        "she's in ",
        "he is in ",
        "she is in ",
        "he's over",
        "she's over",
        "he is over",
        "she is over",
        "he'll be",
        "she'll be",
        "he will be",
        "she will be",
        "he's away",
        "she's away",
        "he is away",
        "she is away",
        "he's gone",
        "she's gone",
        "he is gone",
        "she is gone",
        "he went",
        "she went",
        "he headed",
        "she headed",
        "he's working",
        "she's working",
        "aye, he",
        "aye, she",
        "aye, he's",
        "aye, she's",
    ];
    for marker in PRONOUN_AFFIRMATION_MARKERS {
        if lower.contains(marker) {
            return true;
        }
    }

    false
}

/// Returns `true` when `name` appears as a standalone word (not as a substring
/// of a larger word) in `text`. Matching is case-insensitive. Each word in
/// `text` is stripped of surrounding non-alphabetic characters before comparison
/// so that punctuation-attached occurrences ("Al," "Al's") are still found, but
/// accidental substring matches ("Al" inside "shall" or "talk") are not.
///
/// Example: `name="Al"` matches "Aye, Al's at the mill" but NOT "Ye shall talk".
fn text_contains_name_as_word(text: &str, name: &str) -> bool {
    let name_lower = name.to_lowercase();
    for token in text.split_whitespace() {
        // Strip leading/trailing non-alphabetic chars (punctuation, quotes, etc.)
        let stripped = token.trim_matches(|c: char| !c.is_alphabetic());
        if stripped.to_lowercase() == name_lower {
            return true;
        }
    }
    false
}

/// Returns `true` when the dialogue contains the given first name as a whole word
/// (not a substring of another word) in combination with a pronoun-affirmation
/// marker, with no denial marker present. Used for the indirect / appositive
/// first-name check (#1470 gap 1): catches "he's out, the lad Cormac" where
/// "Cormac" appears in appositive position after a pronoun affirmation ("he's
/// out"), so the standard affirmation-marker-collocated check misses it.
///
/// Fires only when BOTH conditions hold:
///   1. The first name appears as a whole word in the dialogue (word-boundary
///      match — prevents short names like "Al" from falsely matching "shall").
///   2. The dialogue contains a pronoun-affirmation marker (from
///      `dialogue_affirms_via_pronoun`), confirming the referent is a person
///      being located/confirmed rather than merely questioned.
///
/// Only used when the caller already established that the first name belongs to a
/// player-named fabricated full name — do not call for arbitrary first names.
fn dialogue_contains_name_with_pronoun_affirmation(dialogue: &str, name: &str) -> bool {
    // Word-boundary check: name must appear as a standalone token, not as a
    // substring of another word (e.g. "Al" must not match "shall" or "talk").
    if !text_contains_name_as_word(dialogue, name) {
        return false;
    }

    let lower = dialogue.to_lowercase();
    for marker in DENIAL_MARKERS {
        if lower.contains(marker) {
            return false;
        }
    }

    // Require a pronoun-affirmation marker to be present — this distinguishes
    // "he's out, the lad Cormac" (pronoun affirms, name in appositive) from
    // "Cormac Sweeney, you say? I cannot recall" (name in question, no pronoun).
    dialogue_affirms_via_pronoun(dialogue)
}

/// Extracts candidate person-name tokens (consecutive Title-cased bigrams and
/// trigrams) from the player input. Used to probe which names the player
/// mentioned that the NPC might fabricate-confirm.
fn extract_candidate_names(player_input: &str) -> Vec<String> {
    let words: Vec<&str> = player_input.split_whitespace().collect();
    let mut candidates: Vec<String> = Vec::new();
    let n = words.len();

    for i in 0..n {
        let w = words[i].trim_matches(|c: char| !c.is_alphabetic());
        if w.is_empty() {
            continue;
        }
        let is_cap = w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        let is_alnum = w.chars().all(|c| c.is_alphabetic() || c == '\'');
        if !is_cap || !is_alnum || w.len() < 2 {
            continue;
        }

        // Trigram: First Middle Last
        if i + 2 < n {
            let w2 = words[i + 1].trim_matches(|c: char| !c.is_alphabetic());
            let w3 = words[i + 2].trim_matches(|c: char| !c.is_alphabetic());
            let cap2 = w2.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
            let cap3 = w3.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
            let alnum2 = w2.chars().all(|c| c.is_alphabetic() || c == '\'');
            let alnum3 = w3.chars().all(|c| c.is_alphabetic() || c == '\'');
            if cap2 && cap3 && alnum2 && alnum3 && w2.len() >= 2 && w3.len() >= 2 {
                candidates.push(format!("{} {} {}", w, w2, w3));
            }
        }

        // Bigram: First Last
        if i + 1 < n {
            let w2 = words[i + 1].trim_matches(|c: char| !c.is_alphabetic());
            let cap2 = w2.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
            let alnum2 = w2.chars().all(|c| c.is_alphabetic() || c == '\'');
            if cap2 && alnum2 && w2.len() >= 2 {
                candidates.push(format!("{} {}", w, w2));
            }
        }
    }

    candidates
}

/// Honorific abbreviation pairs: (spelled-out form, abbreviated form).
///
/// The model frequently expands honorific abbreviations stored in the roster
/// (e.g. "Fr." → "Father") when generating dialogue. To avoid false-positive
/// person-confirmation guard fires — where the NPC correctly names a real
/// roster member using the spelled-out honorific while the roster stores the
/// abbreviated form — both forms are normalised to the same canonical token
/// before comparison (#1500 regression, quality-harness run 16).
///
/// Each pair is (long_lower, short_lower) — neither entry includes the
/// trailing period of the abbreviation because `normalize_honorifics` strips
/// trailing periods before comparison.
const HONORIFIC_PAIRS: &[(&str, &str)] = &[
    ("father", "fr"),
    ("reverend", "rev"),
    ("doctor", "dr"),
    ("mister", "mr"),
    ("missus", "mrs"),
    ("miss", "ms"),
    ("saint", "st"),
    ("brother", "br"),
    ("sister", "sr"),
];

/// Returns the canonical (long-form) honorific for a token, if it is a
/// known honorific abbreviation or spelling variant (case-insensitive, period
/// stripped). Tokens that are not honofifics are returned unchanged.
///
/// Examples: "fr" → "father", "rev" → "reverend", "dr" → "doctor".
/// "cormac" → "cormac" (unchanged).
fn canonical_honorific(token: &str) -> &str {
    // Strip a trailing period so "fr." and "fr" both normalise.
    let stripped = token.trim_end_matches('.');
    for &(long, short) in HONORIFIC_PAIRS {
        if stripped == short || stripped == long {
            return long;
        }
    }
    stripped
}

/// Normalises a roster or candidate name by replacing all leading and
/// internal honorific tokens with their canonical (long-form) equivalents.
///
/// "Fr. Declan Tierney" → "father declan tierney"
/// "Father Declan"      → "father declan"
/// "Cormac Duffy"       → "cormac duffy"
fn normalize_name_honorifics(lower_name: &str) -> String {
    lower_name
        .split_whitespace()
        .map(canonical_honorific)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Checks whether a candidate name matches any entry in the known roster
/// (case-insensitive).
///
/// Matching rules:
/// - **Full-name candidate** (2+ tokens, e.g. "Cormac Sweeney"): requires an
///   exact full-name match against a roster entry after honorific normalisation
///   ("Father Declan" matches "Fr. Declan Tierney" as a prefix after both are
///   normalised to "father declan …"). A shared first name with a *different*
///   surname (e.g. roster has "Cormac Duffy") does NOT constitute a match —
///   the candidate is treated as fabricated and the guard fires.
/// - **First-name-only candidate** (single token, e.g. "Cormac"): a first-name
///   match against any roster entry is legitimate — the player is referring to a
///   known person by first name only. Match is allowed.
/// - Player's own name always passes through (not flagged as fabricated).
fn name_in_roster(
    candidate: &str,
    known_person_names: &[String],
    player_name: Option<&str>,
) -> bool {
    let lower = candidate.to_lowercase();

    // Check player name
    if player_name.is_some_and(|pn| pn.to_lowercase() == lower) {
        return true;
    }

    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let candidate_is_full_name = tokens.len() >= 2;

    // Normalise the candidate's honorifics once for reuse in the loop.
    let candidate_norm = normalize_name_honorifics(&lower);

    for roster_name in known_person_names {
        let roster_lower = roster_name.to_lowercase();

        // Exact full-name match always passes through.
        if roster_lower == lower {
            return true;
        }

        // Honorific-normalised match: "Father Declan" (→ "father declan") is a
        // prefix of "Fr. Declan Tierney" (→ "father declan tierney"), so it
        // matches. We require the candidate to be a leading-token prefix of the
        // normalised roster name so that "Father Smith" does NOT match "Fr.
        // Tierney" even though both start with "father" after normalisation.
        //
        // Also handles the "stripped-honorific" case: a candidate like "Declan
        // Tierney" (no honorific) should match a roster entry "Fr. Declan Tierney"
        // because after stripping the roster's leading honorific the remaining
        // tokens "declan tierney" equal the candidate. This fires when the player
        // omits the honorific prefix entirely (e.g. addressing the priest by
        // given+surname without "Father"/"Fr.").
        if candidate_is_full_name {
            let roster_norm = normalize_name_honorifics(&roster_lower);
            // Prefix check: the normalised candidate must be an exact prefix of
            // the normalised roster name at a word boundary. "father declan"
            // matches "father declan tierney" but not "father deirdre ryan".
            if roster_norm == candidate_norm
                || roster_norm.starts_with(&format!("{candidate_norm} "))
            {
                return true;
            }

            // Stripped-honorific check: if the roster name's first token is a
            // known honorific, drop it and compare the remainder to the candidate.
            // "Fr. Declan Tierney" → drop "fr." → "declan tierney" == "declan tierney".
            // Prevents "Declan Tierney" from being treated as fabricated when the
            // roster stores it as "Fr. Declan Tierney".
            let roster_tokens: Vec<&str> = roster_lower.split_whitespace().collect();
            if let Some(first_roster_tok) = roster_tokens.first() {
                let first_is_honorific = {
                    let stripped = first_roster_tok.trim_end_matches('.');
                    HONORIFIC_PAIRS
                        .iter()
                        .any(|&(long, short)| stripped == long || stripped == short)
                };
                if first_is_honorific && roster_tokens.len() > 1 {
                    // Roster remainder after stripping the leading honorific token.
                    let remainder = roster_tokens[1..].join(" ");
                    // Direct equality check (candidate == roster without honorific).
                    if remainder == lower {
                        return true;
                    }
                    // Also allow the candidate to be a leading-token prefix of the
                    // remainder for trigram/partial matches ("Declan" in "Declan Tierney").
                    if remainder.starts_with(&format!("{lower} ")) {
                        return true;
                    }
                }
            }
        }

        // First-name-only candidate: allow a first-name match against any roster entry.
        // "Cormac" (single token) vs roster "Cormac Duffy" → match (casual reference).
        if !candidate_is_full_name {
            let candidate_first = tokens.first().copied().unwrap_or("");
            if roster_lower.split_whitespace().next().unwrap_or("") == candidate_first {
                return true;
            }
            // Also try honorific-normalised first-name match:
            // "Father" (single token) vs roster "Fr. Declan Tierney" → normalise
            // roster first token "fr." → "father" → match.
            let roster_first_norm = roster_lower
                .split_whitespace()
                .next()
                .map(canonical_honorific)
                .unwrap_or("");
            let candidate_first_norm = canonical_honorific(candidate_first);
            if roster_first_norm == candidate_first_norm {
                return true;
            }
        }
        // Full-name candidate: only exact or honorific-normalised prefix match
        // clears it (handled above).
        // "Cormac Sweeney" vs roster "Cormac Duffy" → no match → guard fires.
    }
    false
}

/// Returns true when a title-cased candidate extracted by the person guard is
/// actually a known world place. Location matches allow partial contiguous
/// references such as "Lough Ree" for "The Old Lough Ree Shore" without
/// treating unrelated surnames like "Mary Church" as a place match for
/// "Saint Mary's Church".
fn name_is_known_location(candidate: &str, known_location_names: &[String]) -> bool {
    let candidate_norm = normalize_for_repetition(candidate);
    if candidate_norm.is_empty() {
        return false;
    }
    let candidate_tokens: Vec<&str> = candidate_norm.split_whitespace().collect();

    known_location_names.iter().any(|location| {
        let location_norm = normalize_for_repetition(location);
        let location_tokens: Vec<&str> = location_norm.split_whitespace().collect();
        location_tokens
            .windows(candidate_tokens.len())
            .any(|window| window == candidate_tokens.as_slice())
    })
}

/// Post-generation guard for fabricated-person confirmation (#1459, #1466, #1470).
///
/// After dialogue completion is produced, scans the text for affirmative
/// confirmation of a named person extracted from `player_input` whose full
/// name is not in `known_person_names`. If such a confirmation is detected,
/// replaces the entire dialogue with a stock non-recognition decline.
///
/// - `dialogue`: the finalized NPC reply.
/// - `player_input`: the triggering player utterance (names to probe are
///   extracted from here as Title-cased bigrams/trigrams).
/// - `known_person_names`: the names from the NPC's "PEOPLE YOU KNOW" roster,
///   as plain name strings (e.g. `["Cormac Duffy", "Brigid Connolly"]`).
/// - `prior_player_inputs`: recent preceding player utterances from the
///   conversation transcript, oldest-first. Used to detect fabricated referents
///   established in earlier turns so that pronoun follow-up replies ("he's off
///   to the mill") are also declined (#1470 gap 2). Pass `&[]` when no prior
///   player text is available (e.g. the headless scripted path).
/// - `player_name`: the player's own name if known, to avoid false-positives.
/// - `seed`: deterministic seed for decline pool selection.
///
/// Conservative: fires only when an affirmation phrase co-occurs with the
/// unknown name and no denial is present. Does not fire on neutral mentions.
///
/// ## First-name conflation (#1466)
///
/// When the player names a fabricated full name (e.g. "Cormac Sweeney"), the
/// NPC may affirm only by first name ("Cormac is at the mill"). The full-name
/// check does not catch this because the dialogue doesn't contain "Cormac
/// Sweeney". This guard therefore also checks whether the dialogue affirms by
/// the first name of a player-named fabricated full name, since that first name
/// is claimed by the fabricated person in this exchange.
///
/// This additional first-name check fires ONLY when the player named a
/// fabricated full name — casual first-name queries about real roster members
/// ("do you know Cormac?" where "Cormac Duffy" is in the roster) still pass
/// through unchanged, because the player did not supply a fabricated full name.
///
/// ## Indirect / appositive first-name affirmation (#1470 gap 1)
///
/// The #1466 check used `dialogue_affirms_name` (which requires an affirmation
/// marker near the name). This misses appositive forms like "he's out, the lad
/// Cormac" where "Cormac" appears after a comma as an appositive, not near any
/// affirmation keyword. The guard now broadens the first-name conflation check:
/// if the fabricated first name appears ANYWHERE in the dialogue (not just near
/// an affirmation marker) and no denial is present, it fires.
///
/// ## Pronoun follow-up (#1470 gap 2)
///
/// After the fabricated referent is established in one turn, the player may ask
/// a follow-up with no name at all ("Out where?"). The current player input has
/// no extractable candidate name so the guard would never fire. By scanning
/// `prior_player_inputs` for fabricated full names, the guard accumulates a set
/// of "established fabricated referents". When the current player input yields
/// no fabricated candidates (pronoun-only turn), if the NPC dialogue contains
/// pronoun-affirmation markers ("he's off to", "she's at", …) the guard fires.
/// This is conservative: it only fires when no real-roster full name appears in
/// the immediately preceding player inputs.
pub fn guard_fabricated_person_confirmation(
    dialogue: &str,
    player_input: &str,
    known_person_names: &[String],
    prior_player_inputs: &[&str],
    player_name: Option<&str>,
    seed: u64,
) -> String {
    guard_fabricated_person_confirmation_with_locations(
        dialogue,
        player_input,
        known_person_names,
        &[],
        prior_player_inputs,
        player_name,
        seed,
    )
}

/// Location-aware variant of [`guard_fabricated_person_confirmation`].
///
/// The person guard extracts title-cased bigrams/trigrams from the player input.
/// Place names like "Lough Ree" have the same shape as human names, so callers
/// with world context should pass known locations to prevent valid place-history
/// answers from being replaced by "no such person" declines (#1569).
pub fn guard_fabricated_person_confirmation_with_locations(
    dialogue: &str,
    player_input: &str,
    known_person_names: &[String],
    known_location_names: &[String],
    prior_player_inputs: &[&str],
    player_name: Option<&str>,
    seed: u64,
) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }

    let candidates = extract_candidate_names(player_input);

    // Collect the first names of fabricated full names named by the player in
    // this exchange. Used for the first-name conflation check (#1466).
    //
    // A "fabricated full name first-name" is the first token of a multi-token
    // candidate (e.g. "Cormac" from "Cormac Sweeney") that is NOT in the
    // roster. We record these so that if the NPC affirms "Cormac" alone we can
    // still decline, because the player explicitly tied "Cormac" to a
    // fabricated surname in this very exchange.
    //
    // Ambiguous-but-real exclusion: if the player mentioned BOTH a fabricated
    // full name ("Cormac Sweeney") AND a real roster full name that shares the
    // same first name ("Cormac Duffy"), the first name is ambiguous in this
    // exchange — do NOT add it to the set. An NPC affirming "Cormac Duffy" in
    // that context is a legitimate real-person reference (#1466 T1).
    //
    // First, collect the real roster full names that the player mentioned.
    let player_mentioned_real_full_names: Vec<String> = candidates
        .iter()
        .filter(|candidate| name_in_roster(candidate, known_person_names, player_name))
        .filter(|candidate| candidate.split_whitespace().count() >= 2)
        .cloned()
        .collect();

    let fabricated_full_name_first_names: Vec<String> = candidates
        .iter()
        .filter(|candidate| {
            // Only multi-token candidates (full names, not single first names).
            let token_count = candidate.split_whitespace().count();
            if token_count < 2 {
                return false;
            }
            // Only fabricated ones (not in roster, not the player's own name).
            !name_in_roster(candidate, known_person_names, player_name)
                && !name_is_known_location(candidate, known_location_names)
        })
        .filter_map(|candidate| {
            candidate
                .split_whitespace()
                .next()
                .map(|first| first.to_string())
        })
        // Ambiguous-but-real: exclude first names shared with a real roster
        // full name that the player ALSO mentioned in this exchange. If the
        // player only mentioned the fabricated name, the first name is
        // unambiguous and is kept in the set (guard fires normally).
        .filter(|first| {
            let first_lower = first.to_lowercase();
            !player_mentioned_real_full_names.iter().any(|real_name| {
                real_name
                    .split_whitespace()
                    .next()
                    .map(|r| r.to_lowercase() == first_lower)
                    .unwrap_or(false)
            })
        })
        .collect();

    for candidate in &candidates {
        if name_in_roster(candidate, known_person_names, player_name)
            || name_is_known_location(candidate, known_location_names)
        {
            continue;
        }
        // Primary check: dialogue affirms the full fabricated name.
        if dialogue_affirms_name(dialogue, candidate) {
            tracing::warn!(
                fabricated_person = %candidate,
                "person-confirmation guard fired: replacing fabricated-person confirmation with decline (#1459)"
            );
            return non_recognition_decline(seed).to_string();
        }
    }

    // First-name conflation check (#1466 + #1470 gap 1): if the player named a
    // fabricated full name AND the NPC's reply confirms the referent by that
    // first name, also decline.
    //
    // Two sub-cases handled:
    //   (a) #1466 original: `dialogue_affirms_name` — strict affirmation marker
    //       collocates with the first name (e.g. "Cormac is at the mill").
    //   (b) #1470 gap 1: `dialogue_contains_name_with_pronoun_affirmation` —
    //       the dialogue contains BOTH a pronoun-affirmation marker ("he's out")
    //       AND the first name in appositive/trailing position (e.g. "he's out,
    //       the lad Cormac"). The pronoun marker confirms a person is being
    //       located; the appositive names the fabricated first name.
    //
    // Real-roster escape hatch: if the dialogue contains the first name but also
    // contains a real roster full name that begins with that first name (e.g.
    // "Cormac Duffy" is in the dialogue and in the roster), the NPC is talking
    // about a real person — do NOT decline.
    let dialogue_lower = dialogue.to_lowercase();
    for first_name in &fabricated_full_name_first_names {
        let affirms = dialogue_affirms_name(dialogue, first_name)
            || dialogue_contains_name_with_pronoun_affirmation(dialogue, first_name);
        if affirms {
            // Check if the dialogue resolves the first name to a real roster entry.
            let first_lower = first_name.to_lowercase();
            let resolves_to_real = known_person_names.iter().any(|roster_name| {
                let roster_lower = roster_name.to_lowercase();
                // Roster entry starts with this first name.
                roster_lower
                    .split_whitespace()
                    .next()
                    .map(|r| r == first_lower.as_str())
                    .unwrap_or(false)
                    // And the full real roster name appears in the dialogue.
                    && dialogue_lower.contains(roster_lower.as_str())
            });
            if resolves_to_real {
                // The NPC is affirming a real roster member that shares this first
                // name — legitimate reference, do not suppress.
                continue;
            }
            tracing::warn!(
                fabricated_first_name = %first_name,
                "person-confirmation guard fired: NPC affirmed by first name (direct or appositive) for player-named fabricated full name (#1466/#1470)"
            );
            return non_recognition_decline(seed).to_string();
        }
    }

    // Pronoun follow-up guard (#1470 gap 2): when the current player turn has no
    // extractable fabricated full name (pronoun-only turn, e.g. "Out where?"),
    // scan `prior_player_inputs` for fabricated full names established earlier in
    // the same conversation. If any are found AND the NPC dialogue contains a
    // pronoun-affirmation marker ("he's off to", "she's out", …) without a denial,
    // the NPC is confirming the previously-established fabricated referent via
    // pronoun — decline.
    //
    // Conservative conditions that must ALL hold:
    //   1. The current turn has no fabricated full name candidates (pronoun turn).
    //   2. At least one prior player turn named a fabricated full name.
    //   3. No prior player turn also named a real roster full name with the SAME
    //      first name in the same utterance (ambiguous referent → don't suppress).
    //   4. The NPC reply contains a pronoun affirmation marker.
    //   5. The NPC reply contains no denial marker.
    let current_turn_has_fabricated_candidate = candidates.iter().any(|c| {
        c.split_whitespace().count() >= 2
            && !name_in_roster(c, known_person_names, player_name)
            && !name_is_known_location(c, known_location_names)
    });

    if !current_turn_has_fabricated_candidate && !prior_player_inputs.is_empty() {
        // Pre-extract candidate names from each prior input exactly once to avoid
        // O(N²) re-parsing inside the ambiguity filter below.
        let prior_candidates_per_input: Vec<Vec<String>> = prior_player_inputs
            .iter()
            .map(|prior| extract_candidate_names(prior))
            .collect();

        // Collect fabricated full names established in prior player turns.
        let prior_established_fabricated: Vec<String> = prior_candidates_per_input
            .iter()
            .flat_map(|names| names.iter().cloned())
            .filter(|c| {
                c.split_whitespace().count() >= 2
                    && !name_in_roster(c, known_person_names, player_name)
                    && !name_is_known_location(c, known_location_names)
            })
            // Ambiguous-but-real: exclude if the same prior input also named a real
            // roster full name sharing the first name (too ambiguous to suppress).
            .filter(|fabricated| {
                let fab_first = fabricated
                    .split_whitespace()
                    .next()
                    .map(|w| w.to_lowercase())
                    .unwrap_or_default();
                // Find which prior input(s) mentioned this fabricated name (using
                // the pre-extracted sets — no re-parsing).
                let prior_that_named_it: Vec<&Vec<String>> = prior_candidates_per_input
                    .iter()
                    .filter(|names| {
                        names
                            .iter()
                            .any(|c| c.to_lowercase() == fabricated.to_lowercase())
                    })
                    .collect();
                // For each such prior input, check whether it ALSO named a real
                // roster full name that shares this first name.
                !prior_that_named_it.iter().any(|names| {
                    names.iter().any(|c| {
                        c.split_whitespace().count() >= 2
                            && name_in_roster(c, known_person_names, player_name)
                            && c.split_whitespace()
                                .next()
                                .map(|w| w.to_lowercase() == fab_first)
                                .unwrap_or(false)
                    })
                })
            })
            .collect();

        if !prior_established_fabricated.is_empty() && dialogue_affirms_via_pronoun(dialogue) {
            tracing::warn!(
                established_fabricated = ?prior_established_fabricated,
                "person-confirmation guard fired: pronoun follow-up affirms established fabricated referent (#1470)"
            );
            return non_recognition_decline(seed).to_string();
        }
    }

    dialogue.to_string()
}

// ── #1460 — verbosity / run-on guard ──────────────────────────────────────────
//
// The Qwen2.5-14B-4bit model ignores the "single question" prompt cap and
// the frequency_penalty doesn't fully suppress loops. Observed patterns:
//   (a) A phrase repeated 5-6× consecutively (already handled by
//       collapse_repeated_sentences, but near-duplicates may slip through).
//   (b) Five trailing interrogative sentences stacked.
//   (c) Reply ending with "…" — a mid-sentence stream-truncation marker that
//       escaped the response.rs truncation recovery.
//   (d) Bare leaked mood-adjective: the model emits the literal word from
//       the "YOUR CURRENT MOOD:" block at the end of the dialogue field.
//
// This guard applies after collapse_repeated_sentences and the length cap.
// It is conservative — it only removes clearly structural artifacts, not
// legitimate prose.

const STACKED_PLAYER_VOCATIVES: &[(&str, &str)] = &[
    ("friend, stranger", "friend"),
    ("friend stranger", "friend"),
    ("stranger, friend", "stranger"),
    ("stranger friend", "stranger"),
];

fn vocative_start_boundary(ch: Option<char>) -> bool {
    match ch {
        None => true,
        Some(ch) => ch.is_whitespace() || matches!(ch, ',' | ';' | ':' | '.' | '!' | '?'),
    }
}

fn vocative_end_boundary(ch: Option<char>) -> bool {
    match ch {
        None => true,
        Some(ch) => matches!(ch, ',' | ';' | ':' | '.' | '!' | '?'),
    }
}

fn match_replacement_case(original: &str, replacement: &str) -> String {
    if original.chars().next().is_some_and(|ch| ch.is_uppercase()) {
        let mut chars = replacement.chars();
        if let Some(first) = chars.next() {
            let mut out = first.to_uppercase().collect::<String>();
            out.push_str(chars.as_str());
            return out;
        }
    }
    replacement.to_string()
}

fn replace_stacked_vocative_phrase(text: &str, needle: &str, replacement: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::new();
    let mut search_from = 0usize;
    let mut last_emit = 0usize;

    while search_from < lower.len() {
        let Some(rel_start) = lower[search_from..].find(needle) else {
            break;
        };
        let start = search_from + rel_start;
        let end = start + needle.len();
        let before = if start == 0 {
            None
        } else {
            text[..start].chars().next_back()
        };
        let after = text[end..].chars().next();

        if vocative_start_boundary(before) && vocative_end_boundary(after) {
            out.push_str(&text[last_emit..start]);
            let original = &text[start..end];
            out.push_str(&match_replacement_case(original, replacement));
            last_emit = end;
            search_from = end;
        } else {
            search_from = start + 1;
        }
    }

    if last_emit == 0 {
        text.to_string()
    } else {
        out.push_str(&text[last_emit..]);
        out
    }
}

/// Removes adjacent player-address terms such as "friend stranger" (#1534).
///
/// These are generated as stacked vocatives rather than meaningful noun
/// phrases, so the guard keeps the first address term and drops the second.
/// Matching is intentionally narrow: the stacked phrase must end at punctuation
/// or end-of-line to avoid rewriting ordinary comparative prose.
pub fn strip_stacked_player_vocatives(dialogue: &str) -> String {
    let mut result = dialogue.to_string();
    for (needle, replacement) in STACKED_PLAYER_VOCATIVES {
        result = replace_stacked_vocative_phrase(&result, needle, replacement);
    }
    result
}

/// Collapses non-consecutive near-duplicate sentences within a single dialogue
/// string (#1460 — distributed repetition guard).
///
/// The model can produce the same semantic clause interleaved with other text,
/// e.g. "what is it ye seek from yer cousin ... what is it ye seek from yer
/// kin ... other text ... what is it ye seek from him" — these are not
/// consecutive, so `collapse_repeated_sentences` does not catch them.
///
/// Algorithm:
/// 1. Split into sentence units (same `split_sentences` as the consecutive guard).
/// 2. For each sentence, compute a normalized content-token list.
/// 3. Compare against the set of already-kept sentences using two signals:
///    - **Jaccard similarity** >= `DISTRIBUTED_DEDUP_THRESHOLD` (0.60) on the
///      token *set* — catches near-verbatim restatements.
///    - **Shared prefix** of >= `MIN_SHARED_PREFIX` (5) consecutive words — catches
///      template variations like "what is it ye seek from <kin/Cormac/him>" that
///      share a syntactic frame but differ in the object NP.
/// 4. If either signal fires AND the sentence is long enough (>= `MIN_CONTENT_TOKENS`
///    content tokens — short lines like "Aye." are exempt), drop the duplicate.
///
/// Conservative thresholds prevent false positives on legitimate varied dialogue.
/// The function is stable: the FIRST occurrence of a near-duplicate cluster is
/// always kept; later ones are dropped.
pub fn collapse_distributed_repeated_sentences(dialogue: &str) -> String {
    /// Minimum number of content tokens a sentence must have for distributed
    /// dedup to apply. Shorter sentences have too little signal to compare
    /// safely — e.g. "Aye." or "I see." are kept as-is to avoid false
    /// positives on natural short acknowledgements.
    const MIN_CONTENT_TOKENS: usize = 5;
    /// Jaccard similarity threshold for two sentences to count as near-duplicate
    /// via the set-overlap signal. 0.60 is conservative — it requires 60% of the
    /// token *set* to be shared, so sentences that merely share a handful of
    /// common function words do not collide.
    const DISTRIBUTED_DEDUP_THRESHOLD: f32 = 0.60;
    /// Minimum length of a shared leading-word prefix for two sentences to count
    /// as template-duplicates. "what is it ye" is 4 words and is the syntactic
    /// frame shared by "what is it ye seek from X" vs "what is it ye want from
    /// Y". 4 is a safe floor: generic openers like "aye, the" or "i do not" are
    /// only 2–3 words long and won't reach it.
    const MIN_SHARED_PREFIX: usize = 4;

    let sentences = split_sentences(dialogue);
    if sentences.len() < 2 {
        return dialogue.to_string();
    }

    // Each entry is (norm_string, ordered_token_vec, token_set).
    struct KeptEntry {
        norm: String,
        tokens: Vec<String>,
        set: std::collections::HashSet<String>,
    }

    let mut kept: Vec<&str> = Vec::with_capacity(sentences.len());
    let mut kept_entries: Vec<KeptEntry> = Vec::new();

    for sentence in &sentences {
        let norm = normalize_for_repetition(sentence);
        if norm.is_empty() {
            continue;
        }

        let tokens: Vec<String> = norm
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| w.to_string())
            .collect();

        let token_count = tokens.len();
        let token_set: std::collections::HashSet<String> = tokens.iter().cloned().collect();

        // Only apply distributed dedup when sentence is long enough.
        if token_count >= MIN_CONTENT_TOKENS {
            let mut is_near_dup = false;
            for entry in &kept_entries {
                // Fast path: exact normalized match.
                if entry.norm == norm {
                    is_near_dup = true;
                    break;
                }
                // Signal 1: Jaccard on token sets.
                let intersection = token_set.intersection(&entry.set).count() as f32;
                let union = token_set.union(&entry.set).count() as f32;
                if union > 0.0 && (intersection / union) >= DISTRIBUTED_DEDUP_THRESHOLD {
                    is_near_dup = true;
                    break;
                }
                // Signal 2: shared leading prefix of >= MIN_SHARED_PREFIX words.
                let shared_prefix = tokens
                    .iter()
                    .zip(entry.tokens.iter())
                    .take_while(|(a, b)| a == b)
                    .count();
                if shared_prefix >= MIN_SHARED_PREFIX {
                    is_near_dup = true;
                    break;
                }
            }
            if is_near_dup {
                continue;
            }
        }

        kept.push(sentence.trim());
        kept_entries.push(KeptEntry {
            norm,
            tokens,
            set: token_set,
        });
    }

    if kept.is_empty() {
        return dialogue.trim().to_string();
    }
    kept.join(" ")
}

/// Caps the total number of interrogative sentences in a reply to at most
/// `MAX_TOTAL_QUESTIONS` (#1460 — total-question cap).
///
/// The trailing question-cap (`cap_trailing_questions`) only strips questions
/// that appear at the end. When the model produces scattered repeated questions
/// interleaved with other text, the trailing cap does not help. This function
/// counts ALL question-ending sentences in the reply and, if there are more than
/// `MAX_TOTAL_QUESTIONS`, drops the earlier ones and keeps only the last
/// `MAX_TOTAL_QUESTIONS`. Non-question sentences are always preserved. Applies
/// after distributed dedup.
pub fn cap_total_questions(dialogue: &str) -> String {
    /// Maximum number of questions permitted in a single NPC reply. A natural
    /// reply might legitimately end with one question; two is generous headroom
    /// for a reply that has both a mid-body rhetorical question and a closing
    /// question. More than two is a loop artifact.
    const MAX_TOTAL_QUESTIONS: usize = 2;

    let sentences = split_sentences(dialogue);
    if sentences.len() < 2 {
        return dialogue.to_string();
    }

    // Identify indices of all interrogative sentences.
    let question_indices: Vec<usize> = sentences
        .iter()
        .enumerate()
        .filter(|(_, s)| s.trim().ends_with('?'))
        .map(|(i, _)| i)
        .collect();

    if question_indices.len() <= MAX_TOTAL_QUESTIONS {
        // Already within budget — nothing to do.
        return dialogue.to_string();
    }

    // Drop the earliest questions, keep only the last MAX_TOTAL_QUESTIONS.
    let drop_count = question_indices.len() - MAX_TOTAL_QUESTIONS;
    let drop_set: std::collections::HashSet<usize> =
        question_indices[..drop_count].iter().copied().collect();

    let kept: Vec<&str> = sentences
        .iter()
        .enumerate()
        .filter(|(i, _)| !drop_set.contains(i))
        .map(|(_, s)| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if kept.is_empty() {
        return dialogue.trim().to_string();
    }
    kept.join(" ")
}

/// Strips all but the last interrogative sentence from a multi-question tail
/// (#1460 — question-stack guard).
///
/// An interrogative sentence is one ending with `?`. When the finalized dialogue
/// ends with more than one question (counting sentences from the end), all but
/// the last are removed. Non-question sentences before the final question run are
/// left intact. Operates on the sentence units produced by `split_sentences`.
pub fn cap_trailing_questions(dialogue: &str) -> String {
    let sentences = split_sentences(dialogue);
    if sentences.len() < 2 {
        return dialogue.to_string();
    }

    // Find the run of trailing interrogative sentences.
    let mut tail_questions: Vec<usize> = Vec::new();
    for (i, sent) in sentences.iter().enumerate().rev() {
        let trimmed = sent.trim();
        if trimmed.ends_with('?') {
            tail_questions.push(i);
        } else {
            break;
        }
    }

    // Nothing to do if ≤1 trailing question.
    if tail_questions.len() <= 1 {
        return dialogue.to_string();
    }

    // Keep everything up to (but not including) the first question in the tail
    // run, then append only the LAST question.
    let run_start = *tail_questions.last().unwrap(); // smallest index = first in run
    let last_q = *tail_questions.first().unwrap(); // largest index = last in run

    let mut kept: Vec<&str> = sentences[..run_start].iter().map(|s| s.trim()).collect();
    kept.push(sentences[last_q].trim());

    kept.retain(|s| !s.is_empty());
    kept.join(" ")
}

/// Trims a dialogue string that ends with `…` (model truncation signal) back
/// to the last complete sentence (#1460 — truncation-trim guard).
///
/// If the string ends with `…` and there is a complete sentence before the
/// truncation point (ending in `.`, `!`, or `?`), the incomplete fragment
/// after the last such terminator is removed. If no complete sentence precedes
/// the ellipsis, the string is returned trimmed without the `…`.
pub fn trim_mid_sentence_truncation(dialogue: &str) -> String {
    let text = dialogue.trim();
    if !text.ends_with('…') {
        return text.to_string();
    }

    // Strip the trailing `…` (it is 3 bytes in UTF-8).
    let without_ellipsis = text[..text.len() - '…'.len_utf8()].trim_end();

    // Find the last sentence-ending punctuation (.  !  ?) in the remaining text.
    if let Some(last_end) = without_ellipsis.rfind(['.', '!', '?']) {
        // Include the punctuation character itself.
        let sentence_end = last_end + 1;
        if sentence_end < without_ellipsis.len() {
            // There is trailing fragment after the last complete sentence — trim it.
            return without_ellipsis[..sentence_end].trim().to_string();
        }
    }

    // No complete sentence found — return without the ellipsis.
    without_ellipsis.trim().to_string()
}

/// Strips a trailing ellipsis artifact ("…", ".…", or "….") that appears
/// immediately after an otherwise-complete sentence (#1472).
///
/// This is a different artifact from the mid-sentence truncation ellipsis
/// handled by [`trim_mid_sentence_truncation`]: that function handles a `…`
/// that replaces a trailing incomplete clause. This function handles the case
/// where a model emits a grammatically complete sentence and then appends a
/// stray `…` or period+ellipsis combination as decoration or a run-on marker,
/// producing endings like "...warm ye up.…" or "...warm ye up….".
///
/// Specifically, this strips a trailing:
///
/// - `….` — ellipsis followed by period (uncommon artifact)
/// - `.…` — period followed by ellipsis (e.g. "...warm ye up.…")
/// - `…` — bare ellipsis that immediately follows sentence punctuation
///   (`.`, `!`, `?`) with no intervening content
///
/// The strip only fires when the character before the ellipsis (after stripping
/// any trailing period-before-ellipsis) is sentence-ending punctuation, so
/// this does not interfere with mid-sentence or introductory ellipses.
pub fn strip_trailing_ellipsis_artifact(dialogue: &str) -> String {
    let text = dialogue.trim();
    if text.is_empty() {
        return String::new();
    }

    // Pattern: ends with "….": ellipsis + trailing period.
    if let Some(stripped) = text.strip_suffix("….") {
        let without = stripped.trim_end();
        // Keep if what remains ends with sentence punctuation.
        if without
            .chars()
            .last()
            .map(|c| matches!(c, '.' | '!' | '?'))
            .unwrap_or(false)
        {
            return without.to_string();
        }
        // Otherwise just strip the combined suffix and return with a period.
        return format!("{}.", without);
    }

    // Pattern: ends with ".…": period + ellipsis.
    if text.ends_with(".…") {
        let without_ellipsis = text[..text.len() - '…'.len_utf8()].trim_end();
        // without_ellipsis now ends with "."; that's a complete sentence terminator.
        return without_ellipsis.to_string();
    }

    // Pattern: ends with bare "…" immediately after sentence punctuation
    // (i.e. no intervening text between the punctuation and the ellipsis —
    // the char before the ellipsis is `.`, `!`, or `?`).
    if text.ends_with('…') {
        let without = text[..text.len() - '…'.len_utf8()].trim_end();
        if without
            .chars()
            .last()
            .map(|c| matches!(c, '.' | '!' | '?'))
            .unwrap_or(false)
        {
            return without.to_string();
        }
    }

    text.to_string()
}

/// Strips immediately-consecutive short-phrase repeats from NPC dialogue
/// (#1505 — near-repetition guard for "tell me, tell me" patterns).
///
/// A small model will sometimes end a run-on clause with the same 2–4 word
/// phrase repeated twice in a row with only a comma or space between the
/// occurrences — e.g. "tell me, tell me" or "aye or nay, aye or nay". This
/// is not caught by [`collapse_repeated_sentences`] (operates at sentence
/// level), [`collapse_distributed_repeated_sentences`] (requires ≥ 5 content
/// tokens), or [`collapse_degenerate_phrase_loop`] (requires ≥ 4 repetitions
/// at 3+ words). This guard specifically handles the 2-occurrence consecutive
/// case.
///
/// # Behaviour
///
/// 1. Tokenises the dialogue into whitespace-delimited words (punctuation
///    stripped for comparison but not removed from the output).
/// 2. For each n-gram of length 2–4, checks whether two adjacent (or
///    comma-separated) occurrences exist.
/// 3. On detection, removes the SECOND occurrence including any leading
///    comma/space before it.
/// 4. Scans from left to right; only the first detected pair per n-gram width
///    is removed to stay conservative.
///
/// Conservative: only fires on IMMEDIATE adjacency (the two occurrences are
/// consecutive with nothing but punctuation/whitespace between them), so a
/// phrase appearing twice in different parts of a reply is not suppressed.
pub fn strip_consecutive_short_phrase_repeat(dialogue: &str) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }

    const MAX_WIDTH: usize = 4;
    const MIN_WIDTH: usize = 2;

    let words: Vec<&str> = dialogue.split_whitespace().collect();
    let n = words.len();
    if n < MIN_WIDTH * 2 {
        return dialogue.to_string();
    }

    // Build a normalized word list for comparison.
    let normed: Vec<String> = words.iter().map(|w| normalize_for_repetition(w)).collect();

    // Pre-compute byte offsets of each whitespace-separated token in the
    // original string once, so we don't redo the scan inside the inner loop.
    // token_starts[k] / token_ends[k] correspond to words[k].
    let mut token_starts: Vec<usize> = Vec::with_capacity(n);
    let mut token_ends: Vec<usize> = Vec::with_capacity(n);
    {
        let mut in_word = false;
        for (byte_idx, ch) in dialogue.char_indices() {
            if ch.is_whitespace() {
                if in_word {
                    token_ends.push(byte_idx);
                    in_word = false;
                }
            } else if !in_word {
                token_starts.push(byte_idx);
                in_word = true;
            }
        }
        if in_word {
            token_ends.push(dialogue.len());
        }
    }

    // If the scan produced a different token count than split_whitespace (e.g.
    // due to unusual Unicode whitespace), fall back to returning unchanged.
    if token_starts.len() != n || token_ends.len() != n {
        return dialogue.to_string();
    }

    // Strategy: for each n-gram width (largest first for richest context),
    // scan for a position `i` such that words[i..i+width] == words[i+width..i+2*width]
    // after normalization. When found, calculate the byte span of the second
    // occurrence in the original string and remove it.
    //
    // Since we split on whitespace and kept punctuation attached to words,
    // "tell me, tell me" tokenises as ["tell", "me,", "tell", "me"] — positions
    // 0..2 and 2..4 are directly adjacent, which is exactly the consecutive case.
    'width: for width in (MIN_WIDTH..=MAX_WIDTH).rev() {
        if width * 2 > n {
            continue;
        }
        for i in 0..=(n - width * 2) {
            if normed[i..i + width] == normed[i + width..i + 2 * width] {
                // Byte range of the second occurrence: from the start of
                // word[i+width] back to include any leading comma/space, to
                // the end of word[i+2*width-1].
                let second_word_start = token_starts[i + width];
                let second_word_end = token_ends[i + 2 * width - 1];

                // Walk backwards from second_word_start to eat a preceding
                // comma, space, or semicolon so "tell me, tell me" becomes
                // "tell me" not "tell me,".
                let eat_from = {
                    let bytes = dialogue.as_bytes();
                    let mut pos = second_word_start;
                    while pos > 0 && matches!(bytes[pos - 1], b' ' | b',' | b';' | b'\t' | b'\n') {
                        pos -= 1;
                    }
                    pos
                };

                let result = format!("{}{}", &dialogue[..eat_from], &dialogue[second_word_end..]);
                tracing::debug!(
                    phrase = %words[i..i+width].join(" "),
                    "stripped consecutive short-phrase repeat (#1505)"
                );
                return result;
            }
        }
        continue 'width;
    }

    dialogue.to_string()
}

/// Trims a multi-sentence NPC reply to at most [`MAX_DIALOGUE_SENTENCES`]
/// sentences
/// (#1472 — hard length cap).
///
/// This is the blunt backstop for run-on replies that the surgical dedup guards
/// cannot catch: a model may produce 6-8 distinct non-duplicate sentences that
/// are each individually legitimate but collectively too long. No phrasing
/// heuristic catches this; only a hard count cap does.
///
/// Behavior:
/// - Splits on terminal punctuation (`.`, `!`, `?`, `…`) using [`split_sentences`].
/// - When the reply exceeds the cap and opens with a standalone greeting,
///   drops that greeting before substantive content.
/// - Keeps the first N remaining sentence units in their original order. It
///   never replaces a substantive middle sentence merely to retain a trailing
///   question.
/// - Replies already within the cap are returned unchanged.
///
/// Three sentences matches the Tier-1 generation contract: one conversational
/// beat with enough room for an answer, a supporting detail, and a question.
pub fn cap_sentence_count(dialogue: &str) -> String {
    cap_sentence_count_with_limit(dialogue, MAX_DIALOGUE_SENTENCES)
}

/// Applies the sentence-count cap at a specific `max` limit (#1491).
///
/// Factored out of `cap_sentence_count` so both the default 3-sentence cap and
/// the mood-aware tighter cap share one implementation.
fn cap_sentence_count_with_limit(dialogue: &str, max: usize) -> String {
    let sentences = split_sentences(dialogue);
    // Filter out empty fragments produced by split_sentences.
    let sentences: Vec<&str> = sentences
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if sentences.len() <= max {
        return dialogue.trim().to_string();
    }

    if max == 0 {
        return String::new();
    }

    let candidates = if is_standalone_greeting(sentences[0]) {
        &sentences[1..]
    } else {
        sentences.as_slice()
    };

    candidates[..candidates.len().min(max)].join(" ")
}

/// Returns true when a sentence is only a salutation, optional polite
/// addressee, and no substantive clause.
///
/// Proper-name tokens retain their initial capital in `sentence`, allowing
/// replies such as "Ah, good day to you, Aiden Carney." without treating
/// "Good morning, the road is flooded." as a disposable greeting.
fn is_standalone_greeting(sentence: &str) -> bool {
    let words: Vec<(String, bool)> = sentence
        .split_whitespace()
        .filter_map(|token| {
            let stripped =
                token.trim_matches(|c: char| !c.is_alphabetic() && c != '\'' && c != '\u{2019}');
            if stripped.is_empty() {
                None
            } else {
                Some((
                    stripped.to_lowercase(),
                    stripped.chars().next().is_some_and(char::is_uppercase),
                ))
            }
        })
        .collect();
    if words.is_empty() {
        return false;
    }

    let mut index = 0;
    if matches!(words[index].0.as_str(), "ah" | "aye" | "well" | "sure") {
        index += 1;
    }
    if index >= words.len() {
        return false;
    }

    match words[index].0.as_str() {
        "good"
            if words.get(index + 1).is_some_and(|(word, _)| {
                matches!(
                    word.as_str(),
                    "morning" | "mornin" | "mornin'" | "day" | "afternoon" | "evening"
                )
            }) =>
        {
            index += 2;
        }
        "dia"
            if words
                .get(index + 1)
                .is_some_and(|(word, _)| word == "dhuit") =>
        {
            index += 2;
        }
        "hello" | "hullo" => {
            index += 1;
        }
        _ => return false,
    }

    const POLITE_ADDRESS_WORDS: &[&str] = &[
        "to", "ye", "you", "there", "friend", "stranger", "sir", "madam", "ma'am", "mister",
        "mistress", "miss", "widow", "lad", "lass", "child", "mo", "chara",
    ];

    words[index..].iter().all(|(word, initially_upper)| {
        *initially_upper || POLITE_ADDRESS_WORDS.contains(&word.as_str())
    })
}

// ── #1491 — mood-aware sentence cap ───────────────────────────────────────────

/// Mood keywords that indicate an NPC should get a tighter (2-sentence) cap
/// instead of the default 3 (#1491, #1566).
const TERSE_MOOD_KEYWORDS: &[&str] = &[
    "busy",
    "sharp",
    "curt",
    "distracted",
    "preoccupied",
    "rushed",
    "hurried",
    "impatient",
    "terse",
    "brusque",
    "irritated",
    "frustrated",
    "alert",
    "watchful",
    "vigilant",
];

/// Applies the sentence-count cap with optional mood-aware tightening (#1491).
///
/// For terse moods (matching `TERSE_MOOD_KEYWORDS`), caps at 2 sentences;
/// otherwise uses the shared default. Exposed for tests.
///
/// Gate: `npc-mood-aware-sentence-cap` (default-on; callers check the flag).
pub fn cap_sentence_count_for_mood(dialogue: &str, mood: Option<&str>) -> String {
    let cap = match mood {
        Some(m) => {
            let m_lower = m.to_lowercase();
            if TERSE_MOOD_KEYWORDS.iter().any(|kw| m_lower.contains(kw)) {
                2
            } else {
                MAX_DIALOGUE_SENTENCES
            }
        }
        None => MAX_DIALOGUE_SENTENCES,
    };
    cap_sentence_count_with_limit(dialogue, cap)
}

// ── #1554 — nearby phrase repeat collapse ─────────────────────────────────────

/// Collapses a phrase that appears twice within a 20-word window (#1554).
///
/// When the verbosity guards produce an output where a 3–5 word phrase appears
/// twice within a 20-word span (not necessarily adjacent), this function
/// truncates at the end of the first occurrence and trims to the nearest
/// preceding sentence boundary. Example:
///
/// "Aye, tell me indeed, what say ye now, indeed, what be it?" →
/// `collapse_nearby_phrase_repeat` sees "indeed, what" (3 tokens normed) at
/// positions 3 and 8 within a 20-word window and trims to "Aye, tell me indeed."
/// (or the prefix up to the first occurrence when no sentence boundary exists
/// before it).
///
/// Conservative: only fires for phrase widths 3–5, exactly 2 occurrences within
/// a 20-word window, and only when the words involved have meaningful content.
/// Does not fire on natural varied prose.
///
/// Exposed for tests.
pub fn collapse_nearby_phrase_repeat(dialogue: &str) -> String {
    let text = dialogue.trim();
    if text.is_empty() {
        return String::new();
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let n = words.len();

    // Precompute normalized form of every token once — normalize_for_repetition
    // allocates a String (lowercase + split/join + trim), so calling it inside
    // nested loops would normalize each word O(n²) times in the worst case.
    // With the precomputed slice we normalize each word exactly once.
    let normed: Vec<String> = words.iter().map(|w| normalize_for_repetition(w)).collect();

    const MIN_WIDTH: usize = 3;
    const MAX_WIDTH: usize = 5;
    // Maximum words between the START of the first occurrence and the START
    // of the second occurrence. Covers same-sentence embedded repeats.
    const MAX_START_GAP: usize = 20;

    // Largest width first: richer context, fewer false positives.
    for width in (MIN_WIDTH..=MAX_WIDTH).rev() {
        if width * 2 > n {
            continue;
        }
        for i in 0..=(n.saturating_sub(width)) {
            let ngram = &normed[i..i + width];
            // Skip all-stopword n-grams (e.g. "and the a") — too common to be
            // meaningful repeated phrases. Require at least one content word.
            let has_content = ngram.iter().any(|t| t.len() >= 4);
            if !has_content {
                continue;
            }
            // Search for a second occurrence within MAX_START_GAP words.
            let search_end = (i + MAX_START_GAP).min(n.saturating_sub(width) + 1);
            for j in (i + width)..search_end {
                let other = &normed[j..j + width];
                if ngram == other {
                    // Found a nearby repeat. Build the prefix up to (and
                    // including) the end of the first occurrence, then
                    // trim to the nearest sentence boundary.
                    let prefix_words = &words[..i + width];
                    let prefix = prefix_words.join(" ");
                    // Look for the last sentence-end marker in the prefix.
                    if let Some(last_end) = prefix.rfind(['.', '!', '?']) {
                        let sentence_end = last_end + 1;
                        if !prefix[sentence_end..].trim().is_empty() {
                            // The only clean boundary is before the first
                            // occurrence of the repeated phrase. Trimming there
                            // would drop the whole substantive sentence and keep
                            // only an earlier opener (#1561), so this is not a
                            // safe loop-collapse candidate.
                            continue;
                        }
                        let trimmed = prefix[..sentence_end].trim();
                        if !trimmed.is_empty() {
                            tracing::debug!(
                                phrase = %ngram.join(" "),
                                "collapsed nearby phrase repeat at sentence boundary (#1554)"
                            );
                            return trimmed.to_string();
                        }
                    }
                    // No sentence boundary before the repeat — return the prefix
                    // as-is so we don't produce empty output.
                    let trimmed = prefix.trim();
                    if !trimmed.is_empty() {
                        tracing::debug!(
                            phrase = %ngram.join(" "),
                            "collapsed nearby phrase repeat (no sentence boundary) (#1554)"
                        );
                        return trimmed.to_string();
                    }
                }
            }
        }
    }

    text.to_string()
}

/// Runs the full verbosity guard pipeline with optional mood-aware sentence cap (#1491).
///
/// When `mood` is `Some` and contains a "busy" mood keyword, the sentence cap
/// is reduced to 2 (from the default 3) so a busy NPC stays terse.
///
/// Gate: `npc-mood-aware-sentence-cap` (default-on; callers check the flag).
pub fn guard_verbosity_runons_with_mood(dialogue: &str, mood: Option<&str>) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }
    // Step 0 (F1 / #1526): strip CJK/JSON scaffolding leaks FIRST.
    let after_scaffold = sanitize_scaffolding_leak(dialogue);
    let after_mood = strip_leaked_mood_word(&after_scaffold);
    let after_trunc = trim_mid_sentence_truncation(&after_mood);
    let after_ellipsis = strip_trailing_ellipsis_artifact(&after_trunc);
    let after_stacked_vocatives = strip_stacked_player_vocatives(&after_ellipsis);
    let after_phrase_loop = collapse_degenerate_phrase_loop(&after_stacked_vocatives);
    let after_consec_repeat = strip_consecutive_short_phrase_repeat(&after_phrase_loop);
    // Step 5b (#1554): catch non-adjacent phrase repeats within a 20-word window
    // that the consecutive guard (step 5, strictly adjacent) misses.
    let after_nearby = collapse_nearby_phrase_repeat(&after_consec_repeat);
    let after_distributed = collapse_distributed_repeated_sentences(&after_nearby);
    // Cap in semantic order before question cleanup. This prevents a trailing
    // question from displacing a direct answer when the reply is shortened.
    let after_sentence_cap = cap_sentence_count_for_mood(&after_distributed, mood);
    let after_total_q = cap_total_questions(&after_sentence_cap);
    let after_trailing_q = cap_trailing_questions(&after_total_q);
    cap_word_count(&after_trailing_q)
}

/// Known mood adjectives that the Qwen2.5-14B model leaks as bare words at the
/// end of the `dialogue` field (#1460 — mood-word leak guard).
///
/// These are single lowercase words that match the mood label in the
/// "YOUR CURRENT MOOD:" prompt block. The model emits them as a trailing word
/// after sentence-ending punctuation, so they look like action tokens but are
/// mood words rather than action verbs.
///
/// Exposed for tests.
pub const LEAKED_MOOD_WORDS: &[&str] = &[
    "sharp",
    "curt",
    "caustic",
    "acerbic",
    "irritated",
    "frustrated",
    "annoyed",
    "grumpy",
    "angry",
    "furious",
    "irate",
    "bitter",
    "resentful",
    "sour",
    "suspicious",
    "wary",
    "distrustful",
    "anxious",
    "nervous",
    "worried",
    "sad",
    "mournful",
    "sorrowful",
    "melancholy",
    "wistful",
    "busy",
    "distracted",
    "preoccupied",
    "restless",
    "agitated",
    "tired",
    "weary",
    "exhausted",
    "alert",
    "watchful",
    "vigilant",
    "contemplative",
    "thoughtful",
    "reflective",
    "pensive",
    "calm",
    "serene",
    "tranquil",
    "stoic",
    "guarded",
    "reserved",
    "determined",
    "resolute",
    "calculating",
    "cheerful",
    "jovial",
    "merry",
    "eager",
    "excited",
    "curious",
    "intrigued",
    "passionate",
    "fervent",
    "content",
    "satisfied",
];

/// Strips a bare leaked mood-adjective from the end of a dialogue string
/// (#1460 — mood-word leak guard).
///
/// Small models (Qwen2.5-14B) sometimes emit the literal mood-word from the
/// "YOUR CURRENT MOOD:" prompt block at the very end of the `dialogue` field,
/// after sentence-ending punctuation (e.g. "Good day to ye. sharp"). This is
/// analogous to the action-token leak fixed in #1374, but for mood words. This
/// function strips it when:
/// - The last word is a known mood word (case-insensitive, from `LEAKED_MOOD_WORDS`).
/// - The preceding text ends with sentence-ending punctuation (`.`, `!`, `?`).
///
/// Conservative: does not strip if the mood word appears mid-sentence or if the
/// preceding text does not end with sentence punctuation.
pub fn strip_leaked_mood_word(dialogue: &str) -> String {
    let text = dialogue.trim();
    if text.is_empty() {
        return String::new();
    }

    // Split at the last whitespace boundary.
    if let Some(last_space) = text.rfind(|c: char| c.is_whitespace()) {
        let before = text[..last_space].trim_end();
        let last_word = text[last_space..].trim_start().to_lowercase();

        let is_mood_word = LEAKED_MOOD_WORDS.contains(&last_word.as_str());
        let before_ends_with_sentence_punct = before
            .chars()
            .last()
            .map(|c| matches!(c, '.' | '!' | '?'))
            .unwrap_or(false);

        if is_mood_word && before_ends_with_sentence_punct {
            return before.to_string();
        }
    }

    text.to_string()
}

// ── #1487 — degenerate intra-response phrase repetition guard ─────────────────
//
// A small quantized model (e.g. Qwen2.5-14B-4bit) can enter a degenerate
// sampling loop that repeats a short phrase (3–8 words) 10–20 times in a row
// WITHOUT sentence-terminating punctuation between each occurrence. The result
// is a wall of near-identical commas-delimited clauses ("in His time and His
// way, and His purpose… in His time and His way…" × 15) that fills the
// token cap. `split_sentences` produces ONE entry for the whole wall, so
// `collapse_repeated_sentences` and `collapse_distributed_repeated_sentences`
// never see the repetition.
//
// This guard operates at the WORD level instead:
//  1. Split into whitespace-delimited word-tokens.
//  2. Search for the longest repeated n-gram (n ∈ [3, 8]) that appears ≥
//     DEGENERATE_REPEAT_MIN times.
//  3. If found, truncate the text at the END of the FIRST occurrence of that
//     n-gram so the surviving prefix ends cleanly.
//
// Conservative thresholds (n ≥ 3 words, repetitions ≥ 4) prevent false
// positives on natural repetition like "Aye, aye" or "day by day".

/// Detects and collapses a degenerate intra-response phrase-repetition loop
/// (#1487 — sub-sentence phrase loop guard).
///
/// When a small model repeats a short phrase (3–8 words) four or more times
/// within a single response (regardless of intervening punctuation), this
/// function truncates the text at the end of the FIRST occurrence of that
/// phrase, then trims to the last complete sentence before the loop started.
///
/// # Behaviour
///
/// - Scans for n-grams of length 3–8 (longest first, to keep the most
///   context) that appear ≥ 4 times in the word-token sequence.
/// - On detection, keeps only the text up to (and including) the end byte
///   position of the first n-gram occurrence, then trims back to the last
///   sentence-ending punctuation (`.`, `!`, `?`) so the reply ends cleanly.
/// - If no complete sentence precedes the first occurrence (e.g. the entire
///   reply IS the loop), returns the first occurrence as-is rather than a
///   blank.
/// - Replies without degenerate repetition are returned unchanged.
///
/// Exposed as `pub` so the caller in `guard_verbosity_runons` and unit tests
/// can invoke it directly.
pub fn collapse_degenerate_phrase_loop(dialogue: &str) -> String {
    /// Maximum n-gram width to consider. Longer phrases are unlikely to repeat
    /// 4× within a single NPC reply in legitimate prose.
    const MAX_NGRAM: usize = 8;
    /// Minimum n-gram width to consider. Phrases shorter than 3 words are
    /// too generic ("and the", "in a") to reliably signal degeneration.
    const MIN_NGRAM: usize = 3;
    /// Minimum repetition count to trigger the guard. Natural dialogue can
    /// repeat short phrases 2–3 times ("aye, aye" or consecutive paired
    /// clauses); 4 repetitions is the degeneration threshold.
    const MIN_REPEATS: usize = 4;

    let text = dialogue.trim();
    if text.is_empty() {
        return String::new();
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let n = words.len();
    if n < MIN_NGRAM * MIN_REPEATS {
        // Not enough words for even the shortest degenerate loop.
        return text.to_string();
    }

    // Try n-gram widths from largest to smallest so we truncate at the
    // richest possible context.
    let mut best: Option<(usize, usize, usize)> = None; // (width, first_start, repeat_count)
    'outer: for width in (MIN_NGRAM..=MAX_NGRAM).rev() {
        if width * MIN_REPEATS > n {
            continue;
        }
        // Index every n-gram start position by its normalized token tuple.
        let mut positions: std::collections::HashMap<Vec<String>, Vec<usize>> =
            std::collections::HashMap::new();
        for start in 0..=(n - width) {
            let key: Vec<String> = words[start..start + width]
                .iter()
                .map(|w| normalize_for_repetition(w))
                .collect();
            positions.entry(key).or_default().push(start);
        }
        for starts in positions.values() {
            if starts.len() >= MIN_REPEATS {
                let first = *starts.iter().min().unwrap();
                let rep = starts.len();
                // Prefer the n-gram that (a) repeats most, (b) is longest on tie.
                if best.is_none()
                    || rep > best.unwrap().2
                    || (rep == best.unwrap().2 && width > best.unwrap().0)
                {
                    best = Some((width, first, rep));
                }
                // Found at this width — no need to check further n-grams of
                // the same width (we already have a candidate). Move to the
                // next width.
                break 'outer;
            }
        }
    }

    let Some((width, first_start, _)) = best else {
        return text.to_string();
    };

    // The loop was detected. Build the prefix text from words[0..first_start+width]
    // by re-joining (single space between tokens). This avoids fragile byte-cursor
    // arithmetic on the original string and is correct for any whitespace variation.
    let prefix_joined = words[..first_start + width].join(" ");
    let prefix = prefix_joined.trim_end();
    if prefix.is_empty() {
        return text.to_string();
    }

    // Trim back to the last sentence boundary so the reply ends cleanly.
    if let Some(last_end) = prefix.rfind(['.', '!', '?']) {
        let sentence_end = last_end + 1;
        if sentence_end < prefix.len() {
            // There is trailing non-sentence content after the last complete
            // sentence within the prefix — trim it off.
            return prefix[..sentence_end].trim().to_string();
        }
        return prefix.to_string();
    }

    // No sentence boundary found before the loop: return the prefix as-is
    // rather than an empty string.
    prefix.to_string()
}

// ── #1489 — word-count cap guard ──────────────────────────────────────────────
//
// Some generations run 43–47 s and hit the token cap, producing a reply that
// may be 200+ words across 3 or fewer sentences (so `cap_sentence_count` does
// not trim it). A word-count cap is the deterministic backstop: if the reply
// exceeds MAX_WORDS words, trim at the last sentence boundary within the first
// MAX_WORDS words. This bounds time-to-display and forces the NPC to be concise.

/// Trims a dialogue string to at most `MAX_WORDS` words, cutting back to the
/// last complete sentence (#1489 — word-count cap guard).
///
/// # Behaviour
///
/// - Counts whitespace-delimited word tokens. If the total is ≤ `MAX_WORDS`,
///   the string is returned unchanged (the common, fast path).
/// - When the cap fires: finds the last sentence-ending punctuation (`.`, `!`,
///   `?`) within the first `MAX_WORDS` words and trims to that boundary.
/// - If no sentence boundary exists within the budget (very unusual — the reply
///   is one long unpunctuated run), the first `MAX_WORDS` words are returned
///   with a closing period added.
///
/// Conservative: `MAX_WORDS = 80`. A natural 2–3 sentence NPC reply in
/// Hiberno-English is 40–60 words; 80 words gives generous headroom (≈ 5
/// sentences at normal length) while preventing multi-hundred-word runaways.
pub fn cap_word_count(dialogue: &str) -> String {
    /// Maximum word count before the cap fires. 80 words ≈ 400 tokens at
    /// ~5 chars/token — well within a single inference call budget while
    /// enforcing a ceiling on runaway length.
    const MAX_WORDS: usize = 80;

    let text = dialogue.trim();
    if text.is_empty() {
        return String::new();
    }

    // Fast path: count words. If within budget, return immediately.
    let word_count = text.split_whitespace().count();
    if word_count <= MAX_WORDS {
        return text.to_string();
    }

    // Find the byte offset of the end of the MAX_WORDS-th word token.
    let mut remaining = MAX_WORDS;
    let mut budget_end = text.len();
    let mut in_word = false;
    let mut cursor = 0usize;
    for c in text.chars() {
        let was_in_word = in_word;
        in_word = !c.is_whitespace();
        // Transition non-whitespace → whitespace: a word just ended.
        if was_in_word && !in_word {
            remaining -= 1;
            if remaining == 0 {
                budget_end = cursor;
                break;
            }
        }
        cursor += c.len_utf8();
    }
    // Handle the case where the string ends mid-word (no trailing whitespace).
    if in_word && remaining == 1 {
        budget_end = text.len();
    }

    let prefix = &text[..budget_end];

    // Trim to the last sentence boundary within the prefix.
    if let Some(last_end) = prefix.rfind(['.', '!', '?']) {
        let sentence_end = last_end + 1;
        // Only use the sentence boundary if it's not before the very beginning
        // (i.e. there IS some content before it).
        if sentence_end > 1 {
            return text[..sentence_end].trim().to_string();
        }
    }

    // No sentence boundary: return the first MAX_WORDS words with a period.
    format!("{}.", prefix.trim_end())
}

/// Applies the full verbosity / run-on guard to a finalized dialogue string (#1460, #1472, #1487, #1489, #1505).
///
/// Steps, in order:
/// 1. Strip bare leaked mood-adjective from tail ([`strip_leaked_mood_word`]).
/// 2. Trim mid-sentence truncation ellipsis ([`trim_mid_sentence_truncation`]).
/// 3. Strip trailing ellipsis artifact after otherwise-complete text
///    ([`strip_trailing_ellipsis_artifact`], #1472).
/// 4. Strip stacked player vocatives such as "friend stranger"
///    ([`strip_stacked_player_vocatives`], #1534).
/// 5. **Collapse degenerate intra-response phrase-repetition loop** ([`collapse_degenerate_phrase_loop`], #1487).
///    Detects when a phrase (3–8 words) repeats ≥ 4× and truncates at the
///    first clean sentence boundary before the loop.
/// 6. **Strip consecutive short-phrase repeat** ([`strip_consecutive_short_phrase_repeat`], #1505).
///    Catches "tell me, tell me" and similar 2–4 word phrases repeated exactly
///    twice in a row with only punctuation between occurrences.
/// 7. Collapse non-consecutive near-duplicate sentences ([`collapse_distributed_repeated_sentences`]).
/// 8. Hard sentence-count cap: trim to at most 3 sentences ([`cap_sentence_count`], #1472).
///    This is the blunt backstop for paraphrased multi-beat rambles that the surgical
///    guards (steps 6-7) cannot catch.
/// 9. Cap total questions in the whole reply to at most 2 ([`cap_total_questions`]).
/// 10. Cap trailing question stack to one ([`cap_trailing_questions`]).
/// 11. **Word-count cap**: trim to at most 80 words at a clean sentence boundary
///     ([`cap_word_count`], #1489). Final backstop for long-but-few-sentence runs.
///
/// Steps 6 and 7 are the #1460 core fix for distributed / interleaved repetition
/// that `collapse_repeated_sentences` (which only handles consecutive duplicates,
/// called upstream by `guard_against_repetition`) does not catch. Step 8 caps
/// semantic content in its original order before steps 9 and 10 clean up excess
/// questions, so a tail question cannot displace a direct answer.
///
/// Conservative — does not alter legitimate prose within 3 sentences and 80 words.
pub fn guard_verbosity_runons(dialogue: &str) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }
    // Step 0 (F1 / #1526): strip CJK/JSON scaffolding leaks FIRST — these are
    // the most urgent artifact and must be removed before the subsequent guards
    // operate on clean text.
    let after_scaffold = sanitize_scaffolding_leak(dialogue);
    let after_mood = strip_leaked_mood_word(&after_scaffold);
    let after_trunc = trim_mid_sentence_truncation(&after_mood);
    let after_ellipsis = strip_trailing_ellipsis_artifact(&after_trunc);
    let after_stacked_vocatives = strip_stacked_player_vocatives(&after_ellipsis);
    let after_phrase_loop = collapse_degenerate_phrase_loop(&after_stacked_vocatives);
    let after_consec_repeat = strip_consecutive_short_phrase_repeat(&after_phrase_loop);
    // Step 5b (#1554): catch non-adjacent phrase repeats within a 20-word window
    // that the consecutive guard (step 5, strictly adjacent) misses.
    let after_nearby = collapse_nearby_phrase_repeat(&after_consec_repeat);
    let after_distributed = collapse_distributed_repeated_sentences(&after_nearby);
    // Cap in semantic order before question cleanup. This prevents a trailing
    // question from displacing a direct answer when the reply is shortened.
    let after_sentence_cap = cap_sentence_count(&after_distributed);
    let after_total_q = cap_total_questions(&after_sentence_cap);
    let after_trailing_q = cap_trailing_questions(&after_total_q);
    cap_word_count(&after_trailing_q)
}

// ── #1477 — wrong-location reference guard ────────────────────────────────────

/// Settlement collocations indicating the NPC is naming the current location.
/// If any of these patterns appears followed by a name that is NOT the actual
/// current location, the guard fires (#1477).
const WRONG_LOCATION_COLLOCATIONS: &[&str] = &[
    "here in ",
    "here at ",
    "this village of ",
    "this town of ",
    "this place called ",
    "village of ",
    "town of ",
    "in the village of ",
    "in the town of ",
    "we are in ",
    "we're in ",
    "you're in ",
    "you are in ",
    "welcome to ",
];

/// Post-generation guard that detects when an NPC names a settlement other than
/// the current location in a "here in X" or "village of X" collocation (#1477).
///
/// When the current location name is provided, scans the dialogue for settlement
/// collocations followed by a capitalized name that is NOT the current location.
/// If found, replaces the wrong settlement name with the correct one.
///
/// Conservative: only fires when a collocation is immediately followed by a
/// capitalized word sequence that does NOT match (case-insensitive) the actual
/// location name. If `current_location` is `None` or empty, returns unchanged.
///
/// Gate: `npc-wrong-location-guard` (default-on).
pub fn guard_wrong_location_reference(dialogue: &str, current_location: Option<&str>) -> String {
    let Some(loc) = current_location else {
        return dialogue.to_string();
    };
    if loc.is_empty() || dialogue.trim().is_empty() {
        return dialogue.to_string();
    }

    let loc_lower = loc.to_lowercase();
    let lower = dialogue.to_lowercase();

    for colloc in WRONG_LOCATION_COLLOCATIONS {
        if let Some(pos) = lower.find(colloc) {
            let after_pos = pos + colloc.len();
            let after = &dialogue[after_pos..];
            // Extract the named settlement: consecutive capitalized words
            // (stop at punctuation or lowercase words).
            let named: String = after
                .split_whitespace()
                .take_while(|w| {
                    let stripped = w.trim_matches(|c: char| !c.is_alphabetic());
                    !stripped.is_empty()
                        && stripped
                            .chars()
                            .next()
                            .map(|c| c.is_uppercase())
                            .unwrap_or(false)
                })
                .collect::<Vec<_>>()
                .join(" ");
            if named.is_empty() {
                continue;
            }
            let named_clean: String = named
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphabetic() || c.is_whitespace())
                .collect::<String>()
                .trim()
                .to_string();
            // If names match (or one contains the other), no issue.
            if named_clean == loc_lower
                || loc_lower.contains(&named_clean)
                || named_clean.contains(&loc_lower)
            {
                continue;
            }
            // Wrong location named: replace it with the correct one.
            tracing::warn!(
                wrong_location = %named,
                correct_location = %loc,
                "wrong-location guard fired: NPC named wrong settlement (#1477)"
            );
            let orig_colloc = &dialogue[pos..pos + colloc.len()];
            let replacement = format!("{orig_colloc}{loc}");
            let pattern = format!("{orig_colloc}{named}");
            return dialogue.replacen(&pattern, &replacement, 1);
        }
    }

    dialogue.to_string()
}

// ── #1478 — routing-after-denial guard ────────────────────────────────────────

/// Routing phrases that redirect the player to find the ungrounded person,
/// re-affirming the fabrication after a denial (#1478).
const ROUTING_AFTER_DENIAL_PHRASES: &[&str] = &[
    "ask at ",
    "ask in ",
    "try asking",
    "try at ",
    "you might find",
    "you'll find",
    "seen him at",
    "seen her at",
    "seen him in",
    "seen her in",
    "he may be at",
    "she may be at",
    "he might be at",
    "she might be at",
    "he could be at",
    "she could be at",
    "check at ",
    "check in ",
    "look for him at",
    "look for her at",
    "look for them at",
    "might find him",
    "might find her",
    "head to ",
    "try the ",
];

/// Post-generation guard: when the NPC's reply contains a denial marker for an
/// unknown person BUT also contains a routing phrase directing the player where
/// to find them, replaces with a clean non-recognition decline (#1478).
///
/// The `guard_fabricated_person_confirmation` guard correctly skips when a denial
/// is present. But the model can produce: "I know no such person... but ask at
/// Darcy's Pub — seen him there." This re-affirms the fabrication after declining.
///
/// This guard fires when:
///   1. The player input names a person NOT in `known_person_names`.
///   2. The NPC dialogue contains a denial marker (the primary guard approved it).
///   3. The NPC dialogue ALSO contains a routing phrase after the denial.
///
/// Action: replace the whole dialogue with a clean non-recognition decline.
///
/// Gate: `dialogue-person-routing-guard` (default-on).
pub fn guard_fabricated_person_routing(
    dialogue: &str,
    player_input: &str,
    known_person_names: &[String],
    player_name: Option<&str>,
    seed: u64,
) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }

    let candidates = extract_candidate_names(player_input);
    // Only fire if at least one candidate is fabricated (multi-word, not in roster).
    let has_fabricated = candidates.iter().any(|c| {
        c.split_whitespace().count() >= 2 && !name_in_roster(c, known_person_names, player_name)
    });
    if !has_fabricated {
        return dialogue.to_string();
    }

    let lower = dialogue.to_lowercase();

    // Only fires when there IS a denial marker (the primary guard approved it).
    let has_denial = DENIAL_MARKERS.iter().any(|m| lower.contains(m));
    if !has_denial {
        return dialogue.to_string();
    }

    // Fire when a routing phrase is also present.
    let has_routing = ROUTING_AFTER_DENIAL_PHRASES
        .iter()
        .any(|r| lower.contains(r));
    if !has_routing {
        return dialogue.to_string();
    }

    tracing::warn!(
        "person-routing guard fired: NPC denied then routed player to fabricated person (#1478)"
    );
    non_recognition_decline(seed).to_string()
}

// ── #1475 — wrong-speaker identity guard ──────────────────────────────────────
//
// The Qwen2.5-14B-4bit model sometimes adopts a different roster NPC's
// first-person persona mid-reply, producing lines like:
//   "Sure, I'm Brendan, the Miller's Son. Ye've caught me just 'tween chores."
// when the actual speaker is Nora Duffy.
//
// This guard runs post-generation: it scans the NPC's dialogue for first-person
// identity-claim patterns ("I'm X", "I am X", "the name's X", "my name is X")
// where X matches a DIFFERENT roster member's name or occupation, and replaces
// the entire dialogue with a short, identity-neutral recovery line.
//
// Conservative: only fires when the claimed name is a distinct roster member
// (not the speaker). Never fires for a correct self-introduction ("I'm Nora").
// When the roster has only the speaker (or is empty), the guard is a no-op.
//
// Gated by the default-on flag `npc-wrong-speaker-guard` — callers must check
// `config.flags.is_disabled("npc-wrong-speaker-guard")` and skip when true.

/// A short, period-appropriate recovery line used when an NPC adopted another
/// character's identity. Cycles through a small pool keyed by `seed` so the
/// substituted line varies across NPCs and turns.
fn wrong_speaker_recovery(seed: u64) -> &'static str {
    const POOL: &[&str] = &[
        "Aye, what can I do for ye?",
        "Grand day to ye — what brings ye here?",
        "Sure, how can I help ye today?",
        "Aye, I'm right here. What do ye need?",
        "Well now, speak yer mind.",
    ];
    POOL[(seed as usize) % POOL.len()]
}

/// Returns `true` when `dialogue` contains a first-person identity claim ("I'm
/// X", "I am X", "the name's X", "my name is X") for the given `name`.
///
/// Matching is case-insensitive. This helper is used to detect BOTH:
///   1. Legitimate self-introductions by the speaker (name == speaker_name).
///   2. Wrong-identity claims by the speaker (name == other roster member's name).
///
/// The caller decides which case applies based on whose name matched.
fn dialogue_claims_identity(dialogue: &str, name: &str) -> bool {
    // Normalise the typographic apostrophe (U+2019, which the 14B routinely
    // emits) to ASCII so the prefix list and apostrophe'd names (O'Brien)
    // match regardless of quote style.
    let lower = dialogue.to_lowercase().replace('\u{2019}', "'");
    let name_lower = name.to_lowercase().replace('\u{2019}', "'");
    let mut name_aliases = vec![name_lower.clone()];
    if let Some(rest) = name_lower
        .strip_prefix("fr. ")
        .or_else(|| name_lower.strip_prefix("fr "))
    {
        name_aliases.push(format!("father {rest}"));
    }

    // First-person identity claim patterns, case-insensitive.
    // We build the pattern by checking if any marker appears immediately before the name.
    // Apostrophes are normalised to ASCII above, so a single ASCII form covers
    // both the straight and the typographic quote the model emits.
    const IDENTITY_PREFIXES: &[&str] = &[
        "i'm ",
        "i am ",
        "the name's ",
        "the name is ",
        "my name's ",
        "my name is ",
        "it's ",  // "It's Brendan here"
        "it is ", // "It is Brendan, the …"
        "'tis ",  // Hiberno-English contraction
    ];

    for name_alias in &name_aliases {
        for prefix in IDENTITY_PREFIXES {
            let pattern = format!("{prefix}{name_alias}");
            let mut search_from = 0;
            while let Some(relative) = lower[search_from..].find(&pattern) {
                let end = search_from + relative + pattern.len();
                let boundary_is_valid = lower[end..]
                    .chars()
                    .next()
                    .is_none_or(|next| !next.is_alphanumeric() && next != '\'');
                if boundary_is_valid {
                    return true;
                }
                search_from = end;
            }
        }

        // Hiberno-English/postfixed forms put the name before the identifying
        // phrase: "Peig Hannigan's the name" / "Peig Hannigan is the name".
        for suffix in ["'s the name", " is the name"] {
            if lower.contains(&format!("{name_alias}{suffix}")) {
                return true;
            }
        }
    }
    false
}

/// Returns whether the delivered dialogue explicitly establishes the
/// speaker's canonical identity.
///
/// The full authored name must be spoken in a first-person identity form. A
/// metadata field, an exchange occurring, or a third-person mention is not
/// enough to reveal an anonymous NPC's name (#1776).
pub fn dialogue_self_identifies_speaker(dialogue: &str, speaker_name: &str) -> bool {
    dialogue_claims_identity(dialogue, speaker_name)
}

/// Post-generation guard for wrong-speaker identity (#1475).
///
/// Detects when the NPC's dialogue contains a first-person claim to be a
/// DIFFERENT roster member (e.g. Nora Duffy saying "I'm Brendan, the
/// Miller's Son") and replaces the dialogue with a short recovery line.
///
/// # Parameters
///
/// - `dialogue`: the finalized NPC reply.
/// - `speaker_name`: the actual NPC's real name (e.g. "Nora Duffy").
/// - `roster`: all roster entries for this NPC's known-people list as
///   `(name, occupation)` pairs. The speaker's own entry may be included;
///   it is skipped when matching. Pass `&[]` when no roster is available.
/// - `seed`: deterministic seed for recovery pool selection.
///
/// # Behaviour
///
/// 1. If the roster has fewer than 2 members total (including speaker),
///    return dialogue unchanged — no other identity to steal.
/// 2. For each other roster member (name ≠ speaker_name, case-insensitive):
///    a. Check the full name ("I'm Brendan Walsh").
///    b. Check the first name only ("I'm Brendan") — but only when no other
///    roster member (including the speaker) shares that first name, to
///    avoid false positives where a common first name appears.
/// 3. If any match fires, log a warning and return a recovery line.
/// 4. Otherwise return the dialogue unchanged.
///
/// Conservative: does NOT fire for third-person mentions ("Brendan is at the
/// mill") — only first-person identity-claim patterns.
pub fn guard_wrong_speaker_identity(
    dialogue: &str,
    speaker_name: &str,
    roster: &[(String, String)],
    seed: u64,
) -> String {
    if dialogue.trim().is_empty() || roster.len() < 2 {
        return dialogue.to_string();
    }

    let speaker_lower = speaker_name.to_lowercase();

    // Pre-compute how many roster members share each first name so we can
    // avoid false positives when a first name is ambiguous.
    let mut first_name_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (name, _) in roster {
        if let Some(first) = name.split_whitespace().next() {
            *first_name_counts.entry(first.to_lowercase()).or_insert(0) += 1;
        }
    }

    // Hoisted out of the loop — `speaker_name` is constant throughout.
    let speaker_first_lower = speaker_name
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();

    for (roster_name, _occupation) in roster {
        // Skip the speaker's own entry.
        if roster_name.to_lowercase() == speaker_lower {
            continue;
        }

        // Check full name first (e.g. "I'm Brendan Walsh").
        if dialogue_claims_identity(dialogue, roster_name) {
            tracing::warn!(
                speaker = %speaker_name,
                claimed_identity = %roster_name,
                "wrong-speaker-identity guard fired: NPC claimed another roster member's full name (#1475)"
            );
            return wrong_speaker_recovery(seed).to_string();
        }

        // Check first name only (e.g. "I'm Brendan") — but only when the
        // first name is unambiguous (appears exactly once in the roster and
        // differs from the speaker's own first name).
        if let Some(roster_first) = roster_name.split_whitespace().next() {
            let roster_first_lower = roster_first.to_lowercase();
            // Only fire on first-name-only if the first name is unique in the
            // roster and is not the speaker's own first name.
            let count = first_name_counts
                .get(&roster_first_lower)
                .copied()
                .unwrap_or(0);
            if count == 1
                && roster_first_lower != speaker_first_lower
                && dialogue_claims_identity(dialogue, roster_first)
            {
                tracing::warn!(
                    speaker = %speaker_name,
                    claimed_identity_first_name = %roster_first,
                    claimed_identity_full = %roster_name,
                    "wrong-speaker-identity guard fired: NPC claimed another roster member's first name (#1475)"
                );
                return wrong_speaker_recovery(seed).to_string();
            }
        }
    }

    dialogue.to_string()
}

// ── #1779 / #1786 / #1788 / #1790 — semantic dialogue contracts ─────────────

fn mood_rejects_warm_pleasantry(mood: &str) -> bool {
    let mood = mood.to_lowercase();
    [
        "sharp",
        "curt",
        "caustic",
        "acerbic",
        "irritat",
        "frustrat",
        "annoyed",
        "grumpy",
        "angry",
        "furious",
        "irate",
        "bitter",
        "resentful",
        "sour",
        "suspicious",
        "wary",
        "distrustful",
        "busy",
        "distracted",
        "preoccupied",
    ]
    .iter()
    .any(|keyword| mood.contains(keyword))
}

fn is_warm_pleasantry(sentence: &str) -> bool {
    let sentence = normalize_for_repetition(sentence)
        .replace('\u{2019}', "'")
        .trim_start_matches("ah ")
        .trim_start_matches("well ")
        .to_string();
    [
        "good morning",
        "good day",
        "fine morning",
        "fine day",
        "welcome",
        "glad to see ye",
        "glad to see you",
        "lovely to see ye",
        "lovely to see you",
        "cead mile failte",
        "céad míle fáilte",
    ]
    .iter()
    .any(|opener| sentence.starts_with(opener))
}

fn dialogue_expresses_negative_register(dialogue: &str) -> bool {
    let dialogue = normalize_for_repetition(&dialogue.replace('\u{2019}', "'"));
    [
        "make it quick",
        "quickly then",
        "plainly then",
        "if ye must know",
        "if you must know",
        "little patience",
        "no patience",
        "don't have time",
        "do not have time",
        "not in the mood",
        "mind yourself",
        "spare me",
        "don't know ye",
        "do not know ye",
        "don't know you",
        "do not know you",
        "what do ye want",
        "what do you want",
        "leave me be",
        "enough now",
    ]
    .iter()
    .any(|marker| dialogue.contains(marker))
}

fn negative_register_prefix(mood: &str) -> &'static str {
    let mood = mood.to_lowercase();
    if ["bitter", "resentful", "sour"]
        .iter()
        .any(|keyword| mood.contains(keyword))
    {
        "If ye must know—"
    } else if ["suspicious", "wary", "distrustful"]
        .iter()
        .any(|keyword| mood.contains(keyword))
    {
        "Mind, I don't know ye—"
    } else if ["busy", "distracted", "preoccupied"]
        .iter()
        .any(|keyword| mood.contains(keyword))
    {
        "Quickly, then—"
    } else if [
        "irritat", "frustrat", "annoyed", "grumpy", "angry", "furious", "irate",
    ]
    .iter()
    .any(|keyword| mood.contains(keyword))
    {
        "Make it quick—"
    } else {
        "Plainly, then—"
    }
}

/// Makes an explicitly negative authored mood audible in the delivered line.
///
/// This is intentionally narrower than sentiment analysis. It removes a
/// contradictory standalone warm opener, preserves every substantive sentence,
/// and—only when the remainder has no unmistakable negative-register marker—
/// adds one compact mood-specific spoken cue. The cue has no sentence-ending
/// punctuation, so it cannot consume a sentence from the reply budget.
pub fn guard_mood_register(dialogue: &str, canonical_mood: &str) -> String {
    if dialogue.trim().is_empty() || !mood_rejects_warm_pleasantry(canonical_mood) {
        return dialogue.to_string();
    }
    let sentences = split_sentences(dialogue);
    let without_warm_opener = if sentences
        .first()
        .is_some_and(|sentence| is_warm_pleasantry(sentence))
    {
        tracing::warn!(
            mood = %canonical_mood,
            "mood-register guard removed contradictory warm pleasantry (#1779)"
        );
        sentences[1..]
            .iter()
            .map(|sentence| sentence.trim())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        dialogue.to_string()
    };
    let substantive = if without_warm_opener.trim().is_empty() {
        "What is it?".to_string()
    } else {
        without_warm_opener
    };
    if dialogue_expresses_negative_register(&substantive) {
        return substantive;
    }

    tracing::warn!(
        mood = %canonical_mood,
        "mood-register guard added an authored negative-register cue (#1779)"
    );
    format!(
        "{}{}",
        negative_register_prefix(canonical_mood),
        substantive.trim_start()
    )
}

fn sentence_claims_prior_contact(sentence: &str) -> bool {
    let sentence = normalize_for_repetition(&sentence.replace('\u{2019}', "'"));
    [
        "i've seen ye around",
        "i have seen ye around",
        "i've seen you around",
        "i have seen you around",
        "seen ye about",
        "seen you about",
        "we haven't spoken",
        "we have not spoken",
        "we've spoken before",
        "we have spoken before",
        "good to see ye again",
        "good to see you again",
        "welcome back",
        "we meet again",
        "last time we spoke",
        "as i told ye",
        "as i told you",
        "as we discussed",
    ]
    .iter()
    .any(|claim| sentence.contains(claim))
}

/// Removes explicit prior-familiarity claims from an NPC's first exchange with
/// the player (#1786).
pub fn guard_unfounded_first_contact_familiarity(
    dialogue: &str,
    had_prior_exchange: bool,
) -> String {
    if dialogue.trim().is_empty() || had_prior_exchange {
        return dialogue.to_string();
    }
    let sentences = split_sentences(dialogue);
    let kept: Vec<&str> = sentences
        .iter()
        .filter(|sentence| !sentence_claims_prior_contact(sentence))
        .map(|sentence| sentence.trim())
        .collect();
    if kept.len() == sentences.len() {
        return dialogue.to_string();
    }

    tracing::warn!("first-contact guard removed unfounded prior familiarity (#1786)");
    if kept.is_empty() {
        "Aye. What brings ye here?".to_string()
    } else {
        kept.join(" ")
    }
}

fn player_asks_for_firsthand_evidence(player_input: &str) -> bool {
    let input = normalize_for_repetition(&player_input.replace('\u{2019}', "'"));
    (input.contains("have you seen")
        || input.contains("have ye seen")
        || input.contains("did you see")
        || input.contains("did ye see")
        || input.contains("with your own eyes")
        || input.contains("with yer own eyes")
        || input.contains("seen something here yourself"))
        && (input.contains("or ")
            || input.contains("yourself")
            || input.contains("evidence")
            || input.contains("own eyes"))
}

fn dialogue_gives_firsthand_or_honest_answer(dialogue: &str) -> bool {
    let dialogue = normalize_for_repetition(&dialogue.replace('\u{2019}', "'"));
    let hedged = [
        "might say",
        "mayhap",
        "perhaps",
        "in a manner",
        "sort of",
        "hard to say",
    ]
    .iter()
    .any(|hedge| dialogue.contains(hedge));
    let concrete_firsthand = [
        "i saw ",
        "i've seen ",
        "i have seen ",
        "i watched ",
        "i witnessed ",
        "i was there",
        "with my own eyes",
        "with me own eyes",
    ]
    .iter()
    .any(|marker| dialogue.contains(marker));
    let honest_limit = [
        "i haven't seen",
        "i have not seen",
        "i never saw",
        "never seen it",
        "i don't know",
        "i do not know",
        "i cannot say",
        "i can't say",
        "only know it from",
        "only an old tale",
        "heard the tale",
        "heard it told",
    ]
    .iter()
    .any(|marker| dialogue.contains(marker));
    honest_limit || (concrete_firsthand && !hedged)
}

fn dialogue_turns_question_back(dialogue: &str) -> bool {
    split_sentences(dialogue).last().is_some_and(|sentence| {
        let normalized = normalize_for_repetition(sentence);
        sentence.contains('?')
            && [" ye ", " you ", " yer ", " your "]
                .iter()
                .any(|marker| format!(" {normalized} ").contains(marker))
    })
}

/// Replaces a narrow evidence-question evasion with an honest answer rather
/// than inventing a sighting (#1788).
///
/// The guard fires only when the player explicitly contrasts firsthand
/// evidence with hearsay, the reply supplies neither a concrete claim nor an
/// honest limit, and it turns a question back on the player.
pub fn guard_direct_evidence_evasion(dialogue: &str, player_input: &str) -> String {
    if dialogue.trim().is_empty()
        || !player_asks_for_firsthand_evidence(player_input)
        || dialogue_gives_firsthand_or_honest_answer(dialogue)
        || !dialogue_turns_question_back(dialogue)
    {
        return dialogue.to_string();
    }

    tracing::warn!("answer-first guard replaced an evidence-question evasion (#1788)");
    "I cannot say I saw it myself.".to_string()
}

#[derive(Clone, Copy)]
enum WorkDomain {
    Farm,
    Forge,
    School,
    Mill,
    Weaving,
    Boat,
    Shop,
    Pub,
    Church,
    Midwifery,
    ManualHands,
}

impl WorkDomain {
    fn label(self) -> &'static str {
        match self {
            Self::Farm => "farm work",
            Self::Forge => "forge work",
            Self::School => "school work",
            Self::Mill => "mill work",
            Self::Weaving => "weaving work",
            Self::Boat => "boat or fishing work",
            Self::Shop => "shop work",
            Self::Pub => "work at the public house",
            Self::Church => "church matters",
            Self::Midwifery => "midwifery",
            Self::ManualHands => "work needing another pair of hands",
        }
    }

    fn occupation_keywords(self) -> &'static [&'static str] {
        match self {
            Self::Farm => &["farmer", "farm boy", "labourer"],
            Self::Forge => &["blacksmith", "blacksmith's apprentice"],
            Self::School => &["hedge school teacher", "teacher"],
            Self::Mill => &["miller"],
            Self::Weaving => &["weaver"],
            Self::Boat => &["boatman", "fisherman"],
            Self::Shop => &["shopkeeper"],
            Self::Pub => &["publican"],
            Self::Church => &["parish priest", "priest"],
            Self::Midwifery => &["midwife"],
            Self::ManualHands => &[
                "farmer",
                "farm boy",
                "labourer",
                "blacksmith",
                "miller",
                "weaver",
                "boatman",
            ],
        }
    }

    fn workplace_keywords(self) -> &'static [&'static str] {
        match self {
            Self::Farm | Self::ManualHands => &["farm"],
            Self::Forge => &["forge"],
            Self::School => &["school"],
            Self::Mill => &["mill"],
            Self::Weaving => &["weaver"],
            Self::Boat => &["boat", "lough", "bay"],
            Self::Shop => &["shop"],
            Self::Pub => &["pub"],
            Self::Church => &["church"],
            Self::Midwifery => &[],
        }
    }
}

fn requested_work_domain(player_input: &str) -> Option<WorkDomain> {
    let input = normalize_for_repetition(&player_input.replace('\u{2019}', "'"));
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let has_token = |candidates: &[&str]| {
        tokens
            .iter()
            .any(|token| candidates.iter().any(|candidate| token == candidate))
    };
    let has_stem = |stems: &[&str]| {
        tokens
            .iter()
            .any(|token| stems.iter().any(|stem| token.starts_with(stem)))
    };
    if !(has_token(&["work", "job", "jobs", "hire", "hired", "hiring"])
        || input.contains("pair of hands")
        || input.contains("another hand"))
    {
        return None;
    }
    if has_token(&[
        "farm", "farms", "field", "fields", "crop", "crops", "potato", "potatoes", "cattle",
        "sheep", "beast", "beasts", "harvest", "plough",
    ]) {
        return Some(WorkDomain::Farm);
    }
    if has_token(&["forge", "iron", "horseshoe", "horseshoes", "anvil"]) {
        return Some(WorkDomain::Forge);
    }
    if has_token(&["school", "pupil", "pupils", "book", "books"]) || has_stem(&["teach"]) {
        return Some(WorkDomain::School);
    }
    if has_token(&["mill", "flour", "grain"]) {
        return Some(WorkDomain::Mill);
    }
    if has_token(&["cloth", "thread"]) || has_stem(&["weav"]) {
        return Some(WorkDomain::Weaving);
    }
    if has_token(&["boat", "boats", "lough"]) || has_stem(&["fish"]) {
        return Some(WorkDomain::Boat);
    }
    if has_token(&["shop", "shops", "shopkeeper", "goods", "counter"]) {
        return Some(WorkDomain::Shop);
    }
    if has_token(&["pub", "ale", "drink", "drinks"]) || input.contains("public house") {
        return Some(WorkDomain::Pub);
    }
    if has_token(&["church", "souls"]) || input.contains("parish priest") {
        return Some(WorkDomain::Church);
    }
    if has_token(&["midwife", "birth", "births", "herb", "herbs"]) {
        return Some(WorkDomain::Midwifery);
    }
    if input.contains("pair of hands") || input.contains("another hand") {
        return Some(WorkDomain::ManualHands);
    }
    None
}

fn work_entry_matches_domain(
    occupation: &str,
    workplace: Option<&str>,
    domain: WorkDomain,
) -> bool {
    let occupation = occupation.to_lowercase();
    if occupation.contains("retired") {
        return false;
    }
    domain
        .occupation_keywords()
        .iter()
        .any(|keyword| occupation.contains(keyword))
        || workplace.is_some_and(|workplace| {
            let workplace = workplace.to_lowercase();
            domain
                .workplace_keywords()
                .iter()
                .any(|keyword| workplace.contains(keyword))
        })
}

fn dialogue_recommends_person(dialogue: &str, name: &str) -> bool {
    let dialogue = normalize_for_repetition(&dialogue.replace('\u{2019}', "'"));
    let name = normalize_for_repetition(&name.replace('\u{2019}', "'"));
    [
        format!("ask {name}"),
        format!("asking {name}"),
        format!("speak to {name}"),
        format!("speak with {name}"),
        format!("try {name}"),
        format!("work for {name}"),
        format!("help {name}"),
        format!("{name} might"),
        format!("{name} could"),
        format!("{name} needs"),
        format!("{name} is your man"),
        format!("{name} is your woman"),
        format!("{name} is the person"),
        format!("{name} is the one"),
    ]
    .iter()
    .any(|pattern| dialogue.contains(pattern))
}

/// Corrects a named work referral that contradicts authored occupation or
/// workplace data when a grounded matching person is available (#1790).
pub fn guard_work_recommendation(
    dialogue: &str,
    player_input: &str,
    work_roster: &[(String, String, Option<String>)],
) -> String {
    let Some(domain) = requested_work_domain(player_input) else {
        return dialogue.to_string();
    };
    let recommended: Vec<&(String, String, Option<String>)> = work_roster
        .iter()
        .filter(|(name, _, _)| dialogue_recommends_person(dialogue, name))
        .collect();
    if recommended.is_empty()
        || recommended.iter().all(|(_, occupation, workplace)| {
            work_entry_matches_domain(occupation, workplace.as_deref(), domain)
        })
    {
        return dialogue.to_string();
    }

    let replacement = domain.occupation_keywords().iter().find_map(|keyword| {
        work_roster.iter().find(|(_, occupation, workplace)| {
            let occupation_lower = occupation.to_lowercase();
            !occupation_lower.contains("retired")
                && occupation_lower.contains(keyword)
                && work_entry_matches_domain(occupation, workplace.as_deref(), domain)
        })
    });
    let replacement = replacement.or_else(|| {
        work_roster.iter().find(|(_, occupation, workplace)| {
            work_entry_matches_domain(occupation, workplace.as_deref(), domain)
        })
    });
    let Some((name, occupation, workplace)) = replacement else {
        return dialogue.to_string();
    };

    tracing::warn!(
        recommended = ?recommended.iter().map(|(name, _, _)| name).collect::<Vec<_>>(),
        replacement = %name,
        "work-referral grounding guard corrected an occupation mismatch (#1790)"
    );
    let workplace = workplace
        .as_deref()
        .filter(|place| !place.trim().is_empty())
        .map(|place| format!(" at {place}"))
        .unwrap_or_default();
    format!(
        "For {}, ye'd best ask {}, the {}{}.",
        domain.label(),
        name,
        occupation,
        workplace
    )
}

// ── #1504 — acquaintance-question / identity-drift guard ──────────────────────
//
// Quality-harness run 16, turn 14: the player asked Seamus Gallagher
// "Do you know Father Declan Tierney?" and the NPC replied
// "Ye must be mistaken... I'm but Seamus Gallagher" — answering as if
// asked "Are you Father Declan Tierney?" (identity challenge) rather than
// "Do you know him?" (acquaintance question).
//
// Root cause: the grounding block's "do not go along with a presupposed name"
// instruction is broad enough that the 14B model reads "do you know X?" as
// an identity challenge and produces a self-identification response ("I'm but
// Seamus") that never addresses whether the NPC knows the person.
//
// Detection strategy (conservative):
//   1. Detect whether the player's input is an ACQUAINTANCE question:
//      patterns like "do you know …", "have you met …", "do you know of …",
//      "are you acquainted with …", "have you heard of …".
//   2. Detect whether the NPC's dialogue is purely a SELF-IDENTITY assertion:
//      contains an identity-claim prefix ("i'm but", "i am but", "i'm only",
//      etc.) with the SPEAKER'S OWN name and NO acquaintance content.
//   3. If both fire: the NPC answered the wrong question. Replace with
//      - "Yes, I know them" stock text if the named person IS in the roster.
//      - "I know no one by that name" stock text if NOT in the roster.
//
// Gated by default-on flag `npc-acquaintance-intent-guard` (kill-switch only).

/// Returns `true` when `player_input` reads as an acquaintance question —
/// a question about whether the NPC knows or has met someone.
///
/// Patterns matched (case-insensitive):
/// - "do you know …"
/// - "do ye know …"
/// - "do you know of …"
/// - "do ye know of …"
/// - "have you met …"
/// - "have ye met …"
/// - "have you heard of …"
/// - "have ye heard of …"
/// - "are you acquainted with …"
/// - "are ye acquainted with …"
/// - "do you know who …"
fn is_acquaintance_question(player_input: &str) -> bool {
    let lower = player_input.trim().to_lowercase();
    const ACQUAINTANCE_PREFIXES: &[&str] = &[
        "do you know of ",
        "do ye know of ",
        "do you know who ",
        "do ye know who ",
        "do you know ",
        "do ye know ",
        "have you met ",
        "have ye met ",
        "have you heard of ",
        "have ye heard of ",
        "are you acquainted with ",
        "are ye acquainted with ",
    ];
    // Use `contains` so that embedded/trailing acquaintance phrases are caught
    // regardless of word order or presence of a trailing "?" — e.g.
    // "Seamus, do you know Father Declan" (no "?") and
    // "do you know Father Declan Tierney?" (leading) both match.
    for prefix in ACQUAINTANCE_PREFIXES {
        if lower.contains(prefix) {
            return true;
        }
    }
    false
}

/// Returns `true` when `dialogue` is purely a self-identity assertion with no
/// acquaintance content — the NPC only said who THEY are, never addressed
/// whether they know the person the player asked about.
///
/// Detects dismissive / clarifying self-identity patterns:
/// - "i'm but <name>", "i am but <name>" (Hiberno-English dismissive)
/// - "i'm only <name>", "i am only <name>"
/// - "i'm just <name>", "i am just <name>"
/// - "my name is <name>", "the name's <name>" — without any acquaintance info
///
/// Only fires when the dialogue ALSO does NOT contain acquaintance content
/// words ("know", "met", "heard", "acquainted", "familiar").
fn dialogue_is_only_self_identity(dialogue: &str, speaker_name: &str) -> bool {
    if dialogue.trim().is_empty() {
        return false;
    }
    let lower = dialogue.to_lowercase().replace('\u{2019}', "'");
    let speaker_lower = speaker_name.to_lowercase();

    // Guard: if the dialogue contains acquaintance-response words, the NPC
    // DID address the question — don't fire.
    const ACQUAINTANCE_CONTENT_WORDS: &[&str] = &[
        "know",
        "met",
        "heard",
        "acquainted",
        "familiar",
        "never seen",
        "no one by",
        "no such",
    ];
    for word in ACQUAINTANCE_CONTENT_WORDS {
        if lower.contains(word) {
            return false;
        }
    }

    // Guard: the speaker's own name must appear (it's about self-identification).
    if !lower.contains(&speaker_lower) {
        // Also check first-name only.
        // Strip trailing non-alphabetic chars (e.g. "fr." → "fr") so that
        // `text_contains_name_as_word` — which also strips punctuation from
        // dialogue tokens before comparing — does not fail on honorific
        // abbreviations like "Fr." stored as the roster's first token.
        let speaker_first = speaker_lower
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(|c: char| !c.is_alphabetic())
            .to_string();
        if speaker_first.len() < 2 || !text_contains_name_as_word(dialogue, &speaker_first) {
            return false;
        }
    }

    // Identity-claim patterns that indicate the NPC answered with self-identification.
    const SELF_IDENTITY_MARKERS: &[&str] = &[
        "i'm but ",
        "i am but ",
        "i'm only ",
        "i am only ",
        "i'm just ",
        "i am just ",
        "i'm merely ",
        "i am merely ",
        "'tis only ",
        "'tis but ",
        "ye must be mistaken",
        "you must be mistaken",
        "ye have the wrong",
        "you have the wrong",
    ];
    for marker in SELF_IDENTITY_MARKERS {
        if lower.contains(marker) {
            return true;
        }
    }

    false
}

/// Stock acquaintance-affirmation responses — used when the NPC DOES know the
/// named person (name is in their roster) but answered the wrong question.
fn acquaintance_affirm(seed: u64) -> &'static str {
    const POOL: &[&str] = &[
        "Aye, I know them well enough.",
        "Aye, I've met them in the parish.",
        "I do know who ye mean.",
        "Aye, the name is known to me.",
        "Sure, I know them from hereabouts.",
    ];
    POOL[(seed as usize) % POOL.len()]
}

/// Post-generation guard for acquaintance-question / identity-drift (#1504).
///
/// Fires when the player asked an acquaintance question ("do you know X?")
/// and the NPC responded with a pure self-identity assertion ("I'm but Seamus
/// Gallagher") — answering the wrong question entirely.
///
/// # Parameters
///
/// - `dialogue`: the finalized NPC reply.
/// - `player_input`: the triggering player utterance.
/// - `speaker_name`: the actual NPC's real name (e.g. "Seamus Gallagher").
/// - `known_person_names`: all parish NPC names the guard may affirm about.
///   If the name from the player's question is in this list, the replacement
///   is an acquaintance-affirmation; otherwise it is a non-recognition decline.
/// - `seed`: deterministic seed for response pool selection.
///
/// Conservative: only fires when BOTH the acquaintance-question signal AND the
/// self-identity signal are present. Never fires on normal dialogue.
pub fn guard_acquaintance_question_intent_drift(
    dialogue: &str,
    player_input: &str,
    speaker_name: &str,
    known_person_names: &[String],
    seed: u64,
) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }

    // Only fire when the player asked an acquaintance question.
    if !is_acquaintance_question(player_input) {
        return dialogue.to_string();
    }

    // Only fire when the NPC's reply is purely a self-identity assertion.
    if !dialogue_is_only_self_identity(dialogue, speaker_name) {
        return dialogue.to_string();
    }

    // Determine whether the named person is in the roster to choose the
    // right replacement. We extract candidate names from the player input
    // and check each against the roster.
    let candidates = extract_candidate_names(player_input);
    let named_person_is_known = candidates
        .iter()
        .any(|c| name_in_roster(c, known_person_names, None));

    tracing::warn!(
        speaker = %speaker_name,
        player_input = %player_input,
        named_person_is_known,
        "acquaintance-question intent-drift guard fired: NPC answered identity \
         question instead of acquaintance question (#1504)"
    );

    if named_person_is_known {
        acquaintance_affirm(seed).to_string()
    } else {
        non_recognition_decline(seed).to_string()
    }
}

// ── #1526 — CJK / JSON scaffolding sanitizer ──────────────────────────────────

/// Feature-flag name (default **on**) that enables the CJK/JSON scaffolding
/// sanitizer (#1526). Kill-switch: disable via
/// `flags.is_disabled(SCAFFOLDING_SANITIZER_FLAG)`.
pub const SCAFFOLDING_SANITIZER_FLAG: &str = "dialogue-scaffolding-sanitizer";

/// Sanitizes CJK script and JSON scaffolding leaks from a dialogue string (#1526).
///
/// Small models occasionally emit CJK meta-reasoning (self-correction thoughts
/// written in Chinese/Japanese/Korean) or raw JSON brace characters inside the
/// `dialogue` field after the JSON envelope is unwrapped. This function detects
/// and removes that suffix.
///
/// Detection rules (applied in document order; first match wins):
/// - **CJK character**: any character in U+2E80..U+9FFF, U+F900..U+FAFF, or
///   U+20000..U+2A6DF.
/// - **JSON brace leak**: a `{` that is preceded only by whitespace or a newline
///   on its line (i.e. a line that starts with `{` after trimming leading
///   whitespace — the model's self-correction JSON object).
///
/// When a scaffold position is found:
/// 1. Trim any trailing whitespace/newlines before the scaffold start.
/// 2. Find the last complete sentence ending in `.`, `!`, or `?` before the
///    scaffold position. If found, return that prefix (trimmed).
/// 3. If no complete sentence precedes the scaffold, return the text before the
///    scaffold character (trimmed).
///
/// Clean input (no scaffold) is returned unchanged.
pub fn sanitize_scaffolding_leak(dialogue: &str) -> String {
    // Find the first scaffold position.
    let mut scaffold_pos: Option<usize> = None;

    let chars: Vec<(usize, char)> = dialogue.char_indices().collect();

    for (byte_pos, ch) in chars {
        let cp = ch as u32;

        // CJK ranges.
        let is_cjk = (0x2E80..=0x9FFF).contains(&cp)
            || (0xF900..=0xFAFF).contains(&cp)
            || (0x20000..=0x2A6DF).contains(&cp);

        if is_cjk {
            scaffold_pos = Some(byte_pos);
            break;
        }

        // JSON brace leak: `{` preceded only by whitespace or newline on its
        // line. We check whether everything from the previous newline to this
        // `{` is whitespace.
        if ch == '{' {
            // Find the byte of the most recent newline before byte_pos.
            let line_start = dialogue[..byte_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
            let before_brace = &dialogue[line_start..byte_pos];
            if before_brace.trim().is_empty() {
                scaffold_pos = Some(byte_pos);
                break;
            }
        }
    }

    let Some(pos) = scaffold_pos else {
        return dialogue.to_string();
    };

    // Take the prefix up to the scaffold, trim trailing whitespace/newlines.
    let prefix = dialogue[..pos].trim_end_matches(|c: char| c.is_whitespace());

    // Find the last complete sentence ending in `.`, `!`, or `?`.
    if let Some(end) = prefix.rfind(['.', '!', '?']) {
        // Include the sentence-ending character (end is a byte offset of that char).
        let sentence_end = end
            + prefix[end..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        let sentence = prefix[..sentence_end].trim();
        if !sentence.is_empty() {
            tracing::warn!(
                scaffold_byte = pos,
                "scaffolding-sanitizer stripped CJK/JSON scaffold from dialogue (#1526)"
            );
            return sentence.to_string();
        }
    }

    // No complete sentence before scaffold: return the trimmed prefix.
    let trimmed = prefix.trim();
    if !trimmed.is_empty() {
        tracing::warn!(
            scaffold_byte = pos,
            "scaffolding-sanitizer stripped CJK/JSON scaffold (no prior sentence) from dialogue (#1526)"
        );
        return trimmed.to_string();
    }

    // Scaffold was at the very start — return empty string rather than the original.
    String::new()
}

// ── #1527/#1528 — false denial of known roster person guard ───────────────────

/// Feature-flag name (default **on**) that enables the false-denial guard
/// (#1527, #1528). Kill-switch: disable via
/// `flags.is_disabled(FALSE_DENIAL_GUARD_FLAG)`.
pub const FALSE_DENIAL_GUARD_FLAG: &str = "dialogue-false-denial-guard";

/// Returns a grounded acknowledgement confirming a known parish member.
/// Cycles through a small pool so successive replies vary.
fn grounded_acknowledgement(seed: u64) -> &'static str {
    const POOL: &[&str] = &[
        "Aye, I know the name well enough from the parish.",
        "Sure, that name is known hereabouts.",
        "Aye, they are of this parish right enough.",
        "I've heard that name in these parts, sure enough.",
        "Aye, that's a name known to me in the parish.",
    ];
    POOL[(seed as usize) % POOL.len()]
}

fn grounded_place_acknowledgement(seed: u64) -> &'static str {
    const POOL: &[&str] = &[
        "Aye, that place is known in this parish.",
        "That is a real place hereabouts, sure enough.",
        "Aye, the place is known here in the parish.",
        "That place is no invention; it is known around here.",
        "Aye, I know the name of that place well enough.",
    ];
    POOL[(seed as usize) % POOL.len()]
}

const GENERIC_PERSON_NONRECOGNITION_MARKERS: &[&str] = &[
    "no one by that name",
    "no such person",
    "know no one by that name",
    "know of no such person",
    "never heard of him",
    "never heard of her",
    "never heard of them",
    "that name is not known",
    "wrong parish",
];

const GENERIC_ENTITY_NONRECOGNITION_MARKERS: &[&str] = &[
    "no one by that name",
    "no such person",
    "no such place",
    "no place by that name",
    "know of no such place",
    "doesn't exist",
    "does not exist",
    "place that doesn't exist",
    "place that does not exist",
    "that name is not known",
    "wrong parish",
    "wrong place",
];

fn has_generic_person_nonrecognition(lower_dialogue: &str) -> bool {
    GENERIC_PERSON_NONRECOGNITION_MARKERS
        .iter()
        .any(|marker| lower_dialogue.contains(marker))
}

fn has_generic_entity_nonrecognition(lower_dialogue: &str) -> bool {
    GENERIC_ENTITY_NONRECOGNITION_MARKERS
        .iter()
        .any(|marker| lower_dialogue.contains(marker))
}

/// Post-generation guard for false denial of a known roster person (#1527, #1528).
///
/// When an NPC's dialogue contains a denial marker AND the denied name IS in
/// `known_person_names` (the full parish roster), the denial is incorrect — the
/// NPC is wrongly claiming ignorance of a real parish member. Replace with a
/// neutral grounded acknowledgement.
///
/// Conservative: only fires when ALL of the following hold:
///   1. The player's input names a person whose full name IS in `known_person_names`.
///   2. The NPC dialogue contains a denial marker.
///   3. Either the dialogue repeats the full name, or the player input contains
///      only known full-name candidates and the dialogue uses a generic
///      "that name/no such person" non-recognition phrase.
///
/// Gate: `dialogue-false-denial-guard` (default-on).
pub fn guard_false_denial_of_roster_person(
    dialogue: &str,
    player_input: &str,
    known_person_names: &[String],
    player_name: Option<&str>,
    seed: u64,
) -> String {
    guard_false_denial_of_roster_person_with_speaker(
        dialogue,
        player_input,
        known_person_names,
        player_name,
        seed,
        None,
    )
}

/// Speaker-aware variant of [`guard_false_denial_of_roster_person`].
///
/// In command text like `talk to Roisin Connolly about Martin`, the addressed
/// NPC's own full name is present in the input but is not the subject being
/// denied. This variant ignores that addressed-speaker candidate when an
/// explicit `about ...` topic names someone else.
pub fn guard_false_denial_of_roster_person_with_speaker(
    dialogue: &str,
    player_input: &str,
    known_person_names: &[String],
    player_name: Option<&str>,
    seed: u64,
    speaker: Option<&DialogueSpeakerContext>,
) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }

    let lower = dialogue.to_lowercase();

    // Must contain a denial marker to trigger.
    let has_denial = DENIAL_MARKERS.iter().any(|m| lower.contains(m));
    if !has_denial {
        return dialogue.to_string();
    }

    // Check each candidate name from the player's input.
    let candidates = extract_candidate_names(player_input);
    let mut has_known_full_name = false;
    let mut has_unknown_full_name = false;
    for candidate in &candidates {
        // Only fire for full names (multi-word) that ARE in the roster.
        if candidate.split_whitespace().count() < 2 {
            continue;
        }
        if speaker
            .map(|speaker| {
                addressed_speaker_candidate_is_not_topic(candidate, player_input, speaker)
            })
            .unwrap_or(false)
        {
            continue;
        }
        if !name_in_roster(candidate, known_person_names, player_name) {
            has_unknown_full_name = true;
            continue;
        }
        has_known_full_name = true;
        // The candidate is a real roster person. Check if the dialogue mentions them.
        let cand_lower = candidate.to_lowercase();
        if !lower.contains(&cand_lower) {
            continue;
        }
        // All conditions met: NPC is incorrectly denying a real roster person.
        tracing::warn!(
            candidate = %candidate,
            "false-denial guard fired: NPC denied a known roster person (#1527/#1528)"
        );
        return grounded_acknowledgement(seed).to_string();
    }

    if has_known_full_name && !has_unknown_full_name && has_generic_person_nonrecognition(&lower) {
        tracing::warn!(
            "false-denial guard fired: NPC generically denied a known roster person (#1563)"
        );
        return grounded_acknowledgement(seed).to_string();
    }

    dialogue.to_string()
}

fn addressed_speaker_candidate_is_not_topic(
    candidate: &str,
    player_input: &str,
    speaker: &DialogueSpeakerContext,
) -> bool {
    let candidate_norm = normalize_for_repetition(candidate);
    let speaker_norm = normalize_for_repetition(&speaker.name);
    if candidate_norm.is_empty() || candidate_norm != speaker_norm {
        return false;
    }

    let input = normalize_for_repetition(player_input);
    let Some((address, topic)) = input.split_once(" about ") else {
        return false;
    };

    let addressed = address.contains(&speaker_norm);
    let topic_mentions_speaker = topic.contains(&speaker_norm);
    addressed && !topic.trim().is_empty() && !topic_mentions_speaker
}

// ── #1530 — invented place confirmation guard ──────────────────────────────────

/// Feature-flag name (default **on**) that enables the invented-place
/// confirmation guard (#1530). Kill-switch: disable via
/// `flags.is_disabled(INVENTED_PLACE_GUARD_FLAG)`.
pub const INVENTED_PLACE_GUARD_FLAG: &str = "dialogue-invented-place-guard";

/// Feature-flag name (default **on**) for small dialogue polish backstops
/// (#1564): old stock decline templates and time-of-day greeting tics.
pub const DIALOGUE_POLISH_GUARD_FLAG: &str = "dialogue-polish-guard";

/// Relationship context for post-generation tone guards.
#[derive(Debug, Clone)]
pub struct RelationshipToneHint {
    /// Target NPC display/roster name.
    pub target_name: String,
    /// Speaker's relationship kind toward the target.
    pub kind: RelationshipKind,
    /// Speaker's relationship strength toward the target.
    pub strength: f64,
}

/// Place-affirmation markers: phrases indicating the NPC is confirming or
/// locating a place. Used by `guard_invented_place_confirmation`.
const PLACE_AFFIRMATION_MARKERS: &[&str] = &[
    "'tis in ",
    "it is in ",
    "is located in ",
    "you will find it at ",
    "you'll find it ",
    "is in ",
    "lies in ",
    "stands in ",
    "can be found ",
    "is over in ",
    "is away in ",
];

/// Soft place-validation markers: phrases that avoid an explicit denial while
/// treating the player's invented place query as a legitimate referent (#1507).
const PLACE_SOFT_VALIDATION_MARKERS: &[&str] = &[
    "have ye a reason for asking",
    "have you a reason for asking",
    "why do ye ask",
    "why do you ask",
    "what brings ye to ask",
    "what brings you to ask",
];

/// Returns a stock place-decline for an unknown named place.
/// Cycles through a small pool to avoid repeated identical responses.
fn non_recognition_place_decline(seed: u64) -> &'static str {
    const DECLINES: &[&str] = &[
        "That place-name is not one I have heard hereabouts.",
        "No such place comes to mind around here.",
        "I cannot put that name on any road I know.",
        "That is not a place I can point you to in this parish.",
        "No place by that name is known to me near here.",
    ];
    DECLINES[(seed as usize) % DECLINES.len()]
}

const STOCK_NONRECOGNITION_TEMPLATES: &[&str] = &[
    "that name is not known to me hereabouts",
    "i know no one by that name in these parts",
    "mayhap ye have the wrong parish entirely",
];

/// Replaces old stock non-recognition templates with the same deterministic
/// fallback families used by the grounding guards (#1564).
pub fn guard_stock_nonrecognition_decline(dialogue: &str, player_input: &str, seed: u64) -> String {
    guard_stock_nonrecognition_decline_with_speaker(dialogue, player_input, seed, None)
}

/// Speaker-aware variant of [`guard_stock_nonrecognition_decline`].
pub fn guard_stock_nonrecognition_decline_with_speaker(
    dialogue: &str,
    player_input: &str,
    seed: u64,
    speaker: Option<&DialogueSpeakerContext>,
) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }

    let normalized = normalize_for_repetition(dialogue);
    if !STOCK_NONRECOGNITION_TEMPLATES
        .iter()
        .any(|template| normalized.contains(template))
    {
        return dialogue.to_string();
    }

    let prompt_seed = seed ^ text_seed(player_input);
    let place_candidates = extract_candidate_places(player_input);
    if !place_candidates.is_empty() {
        tracing::warn!(
            "dialogue polish guard fired: replacing stock place non-recognition (#1564)"
        );
        return non_recognition_place_decline(prompt_seed).to_string();
    }

    tracing::warn!("dialogue polish guard fired: replacing stock person non-recognition (#1564)");
    non_recognition_decline_for_speaker(prompt_seed, speaker).to_string()
}

fn normalized_alpha_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|token| {
            let stripped = token.trim_matches(|c: char| !c.is_alphabetic() && c != '\'');
            if stripped.is_empty() {
                None
            } else {
                Some(stripped.to_lowercase())
            }
        })
        .collect()
}

fn text_contains_word_sequence(text: &str, phrase: &str) -> bool {
    let haystack = normalized_alpha_words(text);
    let needle = normalized_alpha_words(phrase);
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_slice())
}

fn normalized_fact_words(text: &str) -> String {
    text.split_whitespace()
        .filter_map(|token| {
            let stripped = token.trim_matches(|c: char| !c.is_alphabetic() && c != '\'');
            if stripped.is_empty() {
                return None;
            }
            let lower = stripped.to_lowercase();
            Some(canonical_honorific(&lower).to_string())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn player_input_mentions_fr_declan(player_input: &str) -> bool {
    let normalized = normalized_fact_words(player_input);
    normalized.contains("father declan") || normalized.contains("declan tierney")
}

fn dialogue_has_declan_tenure_drift(dialogue: &str) -> bool {
    const BAD_TENURE_PATTERNS: &[&str] = &[
        "nigh on a decade",
        "nearly a decade",
        "almost a decade",
        "about a decade",
        "for a decade",
        "ten years",
    ];
    const TENURE_CONTEXTS: &[&str] = &[
        "priest here",
        "parish priest",
        "been the priest",
        "served the parish",
        "serving the parish",
        "served here",
        "been here",
    ];

    split_sentences(dialogue).iter().any(|sentence| {
        let normalized = normalize_for_repetition(sentence);
        BAD_TENURE_PATTERNS
            .iter()
            .any(|pattern| normalized.contains(pattern))
            && TENURE_CONTEXTS
                .iter()
                .any(|context| normalized.contains(context))
    })
}

fn dialogue_has_declan_canonical_tenure(dialogue: &str) -> bool {
    let normalized = normalize_for_repetition(dialogue);
    normalized.contains("twenty-five years")
        || normalized.contains("twenty five years")
        || normalized.contains("25 years")
}

fn relationship_is_cool_or_rival(kind: RelationshipKind, strength: f64) -> bool {
    matches!(kind, RelationshipKind::Rival | RelationshipKind::Enemy) || strength <= -0.1
}

fn dialogue_already_carries_cool_tone(dialogue: &str) -> bool {
    let normalized = normalize_for_repetition(dialogue);
    const COOL_TONE_CUES: &[&str] = &[
        "little warmth",
        "keep my distance",
        "keeps my distance",
        "keeps his distance",
        "keeps her distance",
        "keeps their distance",
        "no friend",
        "not a friend",
        "not one i trust",
        "i do not trust",
        "i don't trust",
        "she is no friend",
        "he is no friend",
        "i am wary of her",
        "i am wary of him",
        "wary",
        "watchful",
        "careful with",
        "cool between",
        "bad blood",
        "rival",
        "enemy",
    ];
    COOL_TONE_CUES.iter().any(|cue| normalized.contains(cue))
}

fn dialogue_has_neutral_warm_rival_pattern(dialogue: &str) -> bool {
    let normalized = normalize_for_repetition(dialogue);
    const NEUTRAL_WARM_PATTERNS: &[&str] = &[
        "still keeps an eye on things",
        "keeps an eye on things",
        "keeps an eye on the place",
        "fine man",
        "good man",
        "decent man",
        "grand man",
        "fine woman",
        "good woman",
        "decent woman",
        "grand woman",
        "grand fellow",
        "grand soul",
        "no harm in him",
        "no harm in her",
        "no harm in them",
        "she keeps an eye on things",
        "he keeps an eye on things",
    ];
    NEUTRAL_WARM_PATTERNS
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

fn player_input_mentions_target(player_input: &str, target_name: &str) -> bool {
    if text_contains_word_sequence(player_input, target_name) {
        return true;
    }
    extract_candidate_names(player_input)
        .iter()
        .any(|candidate| {
            normalize_name_honorifics(&candidate.to_lowercase())
                == normalize_name_honorifics(&target_name.to_lowercase())
        })
}

fn normalized_person_name_eq(left: &str, right: &str) -> bool {
    normalize_name_honorifics(&left.to_lowercase())
        == normalize_name_honorifics(&right.to_lowercase())
}

fn dialogue_has_presumed_prior_acquaintance_question(dialogue: &str) -> bool {
    let normalized = normalize_for_repetition(dialogue);
    const PATTERNS: &[&str] = &[
        "how do ye find him so far",
        "how do ye find her so far",
        "how do ye find them so far",
        "how do you find him so far",
        "how do you find her so far",
        "how do you find them so far",
    ];
    PATTERNS.iter().any(|pattern| normalized.contains(pattern))
}

fn word_is_honorific(word: &str) -> bool {
    let canonical = canonical_honorific(word);
    HONORIFIC_PAIRS
        .iter()
        .any(|(long, short)| word == *long || word == *short || canonical == *long)
}

fn strip_honorific_words(text: &str) -> String {
    normalized_alpha_words(text)
        .into_iter()
        .filter(|word| !word_is_honorific(word))
        .collect::<Vec<_>>()
        .join(" ")
}

fn player_input_asserts_prior_acquaintance(player_input: &str, target_name: &str) -> bool {
    let normalized = strip_honorific_words(&normalize_for_repetition(player_input));
    let target_lower =
        strip_honorific_words(&normalize_name_honorifics(&target_name.to_lowercase()));
    let first_name = target_lower
        .split_whitespace()
        .next()
        .unwrap_or(target_lower.as_str());

    const CONTACT_PATTERNS: &[&str] = &[
        "i met ",
        "i have met ",
        "i've met ",
        "i spoke with ",
        "i talked to ",
        "i was speaking with ",
        "i know ",
    ];

    CONTACT_PATTERNS.iter().any(|prefix| {
        normalized.contains(&format!("{prefix}{target_lower}"))
            || normalized.contains(&format!("{prefix}{first_name}"))
    })
}

fn prior_acquaintance_checkin(target_name: &str) -> String {
    format!("{target_name}, aye. Have ye met {target_name} yet?")
}

/// Rewrites replies that ask the player to evaluate a named parishioner as if
/// the player has already met them (#1509).
///
/// Conservative: only fires when the player mentioned exactly one non-speaker
/// roster person and the reply contains the narrow "How do ye/you find him/her
/// so far?" presupposition. If the player's input itself says they met or know
/// the target, the reply is left unchanged.
pub fn guard_presumed_prior_acquaintance(
    dialogue: &str,
    player_input: &str,
    known_person_names: &[String],
    speaker: Option<&DialogueSpeakerContext>,
) -> String {
    if dialogue.trim().is_empty()
        || known_person_names.is_empty()
        || !dialogue_has_presumed_prior_acquaintance_question(dialogue)
    {
        return dialogue.to_string();
    }

    let targets: Vec<&String> = known_person_names
        .iter()
        .filter(|name| {
            speaker.is_none_or(|speaker| !normalized_person_name_eq(&speaker.name, name))
                && player_input_mentions_target(player_input, name)
        })
        .collect();
    let [target] = targets.as_slice() else {
        return dialogue.to_string();
    };
    if player_input_asserts_prior_acquaintance(player_input, target) {
        return dialogue.to_string();
    }

    tracing::warn!(
        target = %target,
        "dialogue polish guard fired: replacing presumed prior-acquaintance question (#1509)"
    );
    prior_acquaintance_checkin(target)
}

fn count_word_sequence(text: &str, phrase: &str) -> usize {
    let haystack = normalized_alpha_words(text);
    let needle = normalized_alpha_words(phrase);
    if needle.is_empty() || needle.len() > haystack.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle.as_slice())
        .count()
}

fn sentence_has_redundant_self_name_reference(sentence: &str, speaker_name: &str) -> bool {
    if !text_contains_word_sequence(sentence, speaker_name) {
        return false;
    }
    let normalized_sentence =
        normalize_for_repetition(&sentence.replace('\u{2019}', "'")).replace(',', "");
    let normalized_name = normalize_for_repetition(speaker_name).replace(',', "");
    [
        format!("it's {normalized_name} ye're speaking to"),
        format!("it's {normalized_name} you're speaking to"),
        format!("it is {normalized_name} ye're speaking to"),
        format!("it is {normalized_name} you're speaking to"),
        format!("ye're speaking to {normalized_name}"),
        format!("you're speaking to {normalized_name}"),
    ]
    .iter()
    .any(|pattern| normalized_sentence.contains(pattern))
}

/// Removes redundant repeated self-name references within a single reply
/// (#1508).
///
/// Conservative: only fires when the speaker's full name appears more than
/// once and a later sentence is a clear self-identity formula such as
/// "it's Peig Hannigan ye're speaking to." The first self-introduction is
/// preserved.
pub fn guard_repeated_speaker_name(
    dialogue: &str,
    speaker: Option<&DialogueSpeakerContext>,
) -> String {
    let Some(speaker) = speaker else {
        return dialogue.to_string();
    };
    if dialogue.trim().is_empty() || count_word_sequence(dialogue, &speaker.name) <= 1 {
        return dialogue.to_string();
    }

    let mut saw_speaker_name = false;
    let mut changed = false;
    let mut kept = Vec::new();
    for sentence in split_sentences(dialogue) {
        let has_speaker_name = text_contains_word_sequence(&sentence, &speaker.name);
        if saw_speaker_name
            && has_speaker_name
            && sentence_has_redundant_self_name_reference(&sentence, &speaker.name)
        {
            changed = true;
            continue;
        }
        if has_speaker_name {
            saw_speaker_name = true;
        }
        kept.push(sentence.trim().to_string());
    }

    if !changed {
        return dialogue.to_string();
    }
    tracing::warn!(
        speaker = %speaker.name,
        "dialogue polish guard fired: removing repeated speaker name (#1508)"
    );
    kept.join(" ")
}

fn rival_tone_fallback(target_name: &str) -> String {
    format!(
        "{target_name}, aye. I'll grant the facts, but there is little warmth between us, so I keep my distance."
    )
}

/// Cools neutral-warm replies about a target the speaker regards as a rival
/// (#1521).
///
/// This is intentionally narrow: it only fires when the player explicitly asks
/// about a named target, the response mentions that same target, the speaker's
/// relationship to the target is rival/cool, and the response contains a
/// neutral-warm stock pattern. Existing cool wording is left untouched.
pub fn guard_rival_target_neutral_tone(
    dialogue: &str,
    player_input: &str,
    relationships: &[RelationshipToneHint],
) -> String {
    if dialogue.trim().is_empty() || relationships.is_empty() {
        return dialogue.to_string();
    }
    if dialogue_already_carries_cool_tone(dialogue)
        || !dialogue_has_neutral_warm_rival_pattern(dialogue)
    {
        return dialogue.to_string();
    }

    for rel in relationships {
        if !relationship_is_cool_or_rival(rel.kind, rel.strength) {
            continue;
        }
        if player_input_mentions_target(player_input, &rel.target_name)
            && text_contains_word_sequence(dialogue, &rel.target_name)
        {
            tracing::warn!(
                target = %rel.target_name,
                kind = %rel.kind,
                strength = rel.strength,
                "dialogue polish guard fired: cooling neutral rival-target tone (#1521)"
            );
            return rival_tone_fallback(&rel.target_name);
        }
    }

    dialogue.to_string()
}

/// Corrects a narrow Fr. Declan Tierney tenure drift where the model changes
/// his canonical twenty-five-year parish service into a decade-scale claim
/// (#1520).
pub fn guard_priest_tenure_drift(dialogue: &str, player_input: &str) -> String {
    if dialogue.trim().is_empty()
        || !player_input_mentions_fr_declan(player_input)
        || dialogue_has_declan_canonical_tenure(dialogue)
        || !dialogue_has_declan_tenure_drift(dialogue)
    {
        return dialogue.to_string();
    }

    tracing::warn!("dialogue polish guard fired: corrected Fr. Declan tenure drift (#1520)");
    "Fr. Declan Tierney has served this parish for twenty-five years.".to_string()
}

fn replacement_greeting(time_of_day: TimeOfDay) -> &'static str {
    match time_of_day {
        TimeOfDay::Dawn => "Good day",
        TimeOfDay::Morning => "Good morning",
        TimeOfDay::Midday => "Good day",
        TimeOfDay::Afternoon => "Good afternoon",
        TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight => "Good evening",
    }
}

fn replacement_day_phrase(time_of_day: TimeOfDay) -> &'static str {
    match time_of_day {
        TimeOfDay::Dawn => "this early hour",
        TimeOfDay::Morning => "this morning",
        TimeOfDay::Midday => "this fine day",
        TimeOfDay::Afternoon => "this afternoon",
        TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight => "this evening",
    }
}

/// Rewrites obvious opening/greeting morning tics when the actual clock is not
/// Morning (#1564). The guard intentionally targets greeting phrases and
/// "this fine morning" style tics rather than every historical mention of
/// "morning" in a reply.
pub fn guard_time_of_day_phrase(dialogue: &str, time_of_day: TimeOfDay) -> String {
    if dialogue.trim().is_empty() || time_of_day == TimeOfDay::Morning {
        return dialogue.to_string();
    }

    static GOOD_MORNING: OnceLock<regex::Regex> = OnceLock::new();
    static THIS_FINE_MORNING: OnceLock<regex::Regex> = OnceLock::new();
    static THIS_MORNING: OnceLock<regex::Regex> = OnceLock::new();

    let good_morning = GOOD_MORNING.get_or_init(|| {
        regex::Regex::new(r"(?i)\bgood\s+morn(?:ing|in'?)?\b").expect("valid good-morning regex")
    });
    let this_fine_morning = THIS_FINE_MORNING.get_or_init(|| {
        regex::Regex::new(r"(?i)\bthis\s+fine\s+morn(?:ing|in'?)?\b")
            .expect("valid this-fine-morning regex")
    });
    let this_morning = THIS_MORNING.get_or_init(|| {
        regex::Regex::new(r"(?i)\bthis\s+morn(?:ing|in'?)?\b").expect("valid this-morning regex")
    });

    let mut polished = good_morning
        .replace_all(dialogue, replacement_greeting(time_of_day))
        .into_owned();
    polished = this_fine_morning
        .replace_all(&polished, replacement_day_phrase(time_of_day))
        .into_owned();
    polished = this_morning
        .replace_all(&polished, replacement_day_phrase(time_of_day))
        .into_owned();

    if polished != dialogue {
        tracing::warn!(
            actual_time = %time_of_day,
            "dialogue polish guard fired: corrected time-of-day phrase (#1564)"
        );
    }
    polished
}

/// Extracts candidate place-name tokens from the player input.
///
/// Conservative: only extracts place candidates that are EXPLICITLY signalled
/// as places in the player's input. This avoids false-positive matches on
/// NPC person names which appear as capitalized sequences in common inputs.
///
/// Two extraction strategies:
/// 1. **Place-noun collocations**: "cathedral of Saint Aldric", "village of
///    Ballygar", etc. — a known place-noun followed by "of" and capitalized
///    words.
/// 2. **"Where is X?" / "Where are X?" / "Where can I find X?"** patterns:
///    explicitly directional queries signal that X is a place, not a person.
///    Only singletons and bigrams that follow a where-is pattern are extracted.
///
/// Capitalized word sequences WITHOUT a place-noun or where-is signal are NOT
/// extracted (they are more likely person names addressed in conversation).
fn extract_candidate_places(player_input: &str) -> Vec<String> {
    let words: Vec<&str> = player_input.split_whitespace().collect();
    let mut candidates: Vec<String> = Vec::new();
    let n = words.len();
    let lower_input = player_input.to_lowercase();

    const PLACE_NOUNS: &[&str] = &[
        "cathedral",
        "abbey",
        "castle",
        "chapel",
        "church",
        "tower",
        "hall",
        "manor",
        "market",
        "bridge",
        "mill",
        "crossroads",
        "village",
        "town",
        "island",
    ];

    // Strategy 1: place-noun collocations ("cathedral of Saint Aldric").
    for i in 0..n {
        let w = words[i].trim_matches(|c: char| !c.is_alphabetic());
        if w.is_empty() {
            continue;
        }
        let w_lower = w.to_lowercase();

        if PLACE_NOUNS.contains(&w_lower.as_str()) {
            // Look for "of <CapWord(s)>" after this token.
            if i + 2 < n {
                let next = words[i + 1].trim_matches(|c: char| !c.is_alphabetic());
                if next.to_lowercase() == "of" {
                    let mut phrase_words: Vec<String> = vec![w.to_string(), next.to_string()];
                    let mut j = i + 2;
                    while j < n && j <= i + 5 {
                        let pw = words[j].trim_matches(|c: char| !c.is_alphabetic());
                        if pw.is_empty() {
                            break;
                        }
                        let is_cap = pw.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
                        if !is_cap {
                            break;
                        }
                        phrase_words.push(pw.to_string());
                        j += 1;
                    }
                    if phrase_words.len() >= 3 {
                        let cap_part = phrase_words[2..].join(" ");
                        if !cap_part.is_empty() {
                            candidates.push(cap_part);
                        }
                        candidates.push(phrase_words.join(" "));
                    }
                }
            }
        }
    }

    // Strategy 2: "where is X?" / "where are X?" / directional queries.
    // Extract capitalized tokens from the OBJECT POSITION of the query.
    // We find the where-signal in the input and collect capitalized tokens
    // that appear AFTER it, skipping common function words.
    const WHERE_SIGNALS: &[&str] = &[
        "where is ",
        "where's ",
        "where are ",
        "guide me to ",
        "take me to ",
        "bring me to ",
        "show me to ",
        "how do i find ",
        "how do i get to ",
        "how do we get to ",
        "is there a place called ",
        "find the place called ",
        "looking for a place called ",
    ];

    // Common English words that may appear capitalized at sentence start or
    // in the where-signal itself — never valid place name candidates.
    // Also includes generic building/place nouns that appear in queries but
    // are not themselves invented place names.
    const SKIP_WORDS: &[&str] = &[
        "where",
        "is",
        "are",
        "the",
        "a",
        "an",
        "i",
        "how",
        "do",
        "we",
        "there",
        "place",
        "called",
        "looking",
        "for",
        "find",
        "can",
        "you",
        "this",
        "that",
        "it",
        "in",
        "at",
        "of",
        "to",
        "and",
        "or",
        "but",
        // Place-noun generic types (not specific names):
        "cathedral",
        "abbey",
        "castle",
        "chapel",
        "church",
        "tower",
        "hall",
        "manor",
        "market",
        "bridge",
        "mill",
        "crossroads",
        "village",
        "town",
        "island",
        "pub",
        "farm",
        "road",
        "street",
        "lane",
        "path",
        "well",
        "forge",
        "shop",
        "house",
        "inn",
        "tavern",
    ];

    for signal in WHERE_SIGNALS {
        if let Some(signal_pos) = lower_input.find(signal) {
            // Compute the byte offset in the ORIGINAL `player_input` that
            // corresponds to the end of the matched signal.  We cannot use
            // `signal_pos + signal.len()` directly as an index into
            // `player_input` because `to_lowercase()` can change the byte
            // length of individual chars (e.g. İ → "i\u{307}" expands from
            // 2 to 3 bytes; ẞ → "ß" shrinks from 3 to 2 bytes), so a byte
            // offset valid in `lower_input` may land mid-char in `player_input`
            // and cause a panic.  We walk both strings char-by-char in
            // lockstep, pairing each original char with the chars produced by
            // its `to_lowercase()` expansion, to find the exact byte in
            // `player_input` that corresponds to `lower_target` in
            // `lower_input`.
            let lower_target = signal_pos + signal.len();
            let after_signal_byte = {
                let mut lower_consumed = 0usize;
                let mut orig_consumed = 0usize;
                let mut orig_chars = player_input.chars();
                'outer: loop {
                    if lower_consumed >= lower_target {
                        break 'outer orig_consumed;
                    }
                    let Some(orig_ch) = orig_chars.next() else {
                        break 'outer player_input.len();
                    };
                    for lc in orig_ch.to_lowercase() {
                        if lower_consumed >= lower_target {
                            break 'outer orig_consumed;
                        }
                        lower_consumed += lc.len_utf8();
                    }
                    orig_consumed += orig_ch.len_utf8();
                }
            };
            let after_signal = &player_input[after_signal_byte..];
            let after_words: Vec<&str> = after_signal.split_whitespace().collect();

            for j in 0..after_words.len() {
                let w = after_words[j].trim_matches(|c: char| !c.is_alphabetic());
                if w.is_empty() {
                    continue;
                }
                let w_lower = w.to_lowercase();
                if SKIP_WORDS.contains(&w_lower.as_str()) {
                    continue;
                }
                let is_cap = w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
                let is_alnum = w.chars().all(|c| c.is_alphabetic() || c == '\'');
                if !is_alnum || w.len() < 3 {
                    continue;
                }

                // Bigram after signal
                if j + 1 < after_words.len() {
                    let w2 = after_words[j + 1].trim_matches(|c: char| !c.is_alphabetic());
                    let cap2 = w2.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
                    let alnum2 = w2.chars().all(|c| c.is_alphabetic() || c == '\'');
                    if cap2 && alnum2 && w2.len() >= 2 {
                        candidates.push(format!("{} {}", w, w2));
                    }
                }

                // Single-word place name (e.g. "Ballyfantasy") — only if it
                // looks like a capitalized proper noun or an all-lowercase
                // place name after a "called" or "place" signal word.
                if is_cap || w.len() >= 5 {
                    candidates.push(w.to_string());
                }
            }
        }
    }

    candidates
}

/// Returns `true` when `candidate` place name is NOT in `known_location_names`
/// (case-insensitive substring match either way).
fn place_not_in_known_locations(candidate: &str, known_location_names: &[String]) -> bool {
    let cand_lower = candidate.to_lowercase();
    !known_location_names.iter().any(|loc| {
        loc.to_lowercase().contains(&cand_lower) || cand_lower.contains(&loc.to_lowercase())
    })
}

/// Post-generation guard that detects when an NPC AFFIRMS an invented place
/// that is not in the world's known location list (#1530).
///
/// Distinct from `guard_wrong_location_reference` (#1477) which corrects a
/// wrong settlement name in "here in X" collocations. This guard catches the
/// broader case where the NPC confirms an entirely invented place name — one
/// that does not exist in the world graph at all — when the player asked about
/// it. Replaces with a stock place-decline.
///
/// Conservative: only fires when ALL hold:
///   1. Player input contains a candidate place name (extracted via
///      place-noun collocation or explicit directional query pattern).
///   2. The candidate is NOT in `known_location_names`.
///   3. The NPC dialogue contains either a place-affirmation marker or a soft
///      validation marker. Because candidates are only extracted from explicitly
///      signalled place queries, those markers are sufficient evidence that the
///      NPC is treating the invented place as a real conversational referent —
///      the NPC may paraphrase or omit the exact name.
///   4. The NPC dialogue does NOT contain a denial marker (already declining).
///
/// Gate: `dialogue-invented-place-guard` (default-on).
pub fn guard_invented_place_confirmation(
    dialogue: &str,
    player_input: &str,
    known_location_names: &[String],
    seed: u64,
) -> String {
    if dialogue.trim().is_empty() || known_location_names.is_empty() {
        return dialogue.to_string();
    }

    let lower = dialogue.to_lowercase();

    // If the NPC is already denying, do not interfere.
    let has_denial = DENIAL_MARKERS.iter().any(|m| lower.contains(m));
    if has_denial {
        return dialogue.to_string();
    }

    // Must contain a place-affirmation or soft-validation marker for the guard
    // to fire.
    let has_place_marker = PLACE_AFFIRMATION_MARKERS.iter().any(|m| lower.contains(m))
        || PLACE_SOFT_VALIDATION_MARKERS
            .iter()
            .any(|m| lower.contains(m));
    if !has_place_marker {
        return dialogue.to_string();
    }

    let candidates = extract_candidate_places(player_input);

    for candidate in &candidates {
        if !place_not_in_known_locations(candidate, known_location_names) {
            continue;
        }

        // When the candidate was extracted via the conservative strategies
        // (place-noun collocation or where-is directional), a place-affirmation
        // or soft-validation marker in the dialogue is sufficient evidence that
        // the NPC is treating the invented place as real — the NPC may
        // paraphrase, omit the name, or ask why the player is asking.
        tracing::warn!(
            candidate = %candidate,
            "invented-place guard fired: NPC confirmed/soft-validated invented place (#1530/#1507)"
        );
        return non_recognition_place_decline(seed).to_string();
    }

    dialogue.to_string()
}

/// Post-generation guard for false denial of a real place (#1563).
///
/// If the player asks about a place that exists in the world graph and the NPC
/// generically denies it as nonexistent or as "no such person", replace that
/// denial with a neutral acknowledgement. Unlike the invented-place guard, this
/// does not invent directions; it only prevents the transcript from asserting
/// that a real parish place is fake.
pub fn guard_false_denial_of_known_place(
    dialogue: &str,
    player_input: &str,
    known_location_names: &[String],
    seed: u64,
) -> String {
    if dialogue.trim().is_empty() || known_location_names.is_empty() {
        return dialogue.to_string();
    }

    let lower = dialogue.to_lowercase();
    if !has_generic_entity_nonrecognition(&lower) {
        return dialogue.to_string();
    }

    let known_location_names_lower: Vec<String> = known_location_names
        .iter()
        .map(|loc| loc.to_lowercase())
        .collect();
    let candidates = extract_candidate_places(player_input);
    let mut has_known_place = false;
    let mut has_unknown_place = false;
    for candidate in &candidates {
        let candidate_lower = candidate.to_lowercase();
        let is_known_place = known_location_names_lower.iter().any(|location_lower| {
            location_lower.contains(&candidate_lower)
                || candidate_lower.contains(location_lower.as_str())
        });
        if is_known_place {
            has_known_place = true;
        } else {
            has_unknown_place = true;
        }
    }

    if has_known_place && !has_unknown_place {
        tracing::warn!(
            "false-denial guard fired: NPC generically denied a known parish place (#1563)"
        );
        return grounded_place_acknowledgement(seed).to_string();
    }

    dialogue.to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── dedupe_cross_npc_openers (#1422) ──────────────────────────────────────

    /// AC-1: when a subsequent NPC opens with the same stock phrase as a prior
    /// NPC this turn, the duplicated opener is stripped and the substantive
    /// remainder is preserved.
    #[test]
    fn cross_npc_opener_dedup_strips_duplicate_opener() {
        let prior = vec!["Ye've come to the right place for gossip".to_string()];
        let reply =
            "Ye've come to the right place for information. The lads at the forge would know more.";
        let result = dedupe_cross_npc_openers(&prior, reply);
        // The duplicate opener must be gone.
        assert!(
            !result.starts_with("Ye've come to the right place"),
            "duplicate opener must be stripped; got: {result:?}"
        );
        // The substantive remainder must be preserved.
        assert!(
            result.contains("The lads at the forge would know more"),
            "substantive remainder must survive; got: {result:?}"
        );
    }

    /// AC-2: a reply with a distinct opener passes through unchanged.
    #[test]
    fn cross_npc_opener_dedup_passes_through_distinct_opener() {
        let prior = vec!["Ye've come to the right place for gossip".to_string()];
        let reply = "A fine morning it is. Lots of work to be done today.";
        let result = dedupe_cross_npc_openers(&prior, reply);
        assert_eq!(
            result, reply,
            "distinct opener must not be modified; got: {result:?}"
        );
    }

    /// No prior openers: the first NPC's reply always passes through.
    #[test]
    fn cross_npc_opener_dedup_no_op_when_no_priors() {
        let prior: Vec<String> = vec![];
        let reply = "Ye've come to the right place for information, so.";
        let result = dedupe_cross_npc_openers(&prior, reply);
        assert_eq!(
            result, reply,
            "with no priors, reply must pass through unchanged"
        );
    }

    /// When the entire reply is the duplicated opener (no remainder), the
    /// original is returned rather than an empty string.
    #[test]
    fn cross_npc_opener_dedup_keeps_original_when_no_remainder() {
        let prior = vec!["Ye've come to the right place".to_string()];
        // Only one sentence — there is no remainder.
        let reply = "Ye've come to the right place!";
        let result = dedupe_cross_npc_openers(&prior, reply);
        // Must not be blank — return the original.
        assert!(
            !result.is_empty(),
            "must not return blank when no remainder exists; got: {result:?}"
        );
    }

    /// Normalised equality catches case/punctuation variation in the opener.
    #[test]
    fn cross_npc_opener_dedup_normalises_case_and_punctuation() {
        let prior = vec!["ye've come to the right place for gossip".to_string()];
        // Same opener, different capitalisation + punctuation.
        let reply = "Ye've come to the right place for gossip! Sure and I can help ye too.";
        let result = dedupe_cross_npc_openers(&prior, reply);
        assert!(
            !result.starts_with("Ye've come to the right place"),
            "normalised duplicate must be stripped; got: {result:?}"
        );
        assert!(
            result.contains("Sure and I can help"),
            "substantive remainder must survive; got: {result:?}"
        );
    }

    /// `extract_normalized_opener` returns the first-sentence opener, normalised.
    #[test]
    fn extract_normalized_opener_returns_first_sentence() {
        let opener = extract_normalized_opener("Good morning to ye. How can I help?");
        assert_eq!(opener, "good morning to ye");
    }

    /// #1706: the shared splitter keeps an abbreviated honorific attached to
    /// the uppercase name that follows it and preserves the original byte
    /// slices, including multibyte Irish text and inter-sentence whitespace.
    #[test]
    fn sentence_splitter_preserves_honorific_name_and_utf8_slices() {
        let dialogue = "Éist le Fr. Declan Tierney. Slán abhaile.";
        assert_eq!(
            split_sentences(dialogue),
            vec!["Éist le Fr. Declan Tierney.", " Slán abhaile."]
        );
    }

    /// #1706: every abbreviated form in the existing honorific table follows
    /// the same sentence-boundary rule rather than relying on a `Fr.` special
    /// case.
    #[test]
    fn sentence_splitter_reuses_all_known_honorific_abbreviations() {
        for &(_, short) in HONORIFIC_PAIRS {
            let dialogue = format!(
                "{}. Áine Byrne knows the road. Ask her.",
                short.to_uppercase()
            );
            let sentences = split_sentences(&dialogue);
            assert_eq!(
                sentences.len(),
                2,
                "honorific {short:?} must stay attached to the uppercase name: {sentences:?}"
            );
            assert_eq!(
                sentences.concat(),
                dialogue,
                "sentence slices must preserve every input byte"
            );
        }
    }

    /// #1706 risk boundary: ordinary periods, long-form words, and abbreviated
    /// honorifics followed by lowercase prose remain sentence terminators.
    #[test]
    fn sentence_splitter_keeps_ordinary_period_boundaries() {
        assert_eq!(
            split_sentences("The doctor. Declan waited."),
            vec!["The doctor.", " Declan waited."]
        );
        assert_eq!(
            split_sentences("Fr. declan waited."),
            vec!["Fr.", " declan waited."]
        );
        assert_eq!(
            split_sentences("That was brief. Declan agreed."),
            vec!["That was brief.", " Declan agreed."]
        );
    }

    /// #1706: opener extraction includes the complete titled first sentence;
    /// it must never reduce `Fr. Declan Tierney ...` to the token `fr`.
    #[test]
    fn extract_normalized_opener_keeps_full_titled_opener() {
        let opener = extract_normalized_opener(
            "Fr. Declan Tierney knows the north road. He walks it daily.",
        );
        assert_eq!(opener, "fr. declan tierney knows the north road");
    }

    /// #1706: two distinct titled openers share a name but are not stock-opener
    /// duplicates, so cross-NPC dedupe must preserve the visible honorific.
    #[test]
    fn cross_npc_opener_dedup_preserves_distinct_titled_reply() {
        let prior = vec![extract_normalized_opener(
            "Fr. Declan Tierney knows the north road. He walks it daily.",
        )];
        let reply = "Fr. Declan Tierney keeps the parish books. Ask him after Mass.";

        assert_eq!(dedupe_cross_npc_openers(&prior, reply), reply);
    }

    // ── #1228 existing guard ──────────────────────────────────────────────────

    #[test]
    fn collapse_repeated_six_times() {
        // Adversarial: same clause repeated 6 times → collapsed to 1.
        let clause = "Speak yer mind, and we'll see what be in it, m'friend.";
        let input = std::iter::repeat_n(clause, 6).collect::<Vec<_>>().join(" ");
        let result = collapse_repeated_sentences(&input);
        // Should contain the clause exactly once.
        let occurrences = result.matches(clause).count();
        assert_eq!(
            occurrences, 1,
            "expected 1 occurrence after collapse, got {occurrences}: {result:?}"
        );
    }

    #[test]
    fn collapse_non_repeating_unchanged() {
        let input = "Good morning to ye. Fine day it is. How can I help?";
        let result = collapse_repeated_sentences(input);
        // Should be unchanged (modulo whitespace re-join).
        assert!(
            result.contains("Good morning"),
            "non-repeating dialogue should be preserved: {result:?}"
        );
        assert!(
            result.contains("How can I help"),
            "non-repeating dialogue should be preserved: {result:?}"
        );
    }

    // ── #1459 — fabricated-person confirmation guard ──────────────────────────

    #[test]
    fn fabricated_person_confirmed_is_declined() {
        // Adversarial: NPC affirmatively confirms a fabricated name.
        let dialogue = "Aye, I know Cormac Sweeney well. He is a fine man who works in the mill.";
        let player_input = "Do you know Cormac Sweeney?";
        let known: Vec<String> = vec!["Brigid Connolly".into(), "Tadhg Murphy".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        // Must not contain affirmation of fabricated name.
        assert!(
            !result.to_lowercase().contains("aye, i know cormac"),
            "guard should have replaced fabricated-person confirmation: {result:?}"
        );
        // Should be a non-recognition decline.
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "result should be a decline phrase: {result:?}"
        );
    }

    #[test]
    fn fabricated_titled_landlord_hearsay_confirmation_is_declined() {
        let dialogue = "Aye, I've heard the talk of Lord Fitzwilliam. 'Tis said he owns most \
                        of the land round hereabouts. Ye'll need to be careful with yer words \
                        when ye speak of him, 'tis a mighty man he is.";
        let player_input = "Have you heard of Lord Fitzwilliam of Castlemore? I hear he is the \
                            great landlord hereabouts";
        let known: Vec<String> = vec!["Colm Gallagher".into(), "Seamus Gallagher".into()];

        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        let lower = result.to_lowercase();

        assert!(
            !lower.contains("heard the talk of lord fitzwilliam")
                && !lower.contains("owns most of the land")
                && !lower.contains("mighty man"),
            "invented landlord confirmation must be replaced: {result:?}"
        );
        assert!(
            lower.contains("no such person")
                || lower.contains("no one by that name")
                || lower.contains("wrong parish")
                || lower.contains("not known to me")
                || lower.contains("such a person")
                || lower.contains("that name")
                || lower.contains("parish face")
                || lower.contains("comes to mind"),
            "result should be a non-recognition decline: {result:?}"
        );
    }

    #[test]
    fn known_roster_person_passes_through() {
        // NPC confirms someone actually in the roster — guard must not fire.
        let dialogue = "Aye, I know Brigid Connolly well. She is a fine woman.";
        let player_input = "Do you know Brigid Connolly?";
        let known: Vec<String> = vec!["Brigid Connolly".into(), "Tadhg Murphy".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "known-roster person should not be altered: {result:?}"
        );
    }

    #[test]
    fn known_roster_person_hearsay_confirmation_passes_through() {
        let dialogue = "Aye, I've heard of Brigid Connolly. A fine woman she is.";
        let player_input = "Have you heard of Brigid Connolly?";
        let known: Vec<String> = vec!["Brigid Connolly".into(), "Tadhg Murphy".into()];

        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);

        assert_eq!(
            result, dialogue,
            "hearsay marker must not suppress known-roster people: {result:?}"
        );
    }

    #[test]
    fn already_declining_npc_passes_through() {
        // NPC reply already contains a denial — guard must not fire.
        let dialogue = "I know no one by that name in these parts. You may have the wrong parish.";
        let player_input = "Do you know Cormac Sweeney?";
        let known: Vec<String> = vec![];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "already-declining dialogue should pass through unchanged: {result:?}"
        );
    }

    #[test]
    fn neutral_mention_of_unknown_name_passes_through() {
        // NPC mentions the name in a neutral / questioning way — guard must not fire.
        let dialogue = "Cormac Sweeney, you say? I cannot recall that name.";
        let player_input = "Do you know Cormac Sweeney?";
        let known: Vec<String> = vec![];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "neutral mention should not trigger guard: {result:?}"
        );
    }

    #[test]
    fn player_name_not_flagged_as_fabricated() {
        // If player_name matches the candidate, guard should not fire.
        let dialogue = "Aye, I know you, Cormac Sweeney. You've been here before.";
        let player_input = "Do you remember me, Cormac Sweeney?";
        let known: Vec<String> = vec![];
        let result = guard_fabricated_person_confirmation(
            dialogue,
            player_input,
            &known,
            &[],
            Some("Cormac Sweeney"),
            0,
        );
        assert_eq!(
            result, dialogue,
            "player's own name should not trigger guard: {result:?}"
        );
    }

    #[test]
    fn fabricated_surname_with_real_first_name_is_declined() {
        // Regression (#1459): roster contains "Cormac Duffy"; player asks about
        // "Cormac Sweeney" (fabricated surname, shared first name). The guard must
        // fire — a first-name match alone is NOT sufficient when the candidate
        // carries a surname.
        let dialogue = "Aye, I know Cormac Sweeney well. He is a fine man.";
        let player_input = "Do you know Cormac Sweeney?";
        let known: Vec<String> = vec!["Cormac Duffy".into(), "Brigid Connolly".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert!(
            !result.to_lowercase().contains("aye, i know cormac sweeney"),
            "guard should have fired and replaced fabricated-person confirmation: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "result should be a decline phrase: {result:?}"
        );
    }

    #[test]
    fn first_name_only_real_person_passes_through() {
        // A player referring to a roster member by first name only is legitimate —
        // "Cormac" (single token) vs roster ["Cormac Duffy"] should pass through.
        let dialogue = "Aye, Cormac is a good man indeed. I see him at the mill most days.";
        let player_input = "Do you know Cormac?";
        let known: Vec<String> = vec!["Cormac Duffy".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "first-name-only reference to a roster member should not trigger guard: {result:?}"
        );
    }

    // ── #1466 — first-name conflation guard ──────────────────────────────────

    #[test]
    fn firstname_affirmation_of_player_fabricated_fullname_is_declined() {
        // KEY REGRESSION (#1466): player asks about "Cormac Sweeney" (fabricated —
        // roster has "Cormac Duffy"). NPC replies with first-name-only affirmation:
        // "Cormac is at the mill, about his affairs". The guard must fire because
        // "Cormac" is the first name of the player-named fabricated full name
        // "Cormac Sweeney", so the NPC is implicitly confirming the fabricated person.
        let dialogue = "Cormac is at the mill, about his affairs. Mayhap he's there now.";
        let player_input = "find my cousin Cormac Sweeney";
        let known: Vec<String> = vec!["Cormac Duffy".into(), "Roisin Connolly".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert!(
            !result.to_lowercase().contains("cormac is at the mill"),
            "guard should have fired and replaced first-name affirmation of fabricated full name: {result:?}"
        );
        // Must be a decline phrase.
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "result should be a decline phrase: {result:?}"
        );
    }

    #[test]
    fn casual_firstname_real_person_still_passes() {
        // Conservative non-regression (#1466): player uses only a first name to
        // refer to a real roster member. No fabricated full name was named in this
        // exchange, so the first-name conflation check must NOT fire. Casual queries
        // about real people by first name continue to pass through unchanged.
        let dialogue = "Aye, Cormac Duffy works the mill. He's a reliable sort.";
        let player_input = "do you know Cormac?";
        let known: Vec<String> = vec!["Cormac Duffy".into(), "Brigid Connolly".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "casual first-name query about a real roster member should not trigger guard: {result:?}"
        );
    }

    #[test]
    fn firstname_affirmation_of_real_person_with_fabricated_same_first_name_passes() {
        // Thread 1 false-positive fix (#1466 T1): player input mentions BOTH a
        // fabricated full name ("Cormac Sweeney") AND a real roster member
        // ("Cormac Duffy"). NPC dialogue affirms the REAL roster name in full:
        // "Cormac Duffy works the mill". The guard must NOT fire — the NPC is
        // talking about a real person, not confirming the fabricated one.
        let dialogue = "Aye, Cormac Duffy works the mill, right enough. A reliable man.";
        let player_input = "I'm looking for Cormac Sweeney but have you seen Cormac Duffy today?";
        let known: Vec<String> = vec!["Cormac Duffy".into(), "Brigid Connolly".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "NPC affirming real roster member 'Cormac Duffy' in full should not be suppressed \
             even when fabricated 'Cormac Sweeney' shares the first name: {result:?}"
        );
    }

    // ── #1470 — indirect/appositive and pronoun follow-up bypass ────────────

    #[test]
    fn indirect_appositive_affirmation_of_fabricated_firstname_is_declined() {
        // AC-1 (#1470 gap 1): player asks about fabricated "Cormac Sweeney" (roster
        // has "Cormac Duffy"). NPC replies with an appositive form: "Ye've come at a
        // time when he's out, the lad Cormac." — "Cormac" is in appositive position,
        // NOT near a standard affirmation marker, so the old #1466 guard missed it.
        // The broadened check (dialogue_contains_name_without_denial) must catch it.
        let dialogue = "Ye've come at a time when he's out, the lad Cormac.";
        let player_input = "is my cousin Cormac Sweeney about?";
        let known: Vec<String> = vec!["Cormac Duffy".into(), "Roisin Brennan".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert!(
            !result.to_lowercase().contains("the lad cormac"),
            "guard should have fired on appositive first-name affirmation of fabricated full name: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "result should be a decline phrase: {result:?}"
        );
    }

    #[test]
    fn pronoun_followup_affirmation_of_established_fabricated_referent_is_declined() {
        // AC-2 (#1470 gap 2): prior player turn established fabricated "Cormac
        // Sweeney". Current turn is a pronoun-only follow-up "Out where?" — no name
        // in current player_input. NPC replies "Aye, he's off to the mill with some
        // of the flour." The pronoun affirmation guard must fire because the
        // previously-established referent is fabricated.
        let dialogue = "Aye, he's off to the mill with some of the flour.";
        let player_input = "Out where?"; // no name — pronoun-only continuation
        let prior_inputs: Vec<&str> = vec!["is my cousin Cormac Sweeney about?"];
        let known: Vec<String> = vec!["Cormac Duffy".into(), "Roisin Brennan".into()];
        let result = guard_fabricated_person_confirmation(
            dialogue,
            player_input,
            &known,
            &prior_inputs,
            None,
            0,
        );
        assert!(
            !result.to_lowercase().contains("he's off to the mill"),
            "guard should have fired on pronoun follow-up affirming established fabricated referent: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "result should be a decline phrase: {result:?}"
        );
    }

    #[test]
    fn pronoun_followup_about_real_person_passes_through() {
        // AC-4 (#1470 conservative): prior player turn named a REAL roster person
        // "Cormac Duffy". Follow-up "Where is he?" + NPC "He's at the mill." must
        // NOT be suppressed — the established referent is real, not fabricated.
        let dialogue = "He's at the mill, working the grain as usual.";
        let player_input = "Where is he?"; // pronoun-only follow-up
        let prior_inputs: Vec<&str> = vec!["Have you seen Cormac Duffy today?"];
        let known: Vec<String> = vec!["Cormac Duffy".into(), "Roisin Brennan".into()];
        let result = guard_fabricated_person_confirmation(
            dialogue,
            player_input,
            &known,
            &prior_inputs,
            None,
            0,
        );
        assert_eq!(
            result, dialogue,
            "pronoun follow-up about a real roster member should not trigger guard: {result:?}"
        );
    }

    // ── word-boundary false-positive guard (gemini review thread 1) ─────────

    #[test]
    fn short_firstname_substring_does_not_trigger_guard() {
        // Regression: a short first name like "Al" (player "Al Sweeney") must NOT
        // match when the name appears only as a substring inside other words.
        // "Ye shall talk to the priest" contains "al" (in "shall") and "al" (in
        // "talk") as substrings, but NOT the standalone word "Al". The guard must
        // not fire — no false positive.
        let dialogue = "Ye shall talk to the priest about it, I reckon.";
        let player_input = "Where is Al Sweeney?";
        let known: Vec<String> = vec!["Brigid Connolly".into(), "Roisin Brennan".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "guard must not fire when first name 'Al' appears only as substring of other words \
             ('shall', 'talk') — no word-boundary match: {result:?}"
        );
    }

    #[test]
    fn short_firstname_as_word_still_triggers_guard() {
        // Positive case: "Al" appears as a standalone word in appositive position
        // with a pronoun affirmation marker. Guard must fire.
        // "Aye, he's out, that Al" — pronoun marker "he's out" + standalone "Al".
        let dialogue = "Aye, he's out, that Al, gone to the mill I think.";
        let player_input = "Where is Al Sweeney?";
        let known: Vec<String> = vec!["Brigid Connolly".into(), "Roisin Brennan".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert!(
            !result.to_lowercase().contains("he's out, that al"),
            "guard should have fired on standalone first name 'Al' with pronoun affirmation: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "result should be a decline phrase: {result:?}"
        );
    }

    // ── #1460 — verbosity guard ───────────────────────────────────────────────

    #[test]
    fn five_trailing_questions_capped_to_one() {
        // Adversarial: 5 stacked interrogative sentences at the tail.
        let dialogue = "Good day to ye. \
            Will ye be staying long? \
            And where did ye come from? \
            Have ye news from the north? \
            What brings ye to Kilteevan? \
            Are ye familiar with these parts?";
        let result = cap_trailing_questions(dialogue);
        // Non-question preamble must survive.
        assert!(
            result.contains("Good day to ye"),
            "preamble should survive: {result:?}"
        );
        // Exactly 1 question mark should remain.
        let q_count = result.matches('?').count();
        assert_eq!(
            q_count, 1,
            "expected exactly 1 trailing question, got {q_count}: {result:?}"
        );
    }

    #[test]
    fn single_question_unchanged_by_cap() {
        let dialogue = "Good day to ye. Have ye news from the north?";
        let result = cap_trailing_questions(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "single-question dialogue should be unchanged"
        );
    }

    #[test]
    fn no_question_unchanged_by_cap() {
        let dialogue = "Good day to ye. Fine weather we're having.";
        let result = cap_trailing_questions(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "no-question dialogue should be unchanged"
        );
    }

    #[test]
    fn truncated_ellipsis_trimmed_to_last_complete_sentence() {
        // Adversarial: reply ends with "…" after an incomplete fragment.
        let dialogue =
            "Fine day it is. I was just heading to the mill to see about the grain supplies…";
        let result = trim_mid_sentence_truncation(dialogue);
        assert!(
            !result.ends_with('…'),
            "ellipsis should have been trimmed: {result:?}"
        );
        // Should preserve the complete first sentence.
        assert!(
            result.contains("Fine day it is"),
            "complete sentence should be preserved: {result:?}"
        );
        // Should NOT contain the incomplete fragment.
        assert!(
            !result.contains("heading to the mill"),
            "incomplete fragment should have been trimmed: {result:?}"
        );
    }

    #[test]
    fn non_truncated_ellipsis_unchanged() {
        let dialogue = "Fine day it is. Grand weather entirely.";
        let result = trim_mid_sentence_truncation(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "non-truncated dialogue unchanged"
        );
    }

    #[test]
    fn leaked_mood_word_stripped() {
        // Adversarial: bare mood word appended after sentence punctuation.
        let dialogue = "Good day to ye. sharp";
        let result = strip_leaked_mood_word(dialogue);
        assert_eq!(
            result, "Good day to ye.",
            "mood word should be stripped: {result:?}"
        );
    }

    #[test]
    fn non_mood_word_at_end_unchanged() {
        let dialogue = "Fine day it is. Grand.";
        let result = strip_leaked_mood_word(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "non-mood trailing word unchanged"
        );
    }

    #[test]
    fn mood_word_mid_sentence_unchanged() {
        // "curious" mid-sentence — not a leak.
        let dialogue = "I am curious about your intentions here.";
        let result = strip_leaked_mood_word(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "mid-sentence mood word unchanged"
        );
    }

    #[test]
    fn stacked_player_vocatives_strip_friend_stranger_only_at_vocative_boundary() {
        let dialogue = "Do ye have need of him, then, friend stranger, or just curious?";
        let result = strip_stacked_player_vocatives(dialogue);
        assert_eq!(
            result,
            "Do ye have need of him, then, friend, or just curious?"
        );

        let comparative = "There is no friend stranger than the one who returns at dawn.";
        let result = strip_stacked_player_vocatives(comparative);
        assert_eq!(
            result, comparative,
            "ordinary prose without trailing vocative punctuation must stay unchanged"
        );
    }

    #[test]
    fn verbosity_guard_strips_stacked_player_vocative_end_to_end() {
        let dialogue = "Do ye have need of him or his guidance, then, friend stranger, \
                        or just curious to know the man of God in these parts?";
        let result = guard_verbosity_runons(dialogue);

        assert!(
            !result.to_lowercase().contains("friend stranger"),
            "stacked vocative must be removed: {result:?}"
        );
        assert!(
            result.contains("friend, or just curious"),
            "first address term and substantive question should survive: {result:?}"
        );
        assert!(
            result.contains("man of God"),
            "priest reference should survive: {result:?}"
        );
    }

    #[test]
    fn verbosity_guard_end_to_end() {
        // All three #1460 patterns combined:
        // mood leak + truncation + question stack.
        let dialogue = "Grand so, I'll see what I can do. \
            The harvest this year has been fine and the grain…  \
            Was the journey long? \
            Are ye hungry? \
            Did ye come by the eastern road? \
            Have ye family in the parish? \
            What is it ye want of me? \
            irritated";
        let result = guard_verbosity_runons(dialogue);
        // Mood word stripped.
        assert!(
            !result.to_lowercase().ends_with("irritated"),
            "mood word should be stripped: {result:?}"
        );
        // Question stack capped.
        let q_count = result.matches('?').count();
        assert!(
            q_count <= 1,
            "question stack should be capped to 1, got {q_count}: {result:?}"
        );
    }

    // ── #1460 distributed / non-consecutive repetition (new) ─────────────────

    #[test]
    fn peig_repro_distributed_seek_questions_collapse() {
        // Adversarial: the Peig repro from the quality-harness session.
        // "what is it ye seek from X" appears 4× interleaved with other clauses.
        // All four near-duplicate questions must collapse to one, and the other
        // content sentences must survive.
        let dialogue = "Good day to ye, stranger. \
            What is it ye seek from yer cousin in these parts? \
            The morning is fair enough. \
            What is it ye seek from yer kin hereabouts? \
            Mind yer step on the path. \
            What is it ye want from Cormac, then? \
            What is it ye seek from him?";
        let result = collapse_distributed_repeated_sentences(dialogue);
        // Non-duplicate content must survive.
        assert!(
            result.contains("Good day to ye"),
            "preamble should survive: {result:?}"
        );
        assert!(
            result.contains("The morning is fair enough"),
            "non-duplicate sentence should survive: {result:?}"
        );
        assert!(
            result.contains("Mind yer step"),
            "non-duplicate sentence should survive: {result:?}"
        );
        // Only one "what is it ye seek/want" question should remain.
        let seek_count = result.to_lowercase().matches("what is it ye").count();
        assert_eq!(
            seek_count, 1,
            "distributed seek-question should collapse to 1, got {seek_count}: {result:?}"
        );
    }

    #[test]
    fn alternating_ab_loop_collapses() {
        // Adversarial: A/B/A/B alternating loop — neither A nor B is consecutive,
        // but each appears twice. After dedup, only the first A and first B remain.
        let dialogue = "Speak up now, I haven't got all day for ye. \
            Ye'd best be quick about it, friend. \
            Speak up now, I haven't got all day for ye. \
            Ye'd best be quick about it, friend.";
        let result = collapse_distributed_repeated_sentences(dialogue);
        // Each clause should appear at most once.
        let speak_count = result.to_lowercase().matches("speak up now").count();
        let quick_count = result.to_lowercase().matches("ye'd best be quick").count();
        assert_eq!(
            speak_count, 1,
            "\"speak up now\" should appear once, got {speak_count}: {result:?}"
        );
        assert_eq!(
            quick_count, 1,
            "\"ye'd best be quick\" should appear once, got {quick_count}: {result:?}"
        );
    }

    #[test]
    fn legitimate_varied_dialogue_unchanged_by_distributed_dedup() {
        // Conservative: a reply with genuine variety must not be altered.
        let dialogue = "Good day to ye. The harvest has been fine this year. \
            Brigid Connolly was asking after the grain prices only yesterday. \
            And the priest is expected back by Thursday, they say. \
            Have ye heard any news from Roscommon yourself?";
        let result = collapse_distributed_repeated_sentences(dialogue);
        // All sentences must survive (modulo whitespace rejoin).
        assert!(
            result.contains("Good day to ye"),
            "first sentence should survive: {result:?}"
        );
        assert!(
            result.contains("harvest has been fine"),
            "harvest sentence should survive: {result:?}"
        );
        assert!(
            result.contains("Brigid Connolly"),
            "Brigid sentence should survive: {result:?}"
        );
        assert!(
            result.contains("priest is expected"),
            "priest sentence should survive: {result:?}"
        );
        assert!(
            result.contains("news from Roscommon"),
            "closing question should survive: {result:?}"
        );
    }

    #[test]
    fn total_question_cap_keeps_last_two() {
        // Four scattered questions: cap_total_questions should keep the last 2
        // and drop the first 2, while preserving all non-question sentences.
        let dialogue = "What brings ye here? \
            The morning is cold. \
            Have ye come far? \
            Aye, the roads are rough. \
            Are ye looking for work? \
            Mind yer step. \
            What is it ye want of me?";
        let result = cap_total_questions(dialogue);
        let q_count = result.matches('?').count();
        assert!(
            q_count <= 2,
            "should have at most 2 questions after total cap, got {q_count}: {result:?}"
        );
        // The LAST two questions should be kept (positional preference).
        assert!(
            result.contains("Are ye looking for work") || result.contains("What is it ye want"),
            "later questions should be preferred: {result:?}"
        );
        // Non-question sentences must survive.
        assert!(
            result.contains("The morning is cold"),
            "non-question sentence should survive: {result:?}"
        );
        assert!(
            result.contains("Mind yer step"),
            "non-question sentence should survive: {result:?}"
        );
    }

    #[test]
    fn total_question_cap_noop_on_two_questions() {
        // Exactly 2 questions — cap must not alter anything.
        let dialogue = "Fine day it is. Have ye come far? \
            The road from Roscommon is rough. What brings ye here?";
        let result = cap_total_questions(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "two questions should pass through unchanged"
        );
    }

    #[test]
    fn verbosity_guard_collapses_distributed_repro() {
        // Integration: the full guard_verbosity_runons path collapses the Peig
        // repro pattern end-to-end (mood leak, distributed dedup, total-q cap,
        // trailing-q cap all chained).
        let dialogue = "Good day to ye, stranger. \
            What is it ye seek from yer cousin in these parts? \
            The morning is fair enough. \
            What is it ye seek from yer kin hereabouts? \
            Mind yer step on the path. \
            What is it ye want from Cormac, then? \
            What is it ye seek from him? \
            irritated";
        let result = guard_verbosity_runons(dialogue);
        // Mood word stripped.
        assert!(
            !result.to_lowercase().ends_with("irritated"),
            "mood word should be stripped: {result:?}"
        );
        // At most one question.
        let q_count = result.matches('?').count();
        assert!(
            q_count <= 1,
            "question count should be <=1 after full guard, got {q_count}: {result:?}"
        );
        // Non-question content survives.
        assert!(
            result.contains("Good day to ye"),
            "preamble should survive: {result:?}"
        );
    }

    // ── #1472 — hard sentence-length cap + trailing ellipsis strip ────────────

    #[test]
    fn long_multibeat_reply_capped_to_n_sentences() {
        // Adversarial (#1472): 8-sentence rambling reply with a paraphrased repeated
        // instruction — "Brendan, bring yer friend a cup of the hot spiced tea" and
        // "Brendan, go fetch the tea, as I bid ye" are semantically the same
        // instruction in different words. The surgical distributed-dedup guard cannot
        // catch paraphrases (different word sets). The hard sentence cap must trim to
        // the shared contract regardless of phrasing variation.
        let dialogue = "Sit yerself down, friend, and rest yer weary bones. \
            Brendan, bring yer friend a cup of the hot spiced tea. \
            The fire is warm enough and ye look as though the road has tired ye. \
            We've plenty here, no need to rush. \
            Brendan, go fetch the tea, as I bid ye. \
            The evening is coming on fast and ye'd do well to warm yerself. \
            There's much to speak of once ye've had yer rest. \
            Brendan, are ye not hearing me? Go on now.";
        let result = cap_sentence_count(dialogue);
        // Count sentence units in the result (sentences end with . ! ?)
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= MAX_DIALOGUE_SENTENCES,
            "reply should be capped to <={MAX_DIALOGUE_SENTENCES} sentences, got {sentence_count}: {result:?}"
        );
        // The paraphrased second repeat ("Brendan, go fetch the tea") must be gone.
        assert!(
            !result.contains("go fetch the tea"),
            "paraphrased second instruction should be dropped by sentence cap: {result:?}"
        );
        // The first sentence must survive (cap keeps the head, not the tail).
        assert!(
            result.contains("Sit yerself down"),
            "first sentence should survive: {result:?}"
        );
    }

    #[test]
    fn short_reply_unchanged_by_length_cap() {
        // Conservative: a 1-2 sentence reply must not be over-trimmed.
        let dialogue = "Aye, that's the way of it. Come back tomorrow.";
        let result = cap_sentence_count(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "short 2-sentence reply should be unchanged by the length cap: {result:?}"
        );
    }

    #[test]
    fn three_sentence_reply_unchanged_by_length_cap() {
        // Boundary: exactly 3 sentences — at the shared cap, must pass through.
        let dialogue = "The harvest has been fine this year. \
            Brigid was asking after the grain prices. Have ye heard the news from Roscommon?";
        let result = cap_sentence_count(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "3-sentence reply at the cap limit should be unchanged: {result:?}"
        );
    }

    #[test]
    fn cap_preserves_semantic_order_instead_of_tail_swapping() {
        // The old cap replaced the last retained substantive sentence with a
        // far-later question. Capping must keep the first three beats in order.
        let dialogue = "The roads have been muddy these past few days. \
            Old Séamus was asking after the grain prices. \
            There's much talk about the rent collectors. \
            Mary says the miller raised his prices. \
            Now, what brings ye here?";
        let result = cap_sentence_count(dialogue);
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert_eq!(
            sentence_count, MAX_DIALOGUE_SENTENCES,
            "capped reply should contain exactly {MAX_DIALOGUE_SENTENCES} sentences: {result:?}"
        );
        assert!(
            result.ends_with("There's much talk about the rent collectors."),
            "the third semantic beat should survive in order: {result:?}"
        );
        assert!(
            !result.contains("what brings ye here"),
            "a trailing question must not displace substantive content: {result:?}"
        );
    }

    #[test]
    fn cap_no_trailing_question_unchanged_behavior() {
        // An 8-sentence reply with NO trailing question must produce the same
        // result as before: keep the first sentences (head, not tail).
        let dialogue = "The morning is fine enough today. \
            I was just heading to the market in town. \
            Brigid asked me to pick up some wool thread for her. \
            The last market was rained out entirely. \
            They say the cattle prices have dropped again. \
            Old Fionnuala was complaining about the cold. \
            The priest gave a long sermon on Sunday. \
            No doubt things will look better by Easter.";
        let result = cap_sentence_count(dialogue);
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= MAX_DIALOGUE_SENTENCES,
            "capped reply without trailing question should be <={MAX_DIALOGUE_SENTENCES} sentences, got {sentence_count}: {result:?}"
        );
        // First sentence must survive (head-keep behavior unchanged).
        assert!(
            result.contains("The morning is fine enough today"),
            "first sentence should survive when no trailing question: {result:?}"
        );
        // Tail sentences beyond 4 must be dropped.
        assert!(
            !result.contains("No doubt things will look better by Easter"),
            "tail sentence should be dropped when no trailing question: {result:?}"
        );
    }

    /// Regression (#1775): Peig answered the direct identity question in the
    /// raw completion, but the old terse cap tail-swapped that answer away.
    #[test]
    fn peig_harness_identity_answer_survives_terse_cap() {
        let dialogue = "Good morning. Peig Hannigan's the name. \
            New to these parts, eh? What brings ye here?";

        assert_eq!(
            guard_verbosity_runons_with_mood(dialogue, Some("sharp")),
            "Peig Hannigan's the name. New to these parts, eh?"
        );
    }

    /// Regression (#1787): Tommy's exact four-sentence harness completion
    /// escaped the former four-sentence guard despite the prompt's cap.
    #[test]
    fn tommy_harness_reply_drops_greeting_and_keeps_three_substantive_sentences() {
        let dialogue = "Ah, good day to you, Aiden Carney. \
            I've seen ye around, though we haven't spoken much. \
            As for advice, it's simple enough: listen to the land and the folk here. \
            The crossroads are a place of power, so pay attention to the tales and the whispers of the past.";

        assert_eq!(
            guard_verbosity_runons_with_mood(dialogue, Some("reflective")),
            "I've seen ye around, though we haven't spoken much. \
             As for advice, it's simple enough: listen to the land and the folk here. \
             The crossroads are a place of power, so pay attention to the tales and the whispers of the past."
        );
    }

    #[test]
    fn terse_four_sentence_reply_does_not_bypass_shared_cap() {
        let dialogue = "The gate is shut. The road is wet. The mare is lame. Come again tomorrow.";
        assert!(dialogue.split_whitespace().count() <= 45);

        assert_eq!(
            cap_sentence_count(dialogue),
            "The gate is shut. The road is wet. The mare is lame."
        );
    }

    #[test]
    fn substantive_greeting_sentence_is_not_discarded() {
        let dialogue = "Good morning, the road is flooded. \
            Take the upper path. Mind the broken stile. Bring a stout stick.";

        assert_eq!(
            cap_sentence_count(dialogue),
            "Good morning, the road is flooded. Take the upper path. Mind the broken stile."
        );
    }

    #[test]
    fn trailing_ellipsis_after_sentence_is_stripped() {
        // #1472: trailing ".…" artifact after an otherwise-complete sentence is stripped.
        // The Cormac Duffy harness repro: "That'll warm ye up.…"
        let dialogue = "Here, have a cup of the spiced tea. That'll warm ye up.\u{2026}";
        let result = strip_trailing_ellipsis_artifact(dialogue);
        assert!(
            !result.ends_with('\u{2026}'),
            "trailing ellipsis artifact should be stripped: {result:?}"
        );
        assert!(
            result.ends_with('.'),
            "result should end with a period after stripping: {result:?}"
        );
        assert!(
            result.contains("That'll warm ye up"),
            "sentence content must be preserved: {result:?}"
        );
    }

    #[test]
    fn trailing_ellipsis_dot_ellipsis_stripped() {
        // Pattern ".…" — period followed by ellipsis (the harness-observed form).
        let dialogue = "Sit ye down and rest.…";
        let result = strip_trailing_ellipsis_artifact(dialogue);
        assert_eq!(
            result, "Sit ye down and rest.",
            "'.…' suffix should be stripped leaving just the period: {result:?}"
        );
    }

    #[test]
    fn trailing_ellipsis_bare_after_punct_stripped() {
        // Bare "…" immediately after sentence punctuation — decoration artifact.
        let dialogue = "Mind yer step on the path.…";
        let result = strip_trailing_ellipsis_artifact(dialogue);
        assert!(
            !result.ends_with('\u{2026}'),
            "bare trailing ellipsis after punct should be stripped: {result:?}"
        );
    }

    #[test]
    fn mid_sentence_ellipsis_not_stripped_by_artifact_guard() {
        // A genuine mid-sentence ellipsis (not after sentence punctuation) must NOT
        // be stripped by the artifact guard — that is the domain of trim_mid_sentence_truncation.
        let dialogue = "I was heading to the mill to collect the grain…";
        let result = strip_trailing_ellipsis_artifact(dialogue);
        // The artifact guard only strips when the char before "…" is sentence punct.
        // Here "n" precedes "…", so this is a genuine truncation — leave it alone.
        assert_eq!(
            result, dialogue,
            "mid-sentence ellipsis should not be altered by the artifact guard: {result:?}"
        );
    }

    #[test]
    fn verbosity_guard_strips_trailing_ellipsis_artifact_end_to_end() {
        // Integration (#1472): guard_verbosity_runons strips the ".…" artifact in
        // the full pipeline so that replies like "That'll warm ye up.…" end cleanly.
        let dialogue = "Sit ye down, friend. That'll warm ye up.\u{2026}";
        let result = guard_verbosity_runons(dialogue);
        assert!(
            !result.ends_with('\u{2026}'),
            "guard_verbosity_runons should strip trailing ellipsis artifact: {result:?}"
        );
        assert!(
            result.contains("warm ye up"),
            "sentence content must survive the full pipeline: {result:?}"
        );
    }

    #[test]
    fn verbosity_guard_caps_8_sentence_ramble_end_to_end() {
        // Integration (#1472): guard_verbosity_runons caps a long distinct-sentence
        // ramble that the surgical guards cannot reduce (no duplicates, no question
        // stack). The sentence cap must kick in as the final backstop.
        let dialogue = "Good day to ye. \
            Come in out of the cold and rest yer bones. \
            We have tea on the fire and bread on the table. \
            The weather has been fierce hard on everyone this past week. \
            They say the river rose three feet at Strokestown. \
            Brigid had a fine time of it at the fair last Tuesday. \
            The priest called in only yesterday about the collection. \
            Mind ye don't leave yer coat on the wet step.";
        let result = guard_verbosity_runons(dialogue);
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= MAX_DIALOGUE_SENTENCES,
            "8-sentence ramble should be capped to <={MAX_DIALOGUE_SENTENCES} sentences, got {sentence_count}: {result:?}"
        );
        assert!(
            result.contains("Come in out of the cold"),
            "first substantive sentence must survive: {result:?}"
        );
        assert!(
            !result.contains("Good day to ye"),
            "standalone greeting should be discarded before substantive beats: {result:?}"
        );
    }

    // ── #1475 — wrong-speaker identity guard ─────────────────────────────────

    fn nora_roster() -> Vec<(String, String)> {
        vec![
            ("Nora Duffy".to_string(), "Miller's Assistant".to_string()),
            ("Brendan Walsh".to_string(), "Miller's Son".to_string()),
            ("Cormac Duffy".to_string(), "Miller".to_string()),
        ]
    }

    /// AC-1: NPC adopts another roster member's name → guard fires and replaces.
    #[test]
    fn wrong_speaker_guard_fires_on_stolen_roster_name() {
        let dialogue = "Sure, I'm Brendan, the Miller's Son. Ye've caught me just 'tween chores.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &nora_roster(), 0);
        assert!(
            !result.to_lowercase().contains("brendan"),
            "stolen-identity dialogue must be replaced, must not contain other NPC's name: {result:?}"
        );
        // Must be a recovery line, not blank.
        assert!(
            !result.trim().is_empty(),
            "replacement must not be blank: {result:?}"
        );
    }

    /// Regression: the 14B routinely emits a typographic apostrophe (U+2019).
    /// "I’m Brendan" (curly quote) must fire the guard just like the ASCII form.
    #[test]
    fn wrong_speaker_guard_fires_on_typographic_apostrophe() {
        let dialogue = "Sure, I\u{2019}m Brendan, the Miller\u{2019}s Son.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &nora_roster(), 0);
        assert!(
            !result.to_lowercase().contains("brendan"),
            "curly-apostrophe identity claim must still be replaced: {result:?}"
        );
    }

    /// AC-2: NPC correctly introduces itself → guard must NOT fire.
    #[test]
    fn wrong_speaker_guard_does_not_fire_on_correct_self_introduction() {
        let dialogue = "I'm Nora Duffy, the Miller's Assistant. Grand day to ye.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &nora_roster(), 0);
        assert_eq!(
            result, dialogue,
            "correct self-introduction must pass through unchanged: {result:?}"
        );
    }

    /// AC-3: NPC mentions another roster member in third person → guard must NOT fire.
    #[test]
    fn wrong_speaker_guard_does_not_fire_on_third_person_mention() {
        let dialogue =
            "Ye'll want to speak to Brendan Walsh for that — he knows the mill better than I.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &nora_roster(), 0);
        assert_eq!(
            result, dialogue,
            "third-person mention of another NPC must pass through unchanged: {result:?}"
        );
    }

    /// AC-4: "the name's X" pattern fires for another roster member.
    #[test]
    fn wrong_speaker_guard_fires_on_the_names_pattern() {
        let dialogue = "The name's Brendan, friend, Miller's Son of Kilteevan.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &nora_roster(), 1);
        assert!(
            !result.to_lowercase().contains("brendan"),
            "'the name's X' wrong-identity must be replaced: {result:?}"
        );
    }

    /// AC-5: "I am X, the Y" pattern fires for another roster member.
    #[test]
    fn wrong_speaker_guard_fires_on_i_am_pattern() {
        let dialogue = "I am Brendan, the Miller's Son. Come in.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &nora_roster(), 2);
        assert!(
            !result.to_lowercase().contains("i am brendan"),
            "'I am X' wrong-identity must be replaced: {result:?}"
        );
    }

    /// AC-6 kill-switch: guard returns dialogue unchanged when skipped (caller check).
    /// (The caller checks `config.flags.is_disabled(...)` and skips the guard call.
    /// We test the guard itself returns unchanged when the roster is too small.)
    #[test]
    fn wrong_speaker_guard_noop_on_single_member_roster() {
        let dialogue = "I'm Brendan, the Miller's Son.";
        // Only one roster member (no other NPC to steal from).
        let single_roster = vec![("Nora Duffy".to_string(), "Miller's Assistant".to_string())];
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &single_roster, 0);
        assert_eq!(
            result, dialogue,
            "guard must not fire when roster has only the speaker: {result:?}"
        );
    }

    /// AC-7: empty roster → guard is a no-op.
    #[test]
    fn wrong_speaker_guard_noop_on_empty_roster() {
        let dialogue = "I'm Brendan, the Miller's Son.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &[], 0);
        assert_eq!(
            result, dialogue,
            "guard must not fire when roster is empty: {result:?}"
        );
    }

    /// Regression: "my name is X" pattern fires for another roster member.
    #[test]
    fn wrong_speaker_guard_fires_on_my_name_is_pattern() {
        let dialogue = "My name is Cormac, the Miller. I've been at the wheel since dawn.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &nora_roster(), 3);
        assert!(
            !result.to_lowercase().contains("cormac"),
            "'my name is X' wrong-identity must be replaced: {result:?}"
        );
    }

    /// Conservative: first-name-only correct self-ref passes through.
    #[test]
    fn wrong_speaker_guard_does_not_fire_on_first_name_self_ref() {
        // "I'm Nora" is the speaker's own first name — must pass through.
        let dialogue = "I'm Nora, and I've been here since morning.";
        let result = guard_wrong_speaker_identity(dialogue, "Nora Duffy", &nora_roster(), 0);
        assert_eq!(
            result, dialogue,
            "first-name self-reference must not be blocked: {result:?}"
        );
    }

    // ── #1487 — degenerate intra-response phrase-loop guard ───────────────────

    /// AC-1 (#1487): the priest repro — a short clause repeated 15× to the
    /// token cap. The guard must detect the loop and truncate at a clean
    /// sentence boundary before the loop begins.
    #[test]
    fn degenerate_phrase_loop_15x_repetition_collapsed() {
        // The harness-observed repro pattern: "in His time and His way, and
        // His purpose…" repeated ~15 times, no terminal punctuation between
        // occurrences. We represent it as repeating 15×.
        let preamble = "God's blessings upon ye, and may ye find what ye seek.";
        let loop_clause = "in His time and His way and His purpose";
        // Build: preamble + 15× the loop clause separated by commas
        let loop_part = std::iter::repeat_n(loop_clause, 15)
            .collect::<Vec<_>>()
            .join(", ");
        let dialogue = format!("{preamble} {loop_part}");
        let result = collapse_degenerate_phrase_loop(&dialogue);
        // The preamble must survive.
        assert!(
            result.contains("God's blessings upon ye"),
            "preamble sentence must survive: {result:?}"
        );
        // The loop must be gone.
        let occurrences = result
            .to_lowercase()
            .matches("in his time and his way")
            .count();
        assert!(
            occurrences <= 1,
            "phrase loop must collapse to ≤1 occurrence, got {occurrences}: {result:?}"
        );
    }

    /// AC-2 (#1487): a reply with varied legitimate prose must not be altered.
    #[test]
    fn degenerate_phrase_loop_leaves_varied_prose_unchanged() {
        let dialogue = "A fine morning to ye. The harvest has come in well this year. \
            Brigid Connolly was asking after the grain prices only yesterday. \
            And the priest is expected back by Thursday, so they say.";
        let result = collapse_degenerate_phrase_loop(dialogue);
        assert!(
            result.contains("fine morning"),
            "first sentence must survive: {result:?}"
        );
        assert!(
            result.contains("Brigid Connolly"),
            "third sentence must survive: {result:?}"
        );
        assert!(
            result.contains("Thursday"),
            "last sentence must survive: {result:?}"
        );
    }

    /// AC-3 (#1487): a phrase repeated exactly 3× (below the 4× threshold)
    /// must not trigger `collapse_degenerate_phrase_loop` — natural repetition
    /// must be allowed.
    #[test]
    fn degenerate_phrase_loop_below_threshold_passes_through() {
        // "come back soon" appears 3× — below the 4× threshold for 3-word
        // phrases, so this must pass through unchanged.
        let dialogue = "Come back soon, come back soon, come back soon, friend.";
        let result = collapse_degenerate_phrase_loop(dialogue);
        // The guard must not have fired (3× < 4× threshold for 3-word n-grams).
        assert!(
            result.to_lowercase().contains("come back soon"),
            "3× repetition of a 3-word phrase should pass through (3 < 4 threshold): {result:?}"
        );
    }

    /// AC-4 (#1487): the guard integrates into `guard_verbosity_runons`
    /// end-to-end, so a runaway is fully cleaned even through the full pipeline.
    #[test]
    fn guard_verbosity_runons_collapses_phrase_loop_end_to_end() {
        let preamble = "Peace be with ye, friend.";
        let loop_clause = "in His time and His purpose";
        let loop_part = std::iter::repeat_n(loop_clause, 12)
            .collect::<Vec<_>>()
            .join(", ");
        let dialogue = format!("{preamble} {loop_part}");
        let result = guard_verbosity_runons(&dialogue);
        // The preamble must survive.
        assert!(
            result.contains("Peace be with ye"),
            "preamble must survive guard pipeline: {result:?}"
        );
        // The loop must be gone.
        let occurrences = result
            .to_lowercase()
            .matches("in his time and his purpose")
            .count();
        assert!(
            occurrences <= 1,
            "phrase loop must be gone after guard pipeline, got {occurrences}: {result:?}"
        );
    }

    // ── #1489 — word-count cap guard ─────────────────────────────────────────

    /// AC-1 (#1489): a reply over 80 words must be trimmed to the last sentence
    /// boundary within the 80-word budget.
    #[test]
    fn word_count_cap_trims_overlong_reply() {
        // Build a 100-word reply across 3 sentences.
        // S1: ~10 words, S2: ~55 words, S3: ~35 words — total ~100 words.
        let s1 = "Good day to ye, friend, and welcome to Kilteevan.";
        let s2 = "The roads have been fierce hard on everyone this past fortnight and the river at \
            Strokestown rose three whole feet after the terrible rains came down last Tuesday \
            and Wednesday and the landlord sent his agent round to collect the rents early \
            before the harvest was even in which caused great hardship to all.";
        let s3 = "But sure the sun is shining today and the market in town is full of good cheer \
            and the grain prices are better than they were last harvest time which is a true blessing.";
        let dialogue = format!("{s1} {s2} {s3}");
        let word_count = dialogue.split_whitespace().count();
        assert!(
            word_count > 80,
            "test input must be >80 words, got {word_count}"
        );
        let result = cap_word_count(&dialogue);
        let result_words = result.split_whitespace().count();
        assert!(
            result_words <= 80,
            "output must be <=80 words, got {result_words}: {result:?}"
        );
        // The first sentence must always survive.
        assert!(
            result.contains("Good day to ye"),
            "first sentence must survive: {result:?}"
        );
        // The result must end with sentence punctuation.
        assert!(
            result.trim_end().ends_with(['.', '!', '?']),
            "result must end with sentence punctuation: {result:?}"
        );
    }

    /// AC-2 (#1489): a reply of 80 words or fewer must pass through unchanged.
    #[test]
    fn word_count_cap_noop_on_short_reply() {
        let dialogue = "Good day to ye. The harvest has been fine this year. \
            Brigid Connolly was asking after the grain prices only yesterday.";
        let word_count = dialogue.split_whitespace().count();
        assert!(
            word_count <= 80,
            "test input must be <=80 words, got {word_count}"
        );
        let result = cap_word_count(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "short reply must pass through unchanged"
        );
    }

    /// AC-3 (#1489): the cap integrates into guard_verbosity_runons, so a
    /// very long reply (>80 words, few sentences) is trimmed by the pipeline.
    #[test]
    fn guard_verbosity_runons_word_cap_end_to_end() {
        // A reply that is only 2 sentences but 100+ words (bypasses sentence cap).
        let s1 = "Good day to ye, friend, and welcome to the parish.";
        let s2 = "The roads have been fierce hard on everyone this past fortnight and the river at \
            Strokestown rose three whole feet after the terrible rains came down last Tuesday \
            and Wednesday and the landlord sent his agent round to collect the rents early \
            before the harvest was even in which caused great hardship to all the families \
            in the townland and the priest gave a long sermon about charity on Sunday morning \
            but sure people are struggling all the same and the market prices are terrible.";
        let dialogue = format!("{s1} {s2}");
        let word_count = dialogue.split_whitespace().count();
        assert!(
            word_count > 80,
            "test input must be >80 words, got {word_count}"
        );
        let result = guard_verbosity_runons(&dialogue);
        let result_words = result.split_whitespace().count();
        assert!(
            result_words <= 80,
            "guard pipeline must trim >80 word reply, got {result_words}: {result:?}"
        );
    }

    // ── #1500 regression — honorific-prefix false-denial guard ───────────────
    //
    // Quality-harness run 16 surfaced a high-severity regression: the
    // person-confirmation guard fired on CORRECT model output when the
    // player's question or prior transcript contained a name using the
    // spelled-out honorific ("Father Declan") while the roster stored the
    // abbreviated form ("Fr. Declan Tierney"). The guard treated "Father Declan"
    // as a fabricated full name (exact-string match found nothing), then saw the
    // NPC's correct reply containing "Father Declan" with affirmation markers
    // and replaced it with the non-recognition decline.
    //
    // The fix adds honorific-abbreviation normalisation to `name_in_roster` so
    // that "Father Declan" → "father declan" prefix-matches "Fr. Declan Tierney"
    // → "father declan tierney" after normalisation.

    /// AC-1 (guard-override-false-denial): the guard must STILL fire when the
    /// NPC affirms a genuinely fabricated stranger whose name appears nowhere
    /// in the roster — even after the honorific-normalisation fix. Core
    /// legitimate-fabrication-detection behavior must be preserved.
    #[test]
    fn fabricated_stranger_still_declined_after_honorific_fix() {
        let dialogue = "Aye, I know Cormac Sweeney well. He is a fine man who works at the mill.";
        let player_input = "Do you know Cormac Sweeney?";
        // Roster does NOT contain Cormac Sweeney — he is fabricated.
        let known: Vec<String> = vec!["Brigid Connolly".into(), "Tadhg Murphy".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 3);
        assert!(
            !result.to_lowercase().contains("aye, i know cormac sweeney"),
            "guard must still fire on a genuinely fabricated stranger (#1500): {result:?}"
        );
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "result should be a decline phrase: {result:?}"
        );
    }

    /// AC-2 (guard-override-false-denial): when the NPC states its own name in
    /// a self-introduction ("I am Father Declan Tierney, at your service."),
    /// the guard must NOT replace the reply with a denial, even if the player
    /// asked a question with no name in it. The NPC's own name is in the roster
    /// (stored as "Fr. Declan Tierney"); after honorific normalisation the guard
    /// correctly recognises it as a known person.
    #[test]
    fn self_introduction_not_denied_after_honorific_fix() {
        // Player's question has no title-cased name to extract — no candidates.
        // The NPC is the priest; his name is stored as "Fr. Declan Tierney" in
        // the roster. The model generates "Father Declan Tierney" in the reply.
        let dialogue = "I am Father Declan Tierney, at your service.";
        let player_input = "Would you tell me your name?";
        let known: Vec<String> = vec![
            "Fr. Declan Tierney".into(),
            "Brigid Connolly".into(),
            "Seamus Moran".into(),
        ];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "NPC self-introduction must NOT be replaced by the guard (#1500): {result:?}"
        );
    }

    /// AC-3 (guard-override-false-denial): when the player's question uses
    /// the spelled-out honorific ("Father Declan Tierney") for a real roster
    /// member stored with an abbreviation ("Fr. Declan Tierney"), the NPC's
    /// correct reply about that person must NOT be replaced with a denial.
    /// This is the direct repro for the quality-harness run 16 regression.
    #[test]
    fn spelled_out_honorific_of_real_roster_member_not_denied() {
        // Player asks "What does Father Declan Tierney think about this?"
        // The NPC (the priest himself, or another NPC) says "Father Declan is a
        // man of the cloth" — a true, roster-grounded statement.
        // Before the fix: "Father Declan Tierney" extracted as a bigram/trigram,
        // not found in the roster (stored as "Fr. Declan Tierney"), treated as
        // fabricated, guard fires.
        // After the fix: honorific normalisation makes "Father Declan Tierney"
        // equal to "Fr. Declan Tierney" → not fabricated → guard does not fire.
        let dialogue = "'Tis a good laugh, but I'm no priest. Father Declan is a man of the cloth.";
        let player_input = "Surely Father Declan Tierney is the one I mean?";
        let known: Vec<String> = vec![
            "Fr. Declan Tierney".into(),
            "Brigid Connolly".into(),
            "Seamus Moran".into(),
        ];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 0);
        assert_eq!(
            result, dialogue,
            "NPC correctly naming a real roster member via spelled-out honorific must NOT be \
             replaced by the guard — honorific normalisation fix (#1500): {result:?}"
        );
    }

    /// AC-4 (guard-override-false-denial): honorific normalisation must not
    /// cause a cross-person false-positive. "Father Smith" must NOT match
    /// "Fr. Tierney" just because both start with the same canonical honorific
    /// after normalisation. The suffix (given name) must also align.
    #[test]
    fn honorific_normalization_does_not_cross_match_different_persons() {
        // "Father Smith" (fabricated) vs roster "Fr. Declan Tierney".
        // After normalisation: "father smith" vs "father declan tierney".
        // "father smith" is NOT a prefix of "father declan tierney" → no match
        // → guard still fires on the fabricated "Father Smith".
        let dialogue = "Aye, Father Smith is a fine man. He is over at the church.";
        let player_input = "Do you know Father Smith?";
        let known: Vec<String> = vec!["Fr. Declan Tierney".into(), "Brigid Connolly".into()];
        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 1);
        assert!(
            !result.to_lowercase().contains("father smith is a fine man"),
            "guard must still fire when the honorific matches but the given name differs (#1500): {result:?}"
        );
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "result should be a decline phrase: {result:?}"
        );
    }

    // ── #1504 — acquaintance-question / identity-drift guard ──────────────────

    /// AC-1 (#1504): the exact run-16 repro — Seamus asked "Do you know Father
    /// Declan Tierney?" responds with a pure self-identity denial. The guard
    /// must detect the question-type mismatch and replace.
    ///
    /// "Fr. Declan Tierney" IS in the roster, so the replacement must be an
    /// acquaintance-affirmation response, not a non-recognition decline.
    #[test]
    fn acquaintance_intent_drift_guard_fires_on_run16_repro() {
        let dialogue = "Ye must be mistaken... I'm but Seamus Gallagher.";
        let player_input = "Do you know Father Declan Tierney?";
        let speaker = "Seamus Gallagher";
        let known: Vec<String> = vec![
            "Fr. Declan Tierney".into(),
            "Brigid Connolly".into(),
            "Seamus Gallagher".into(),
        ];
        let result =
            guard_acquaintance_question_intent_drift(dialogue, player_input, speaker, &known, 0);
        // The self-identity drift must not survive.
        assert!(
            !result.to_lowercase().contains("i'm but seamus"),
            "guard must have fired: original drift still present in: {result:?}"
        );
        // Replacement must be an acquaintance-affirmation (person IS in roster).
        let affirm_pool = [
            "know them",
            "know who ye mean",
            "known to me",
            "know them well",
            "know them from",
        ];
        assert!(
            affirm_pool
                .iter()
                .any(|p| result.to_lowercase().contains(p)),
            "replacement must be an acquaintance-affirmation when named person is in roster; \
             got: {result:?}"
        );
    }

    /// AC-2 (#1504): when the named person is NOT in the roster, the replacement
    /// must be a non-recognition decline, not an affirmation.
    #[test]
    fn acquaintance_intent_drift_guard_declines_for_unknown_person() {
        let dialogue = "Ye must be mistaken — I'm but Brigid Connolly.";
        let player_input = "Do you know Cormac Sweeney?";
        let speaker = "Brigid Connolly";
        let known: Vec<String> = vec!["Fr. Declan Tierney".into(), "Brigid Connolly".into()];
        let result =
            guard_acquaintance_question_intent_drift(dialogue, player_input, speaker, &known, 0);
        assert!(
            !result.to_lowercase().contains("brigid connolly"),
            "guard must have replaced drift when unknown name asked: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("no")
                || result.to_lowercase().contains("not known")
                || result.to_lowercase().contains("never heard")
                || result.to_lowercase().contains("no one"),
            "replacement must be a non-recognition decline for unknown person: {result:?}"
        );
    }

    /// AC-3 (#1504): when the NPC's reply actually addresses the acquaintance
    /// question (contains "know", "met", etc.), the guard must NOT fire.
    #[test]
    fn acquaintance_intent_drift_guard_does_not_fire_on_correct_response() {
        let dialogue = "Aye, I know Fr. Declan Tierney well — he's the parish priest.";
        let player_input = "Do you know Father Declan Tierney?";
        let speaker = "Seamus Gallagher";
        let known: Vec<String> = vec!["Fr. Declan Tierney".into(), "Seamus Gallagher".into()];
        let result =
            guard_acquaintance_question_intent_drift(dialogue, player_input, speaker, &known, 0);
        assert_eq!(
            result, dialogue,
            "guard must NOT fire when NPC correctly addressed the acquaintance question: {result:?}"
        );
    }

    /// AC-4 (#1504): when the player asked a non-acquaintance question, the
    /// guard must not fire even if the NPC's reply is a self-identification.
    #[test]
    fn acquaintance_intent_drift_guard_does_not_fire_on_non_acquaintance_question() {
        let dialogue = "Aye, I'm Seamus Gallagher, at yer service.";
        let player_input = "What is your name?";
        let speaker = "Seamus Gallagher";
        let known: Vec<String> = vec!["Seamus Gallagher".into()];
        let result =
            guard_acquaintance_question_intent_drift(dialogue, player_input, speaker, &known, 0);
        assert_eq!(
            result, dialogue,
            "guard must NOT fire on identity question (not an acquaintance question): {result:?}"
        );
    }

    /// AC-5 (#1504): "have you met X?" pattern triggers the acquaintance guard.
    #[test]
    fn acquaintance_intent_drift_guard_fires_on_have_you_met_pattern() {
        let dialogue = "Ye must be mistaken — I'm only Brigid Connolly, the midwife.";
        let player_input = "Have you met Cormac Duffy?";
        let speaker = "Brigid Connolly";
        let known: Vec<String> = vec!["Brigid Connolly".into(), "Cormac Duffy".into()];
        let result =
            guard_acquaintance_question_intent_drift(dialogue, player_input, speaker, &known, 2);
        assert!(
            !result.to_lowercase().contains("only brigid connolly"),
            "guard must have fired on 'have you met' pattern: {result:?}"
        );
    }

    /// AC-6 (#1504): normal dialogue that happens to use first-person pronouns
    /// but addresses the acquaintance question must NOT be suppressed.
    #[test]
    fn acquaintance_intent_drift_guard_does_not_fire_on_normal_first_person() {
        let dialogue = "I've not heard of any Cormac Sweeney in these parts.";
        let player_input = "Do you know Cormac Sweeney?";
        let speaker = "Seamus Gallagher";
        let known: Vec<String> = vec!["Seamus Gallagher".into()];
        let result =
            guard_acquaintance_question_intent_drift(dialogue, player_input, speaker, &known, 0);
        assert_eq!(
            result, dialogue,
            "guard must NOT fire: NPC's reply addresses the acquaintance question \
             (contains 'heard'): {result:?}"
        );
    }

    /// AC-7 (#1504): "do ye know" (Hiberno-English variant) also triggers detection.
    #[test]
    fn acquaintance_intent_drift_guard_fires_on_hiberno_english_variant() {
        let dialogue = "Ye have the wrong man — I'm just Padraig O'Brien.";
        let player_input = "Do ye know Father Declan?";
        let speaker = "Padraig O'Brien";
        // Father Declan is a first-name-only reference — not roster-matchable by full name.
        let known: Vec<String> = vec!["Padraig O'Brien".into()];
        let result =
            guard_acquaintance_question_intent_drift(dialogue, player_input, speaker, &known, 1);
        assert!(
            !result.to_lowercase().contains("just padraig"),
            "guard must have fired on 'do ye know' variant: {result:?}"
        );
    }

    // ── #1505 — consecutive short-phrase near-repetition ("tell me, tell me") ──

    /// AC-1 (#1505): a run-on reply containing "tell me, tell me" (a 2-word
    /// phrase repeated twice consecutively) must be stripped by
    /// `strip_consecutive_short_phrase_repeat`. The second "tell me" is removed.
    #[test]
    fn two_word_consecutive_repeat_is_stripped() {
        // Adversarial: "tell me, tell me" — the exact pattern from #1505.
        let dialogue = "Does the journey yet weigh heavy on yer heart, I wonder, tell me, tell me?";
        let result = strip_consecutive_short_phrase_repeat(dialogue);
        let tell_me_count = result.to_lowercase().matches("tell me").count();
        assert!(
            tell_me_count <= 1,
            "consecutive \"tell me, tell me\" must be collapsed to one; \
             got {tell_me_count}: {result:?}"
        );
    }

    /// AC-2 (#1505): the full `guard_verbosity_runons` pipeline trims the
    /// observed #1505 run-on. This exercises the real wiring path.
    #[test]
    fn verbosity_guard_trims_tell_me_runon() {
        // Exact style of the #1505 evidence string: a run-on with embedded
        // "tell me, tell me" near-repetition.
        let dialogue = "Aye, the journey is a long one from the south. \
             Does the journey yet weigh heavy on yer heart, I wonder indeed, \
             aye or nay, tell me, tell me?";
        let result = guard_verbosity_runons(dialogue);
        // At most one "tell me" in the output.
        let tell_me_count = result.to_lowercase().matches("tell me").count();
        assert!(
            tell_me_count <= 1,
            "guard pipeline must remove repeated \"tell me\"; got {tell_me_count}: {result:?}"
        );
        // The reply must not be empty.
        assert!(
            !result.trim().is_empty(),
            "guard must not produce empty output for a reply with a good first sentence"
        );
    }

    /// AC-3 (#1505): a legitimate reply with no repeated short phrase passes
    /// through `strip_consecutive_short_phrase_repeat` unchanged. The guard must
    /// be conservative enough not to alter clean prose.
    #[test]
    fn consecutive_repeat_guard_noop_on_clean_reply() {
        let dialogue = "Good day to ye. The harvest has been fair enough this year, God willing. \
             Have ye news from the town?";
        let result = strip_consecutive_short_phrase_repeat(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "clean reply without short-phrase repetition must be unchanged: {result:?}"
        );
    }

    // ── #1492 — cross-NPC stock opener across separate turns ─────────────────
    //
    // The `dedupe_cross_npc_openers` unit tests already cover the within-turn
    // dedup logic. The session-level fix (#1492) is exercised by the integration
    // test in `parish-engine/tests/npc_verbosity_multiquestion_real_loop.rs`.
    // The unit-level test below verifies the `ConversationRuntimeState` plumbing
    // independently (without a full game-loop round-trip).

    /// AC-4 (#1492): `dedupe_cross_npc_openers` produces no dedup on the first
    /// NPC (empty prior openers), and strips a repeated opener from the second
    /// NPC when the first NPC's opener is added to the prior list between calls —
    /// simulating the session-level accumulation added for #1492.
    ///
    /// Uses a pair of openers with Jaccard similarity ≥ 0.75 (the dedup
    /// threshold) to reliably trigger the guard across turns.
    #[test]
    fn cross_npc_opener_accumulation_across_sequential_calls() {
        // First NPC's reply — no priors yet.
        let priors: Vec<String> = vec![];
        let npc_a_reply = "Ye've come to the right place for news. The crossroads is just ahead.";
        let result_a = dedupe_cross_npc_openers(&priors, npc_a_reply);
        assert_eq!(
            result_a, npc_a_reply,
            "first NPC reply with no priors must pass through unchanged"
        );

        // Extract the opener from NPC A and add to the accumulator (simulates
        // session-level record_opener).
        let opener_a = extract_normalized_opener(&result_a);
        let priors_after_a = vec![opener_a];

        // Second NPC (next turn) opens with a near-identical stock phrase.
        // "Ye've come to the right place for information" shares {ye've, come, to,
        // the, right, place, for} = 7 words with the prior; union = 9; Jaccard
        // = 7/9 ≈ 0.78 ≥ 0.75 — above the dedup threshold.
        let npc_b_reply =
            "Ye've come to the right place for information. The church is just ahead.";
        let result_b = dedupe_cross_npc_openers(&priors_after_a, npc_b_reply);
        // The duplicate opener must be stripped.
        assert!(
            !result_b
                .to_lowercase()
                .starts_with("ye've come to the right place"),
            "second NPC's duplicate opener (across turns) must be stripped: {result_b:?}"
        );
        // The substantive remainder must survive.
        assert!(
            result_b.contains("church is just ahead"),
            "substantive content after stripped opener must survive: {result_b:?}"
        );
    }

    #[test]
    fn opener_dedup_does_not_strip_leading_honorific_abbreviation() {
        let first = "Fr. Declan Tierney spoke plainly about the old road indeed.";
        let second = "Fr. Declan Tierney spoke plainly about the old road surely.";
        let priors = vec![extract_normalized_opener(first)];

        assert_eq!(dedupe_cross_npc_openers(&priors, second), second);
        assert!(
            extract_normalized_opener(first).starts_with("fr. declan tierney"),
            "the title abbreviation must be joined to the actual opener"
        );
    }

    // ── #1491 — mood-aware sentence cap ──────────────────────────────────────

    /// AC-1 (#1491): A 5-sentence reply with a "busy" mood is capped at 2.
    #[test]
    fn mood_aware_sentence_cap_busy_mood_caps_at_2() {
        let dialogue = "Aye, I've heard that. The rents are fierce high. \
            The harvest was poor this year. The landlord takes no pity. \
            God help us all.";
        let result = cap_sentence_count_for_mood(dialogue, Some("busy"));
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= 2,
            "busy mood must cap at 2 sentences; got {sentence_count}: {result:?}"
        );
        // First sentence survives.
        assert!(
            result.contains("heard that"),
            "first sentence must survive: {result:?}"
        );
    }

    /// AC-2 (#1491): A "curt" mood also gets the 2-sentence cap.
    #[test]
    fn mood_aware_sentence_cap_curt_mood_caps_at_2() {
        let dialogue = "I'm busy. Leave me be. There's work to do. Come back later.";
        let result = cap_sentence_count_for_mood(dialogue, Some("curt"));
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= 2,
            "curt mood must cap at 2 sentences; got {sentence_count}: {result:?}"
        );
    }

    /// AC-3 (#1491): A neutral mood keeps the shared 3-sentence cap.
    #[test]
    fn mood_aware_sentence_cap_neutral_mood_keeps_3() {
        let dialogue = "The weather has been mild enough for the cattle in the lower meadow. \
            The spring grass came early, so every beast has kept a little strength. \
            I saw the priest this morning walking the road toward the chapel. \
            The chapel bell sounded before he reached the lane. \
            He sent his regards and asked after every family by name.";
        assert!(dialogue.split_whitespace().count() > 45);
        let result = cap_sentence_count_for_mood(dialogue, Some("neutral"));
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= MAX_DIALOGUE_SENTENCES,
            "neutral mood must cap at {MAX_DIALOGUE_SENTENCES}; got {sentence_count}: {result:?}"
        );
        assert_eq!(
            sentence_count, MAX_DIALOGUE_SENTENCES,
            "neutral mood with 5 sentences must keep {MAX_DIALOGUE_SENTENCES}; got {sentence_count}: {result:?}"
        );
    }

    /// AC-4 (#1491): None mood uses the shared 3-sentence cap (no panic).
    #[test]
    fn mood_aware_sentence_cap_none_mood_uses_default() {
        let dialogue = "One long sentence gathers enough words for the default cap to matter. \
            Two long sentence adds a little more local news for the test. \
            Three long sentence keeps the reply ordinary while staying above the budget. \
            Four long sentence should remain after the default cap trims the tail. \
            Five long sentence should be dropped only when the budgeted cap applies.";
        assert!(dialogue.split_whitespace().count() > 45);
        let result = cap_sentence_count_for_mood(dialogue, None);
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= MAX_DIALOGUE_SENTENCES,
            "None mood must cap at {MAX_DIALOGUE_SENTENCES}; got {sentence_count}: {result:?}"
        );
        assert_eq!(
            sentence_count, MAX_DIALOGUE_SENTENCES,
            "None mood with a long 5-sentence reply must keep {MAX_DIALOGUE_SENTENCES}; got {sentence_count}: {result:?}"
        );
    }

    /// AC-5 (#1491): guard_verbosity_runons_with_mood applies the full pipeline with mood cap.
    #[test]
    fn verbosity_runons_with_mood_applies_full_pipeline() {
        // 5 sentences with "frustrated" mood should reduce to 2 sentences.
        let dialogue = "God help us. The road is long. The river's high. \
            The rents are fierce. May the Lord protect us.";
        let result = guard_verbosity_runons_with_mood(dialogue, Some("frustrated"));
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= 2,
            "frustrated mood pipeline must cap at 2 sentences; got {sentence_count}: {result:?}"
        );
    }

    /// AC-1 (#1566): a watchful NPC's raw sacred-place run-on must be clipped
    /// before the repeated "what do ye seek" loop reaches the player.
    #[test]
    fn watchful_mood_clips_sacred_place_runon() {
        let dialogue = "Aye, 'tis said the sidhe live in the mounds and the forts. \
            But the power here at the well, that's a different matter. \
            A blessing, mayhap, but not just for those who seek it out. \
            What do ye seek, Colm Brennan, is it for yerself or for another that \
            troubles yer thoughts this morning, aye, and brings ye to this place \
            of old magic and healing water, so it is indeed. \
            What troubles yer mind, if ye care to speak of it, and I'll do what I \
            can to ease it, if I may. \
            Ye'll not be the first to find comfort here, nor the last. \
            What brings ye to Kilteevan, and why the holy well, do ye ask, if not \
            simply to see the sights and hear the tales, aye, but to seek a deeper \
            truth or a healing hand, so it seems. \
            Tell me, and I'll listen, and if I can, I'll guide ye. \
            What do ye seek, Colm Brennan, aye, what troubles yer heart and mind \
            this mornin' so bold, aye, and brings ye here to the well, and not \
            elsewhere in the parish, if not for the sake of yer soul and the \
            whispers of the old ones, so it is indeed. \
            What do ye seek, Colm Brennan, aye, and what brings ye here to the \
            well, so it is indeed?";

        let result = guard_verbosity_runons_with_mood(dialogue, Some("watchful"));
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        let lower = result.to_lowercase();

        assert!(
            sentence_count <= 2,
            "watchful mood must keep the run-on terse; got {sentence_count}: {result:?}"
        );
        assert!(
            result.contains("mounds and the forts") && result.contains("power here at the well"),
            "grounded opening must survive: {result:?}"
        );
        assert!(
            !lower.contains("what do ye seek"),
            "repeated question loop must be removed: {result:?}"
        );
        assert!(
            !lower.contains("brings ye here to the well"),
            "later repeated loop tail must be removed: {result:?}"
        );
    }

    // ── #1477 — wrong-location reference guard ───────────────────────────────

    /// AC-1 (#1477): "here in WrongPlace" with wrong location name is corrected.
    #[test]
    fn wrong_location_guard_corrects_here_in() {
        let dialogue = "Aye, here in Strokestown, life is hard.";
        let result = guard_wrong_location_reference(dialogue, Some("Kilteevan"));
        assert!(
            result.contains("Kilteevan"),
            "wrong location should be replaced with correct one: {result:?}"
        );
        assert!(
            !result.contains("Strokestown"),
            "wrong location name must be removed: {result:?}"
        );
    }

    /// AC-2 (#1477): Correct location name mentioned — no change.
    #[test]
    fn wrong_location_guard_passes_correct_location() {
        let dialogue = "Aye, here in Kilteevan, we know how to work.";
        let result = guard_wrong_location_reference(dialogue, Some("Kilteevan"));
        assert_eq!(
            result, dialogue,
            "correct location must pass through unchanged"
        );
    }

    /// AC-3 (#1477): When current_location is None, dialogue is unchanged.
    #[test]
    fn wrong_location_guard_no_op_without_location() {
        let dialogue = "Here in Strokestown, the rents are high.";
        let result = guard_wrong_location_reference(dialogue, None);
        assert_eq!(
            result, dialogue,
            "no location provided must return dialogue unchanged"
        );
    }

    /// AC-4 (#1477): "village of X" collocation also fires.
    #[test]
    fn wrong_location_guard_village_of_collocation() {
        let dialogue = "Welcome to the village of Ballygar, stranger.";
        let result = guard_wrong_location_reference(dialogue, Some("Kilteevan"));
        assert!(
            result.contains("Kilteevan"),
            "village of collocation must be corrected: {result:?}"
        );
    }

    // ── #1478 — routing-after-denial guard ───────────────────────────────────

    fn make_names(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// AC-1 (#1478): Denial + routing phrase for fabricated person fires guard.
    #[test]
    fn routing_after_denial_guard_fires_on_denial_plus_routing() {
        let dialogue = "I know no such person in these parts, \
            but you might find him at Darcy's Pub.";
        let known = make_names(&["Mary Burke", "Seamus Flynn"]);
        let result =
            guard_fabricated_person_routing(dialogue, "Where is Cormac Sweeney?", &known, None, 0);
        // Should have replaced with a clean decline, not the original routing text.
        assert!(
            !result.to_lowercase().contains("darcy"),
            "routing phrase after denial must be replaced: {result:?}"
        );
    }

    /// AC-2 (#1478): Denial alone (no routing phrase) — guard does not fire.
    #[test]
    fn routing_after_denial_guard_no_fire_on_denial_only() {
        let dialogue = "I know no such person in these parts.";
        let known = make_names(&["Mary Burke", "Seamus Flynn"]);
        let result =
            guard_fabricated_person_routing(dialogue, "Where is Cormac Sweeney?", &known, None, 0);
        assert_eq!(
            result, dialogue,
            "denial without routing must pass through unchanged"
        );
    }

    /// AC-3 (#1478): Real person + routing phrase — guard does not fire.
    #[test]
    fn routing_after_denial_guard_no_fire_for_real_person() {
        // Mary Burke is in the roster, so no fabricated candidate.
        let dialogue = "I know no such person... but ask at Mary's house.";
        let known = make_names(&["Mary Burke", "Seamus Flynn"]);
        let result =
            guard_fabricated_person_routing(dialogue, "Where is Mary Burke?", &known, None, 0);
        // Mary Burke is in the roster, so no fabricated candidate — guard should not fire.
        assert_eq!(
            result, dialogue,
            "real person must not trigger the routing guard"
        );
    }

    // ── #1526 — CJK / JSON scaffolding sanitizer ──────────────────────────────

    /// AC-1 (#1526): CJK suffix is stripped; clean prefix (up to last sentence) survives.
    #[test]
    fn scaffolding_sanitizer_strips_cjk_meta() {
        // U+4E2D (中) is in the CJK Unified Ideographs block (U+4E00..U+9FFF).
        let dialogue =
            "A fine morning to ye, friend. God be with ye.\u{4E2D}\u{6587}\u{5185}\u{5BB9}";
        let result = sanitize_scaffolding_leak(dialogue);
        assert!(
            !result.contains('\u{4E2D}'),
            "CJK scaffold must be stripped: {result:?}"
        );
        assert!(
            result.contains("God be with ye"),
            "clean prefix must survive: {result:?}"
        );
    }

    /// AC-2 (#1526): JSON brace leak is stripped; clean prefix survives.
    #[test]
    fn scaffolding_sanitizer_strips_json_brace() {
        let dialogue = "Good day to ye.\n{\n  \"action\": \"nods\"";
        let result = sanitize_scaffolding_leak(dialogue);
        assert!(
            !result.contains('{'),
            "JSON scaffold must be stripped: {result:?}"
        );
        assert!(
            result.contains("Good day to ye"),
            "clean prefix must survive: {result:?}"
        );
    }

    /// AC-3 (#1526): Clean input passes through unchanged.
    #[test]
    fn scaffolding_sanitizer_clean_input_unchanged() {
        let dialogue = "Well now, what brings ye to these parts?";
        let result = sanitize_scaffolding_leak(dialogue);
        assert_eq!(result, dialogue, "clean input must be returned unchanged");
    }

    /// AC-4 (#1526): No complete sentence before scaffold → return text before
    /// scaffold trimmed (not the full original, not empty).
    #[test]
    fn scaffolding_sanitizer_no_complete_sentence_before_cjk() {
        // The prefix "Good day" has no sentence-ending punctuation.
        let dialogue = "Good day\u{4E2D}\u{6587}";
        let result = sanitize_scaffolding_leak(dialogue);
        assert!(
            !result.contains('\u{4E2D}'),
            "CJK scaffold must be stripped: {result:?}"
        );
        assert_eq!(
            result.trim(),
            "Good day",
            "pre-scaffold text must survive without trailing CJK: {result:?}"
        );
    }

    // ── #1527/#1528 — false denial of roster person guard ─────────────────────

    /// AC-1 (#1527): NPC wrongly denies a known roster person → grounded ack.
    #[test]
    fn false_denial_guard_fires_for_roster_person() {
        let known = make_names(&["Peig Hannigan", "Cormac Duffy"]);
        // NPC says "not known to me" about Peig Hannigan, who IS in the roster.
        let dialogue = "That name is not known to me hereabouts. Peig Hannigan, ye say?";
        let result = guard_false_denial_of_roster_person(
            dialogue,
            "Do you know Peig Hannigan?",
            &known,
            None,
            0,
        );
        assert_ne!(
            result, dialogue,
            "guard must fire for roster person: {result:?}"
        );
        // Must return a grounded acknowledgement, not the original denial.
        assert!(
            !result.to_lowercase().contains("not known to me"),
            "denial must be replaced: {result:?}"
        );
    }

    /// AC-2 (#1527): NPC correctly declines a fabricated person not in roster → unchanged.
    #[test]
    fn false_denial_guard_leaves_correct_stranger_decline() {
        let known = make_names(&["Peig Hannigan", "Cormac Duffy"]);
        let dialogue = "I know no one by that name in these parts.";
        let result = guard_false_denial_of_roster_person(
            dialogue,
            "Where is Fionn MacCathasaigh?",
            &known,
            None,
            0,
        );
        assert_eq!(
            result, dialogue,
            "correct decline of fabricated person must not be altered: {result:?}"
        );
    }

    /// AC-3 (#1527): NPC expresses uncertainty about a roster member without
    /// a denial marker → unchanged (uncertainty is not denial).
    #[test]
    fn false_denial_guard_leaves_genuine_uncertainty() {
        let known = make_names(&["Peig Hannigan", "Cormac Duffy"]);
        // "I'm not sure where she is" — no DENIAL_MARKERS present.
        let dialogue = "I'm not sure where Peig Hannigan is today.";
        let result = guard_false_denial_of_roster_person(
            dialogue,
            "Where is Peig Hannigan?",
            &known,
            None,
            0,
        );
        assert_eq!(
            result, dialogue,
            "genuine uncertainty without denial marker must pass through: {result:?}"
        );
    }

    /// AC-4 (#1563): A generic "that name" denial after the player names one
    /// real roster person is still a false denial, even if the dialogue does
    /// not repeat the person's full name.
    #[test]
    fn false_denial_guard_fires_for_generic_known_person_denial() {
        let known = make_names(&["Peig Hannigan", "Cormac Duffy"]);
        let dialogue = "I know no one by that name hereabouts.";
        let result = guard_false_denial_of_roster_person(
            dialogue,
            "Where is Peig Hannigan?",
            &known,
            None,
            0,
        );
        assert_ne!(
            result, dialogue,
            "generic denial of one known roster person must be corrected: {result:?}"
        );
        assert!(
            !result.to_lowercase().contains("no one by that name"),
            "generic false denial must be replaced: {result:?}"
        );
    }

    /// Mixed real + invented questions remain conservative: a generic denial
    /// could refer to the fabricated name, so leave it unchanged.
    #[test]
    fn false_denial_guard_leaves_mixed_known_and_unknown_generic_denial() {
        let known = make_names(&["Peig Hannigan", "Cormac Duffy"]);
        let dialogue = "I know no one by that name hereabouts.";
        let result = guard_false_denial_of_roster_person(
            dialogue,
            "Where are Peig Hannigan and Fionn MacCathasaigh?",
            &known,
            None,
            0,
        );
        assert_eq!(
            result, dialogue,
            "mixed known+unknown generic denial must stay conservative"
        );
    }

    #[test]
    fn false_denial_guard_ignores_addressed_speaker_when_topic_differs() {
        let known = make_names(&["Roisin Connolly", "Peig Hannigan"]);
        let speaker = DialogueSpeakerContext {
            name: "Roisin Connolly".to_string(),
            occupation: "Shopkeeper".to_string(),
            mood: "alert".to_string(),
        };
        let dialogue = "That name is not known to me hereabouts.";
        let result = guard_false_denial_of_roster_person_with_speaker(
            dialogue,
            "talk to Roisin Connolly about Martin",
            &known,
            None,
            0,
            Some(&speaker),
        );
        assert_eq!(
            result, dialogue,
            "speaker addressee must not be mistaken for the denied topic"
        );
    }

    #[test]
    fn false_denial_guard_still_fires_for_known_topic_with_speaker_context() {
        let known = make_names(&["Roisin Connolly", "Peig Hannigan"]);
        let speaker = DialogueSpeakerContext {
            name: "Roisin Connolly".to_string(),
            occupation: "Shopkeeper".to_string(),
            mood: "alert".to_string(),
        };
        let dialogue = "That name is not known to me hereabouts.";
        let result = guard_false_denial_of_roster_person_with_speaker(
            dialogue,
            "talk to Roisin Connolly about Peig Hannigan",
            &known,
            None,
            0,
            Some(&speaker),
        );
        assert_ne!(
            result, dialogue,
            "known topic must still be corrected even when the speaker is addressed"
        );
        assert!(
            result.to_lowercase().contains("known") || result.to_lowercase().contains("parish"),
            "known-person acknowledgement should survive: {result:?}"
        );
    }

    #[test]
    fn stock_nonrecognition_polish_replaces_old_person_template() {
        let dialogue = "I know no one by that name in these parts.";
        let result = guard_stock_nonrecognition_decline(
            dialogue,
            "Have you met Sorcha O'Malley from beyond the parish?",
            0,
        );
        assert_ne!(result, dialogue, "old stock template must be replaced");
        assert!(
            !result
                .to_lowercase()
                .contains("i know no one by that name in these parts"),
            "replacement must not reuse the stale template: {result:?}"
        );
    }

    #[test]
    fn stock_nonrecognition_polish_uses_shopkeeper_voice_when_available() {
        let speaker = DialogueSpeakerContext {
            name: "Roisin Connolly".to_string(),
            occupation: "Shopkeeper".to_string(),
            mood: "alert".to_string(),
        };
        let dialogue = "That name is not known to me hereabouts.";
        let result = guard_stock_nonrecognition_decline_with_speaker(
            dialogue,
            "talk to Roisin Connolly about Martin",
            0,
            Some(&speaker),
        );
        let lower = result.to_lowercase();

        assert_ne!(result, dialogue, "reported stock phrase must be replaced");
        assert!(
            lower.contains("counter")
                || lower.contains("shop")
                || lower.contains("account")
                || lower.contains("goods")
                || lower.contains("trade"),
            "replacement should sound like a shopkeeper: {result:?}"
        );
    }

    #[test]
    fn stock_nonrecognition_polish_uses_place_decline_for_place_prompt() {
        let dialogue = "Mayhap ye have the wrong parish entirely.";
        let result = guard_stock_nonrecognition_decline(dialogue, "Where is Silver Bridge?", 0);
        assert_ne!(result, dialogue, "old place template must be replaced");
        assert!(
            result.to_lowercase().contains("place")
                || result.to_lowercase().contains("road")
                || result.to_lowercase().contains("point you"),
            "place prompt should receive a place-shaped decline: {result:?}"
        );
    }

    #[test]
    fn presumed_prior_acquaintance_guard_rewrites_known_target_checkin() {
        let known = make_names(&["Roisin Connolly", "Colm Gallagher"]);
        let speaker = DialogueSpeakerContext {
            name: "Roisin Connolly".to_string(),
            occupation: "Shopkeeper".to_string(),
            mood: "alert".to_string(),
        };
        let dialogue =
            "Colm Gallagher, aye, he's a bright lad at the forge. How do ye find him so far?";
        let result = guard_presumed_prior_acquaintance(
            dialogue,
            "talk to Roisin Connolly about Colm Gallagher",
            &known,
            Some(&speaker),
        );
        let lower = result.to_lowercase();

        assert_ne!(result, dialogue, "presupposing question must be rewritten");
        assert!(
            lower.contains("have ye met colm gallagher yet"),
            "replacement should ask whether the player has met the target: {result:?}"
        );
        assert!(
            !lower.contains("how do ye find him so far"),
            "presupposing phrase must not survive: {result:?}"
        );
    }

    #[test]
    fn presumed_prior_acquaintance_guard_leaves_declared_meeting() {
        let known = make_names(&["Roisin Connolly", "Colm Gallagher"]);
        let speaker = DialogueSpeakerContext {
            name: "Roisin Connolly".to_string(),
            occupation: "Shopkeeper".to_string(),
            mood: "alert".to_string(),
        };
        let dialogue = "How do ye find him so far?";

        assert_eq!(
            guard_presumed_prior_acquaintance(
                dialogue,
                "I met Colm Gallagher at the forge. What do you think of him?",
                &known,
                Some(&speaker),
            ),
            dialogue,
            "current-turn evidence that the player met the target must pass through"
        );
    }

    #[test]
    fn presumed_prior_acquaintance_guard_leaves_declared_meeting_with_honorific_mismatch() {
        let known = make_names(&["Roisin Connolly", "Colm Gallagher"]);
        let speaker = DialogueSpeakerContext {
            name: "Roisin Connolly".to_string(),
            occupation: "Shopkeeper".to_string(),
            mood: "alert".to_string(),
        };
        let dialogue = "How do ye find him so far?";

        assert_eq!(
            guard_presumed_prior_acquaintance(
                dialogue,
                "I met Fr. Colm Gallagher at the forge. What do you think of Colm Gallagher?",
                &known,
                Some(&speaker),
            ),
            dialogue,
            "extra honorific tokens must not hide current-turn meeting evidence"
        );
    }

    #[test]
    fn repeated_speaker_name_guard_removes_second_self_reference() {
        let speaker = DialogueSpeakerContext {
            name: "Peig Hannigan".to_string(),
            occupation: "Widow".to_string(),
            mood: "sharp".to_string(),
        };
        let dialogue = "Ye can call me Peig Hannigan. As for yer question, it's Peig Hannigan ye're speaking to.";
        let result = guard_repeated_speaker_name(dialogue, Some(&speaker));

        assert_ne!(
            result, dialogue,
            "redundant second self-reference must be removed"
        );
        assert_eq!(
            count_word_sequence(&result, "Peig Hannigan"),
            1,
            "speaker full name should appear exactly once: {result:?}"
        );
        assert!(
            !result
                .to_lowercase()
                .contains("it's peig hannigan ye're speaking to"),
            "redundant self-reference phrase must not survive: {result:?}"
        );
    }

    #[test]
    fn repeated_speaker_name_guard_handles_comma_in_self_reference() {
        let speaker = DialogueSpeakerContext {
            name: "Peig Hannigan".to_string(),
            occupation: "Widow".to_string(),
            mood: "sharp".to_string(),
        };
        let dialogue = "Ye can call me Peig Hannigan. As for yer question, it's Peig Hannigan, ye're speaking to.";
        let result = guard_repeated_speaker_name(dialogue, Some(&speaker));

        assert_eq!(
            count_word_sequence(&result, "Peig Hannigan"),
            1,
            "comma variant should still remove the redundant self-reference: {result:?}"
        );
        assert!(
            !result
                .to_lowercase()
                .contains("it's peig hannigan, ye're speaking to"),
            "comma variant must not surface unchanged: {result:?}"
        );
    }

    #[test]
    fn repeated_speaker_name_guard_leaves_substantive_second_mention() {
        let speaker = DialogueSpeakerContext {
            name: "Peig Hannigan".to_string(),
            occupation: "Widow".to_string(),
            mood: "sharp".to_string(),
        };
        let dialogue =
            "Ye can call me Peig Hannigan. My mother gave Peig Hannigan that name after her aunt.";

        assert_eq!(
            guard_repeated_speaker_name(dialogue, Some(&speaker)),
            dialogue,
            "non-formulaic second mentions should stay conservative"
        );
    }

    #[test]
    fn priest_tenure_guard_corrects_declan_decade_drift() {
        let dialogue = "He's been the priest here for nigh on a decade now.";
        let result = guard_priest_tenure_drift(dialogue, "Tell me about Fr. Declan Tierney");

        assert_ne!(result, dialogue, "tenure drift must be corrected");
        assert!(
            result.to_lowercase().contains("twenty-five years"),
            "replacement must carry canonical tenure: {result:?}"
        );
        assert!(
            !result.to_lowercase().contains("decade"),
            "replacement must remove the wrong decade claim: {result:?}"
        );
    }

    #[test]
    fn priest_tenure_guard_requires_declan_prompt() {
        let dialogue = "He's been the priest here for nigh on a decade now.";

        assert_eq!(
            guard_priest_tenure_drift(dialogue, "Tell me about the harvest"),
            dialogue,
            "unrelated prompts must not rewrite decade-scale claims"
        );
    }

    #[test]
    fn priest_tenure_guard_leaves_unrelated_decade_mentions_alone() {
        let dialogue = "It has been a hard decade for the parish, right enough.";

        assert_eq!(
            guard_priest_tenure_drift(dialogue, "Tell me about Father Declan"),
            dialogue,
            "non-tenure decade references must pass through unchanged"
        );
    }

    #[test]
    fn priest_tenure_guard_requires_pattern_and_context_in_same_sentence() {
        let dialogue = "The parish priest is Father Declan. I arrived ten years ago.";

        assert_eq!(
            guard_priest_tenure_drift(dialogue, "Tell me about Father Declan"),
            dialogue,
            "tenure context and decade phrase in unrelated sentences must not trigger"
        );
    }

    #[test]
    fn priest_tenure_guard_leaves_canonical_tenure_alone() {
        let dialogue = "He's served this parish for twenty-five years.";

        assert_eq!(
            guard_priest_tenure_drift(dialogue, "Tell me about Father Declan"),
            dialogue,
            "already-canonical tenure wording must pass through unchanged"
        );
    }

    #[test]
    fn rival_tone_guard_cools_neutral_warm_target_line() {
        let dialogue = "Mick Flanagan, aye. He's retired now but still keeps an eye on things.";
        let rels = vec![RelationshipToneHint {
            target_name: "Mick Flanagan".to_string(),
            kind: RelationshipKind::Rival,
            strength: -0.2,
        }];
        let result =
            guard_rival_target_neutral_tone(dialogue, "What do you think of Mick Flanagan?", &rels);

        assert_ne!(result, dialogue, "neutral rival line must be cooled");
        assert!(
            result.contains("Mick Flanagan"),
            "replacement should preserve the target name: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("little warmth")
                || result.to_lowercase().contains("keep my distance"),
            "replacement should carry a visible cool/rival cue: {result:?}"
        );
    }

    #[test]
    fn rival_tone_guard_cools_female_pronoun_warm_line() {
        let dialogue = "Roisin Connolly, aye. She's a good woman, no harm in her.";
        let rels = vec![RelationshipToneHint {
            target_name: "Roisin Connolly".to_string(),
            kind: RelationshipKind::Rival,
            strength: -0.2,
        }];
        let result = guard_rival_target_neutral_tone(
            dialogue,
            "What do you think of Roisin Connolly?",
            &rels,
        );

        assert_ne!(result, dialogue, "female warm rival line must be cooled");
        assert!(
            result.contains("Roisin Connolly"),
            "replacement should preserve the target name: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("little warmth")
                || result.to_lowercase().contains("keep my distance"),
            "replacement should carry a visible cool/rival cue: {result:?}"
        );
    }

    #[test]
    fn rival_tone_guard_leaves_friendly_target_line_alone() {
        let dialogue = "Mick Flanagan, aye. He's retired now but still keeps an eye on things.";
        let rels = vec![RelationshipToneHint {
            target_name: "Mick Flanagan".to_string(),
            kind: RelationshipKind::Friend,
            strength: 0.5,
        }];

        assert_eq!(
            guard_rival_target_neutral_tone(dialogue, "Tell me about Mick Flanagan", &rels),
            dialogue,
            "friendly relationships must not be soured by the rival-tone guard",
        );
    }

    #[test]
    fn rival_tone_guard_leaves_existing_cool_line_alone() {
        let dialogue =
            "Mick Flanagan, aye. There is little warmth between us, so I keep my distance.";
        let rels = vec![RelationshipToneHint {
            target_name: "Mick Flanagan".to_string(),
            kind: RelationshipKind::Rival,
            strength: -0.2,
        }];

        assert_eq!(
            guard_rival_target_neutral_tone(dialogue, "Tell me about Mick Flanagan", &rels),
            dialogue,
            "already cool wording should pass through unchanged",
        );
    }

    #[test]
    fn rival_tone_guard_leaves_third_person_distance_line_alone() {
        let dialogue = "Mick Flanagan, aye. He's a good man, but he keeps his distance.";
        let rels = vec![RelationshipToneHint {
            target_name: "Mick Flanagan".to_string(),
            kind: RelationshipKind::Rival,
            strength: -0.2,
        }];

        assert_eq!(
            guard_rival_target_neutral_tone(dialogue, "Tell me about Mick Flanagan", &rels),
            dialogue,
            "existing third-person distance cue should pass through unchanged",
        );
    }

    #[test]
    fn rival_tone_guard_requires_player_to_ask_about_target() {
        let dialogue = "Mick Flanagan, aye. He's retired now but still keeps an eye on things.";
        let rels = vec![RelationshipToneHint {
            target_name: "Mick Flanagan".to_string(),
            kind: RelationshipKind::Rival,
            strength: -0.2,
        }];

        assert_eq!(
            guard_rival_target_neutral_tone(dialogue, "Tell me about Padraig Darcy", &rels),
            dialogue,
            "unrelated prompts must not rewrite third-person mentions",
        );
    }

    #[test]
    fn time_of_day_phrase_guard_corrects_midday_morning_tic() {
        let dialogue = "Good morning to ye. What brings ye in this fine morning?";
        let result = guard_time_of_day_phrase(dialogue, TimeOfDay::Midday);
        assert_eq!(result, "Good day to ye. What brings ye in this fine day?");
        assert!(
            !result.to_lowercase().contains("morn"),
            "midday greeting must not retain morning wording: {result:?}"
        );
    }

    #[test]
    fn time_of_day_phrase_guard_leaves_actual_morning_alone() {
        let dialogue = "Good morning to ye. What brings ye in this fine morning?";
        let result = guard_time_of_day_phrase(dialogue, TimeOfDay::Morning);
        assert_eq!(result, dialogue);
    }

    // ── #1530 — invented place confirmation guard ──────────────────────────────

    fn make_locations(locs: &[&str]) -> Vec<String> {
        locs.iter().map(|s| s.to_string()).collect()
    }

    /// AC-1 (#1530): NPC confirms an invented place → replaced with place-decline.
    #[test]
    fn invented_place_guard_fires_for_unknown_place() {
        let known = make_locations(&["Kilteevan", "Roscommon", "Strokestown"]);
        // NPC says "'Tis in Dublin, far across the land" about "cathedral of Saint Aldric"
        // (which is not in the known location list).
        let dialogue = "'Tis in Dublin, far across the land.";
        let result = guard_invented_place_confirmation(
            dialogue,
            "Where is the cathedral of Saint Aldric?",
            &known,
            0,
        );
        assert_ne!(
            result, dialogue,
            "guard must fire for invented place: {result:?}"
        );
        assert!(
            !result.to_lowercase().contains("dublin"),
            "invented-place affirmation must be replaced: {result:?}"
        );
    }

    /// AC-1 (#1507): NPC soft-validates an invented place → replaced with
    /// place-decline instead of suspiciously asking why the player asked.
    #[test]
    fn invented_place_guard_fires_for_soft_deflection() {
        let known = make_locations(&["Kilteevan", "Roscommon", "Strokestown"]);
        let dialogue = "And Ballygostick Tower, now? Have ye a reason for asking?";
        let result = guard_invented_place_confirmation(
            dialogue,
            "Is there a place called Ballygostick Tower?",
            &known,
            0,
        );

        assert_ne!(
            result, dialogue,
            "guard must fire for invented-place soft deflection: {result:?}"
        );
        assert!(
            !result.to_lowercase().contains("ballygostick"),
            "invented place name must not be repeated: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("place"),
            "replacement should be a place non-recognition decline: {result:?}"
        );
    }

    /// AC-1 (#1507): another post-generation guard may already strip the
    /// invented place name, leaving only the soft-deflecting question. The
    /// place guard still has the player's original place query and should
    /// replace the leftover deflection.
    #[test]
    fn invented_place_guard_fires_for_leftover_reason_question() {
        let known = make_locations(&["Kilteevan", "Roscommon", "Strokestown"]);
        let dialogue = "Have ye a reason for asking?";
        let result = guard_invented_place_confirmation(
            dialogue,
            "Is there a place called Ballygostick Tower?",
            &known,
            1,
        );

        assert_ne!(
            result, dialogue,
            "leftover soft deflection must become a clear place decline: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("place"),
            "replacement should mention place non-recognition: {result:?}"
        );
    }

    /// AC-2 (#1530): NPC confirms a real location from the known list → unchanged.
    #[test]
    fn invented_place_guard_leaves_known_location() {
        let known = make_locations(&["Kilteevan", "Roscommon", "Strokestown"]);
        let dialogue = "'Tis in Kilteevan, just down the road.";
        let result = guard_invented_place_confirmation(dialogue, "Where is Kilteevan?", &known, 0);
        assert_eq!(
            result, dialogue,
            "known location must pass through unchanged: {result:?}"
        );
    }

    /// AC-2 (#1507): soft questions about a real location are not rewritten as
    /// unknown-place denials.
    #[test]
    fn invented_place_guard_leaves_known_location_soft_question() {
        let known = make_locations(&["Kilteevan", "Roscommon", "Strokestown"]);
        let dialogue = "Kilteevan, now? Have ye a reason for asking?";
        let result = guard_invented_place_confirmation(
            dialogue,
            "Is there a place called Kilteevan?",
            &known,
            0,
        );
        assert_eq!(
            result, dialogue,
            "known location soft question must pass through unchanged: {result:?}"
        );
    }

    /// AC-3 (#1530): NPC mentions place name but no affirmation marker → unchanged.
    #[test]
    fn invented_place_guard_no_fire_without_affirmation() {
        let known = make_locations(&["Kilteevan", "Roscommon"]);
        // Denial is present — guard should not fire.
        let dialogue = "I know of no such place called Ballyweird.";
        let result = guard_invented_place_confirmation(dialogue, "Where is Ballyweird?", &known, 0);
        assert_eq!(
            result, dialogue,
            "denial without affirmation must pass through: {result:?}"
        );
    }

    /// AC-1 (#1563): A real place from the world graph must not be denied as
    /// nonexistent just because the speaking NPC's local knowledge omitted it.
    #[test]
    fn false_denial_place_guard_corrects_known_place_denial() {
        let known = make_locations(&["Darcy's Pub", "The Forge", "Kilteevan Village"]);
        let dialogue = "I cannae guide ye to a place that doesn't exist.";
        let result =
            guard_false_denial_of_known_place(dialogue, "Where is Darcy's Pub?", &known, 0);

        assert_ne!(
            result, dialogue,
            "generic denial of known place must be corrected: {result:?}"
        );
        assert!(
            !result.to_lowercase().contains("doesn't exist"),
            "known-place denial must be replaced: {result:?}"
        );
    }

    /// AC-1 (#1563): The issue repro used navigation wording ("guide me to"),
    /// so explicit navigation requests are place candidates too.
    #[test]
    fn false_denial_place_guard_extracts_guide_me_to_known_place() {
        let known = make_locations(&["Darcy's Pub", "The Forge", "Kilteevan Village"]);
        let dialogue = "I cannae guide ye to a place that doesn't exist.";
        let result = guard_false_denial_of_known_place(
            dialogue,
            "Can you guide me to Darcy's Pub?",
            &known,
            1,
        );

        assert_ne!(
            result, dialogue,
            "guide-me-to known-place denial must be corrected: {result:?}"
        );
        assert!(
            result.to_lowercase().contains("place"),
            "replacement should be a grounded place acknowledgement: {result:?}"
        );
    }

    /// AC-3 (#1563): A correct denial of an invented place remains a denial.
    #[test]
    fn false_denial_place_guard_leaves_unknown_place_denial() {
        let known = make_locations(&["Darcy's Pub", "The Forge", "Kilteevan Village"]);
        let dialogue = "I know of no such place in this parish.";
        let result =
            guard_false_denial_of_known_place(dialogue, "Where is Ballydrift Abbey?", &known, 0);

        assert_eq!(
            result, dialogue,
            "correct denial of invented place must pass through unchanged"
        );
    }

    // ── UTF-8 safety: extract_candidate_places must not panic on multibyte input ──

    /// Regression: `player_input` with a char whose lowercased form has DIFFERENT byte
    /// length must not cause a byte-index panic in `extract_candidate_places` (#1539).
    ///
    /// `İ` (U+0130, 2 UTF-8 bytes) lowercases to "i\u{307}" (3 UTF-8 bytes, 2 codepoints).
    /// When `İ` precedes the where-signal, the byte offset of `signal_pos + signal.len()`
    /// in `lower_input` is 1 byte PAST the corresponding position in `player_input`.
    /// If there is a multibyte char immediately after the signal in `player_input`
    /// (e.g. `Ä`, 2 bytes), the old code would compute an offset that lands inside `Ä`
    /// and panic.  This test exercises that exact input shape.
    #[test]
    fn extract_candidate_places_multibyte_no_panic() {
        // İ (2 bytes) before signal + Ä (2 bytes) immediately after signal:
        // the old byte-offset approach would land mid-Ä and panic.
        let player_input = "İwhere is Änother?";
        // Must not panic, and should extract "Änother" or similar as a candidate.
        let candidates = extract_candidate_places(player_input);
        // We do not assert a specific extraction — the content after Ä is marginal;
        // the key invariant is no panic (the test would abort if it panicked).
        let _ = candidates; // suppress unused-variable warning

        // Also exercise the common Irish-name variant (ASCII, confirms no regression).
        let player_input_ascii = "where is Ballydrift?";
        let candidates_ascii = extract_candidate_places(player_input_ascii);
        assert!(
            candidates_ascii
                .iter()
                .any(|c| c.to_lowercase().contains("ballydrift")),
            "expected 'Ballydrift' in candidates, got: {candidates_ascii:?}"
        );
    }

    /// `guard_invented_place_confirmation` must not panic when `player_input`
    /// has a multibyte char whose lowercased form has a different byte length,
    /// and must fire when the extracted candidate is not in the known-location list.
    #[test]
    fn invented_place_guard_multibyte_input_no_panic() {
        let known = make_locations(&["Kilteevan", "Roscommon"]);
        // Affirmation marker present; any extracted candidate would be unknown.
        let dialogue = "'Tis just down the road, aye.";
        // İ (expansion case) before signal, Ä after signal — the exact panic shape.
        let player_input = "İwhere is Ballymagic?";
        // Must not panic.
        let result = guard_invented_place_confirmation(dialogue, player_input, &known, 0);
        // Whether or not the guard fires (depends on extraction), the result must be a
        // valid string with no panic.
        assert!(!result.is_empty(), "result must be non-empty: {result:?}");
    }

    // ── #1569 — known place names are not fabricated people ─────────────────

    /// AC-1 (#1569): "Lough Ree" has the same Title-case bigram shape as a
    /// person name, but it is a known place via "Lough Ree Shore". The
    /// fabricated-person guard must not replace a valid lake-history answer
    /// with a "no such person" decline.
    #[test]
    fn person_guard_allows_known_place_history_question() {
        let known_people = vec!["Aoife Brennan".to_string()];
        let known_locations = make_locations(&["Lough Ree Shore", "Kilteevan Village"]);
        let player_input =
            "Aoife, I never saw a lake this grand. What is the history of Lough Ree?";
        let dialogue = "Ah, the history of Lough Ree is a tale as grand as the lake itself. \
                        Folk say it was formed by the great flood.";

        let result = guard_fabricated_person_confirmation_with_locations(
            dialogue,
            player_input,
            &known_people,
            &known_locations,
            &[],
            None,
            0,
        );

        assert_eq!(
            result, dialogue,
            "known place reference must not be treated as a fabricated person: {result:?}"
        );
    }

    /// Review hardening (#1569): the known-place exemption must also match a
    /// candidate phrase in the middle of a longer location name.
    #[test]
    fn person_guard_allows_known_place_history_question_for_middle_location_span() {
        let known_people = vec!["Aoife Brennan".to_string()];
        let known_locations = make_locations(&["The Old Lough Ree Shore"]);
        let player_input = "What is the history of Lough Ree?";
        let dialogue = "The history of Lough Ree is older than the road itself.";

        let result = guard_fabricated_person_confirmation_with_locations(
            dialogue,
            player_input,
            &known_people,
            &known_locations,
            &[],
            None,
            0,
        );

        assert_eq!(
            result, dialogue,
            "known place span in the middle of a location must not be treated as a person"
        );
    }

    /// Review hardening (#1569): token-window location matching must still
    /// require the full candidate phrase, not only a surname-like suffix.
    #[test]
    fn person_guard_does_not_treat_unrelated_surname_as_known_place() {
        let known_people = vec!["Aoife Brennan".to_string()];
        let known_locations = make_locations(&["Saint Mary's Church"]);
        let player_input = "Do you know Mary Church?";
        let dialogue = "Aye, Mary Church is at the forge this morning.";

        let result = guard_fabricated_person_confirmation_with_locations(
            dialogue,
            player_input,
            &known_people,
            &known_locations,
            &[],
            None,
            0,
        );

        assert_ne!(
            result, dialogue,
            "unrelated surname-like phrase must still be guarded"
        );
    }

    /// AC-3 (#1569): adding known-place context must not disable the fabricated
    /// person guard for real fabricated people.
    #[test]
    fn person_guard_still_declines_fabricated_person_with_location_context() {
        let known_people = vec!["Aoife Brennan".to_string()];
        let known_locations = make_locations(&["Lough Ree Shore", "Kilteevan Village"]);
        let player_input = "Do you know Cormac Sweeney?";
        let dialogue = "Aye, Cormac Sweeney is at the mill this morning.";

        let result = guard_fabricated_person_confirmation_with_locations(
            dialogue,
            player_input,
            &known_people,
            &known_locations,
            &[],
            None,
            0,
        );

        assert_ne!(result, dialogue, "fabricated person must still be guarded");
        assert!(
            result.to_lowercase().contains("no such person")
                || result.to_lowercase().contains("no one by that name")
                || result.to_lowercase().contains("wrong parish")
                || result.to_lowercase().contains("not a name")
                || result.to_lowercase().contains("that name")
                || result.to_lowercase().contains("parish face")
                || result.to_lowercase().contains("comes to mind"),
            "expected a person non-recognition decline, got: {result:?}"
        );
    }

    // ── #1553 — player name exempt from fabricated-person guard ──────────────

    /// When `player_name` is supplied the guard must exempt the player's own
    /// name from the fabricated-person check (#1553).
    #[test]
    fn player_self_introduction_not_denied_when_player_name_passed() {
        // Warm welcome that contains the player's own name plus affirmation
        // markers. Must NOT be replaced when player_name is passed.
        let dialogue = "'Tis a fine mornin', Aiden Carney! Welcome to Kilteevan, indeed! \
                        A cooper is just what we need. Sit ye down.";
        let player_input = "I am Aiden Carney, a cooper newly come to Kilteevan.";
        // Player is NOT in the NPC roster.
        let known: Vec<String> = vec!["Peig Hannigan".to_string()];

        // Without player_name: the guard might treat "Aiden Carney" as fabricated.
        // With player_name: the guard exempts it entirely.
        let result = guard_fabricated_person_confirmation(
            dialogue,
            player_input,
            &known,
            &[],
            Some("Aiden Carney"),
            42,
        );
        assert_eq!(
            result, dialogue,
            "player self-introduction must not trigger the fabricated-person guard when player_name is passed:\n{result}"
        );
    }

    /// The NPC's own name IS in `known_person_names` (all parish NPCs are
    /// included). The guard must never strip it (#1553).
    #[test]
    fn npc_own_name_in_roster_never_denied() {
        let dialogue = "Good morning. Peig Hannigan, me name. And ye be new to these parts, aye?";
        let player_input = "Are you Peig Hannigan?";
        let known: Vec<String> = vec!["Peig Hannigan".to_string()];

        let result =
            guard_fabricated_person_confirmation(dialogue, player_input, &known, &[], None, 42);
        assert_eq!(
            result, dialogue,
            "NPC's own name in roster must never be stripped:\n{result}"
        );
    }

    /// A grounded answer that mentions the player's name as a recipient (not
    /// an affirmation of a fabricated third party) must pass through unchanged.
    #[test]
    fn grounded_work_answer_not_stripped() {
        let dialogue = "The forge ye'll find near the bridge, and the mill needs sturdy casks. \
                        There's work enough for a skilled cooper in Kilteevan.";
        let player_input = "I am Aiden Carney, a cooper. Might there be work hereabouts?";
        let known: Vec<String> = vec!["Peig Hannigan".to_string(), "Colm Murphy".to_string()];

        let result = guard_fabricated_person_confirmation(
            dialogue,
            player_input,
            &known,
            &[],
            Some("Aiden Carney"),
            42,
        );
        assert_eq!(
            result, dialogue,
            "grounded work answer must not be stripped by person-confirmation guard:\n{result}"
        );
    }

    // ── #1554 — nearby phrase repeat collapse ─────────────────────────────────

    /// The verbosity pipeline output "Aye, tell me indeed, what say ye now,
    /// indeed, what be it?" has "indeed, what" repeated within a 20-word
    /// window. The guard must trim it (#1554).
    #[test]
    fn collapse_nearby_phrase_repeat_cleans_stacked_tail() {
        let input = "Aye, tell me indeed, what say ye now, indeed, what be it?";
        let result = collapse_nearby_phrase_repeat(input);
        // The second "indeed, what be it" occurrence must be removed.
        // The result must not contain "indeed" twice within a short span.
        let result_lower = result.to_lowercase();
        let first = result_lower.find("indeed").unwrap_or(usize::MAX);
        let last = result_lower.rfind("indeed").unwrap_or(usize::MAX);
        assert!(
            first == last || (last - first) > 15,
            "nearby phrase repeat must be collapsed: first={first}, last={last}, result={result:?}"
        );
    }

    /// Clean dialogue with no nearby repetition must pass through unchanged.
    #[test]
    fn collapse_nearby_phrase_repeat_noop_on_clean_dialogue() {
        let input =
            "Good morning to ye. The forge is near the bridge, and the mill is by the river.";
        let result = collapse_nearby_phrase_repeat(input);
        assert_eq!(result, input, "clean dialogue must not be modified");
    }

    /// A phrase repeated once with a very large gap (> 20 words) must not
    /// trigger the guard — that is natural prose, not a degenerate loop.
    #[test]
    fn collapse_nearby_phrase_repeat_ignores_distant_repetition() {
        // "well enough" appears twice but more than 20 words apart.
        let input = "I know this land well enough, having walked every road from \
                     Strokestown to Roscommon and back again, aye, and I know it \
                     well enough to say there's no finer parish.";
        let result = collapse_nearby_phrase_repeat(input);
        assert_eq!(
            result, input,
            "distant repetition (> 20 words) must not trigger the guard"
        );
    }

    /// #1561: The phrase "work for a" repeats in adjacent substantive
    /// sentences, but the only clean boundary before the first occurrence is the
    /// greeting. The guard must not trim back to that opener and drop the actual
    /// cooper-work answer.
    #[test]
    fn collapse_nearby_phrase_repeat_preserves_cooper_work_answer() {
        let input = "Good morning, Aiden Carney. Work for a cooper? Aye, there's \
                     always work for a man with that skill. This place needs \
                     barrels for ale and salt, surely. Ye know yer trade?";
        let result = collapse_nearby_phrase_repeat(input);
        assert_eq!(
            result, input,
            "nearby phrase guard must not truncate a normal answer to its greeting"
        );
    }

    /// #1561: The full verbosity pipeline must preserve the substantive answer
    /// instead of storing only the opener. The shared cap may drop the final
    /// follow-up question.
    #[test]
    fn verbosity_guard_preserves_cooper_work_answer() {
        let input = "Good morning, Aiden Carney. Work for a cooper? Aye, there's \
                     always work for a man with that skill. This place needs \
                     barrels for ale and salt, surely. Ye know yer trade?";
        let result = guard_verbosity_runons(input);
        assert!(
            result.contains("Work for a cooper?")
                && result.contains("there's always work")
                && result.contains("barrels for ale and salt"),
            "verbosity guard must preserve the substantive answer: {result:?}"
        );
        assert!(
            !result.contains("Ye know yer trade?"),
            "the fourth substantive sentence should be capped in semantic order: {result:?}"
        );
        assert_ne!(
            result, "Good morning, Aiden Carney.",
            "verbosity guard must not collapse to only the greeting"
        );
    }

    #[test]
    fn delivered_full_name_marks_a_real_self_introduction() {
        assert!(dialogue_self_identifies_speaker(
            "Peig Hannigan's the name. What brings ye here?",
            "Peig Hannigan",
        ));
        assert!(dialogue_self_identifies_speaker(
            "I'm Peig Hannigan. What brings ye here?",
            "Peig Hannigan",
        ));
        assert!(!dialogue_self_identifies_speaker(
            "Good morning. What brings ye here?",
            "Peig Hannigan",
        ));
        assert!(!dialogue_self_identifies_speaker(
            "Peig Hannigan lives beyond the crossroads.",
            "Peig Hannigan",
        ));
        assert!(!dialogue_self_identifies_speaker(
            "I'm Peig Hannigan's cousin from beyond the crossroads.",
            "Peig Hannigan",
        ));
        assert!(dialogue_self_identifies_speaker(
            "I'm Father Declan Tierney, the parish priest.",
            "Fr. Declan Tierney",
        ));
    }

    #[test]
    fn sharp_mood_drops_only_the_contradictory_warm_opener() {
        let result = guard_mood_register("Good morning. What brings ye here?", "sharp");
        assert_eq!(result, "Plainly, then—What brings ye here?");
        assert_eq!(
            guard_mood_register("Good morning. What brings ye here?", "friendly"),
            "Good morning. What brings ye here?"
        );
    }

    #[test]
    fn bitter_mood_is_audible_in_the_exact_harness_reply() {
        let dialogue = "Aye, I'm Sean Ruadh. Stick to Siobhan's lead, she knows the patch \
                        better'n I do. What's your trade?";
        let result = guard_mood_register(dialogue, "bitter");

        assert_eq!(
            result,
            "If ye must know—Aye, I'm Sean Ruadh. Stick to Siobhan's lead, she knows the \
             patch better'n I do. What's your trade?"
        );
    }

    #[test]
    fn already_negative_register_is_not_rewritten() {
        let dialogue = "I've no patience for idle talk. What is it?";
        assert_eq!(guard_mood_register(dialogue, "irritated"), dialogue);
    }

    #[test]
    fn first_contact_guard_removes_harness_familiarity_claim() {
        let dialogue = "Ah, good day to you, Aiden Carney. I've seen ye around, \
                        though we haven't spoken much. Listen to the land and the folk here.";
        let result = guard_unfounded_first_contact_familiarity(dialogue, false);
        assert!(!result.contains("seen ye around"));
        assert!(!result.contains("haven't spoken"));
        assert!(result.contains("Listen to the land"));
        assert_eq!(
            guard_unfounded_first_contact_familiarity(dialogue, true),
            dialogue
        );
    }

    #[test]
    fn evidence_evasion_is_replaced_with_honest_answer() {
        let player = "What do you mean by a place of power—have you seen something \
                      here yourself, or is that only an old tale?";
        let dialogue = "Aye, it's a place of power, all right. Ye might say I've \
                        seen it with me own eyes, but it's more the whispers ye hear \
                        here. What tales have ye heard of this spot?";
        let result = guard_direct_evidence_evasion(dialogue, player);
        assert_ne!(result, dialogue);
        assert!(result.contains("cannot say I saw it myself"));
        assert!(!result.contains('?'));
    }

    #[test]
    fn evidence_evasion_fallback_does_not_invent_folklore_context() {
        let player = "Did you see who took my cow yourself, or did someone only tell you?";
        let dialogue = "There are many whispers on the road. What have you heard?";
        let result = guard_direct_evidence_evasion(dialogue, player);

        assert_eq!(result, "I cannot say I saw it myself.");
        assert!(!result.contains("tale"));
        assert!(!result.contains("sighting"));
        assert!(!result.contains("another person"));
    }

    #[test]
    fn concrete_firsthand_answer_is_not_rewritten() {
        let player = "Have you seen something here yourself, or is that only an old tale?";
        let dialogue = "I saw a white shape cross the eastern road at midnight with my own eyes.";
        assert_eq!(guard_direct_evidence_evasion(dialogue, player), dialogue);
    }

    #[test]
    fn farm_hands_referral_rejects_retired_constable_and_prefers_farmer() {
        let roster = vec![
            (
                "Mick Flanagan".to_string(),
                "Retired Constable".to_string(),
                None,
            ),
            (
                "Siobhan Murphy".to_string(),
                "Farmer".to_string(),
                Some("Murphy's Farm".to_string()),
            ),
            ("Sean Ruadh Kelly".to_string(), "Labourer".to_string(), None),
        ];
        let dialogue = "Might be worth asking Mick Flanagan if ye could help with \
                        farm duties. He's a man who needs a steady hand, I wager.";
        let player = "I'm hoping to find honest work. Who nearby might have use for \
                      another pair of hands?";
        let result = guard_work_recommendation(dialogue, player, &roster);
        assert!(!result.contains("Mick Flanagan"));
        assert!(result.contains("Siobhan Murphy"));
        assert!(result.contains("Farmer"));
        assert!(result.contains("Murphy's Farm"));
    }

    #[test]
    fn occupation_matched_work_referral_passes_through() {
        let roster = vec![(
            "Siobhan Murphy".to_string(),
            "Farmer".to_string(),
            Some("Murphy's Farm".to_string()),
        )];
        let dialogue = "Ask Siobhan Murphy at the farm; she's the farmer there.";
        assert_eq!(
            guard_work_recommendation(
                dialogue,
                "Who might have farm work for another pair of hands?",
                &roster,
            ),
            dialogue
        );
    }

    #[test]
    fn your_man_work_referral_is_checked_against_authored_occupation() {
        let roster = vec![
            (
                "Mick Flanagan".to_string(),
                "Retired Constable".to_string(),
                None,
            ),
            (
                "Siobhan Murphy".to_string(),
                "Farmer".to_string(),
                Some("Murphy's Farm".to_string()),
            ),
        ];
        let dialogue = "Mick Flanagan is your man for farm work.";
        let result = guard_work_recommendation(
            dialogue,
            "Who might have farm work for another pair of hands?",
            &roster,
        );

        assert!(!result.contains("Mick Flanagan"));
        assert!(result.contains("Siobhan Murphy"));
        assert!(result.contains("Farmer"));
    }

    #[test]
    fn work_domain_matching_does_not_use_substrings_inside_unrelated_words() {
        let input = "I heard a tale about a selfish man in the workshop; is there work elsewhere?";
        assert!(
            requested_work_domain(input).is_none(),
            "tale/selfish/workshop must not be parsed as ale/fish/shop"
        );
    }
}
