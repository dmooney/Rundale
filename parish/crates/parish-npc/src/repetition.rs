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

/// Splits a dialogue body into sentence-ish units on terminal punctuation,
/// keeping the delimiter with each piece. Used by [`collapse_repeated_sentences`].
fn split_sentences(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in s.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '…') {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
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
    let normalized = match sentences.first() {
        Some(first) => normalize_for_repetition(first),
        None => normalize_for_repetition(dialogue),
    };
    let remainder = if sentences.len() < 2 {
        ""
    } else {
        // The first sentence spans [0, sentences[0].len()) bytes in `dialogue`;
        // everything after it (leading whitespace trimmed) is the remainder.
        dialogue[sentences[0].len()..].trim_start()
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
///    ([`is_near_identical`] at `threshold`), substitute a deterministic varied
///    fallback ([`varied_repetition_fallback`] keyed by `seed`).
///
/// `previous_line` is `None` when the NPC has no prior line at this location.
/// A `threshold` of `0.0` disables the cross-turn check (intra-line collapse
/// still runs, since runaway loops are always undesirable).
pub fn guard_against_repetition(
    new_line: &str,
    previous_line: Option<&str>,
    threshold: f32,
    seed: u64,
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
        "I know no one by that name in these parts.",
        "That name is not known to me hereabouts.",
        "I cannot say I've ever heard of such a person here.",
        "No one by that name that I know of in this parish.",
        "I know of no such person — you may have the wrong parish entirely.",
    ];
    DECLINES[(seed as usize) % DECLINES.len()]
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

/// Checks whether a candidate name matches any entry in the known roster
/// (case-insensitive).
///
/// Matching rules:
/// - **Full-name candidate** (2+ tokens, e.g. "Cormac Sweeney"): requires an
///   exact full-name match against a roster entry ("Cormac Sweeney" must appear
///   verbatim in the roster). A shared first name with a *different* surname
///   (e.g. roster has "Cormac Duffy") does NOT constitute a match — the
///   candidate is treated as fabricated and the guard fires.
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

    for roster_name in known_person_names {
        let roster_lower = roster_name.to_lowercase();

        // Exact full-name match always passes through.
        if roster_lower == lower {
            return true;
        }

        // First-name-only candidate: allow a first-name match against any roster entry.
        // "Cormac" (single token) vs roster "Cormac Duffy" → match (casual reference).
        if !candidate_is_full_name {
            let candidate_first = tokens.first().copied().unwrap_or("");
            if roster_lower.split_whitespace().next().unwrap_or("") == candidate_first {
                return true;
            }
        }
        // Full-name candidate: only exact match clears it (handled above).
        // "Cormac Sweeney" vs roster "Cormac Duffy" → no match → guard fires.
    }
    false
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
        if name_in_roster(candidate, known_person_names, player_name) {
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
        c.split_whitespace().count() >= 2 && !name_in_roster(c, known_person_names, player_name)
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

/// Trims a multi-sentence NPC reply to at most `MAX_SENTENCE_COUNT` sentences
/// (#1472 — hard length cap).
///
/// This is the blunt backstop for run-on replies that the surgical dedup guards
/// cannot catch: a model may produce 6-8 distinct non-duplicate sentences that
/// are each individually legitimate but collectively too long. No phrasing
/// heuristic catches this; only a hard count cap does.
///
/// Behavior:
/// - Splits on terminal punctuation (`.`, `!`, `?`, `…`) using [`split_sentences`].
/// - Keeps the first `MAX_SENTENCE_COUNT` sentence units.
/// - If the ORIGINAL final sentence is a question (ends with `?`) and would be
///   cut by the cap, it is preserved as the last sentence of the result — keeping
///   first (N-1) sentences plus the trailing question — so the NPC remains
///   interactive. If the kept-N sentences already end with a `?`, no swap is made.
/// - Short replies (<= `MAX_SENTENCE_COUNT` sentences) are returned unchanged.
///
/// The cap is intentionally conservative (4 sentences) — a natural NPC
/// turn should deliver one thought in 2-3 sentences and optionally ask one
/// question; 4 gives generous headroom without admitting multi-beat rambles.
pub fn cap_sentence_count(dialogue: &str) -> String {
    /// Maximum number of sentences kept from an NPC reply. Replies longer than
    /// this are trimmed at the nearest sentence boundary after the Nth sentence.
    /// Default 4 — enough for a natural period-appropriate response (greeting +
    /// substance + optional qualifier + one question).
    const MAX_SENTENCE_COUNT: usize = 4;

    let sentences = split_sentences(dialogue);
    // Filter out empty fragments produced by split_sentences.
    let sentences: Vec<&str> = sentences
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if sentences.len() <= MAX_SENTENCE_COUNT {
        return dialogue.trim().to_string();
    }

    // Keep the first N sentences.
    let mut kept: Vec<&str> = sentences[..MAX_SENTENCE_COUNT].to_vec();

    // If the original reply ends with a question that would be cut, preserve it.
    // Only do this when the last sentence is genuinely interrogative (ends with '?')
    // and the already-kept slice does not already end with a question.
    let original_last = *sentences.last().expect("non-empty checked above");
    let kept_last = *kept.last().expect("MAX_SENTENCE_COUNT >= 1");
    if original_last.ends_with('?') && !kept_last.ends_with('?') {
        // Swap out the Nth sentence for the trailing question so the reply
        // stays at exactly MAX_SENTENCE_COUNT sentences and ends interactively.
        *kept.last_mut().expect("non-empty") = original_last;
    }

    kept.join(" ")
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
// may be 200+ words across 4 or fewer sentences (so `cap_sentence_count` does
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
/// Conservative: `MAX_WORDS = 80`. A natural 3–4 sentence NPC reply in
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

/// Applies the full verbosity / run-on guard to a finalized dialogue string (#1460, #1472, #1487, #1489).
///
/// Steps, in order:
/// 1. Strip bare leaked mood-adjective from tail ([`strip_leaked_mood_word`]).
/// 2. Trim mid-sentence truncation ellipsis ([`trim_mid_sentence_truncation`]).
/// 3. Strip trailing ellipsis artifact after otherwise-complete text
///    ([`strip_trailing_ellipsis_artifact`], #1472).
/// 4. **Collapse degenerate intra-response phrase-repetition loop** ([`collapse_degenerate_phrase_loop`], #1487).
///    Detects when a short phrase (3–8 words) repeats ≥ 4× and truncates at the
///    first clean sentence boundary before the loop.
/// 5. Collapse non-consecutive near-duplicate sentences ([`collapse_distributed_repeated_sentences`]).
/// 6. Cap total questions in the whole reply to at most 2 ([`cap_total_questions`]).
/// 7. Cap trailing question stack to one ([`cap_trailing_questions`]).
/// 8. Hard sentence-count cap: trim to at most 4 sentences ([`cap_sentence_count`], #1472).
///    This is the blunt backstop for paraphrased multi-beat rambles that the surgical
///    guards (steps 5-7) cannot catch.
/// 9. **Word-count cap**: trim to at most 80 words at a clean sentence boundary
///    ([`cap_word_count`], #1489). Final backstop for long-but-few-sentence runs.
///
/// Steps 5 and 6 are the #1460 core fix for distributed / interleaved repetition
/// that `collapse_repeated_sentences` (which only handles consecutive duplicates,
/// called upstream by `guard_against_repetition`) does not catch. Step 7 keeps
/// the earlier trailing-question guard in place as a final cleanup. Step 8 runs
/// before step 9 so it operates on already-cleaned text, giving the surgical
/// guards their best shot before the blunt caps apply.
///
/// Conservative — does not alter legitimate prose within 4 sentences and 80 words.
pub fn guard_verbosity_runons(dialogue: &str) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }
    let after_mood = strip_leaked_mood_word(dialogue);
    let after_trunc = trim_mid_sentence_truncation(&after_mood);
    let after_ellipsis = strip_trailing_ellipsis_artifact(&after_trunc);
    let after_phrase_loop = collapse_degenerate_phrase_loop(&after_ellipsis);
    let after_distributed = collapse_distributed_repeated_sentences(&after_phrase_loop);
    let after_total_q = cap_total_questions(&after_distributed);
    let after_trailing_q = cap_trailing_questions(&after_total_q);
    let after_sentence_cap = cap_sentence_count(&after_trailing_q);
    cap_word_count(&after_sentence_cap)
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

    // Must contain the name at all.
    if !lower.contains(&name_lower) {
        return false;
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

    for prefix in IDENTITY_PREFIXES {
        let pattern = format!("{}{}", prefix, name_lower);
        if lower.contains(&pattern) {
            return true;
        }
    }
    false
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
        // <=4 sentences regardless of phrasing variation.
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
            sentence_count <= 4,
            "reply should be capped to <=4 sentences, got {sentence_count}: {result:?}"
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
    fn four_sentence_reply_unchanged_by_length_cap() {
        // Boundary: exactly 4 sentences — at the cap limit, must pass through.
        let dialogue = "Good day to ye. The harvest has been fine this year. \
            Brigid was asking after the grain prices. Have ye heard the news from Roscommon?";
        let result = cap_sentence_count(dialogue);
        assert_eq!(
            result.trim(),
            dialogue.trim(),
            "4-sentence reply at the cap limit should be unchanged: {result:?}"
        );
    }

    #[test]
    fn cap_preserves_trailing_question() {
        // An 8-sentence reply whose LAST sentence is a question must be capped to
        // <=4 sentences AND the trailing question must appear at the end of the
        // result so the NPC remains interactive (#1472 — trailing-question preserve).
        let dialogue = "Good mornin' to ye, friend. \
            The roads have been muddy these past few days. \
            Old Séamus from across the hill was asking after the grain prices. \
            There's much talk about the rent collectors coming round again. \
            Mary Brien says the miller raised his prices last Tuesday. \
            The bishop passed through Strokestown not a fortnight ago. \
            Sure, the weather has been unkind to us all this season. \
            Now, what brings ye here?";
        let result = cap_sentence_count(dialogue);
        let sentence_count = split_sentences(&result)
            .iter()
            .filter(|s| !s.trim().is_empty())
            .count();
        assert!(
            sentence_count <= 4,
            "capped reply should be <=4 sentences, got {sentence_count}: {result:?}"
        );
        assert!(
            result.trim_end().ends_with("what brings ye here?"),
            "trailing question should be preserved at end of capped reply: {result:?}"
        );
    }

    #[test]
    fn cap_no_trailing_question_unchanged_behavior() {
        // An 8-sentence reply with NO trailing question must produce the same
        // result as before: keep the first 4 sentences (head, not tail).
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
            sentence_count <= 4,
            "capped reply without trailing question should be <=4 sentences, got {sentence_count}: {result:?}"
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
            sentence_count <= 4,
            "8-sentence ramble should be capped to <=4 sentences, got {sentence_count}: {result:?}"
        );
        assert!(
            result.contains("Good day to ye"),
            "first sentence must survive: {result:?}"
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

    /// AC-3 (#1487): a short phrase repeated exactly 3× (below the 4× threshold)
    /// must not trigger the guard — natural repetition must be allowed.
    #[test]
    fn degenerate_phrase_loop_below_threshold_passes_through() {
        // "come back soon" appears 3× — below the 4× threshold.
        let dialogue = "Come back soon, come back soon, come back soon, friend.";
        let result = collapse_degenerate_phrase_loop(dialogue);
        // The guard must not have fired (3× < 4× threshold).
        assert!(
            result.contains("come back soon"),
            "3× repetition should pass through (below threshold): {result:?}"
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
}
