//! JSON-mode system-prompt isolation and structural-tag hardening.
//!
//! Anthropic has no native `response_format`, so `generate_json` /
//! `generate_stream_json` augment the caller's system prompt with a
//! JSON-only instruction. To keep an adversarial (or contaminated) caller
//! from escaping the wrapper, the caller text is isolated inside a
//! `<caller_system>` XML delimiter and every close-tag variant of a
//! structural tag is neutralised (#458 / #599). Split out of the monolithic
//! `anthropic_client` module (#1200).

use regex::Regex;
use std::sync::LazyLock;

/// Engine instruction appended after every generate_json system prompt.
/// Kept separate from the caller's text so the model can always attribute
/// it to the engine, not to the caller.
pub(super) const JSON_INSTRUCTION: &str =
    "Respond ONLY with a single JSON object. No prose, no code fences, no commentary.";

/// Wraps the caller-supplied `system` string inside an XML delimiter and
/// places the engine's JSON instruction in its own block (#458).
///
/// - If `system` is `Some`, returns
///   `<caller_system>\n{sanitised}\n</caller_system>\n\n<engine_instruction>\n{JSON_INSTRUCTION}\n</engine_instruction>`
///   where any close of the `<caller_system>` or `<engine_instruction>` tag in
///   the input — in any XML-lax whitespace variant — is rewritten to the inert
///   bracketed sentinel so the caller cannot escape either wrapper (#599).
/// - If `system` is `None`, returns the bare engine instruction (no
///   wrapping needed; there is no untrusted content to isolate).
pub(super) fn isolate_system_for_json(system: Option<&str>) -> String {
    match system {
        Some(s) => {
            let safe = neutralise_structural_tags(s);
            format!(
                "<caller_system>\n{safe}\n</caller_system>\n\n<engine_instruction>\n{JSON_INSTRUCTION}\n</engine_instruction>"
            )
        }
        None => JSON_INSTRUCTION.to_string(),
    }
}

/// The set of XML tag names used as structural delimiters in the assembled
/// system prompt.  Any close-tag variant for any of these names found in
/// caller-supplied content is rewritten to `[/<name>]` so an attacker cannot
/// escape the `<caller_system>` wrapper or inject a fake `<engine_instruction>`
/// block (#458 / #599).
///
/// Sentinels use square brackets so they are visible in logs but not parseable
/// as XML tags by the model.
pub(super) const STRUCTURAL_TAGS: &[(&str, &str)] = &[
    ("caller_system", "[/caller_system]"),
    ("engine_instruction", "[/engine_instruction]"),
];

/// Regex matching any XML-lax close-tag variant of a structural tag name.
/// Matches `<` + optional whitespace + `/` + optional whitespace + tag name
/// (case-insensitive) + optional whitespace + `>`.
static STRUCTURAL_CLOSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    let parts: Vec<String> = STRUCTURAL_TAGS
        .iter()
        .map(|(name, _)| regex::escape(name))
        .collect();
    let pattern = format!("(?i)<\\s*/\\s*({})\\s*>", parts.join("|"));
    Regex::new(&pattern).expect("invalid structural close-tag regex")
});

/// Rewrites every close-tag variant of any structural tag to the inert
/// bracketed sentinel (codex P1 on #458/#564/#599).
///
/// XML permits whitespace anywhere inside a tag, and is case-insensitive
/// for HTML-style parsers, so `</caller_system>`, `</caller_system >`,
/// `</ caller_system>`, and `</CALLER_SYSTEM>` are all equivalent.
/// Replacing only the exact lowercase no-whitespace form would still let
/// an attacker break out of the wrapper with any of the other variants.
///
/// Replaces the matched tag with the corresponding bracketed sentinel
/// (e.g. `[/caller_system]`) so the injected close-tag is visible in logs
/// but not parseable as XML by the model.
pub(super) fn neutralise_structural_tags(input: &str) -> String {
    STRUCTURAL_CLOSE_RE
        .replace_all(input, |caps: &regex::Captures| {
            let matched = caps.get(1).map_or("", |m| m.as_str());
            STRUCTURAL_TAGS
                .iter()
                .find(|(name, _)| matched.eq_ignore_ascii_case(name))
                .map(|(_, sentinel)| *sentinel)
                .expect("captured tag name must match a structural tag")
        })
        .into_owned()
}
