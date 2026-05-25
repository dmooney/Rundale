//! Text-quality detectors for LLM output.
//!
//! Sibling to [`crate::anachronism`]. Where `anachronism` flags
//! out-of-period concepts in *player* input (and feeds an alert into the
//! NPC prompt), `quality` scans LLM-emitted text (player auto-actions and
//! NPC replies) for structural pathologies that no judge slice currently
//! covers:
//!
//! 1. **JSON envelope leak** — raw `action": "..."}` literal from the
//!    auto-player completion bleeding past `extract_action_from_response`.
//! 2. **Template token leak** — unfilled placeholders like `[Your Name]`,
//!    `{npc_name}`, `{{location}}`.
//! 3. **Simulator corpus overlap** — n-gram match against the static
//!    [`parish_inference::simulator::CORPUS`]. The simulator is configured
//!    for `reaction` / `simulation` categories; its Markov gibberish must
//!    never render as user-visible dialogue.
//! 4. **Hallucinated Gaelic** — fada-bearing words that aren't in a small
//!    vetted vocabulary of period-appropriate Irish (e.g. `Slán`, `abhaile`).
//! 5. **Modern register** — diction words that existed in 1820 but read as
//!    21st-century traveller speech (`fascinating`, `decided to visit`).
//!    Soft signal, not an anachronism.
//! 6. **Reaction emoji monoculture** — Shannon-style diversity check on a
//!    rolling window of NPC reaction emoji.
//!
//! All detectors are pure functions returning [`QualityIssue`]s. Wire them
//! into runtime sites (the demo turn, the NPC reply path) to emit `WARN`
//! diagnostics that show up in `/tmp/demo-*.log`, and into fixture tests to
//! catch regressions on hand-picked positive/negative samples.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

// ---------------------------------------------------------------------------
// Issue kinds
// ---------------------------------------------------------------------------

/// Categorical kind for a [`QualityIssue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QualityIssueKind {
    /// Raw JSON syntax (`action": "..."}`) bleeding into chat text.
    JsonEnvelopeLeak,
    /// Unfilled placeholder like `[Your Name]` or `{npc_name}`.
    TemplateTokenLeak,
    /// N-gram overlap with the simulator Markov corpus.
    SimulatorCorpusOverlap,
    /// Word with Irish fada chars not on the period-vetted vocab list.
    HallucinatedGaelic,
    /// Diction that reads as modern English in 1820 rural Ireland.
    ModernRegister,
    /// Low diversity in a rolling window of NPC reaction emoji.
    ReactionEmojiMonoculture,
}

impl QualityIssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            QualityIssueKind::JsonEnvelopeLeak => "json-envelope-leak",
            QualityIssueKind::TemplateTokenLeak => "template-token-leak",
            QualityIssueKind::SimulatorCorpusOverlap => "simulator-corpus-overlap",
            QualityIssueKind::HallucinatedGaelic => "hallucinated-gaelic",
            QualityIssueKind::ModernRegister => "modern-register",
            QualityIssueKind::ReactionEmojiMonoculture => "reaction-emoji-monoculture",
        }
    }
}

/// One detected text-quality issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityIssue {
    pub kind: QualityIssueKind,
    /// Human-readable description, including the offending fragment.
    pub detail: String,
    /// Optional byte-offset span into the source text.
    pub span: Option<(usize, usize)>,
}

impl QualityIssue {
    fn new(kind: QualityIssueKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            span: None,
        }
    }

    fn with_span(mut self, start: usize, end: usize) -> Self {
        self.span = Some((start, end));
        self
    }
}

// ---------------------------------------------------------------------------
// (1) JSON envelope leak
// ---------------------------------------------------------------------------

pub fn detect_json_envelope_leak(text: &str) -> Vec<QualityIssue> {
    let mut out = Vec::new();
    let t = text.trim();

    if t.starts_with("action\":")
        || t.starts_with("\"action\":")
        || t.starts_with("{\"action\"")
        || t.starts_with("{ \"action\"")
    {
        let preview_end = 40.min(t.len());
        out.push(
            QualityIssue::new(
                QualityIssueKind::JsonEnvelopeLeak,
                format!(
                    "text starts with JSON action field: `{}`",
                    &t[..preview_end]
                ),
            )
            .with_span(0, preview_end),
        );
    }

    if t.ends_with("\"}") && !t.starts_with('{') {
        let tail_start = t.len().saturating_sub(20);
        out.push(
            QualityIssue::new(
                QualityIssueKind::JsonEnvelopeLeak,
                format!(
                    "text ends with unmatched JSON close: `...{}`",
                    &t[tail_start..]
                ),
            )
            .with_span(t.len().saturating_sub(2), t.len()),
        );
    }

    out
}

// ---------------------------------------------------------------------------
// (2) Template token leak
// ---------------------------------------------------------------------------

static TEMPLATE_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
          \[ [A-Z][a-zA-Z]+ (?:\s+[A-Z][a-zA-Z]+)* \]      # [Your Name]
        | \{\{? [a-zA-Z_][a-zA-Z0-9_]* \}?\}              # {npc_name} or {{location}}
        ",
    )
    .expect("template-token regex compiles")
});

pub fn detect_template_tokens(text: &str) -> Vec<QualityIssue> {
    TEMPLATE_TOKEN_RE
        .find_iter(text)
        .map(|m| {
            QualityIssue::new(
                QualityIssueKind::TemplateTokenLeak,
                format!("unfilled template token: `{}`", m.as_str()),
            )
            .with_span(m.start(), m.end())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// (3) Simulator corpus overlap
// ---------------------------------------------------------------------------
//
// Threshold note (2026-05-17 retune): originally 4-gram. False positives
// observed on period dialogue that happened to share a common English
// idiom with the corpus — e.g. NPC reply `Me da'll be the one to tell ye`
// tripped on "be the one to" which appears verbatim in the Markov source.
// Bumped to **5-gram** — true simulator leakage chains many consecutive
// corpus words, while organic English rarely matches 5 in a row. Lifts
// false-positive rate to ~0 on live demo output without losing the gibberish
// signal (corpus phrasing like "nobody can quite explain Bridget from
// beyond" still trips on its 5-gram prefix).

static CORPUS_5GRAMS: LazyLock<HashSet<[String; 5]>> = LazyLock::new(|| {
    let words: Vec<String> = parish_inference::simulator::CORPUS
        .split_whitespace()
        .map(normalize_word)
        .filter(|w| !w.is_empty())
        .collect();

    let mut set: HashSet<[String; 5]> = HashSet::with_capacity(words.len());
    for window in words.windows(5) {
        set.insert([
            window[0].clone(),
            window[1].clone(),
            window[2].clone(),
            window[3].clone(),
            window[4].clone(),
        ]);
    }
    set
});

fn normalize_word(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .to_lowercase()
}

pub fn detect_simulator_corpus_overlap(text: &str) -> Option<QualityIssue> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(normalize_word)
        .filter(|w| !w.is_empty())
        .collect();

    for win in words.windows(5) {
        let key = [
            win[0].clone(),
            win[1].clone(),
            win[2].clone(),
            win[3].clone(),
            win[4].clone(),
        ];
        if CORPUS_5GRAMS.contains(&key) {
            return Some(QualityIssue::new(
                QualityIssueKind::SimulatorCorpusOverlap,
                format!(
                    "5-gram match with simulator corpus: `{} {} {} {} {}`",
                    win[0], win[1], win[2], win[3], win[4]
                ),
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// (4) Hallucinated Gaelic
// ---------------------------------------------------------------------------

const GAELIC_VOCAB: &[&str] = &[
    "slán",
    "abhaile",
    "dia",
    "dhuit",
    "duit",
    "fáilte",
    "céad",
    "míle",
    "sláinte",
    "go",
    "raibh",
    "maith",
    "agat",
    "leat",
    "agaibh",
    "n-éirí",
    "éirí",
    "bóthar",
    "agus",
    "ní",
    "tá",
    "is",
    "an",
    "na",
    "sé",
    "sí",
    "ag",
    "le",
    "do",
    "mo",
    "ar",
    "i",
    "in",
    "tír",
    "éire",
    "oíche",
    "lá",
    "tine",
    "uisce",
    "arán",
    "bia",
    "tae",
    "máthair",
    "athair",
    "deartháir",
    "deirfiúr",
    "fear",
    "bean",
    "leanbh",
    "cailín",
    "buachaill",
    "céilí",
    "seisiún",
    "amhrán",
    "rince",
    "poitín",
    "sídhe",
    "sídh",
    "síthe",
    "mór",
    "beag",
    "fada",
    "gairid",
    "óg",
    "sean",
    "deas",
    "olc",
    "saor",
    "daor",
    "óstán",
];

fn has_fada(s: &str) -> bool {
    s.chars()
        .any(|c| matches!(c, 'á' | 'é' | 'í' | 'ó' | 'ú' | 'Á' | 'É' | 'Í' | 'Ó' | 'Ú'))
}

pub fn detect_hallucinated_gaelic(text: &str) -> Vec<QualityIssue> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for word in text.split(|c: char| {
        !c.is_alphabetic()
            && c != '\''
            && c != '-'
            && c != 'á'
            && c != 'é'
            && c != 'í'
            && c != 'ó'
            && c != 'ú'
            && c != 'Á'
            && c != 'É'
            && c != 'Í'
            && c != 'Ó'
            && c != 'Ú'
    }) {
        if word.is_empty() {
            continue;
        }
        if !has_fada(word) {
            continue;
        }
        let lc = word.to_lowercase();
        if GAELIC_VOCAB.iter().any(|v| *v == lc) {
            continue;
        }
        if word
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            const FADA_NAMES: &[&str] = &[
                "séamus", "tomás", "máire", "róisín", "siobhán", "pádraig", "tadhg",
            ];
            if FADA_NAMES.iter().any(|n| *n == lc) {
                continue;
            }
        }
        if seen.insert(lc.clone()) {
            out.push(QualityIssue::new(
                QualityIssueKind::HallucinatedGaelic,
                format!("unrecognized Gaelic word: `{}`", word),
            ));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// (5) Modern register
// ---------------------------------------------------------------------------

const MODERN_REGISTER_TERMS: &[&str] = &[
    "fascinating",
    "incredible",
    "amazing",
    "awesome",
    "definitely",
    "totally",
    "absolutely",
    "guys",
    "stuff",
    "decided to visit",
    "taking in the sights",
    "i find it",
    "it's been a pleasure",
    "healing properties",
];

/// Renders a context alert for the dialogue prompt when the player's
/// input contains any `MODERN_REGISTER_TERMS` phrase. Mirrors
/// `anachronism::format_context_alert` in shape so the NPC dialogue
/// prompt builder can inject both alerts alongside each other.
///
/// TODO #55: pre-fix the validator caught modern-register phrases on
/// NPC *output* only, so a player saying "taking in the sights"
/// could feed it through and trip a WARN on the NPC's echo. This
/// alert closes the upstream gap by warning the model up front.
pub fn format_player_register_alert(player_input: &str) -> Option<String> {
    let issues = detect_modern_register(player_input);
    if issues.is_empty() {
        return None;
    }
    let mut alert = String::from(
        "\nMODERN-REGISTER ALERT: The player used 21st-century phrasing. \
         Do NOT echo any of these phrases back in your reply — paraphrase \
         the idea in 1820 Irish-English instead. Specific phrases detected:\n",
    );
    for issue in &issues {
        // QualityIssue::detail looks like "modern-register phrase: 'taking in the sights'"
        // or "modern-register word: 'fascinating'" — surface the whole detail
        // string so the model can see what to avoid verbatim.
        alert.push_str(&format!("- {}\n", issue.detail));
    }
    alert.push_str(
        "\nReply in your character's natural Hiberno-English register. \
         If the player's wording would be unfamiliar to a 1820 villager, \
         react to the *idea* (a stroll, a look around, fine sights, a \
         day's diversion) without using the modern formulation.",
    );
    Some(alert)
}

pub fn detect_modern_register(text: &str) -> Vec<QualityIssue> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();

    // Both branches use the same word-boundary scan — phrases just include
    // internal spaces. Single-word and multi-word terms are checked
    // uniformly so `decided to visit` cannot match inside `undecided to
    // visit`. Boundary uses Rust char semantics, not raw bytes, so non-
    // ASCII letters on either side of a term also count as word chars.
    for &term in MODERN_REGISTER_TERMS {
        let label = if term.contains(' ') {
            "modern-register phrase"
        } else {
            "modern-register word"
        };
        let mut idx = 0;
        while let Some(found) = lower[idx..].find(term) {
            let abs = idx + found;
            let before_ok = abs == 0 || !is_word_char_at(&lower, abs - 1);
            let after = abs + term.len();
            let after_ok = after == lower.len() || !is_word_char_at(&lower, after);
            if before_ok && after_ok && seen.insert(term) {
                out.push(QualityIssue::new(
                    QualityIssueKind::ModernRegister,
                    format!("{}: `{}`", label, term),
                ));
                break;
            }
            idx = abs + term.len();
        }
    }
    out
}

/// True iff the char at `byte_offset` is a "word" char: any Unicode letter,
/// digit, underscore, or apostrophe. Used as the boundary predicate for
/// modern-register matching so that phrases bordering on `úna` / `n'éirí` /
/// 's are treated correctly across the Hiberno-English / Irish split.
fn is_word_char_at(s: &str, byte_offset: usize) -> bool {
    s[byte_offset..]
        .chars()
        .next()
        .map(|c| c.is_alphanumeric() || c == '_' || c == '\'')
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// (6) Reaction emoji monoculture
// ---------------------------------------------------------------------------

/// Minimum samples before [`detect_emoji_monoculture`] reports a verdict.
///
/// Exposed so callers (e.g. `NpcManager::record_reaction_emoji`) can
/// short-circuit before allocating a snapshot slice for the detector.
pub const DEFAULT_EMOJI_MIN_SAMPLES: usize = 4;

pub fn detect_emoji_monoculture(emojis: &[&str]) -> Option<QualityIssue> {
    detect_emoji_monoculture_with_thresholds(emojis, DEFAULT_EMOJI_MIN_SAMPLES, 0.30)
}

pub fn detect_emoji_monoculture_with_thresholds(
    emojis: &[&str],
    min_samples: usize,
    min_distinct_ratio: f64,
) -> Option<QualityIssue> {
    if emojis.len() < min_samples {
        return None;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for e in emojis {
        *counts.entry(e).or_insert(0) += 1;
    }
    let distinct = counts.len();
    let total = emojis.len();
    let ratio = distinct as f64 / total as f64;
    if ratio < min_distinct_ratio {
        return Some(QualityIssue::new(
            QualityIssueKind::ReactionEmojiMonoculture,
            format!(
                "emoji diversity {}/{} = {:.0}% (threshold {:.0}%)",
                distinct,
                total,
                ratio * 100.0,
                min_distinct_ratio * 100.0
            ),
        ));
    }
    None
}

// ---------------------------------------------------------------------------
// Aggregate
// ---------------------------------------------------------------------------

pub fn detect_all_text_issues(text: &str) -> Vec<QualityIssue> {
    let mut out = Vec::new();
    out.extend(detect_json_envelope_leak(text));
    out.extend(detect_template_tokens(text));
    if let Some(issue) = detect_simulator_corpus_overlap(text) {
        out.push(issue);
    }
    out.extend(detect_hallucinated_gaelic(text));
    out.extend(detect_modern_register(text));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_leak_leading_action_field() {
        let s = r#"action": "Good morning, Peig Hannigan. My name is [Your Name]. I'm just wandering."}"#;
        let issues = detect_json_envelope_leak(s);
        assert!(!issues.is_empty(), "expected leading JSON leak");
        assert_eq!(issues[0].kind, QualityIssueKind::JsonEnvelopeLeak);
    }

    #[test]
    fn json_leak_trailing_close_brace() {
        let s = r#"I'm just curious about the community."}"#;
        let issues = detect_json_envelope_leak(s);
        assert!(!issues.is_empty(), "expected trailing JSON leak");
    }

    #[test]
    fn json_leak_clean_text_passes() {
        let s = "Good morning to you, Peig. I'm just wandering through.";
        assert!(detect_json_envelope_leak(s).is_empty());
    }

    #[test]
    fn template_token_bracket_form() {
        let s = "Ah, [Your Name], it's a pleasure to make yer acquaintance.";
        let issues = detect_template_tokens(s);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].detail.contains("[Your Name]"));
    }

    #[test]
    fn template_token_snake_form() {
        let s = "Hello there, {player_name}, welcome to {{location}}.";
        let issues = detect_template_tokens(s);
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn template_token_stage_direction_passes() {
        let s = "She paused. [laughs] Then continued.";
        assert!(detect_template_tokens(s).is_empty());
    }

    #[test]
    fn corpus_overlap_actual_demo_gibberish() {
        let s = "Can quite explain bridget from the new collection for them \
                 but so says herself says the pub saying things that we do. \
                 God help us.";
        let issue = detect_simulator_corpus_overlap(s);
        assert!(
            issue.is_some(),
            "should flag known simulator-corpus 5-grams"
        );
    }

    #[test]
    fn corpus_overlap_clean_npc_reply_passes() {
        let s = "Good mornin' to ye. Ye seem like a stranger 'round these parts. \
                 Name's Peig Hannigan. And ye be...?";
        assert!(detect_simulator_corpus_overlap(s).is_none());
    }

    #[test]
    fn corpus_overlap_no_false_positive_on_common_idiom() {
        // Live demo (2026-05-17): Brendan Duffy at The Mill said
        // "Me da'll be the one to tell ye how it's done around here."
        // The 4-gram "be the one to" appears in the simulator corpus AND
        // in normal English. Threshold must be high enough to skip this
        // organic match.
        let s = "Me da'll be the one to tell ye how it's done around here. 'Tis a fair price, so it is.";
        assert!(
            detect_simulator_corpus_overlap(s).is_none(),
            "5-gram threshold must not trip on common-English idiom; \
             saw: {:?}",
            detect_simulator_corpus_overlap(s)
        );
    }

    #[test]
    fn gaelic_hallucinated_connachtu() {
        let s = "Slán abhaile go connachtú (Safe home and good journey).";
        let issues = detect_hallucinated_gaelic(s);
        assert!(
            issues.iter().any(|i| i.detail.contains("connachtú")),
            "expected `connachtú` flagged; got {:?}",
            issues
        );
    }

    #[test]
    fn gaelic_valid_phrase_passes() {
        let s = "Slán abhaile, mo chara.";
        let issues = detect_hallucinated_gaelic(s);
        assert!(
            issues.is_empty(),
            "valid Irish should pass; got {:?}",
            issues
        );
    }

    #[test]
    fn gaelic_fada_name_passes() {
        let s = "Séamus is after telling me.";
        assert!(detect_hallucinated_gaelic(s).is_empty());
    }

    #[test]
    fn gaelic_poitin_passes() {
        // poitín is a real 1820-period Irish word for home-distilled spirits;
        // must not trip the hallucinated-Gaelic detector.
        let s = "A drop of poitín warms the bones.";
        let issues = detect_hallucinated_gaelic(s);
        assert!(
            issues.is_empty(),
            "poitín must be allow-listed; got {:?}",
            issues
        );
    }

    #[test]
    fn format_player_register_alert_fires_on_hit() {
        // TODO #55 — the exact phrase the demo audit caught Concannon
        // echoing in cycle 12 because the player had used it first.
        let alert = format_player_register_alert("I'm just taking in the sights")
            .expect("alert must render when a MODERN_REGISTER_TERMS phrase is matched");
        assert!(
            alert.contains("MODERN-REGISTER ALERT"),
            "missing header:\n{alert}"
        );
        assert!(
            alert.contains("Do NOT echo any of these phrases"),
            "missing echo guard:\n{alert}"
        );
        assert!(
            alert.contains("taking in the sights"),
            "alert must surface the matched phrase:\n{alert}"
        );
    }

    #[test]
    fn format_player_register_alert_silent_on_clean_input() {
        assert!(format_player_register_alert("the road be long this morning").is_none());
        assert!(format_player_register_alert("").is_none());
    }

    #[test]
    fn modern_register_fascinating() {
        let s = "I find it fascinating to learn about different places.";
        let issues = detect_modern_register(s);
        assert!(issues.iter().any(|i| i.detail.contains("fascinating")));
    }

    #[test]
    fn modern_register_phrase() {
        let s = "I'm just traveling and decided to visit Kilteevan.";
        let issues = detect_modern_register(s);
        assert!(issues.iter().any(|i| i.detail.contains("decided to visit")));
    }

    #[test]
    fn modern_register_healing_properties() {
        let s = "ginger for its healing properties";
        let issues = detect_modern_register(s);
        assert!(
            issues
                .iter()
                .any(|i| i.detail.contains("healing properties"))
        );
    }

    #[test]
    fn modern_register_period_speech_passes() {
        let s = "Good mornin' to ye. 'Tis a pleasure to greet ye.";
        let issues = detect_modern_register(s);
        assert!(issues.is_empty(), "got {:?}", issues);
    }

    #[test]
    fn modern_register_phrase_respects_word_boundary() {
        // Bot review (PR #990, gemini medium): the multi-word phrase
        // branch previously used `lower.contains(term)` with no boundary
        // check, so "undecided to visit" would match "decided to visit".
        // After the fix, only true standalone occurrences trip.
        let s = "She was undecided to visit Aoife.";
        assert!(
            detect_modern_register(s).is_empty(),
            "phrase as substring of another word must not match; got {:?}",
            detect_modern_register(s)
        );
        let s2 = "I decided to visit Aoife.";
        assert!(
            !detect_modern_register(s2).is_empty(),
            "standalone phrase must still match"
        );
    }

    #[test]
    fn modern_register_word_boundary_unicode() {
        // Boundary check must use Char semantics, not raw bytes. A non-
        // ASCII alphanumeric on either side of a term still counts as a
        // word char and must block a false match.
        let s = "fascinatingñ"; // trailing non-ASCII letter
        assert!(
            detect_modern_register(s).is_empty(),
            "non-ASCII letter abutting term must not be treated as boundary"
        );
    }

    #[test]
    fn emoji_monoculture_all_same() {
        let r = ["😊", "😊", "😊", "😊", "😊"];
        let issue = detect_emoji_monoculture(&r);
        assert!(issue.is_some(), "5×😊 should flag");
    }

    #[test]
    fn emoji_monoculture_diverse_passes() {
        let r = ["😊", "🤔", "😢", "😄", "😡"];
        assert!(detect_emoji_monoculture(&r).is_none());
    }

    #[test]
    fn emoji_monoculture_too_few_samples_passes() {
        let r = ["😊", "😊"];
        assert!(detect_emoji_monoculture(&r).is_none());
    }

    #[test]
    fn aggregate_catches_multiple_issues() {
        let s = r#"action": "Ah, [Your Name], decided to visit ye. Slán go connachtú."}"#;
        let issues = detect_all_text_issues(s);
        let kinds: HashSet<QualityIssueKind> = issues.iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&QualityIssueKind::JsonEnvelopeLeak));
        assert!(kinds.contains(&QualityIssueKind::TemplateTokenLeak));
        assert!(kinds.contains(&QualityIssueKind::ModernRegister));
        assert!(kinds.contains(&QualityIssueKind::HallucinatedGaelic));
    }
}
