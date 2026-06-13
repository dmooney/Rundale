//! Deterministic anti-repetition guard (#1228) for degenerate model output.

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

/// Extracts the first sentence (opener) from a dialogue string, normalized for
/// cross-NPC duplicate detection.
///
/// The opener is everything up to and including the first terminal punctuation
/// character (`.`, `!`, `?`, `…`). If no terminal punctuation exists the whole
/// string is the opener. The result is normalized (lowercased, whitespace
/// collapsed, trailing punctuation stripped) for comparison purposes.
fn extract_opener(dialogue: &str) -> String {
    let raw_opener = split_sentences(dialogue)
        .into_iter()
        .next()
        .unwrap_or_else(|| dialogue.to_string());
    normalize_for_repetition(&raw_opener)
}

/// Extracts the content AFTER the first sentence of `dialogue`.
///
/// Returns an empty string when there is no second sentence. Leading whitespace
/// of the remainder is trimmed.
fn remainder_after_opener(dialogue: &str) -> &str {
    let sentences = split_sentences(dialogue);
    if sentences.len() < 2 {
        return "";
    }
    // Find the byte offset of the second sentence's start in the original string.
    let first_len = sentences[0].len();
    dialogue[first_len..].trim_start()
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
    let my_opener = extract_opener(reply);
    if my_opener.is_empty() {
        return reply.to_string();
    }
    let is_duplicate = prior_openers.iter().any(|prev| {
        normalize_for_repetition(prev) == my_opener || dialogue_similarity(&my_opener, prev) >= 0.75
    });
    if !is_duplicate {
        return reply.to_string();
    }
    let rest = remainder_after_opener(reply);
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
    extract_opener(reply)
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
}
