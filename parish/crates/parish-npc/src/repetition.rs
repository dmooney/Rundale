//! Deterministic anti-repetition guard (#1228) and post-generation dialogue
//! guards (#1459, #1460) for degenerate model output.

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

/// Post-generation guard for fabricated-person confirmation (#1459, #1466).
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
pub fn guard_fabricated_person_confirmation(
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

    // First-name conflation check (#1466): if the player named a fabricated
    // full name AND the NPC affirms by that first name alone, also decline.
    // Only runs when there are fabricated full names in this exchange.
    //
    // Real-roster escape hatch: if the dialogue affirms the first name but the
    // affirmation is accompanied by a real roster full name that begins with
    // that first name (e.g. dialogue contains "Cormac Duffy" and roster has
    // "Cormac Duffy"), the NPC is talking about a real person — do NOT decline.
    let dialogue_lower = dialogue.to_lowercase();
    for first_name in &fabricated_full_name_first_names {
        if dialogue_affirms_name(dialogue, first_name) {
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
                "person-confirmation guard fired: NPC affirmed by first name only for player-named fabricated full name (#1466)"
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

/// Applies the full verbosity / run-on guard to a finalized dialogue string (#1460).
///
/// Steps, in order:
/// 1. Strip bare leaked mood-adjective from tail ([`strip_leaked_mood_word`]).
/// 2. Trim mid-sentence truncation ellipsis ([`trim_mid_sentence_truncation`]).
/// 3. Collapse non-consecutive near-duplicate sentences ([`collapse_distributed_repeated_sentences`]).
/// 4. Cap total questions in the whole reply to at most 2 ([`cap_total_questions`]).
/// 5. Cap trailing question stack to one ([`cap_trailing_questions`]).
///
/// Steps 3 and 4 are the #1460 core fix for distributed / interleaved repetition
/// that `collapse_repeated_sentences` (which only handles consecutive duplicates,
/// called upstream by `guard_against_repetition`) does not catch. Step 5 keeps
/// the earlier trailing-question guard in place as a final cleanup.
///
/// Conservative — does not alter legitimate prose.
pub fn guard_verbosity_runons(dialogue: &str) -> String {
    if dialogue.trim().is_empty() {
        return dialogue.to_string();
    }
    let after_mood = strip_leaked_mood_word(dialogue);
    let after_trunc = trim_mid_sentence_truncation(&after_mood);
    let after_distributed = collapse_distributed_repeated_sentences(&after_trunc);
    let after_total_q = cap_total_questions(&after_distributed);
    cap_trailing_questions(&after_total_q)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
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
        let result = guard_fabricated_person_confirmation(dialogue, player_input, &known, None, 0);
        assert_eq!(
            result, dialogue,
            "NPC affirming real roster member 'Cormac Duffy' in full should not be suppressed \
             even when fabricated 'Cormac Sweeney' shares the first name: {result:?}"
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
}
