//! Best-effort API-key scrubbing for text bound for disk.
//!
//! Used by the on-disk inference log and chat transcript writers to redact
//! known provider key shapes before they hit the filesystem. The in-memory
//! debug log is unaffected — that sink is already redacted via
//! `redact_call_log` for the debug panel.
//!
//! Curated regex patterns only; no high-entropy fallback. The patterns
//! target the leading-literal prefixes used by major providers so they
//! short-circuit cheaply on text that contains no such marker.
//! False positives on Irish placenames / NPC names were the reason a
//! generic high-entropy detector was rejected.

use std::borrow::Cow;
use std::sync::OnceLock;

use regex::Regex;

/// Lazily compiled list of `(pattern, replacement)` pairs.
fn patterns() -> &'static [(Regex, &'static str)] {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            // Order matters: more specific patterns before more general ones.
            // Anthropic must match before OpenAI's generic `sk-` rule.
            (
                Regex::new(r"sk-ant-[A-Za-z0-9_\-]{20,}").unwrap(),
                "[REDACTED:anthropic_key]",
            ),
            (
                Regex::new(r"sk-(?:proj-)?[A-Za-z0-9_\-]{20,}").unwrap(),
                "[REDACTED:openai_key]",
            ),
            (
                Regex::new(r"gsk_[A-Za-z0-9]{40,}").unwrap(),
                "[REDACTED:groq_key]",
            ),
            (
                Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                "[REDACTED:aws_access_key]",
            ),
            (
                Regex::new(r"AIza[0-9A-Za-z_\-]{35}").unwrap(),
                "[REDACTED:google_api_key]",
            ),
            (
                Regex::new(r"github_pat_[A-Za-z0-9_]{82}").unwrap(),
                "[REDACTED:github_token]",
            ),
            (
                Regex::new(r"gh[ops]_[A-Za-z0-9]{36}").unwrap(),
                "[REDACTED:github_token]",
            ),
            (
                Regex::new(r"(?i)bearer\s+[A-Za-z0-9_.+/=\-]{20,}").unwrap(),
                "Bearer [REDACTED:bearer_token]",
            ),
        ]
    })
}

/// Replaces any matching provider-key shape in `text` with a `[REDACTED:*]`
/// marker. Returns `Cow::Borrowed(text)` unchanged when no pattern matches,
/// so callers pay no allocation cost on the overwhelmingly common case.
pub fn scrub(text: &str) -> Cow<'_, str> {
    let pats = patterns();
    let mut current: Cow<'_, str> = Cow::Borrowed(text);
    for (re, replacement) in pats {
        if re.is_match(&current) {
            current = Cow::Owned(re.replace_all(&current, *replacement).into_owned());
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_match_is_borrowed() {
        let input = "Hello, Roscommon! The road to Kilteevan is muddy.";
        let out = scrub(input);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(out, input);
    }

    #[test]
    fn scrubs_openai_key() {
        let input = "Authorization is sk-proj-AAAAAAAAAAAAAAAAAAAAAAAA right there";
        let out = scrub(input);
        assert!(out.contains("[REDACTED:openai_key]"));
        assert!(!out.contains("AAAAAAAAAAAAAAAAAAAAAAAA"));
    }

    #[test]
    fn scrubs_anthropic_key() {
        let input = "key=sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAA";
        let out = scrub(input);
        assert!(out.contains("[REDACTED:anthropic_key]"));
        assert!(!out.contains("api03"));
    }

    #[test]
    fn anthropic_takes_priority_over_openai() {
        // The OpenAI pattern would also match `sk-...`, but Anthropic's more
        // specific `sk-ant-` prefix must run first.
        let input = "sk-ant-foobarbazquxquuxquuzcorgegrault";
        let out = scrub(input);
        assert!(out.contains("[REDACTED:anthropic_key]"));
        assert!(!out.contains("[REDACTED:openai_key]"));
    }

    #[test]
    fn scrubs_groq_key() {
        let input = "gsk_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUV";
        let out = scrub(input);
        assert!(out.contains("[REDACTED:groq_key]"));
    }

    #[test]
    fn scrubs_aws_access_key() {
        let input = "aws=AKIAIOSFODNN7EXAMPLE done";
        let out = scrub(input);
        assert!(out.contains("[REDACTED:aws_access_key]"));
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn scrubs_google_api_key() {
        let input = "key=AIzaSyA-EXAMPLE0123456789abcdefghijklmnopq";
        let out = scrub(input);
        assert!(out.contains("[REDACTED:google_api_key]"));
    }

    #[test]
    fn scrubs_github_pat() {
        // "ghp_" + exactly 36 alphanumerics — matches `gh[ops]_[A-Za-z0-9]{36}`.
        let body = "abcdefghijklmnopqrstuvwxyz0123456789";
        assert_eq!(body.len(), 36);
        let input = format!("ghp_{body}");
        let out = scrub(&input);
        assert!(out.contains("[REDACTED:github_token]"));
    }

    #[test]
    fn scrubs_github_fine_grained_pat() {
        // 82 chars after the `github_pat_` literal prefix.
        let body = "A".repeat(82);
        let input = format!("github_pat_{body}");
        let out = scrub(&input);
        assert!(out.contains("[REDACTED:github_token]"));
    }

    #[test]
    fn scrubs_bearer_header() {
        let input = "Authorization: Bearer abcdef0123456789ABCDEF0123";
        let out = scrub(input);
        assert!(out.contains("Bearer [REDACTED:bearer_token]"));
        assert!(!out.contains("abcdef0123456789ABCDEF0123"));
    }

    #[test]
    fn multiple_tokens_in_one_string() {
        let input = "first sk-proj-AAAAAAAAAAAAAAAAAAAAAAAA then AKIAIOSFODNN7EXAMPLE";
        let out = scrub(input);
        assert!(out.contains("[REDACTED:openai_key]"));
        assert!(out.contains("[REDACTED:aws_access_key]"));
    }

    #[test]
    fn does_not_redact_short_sk_strings() {
        // A 5-char "sk-foo" is well below the 20-char minimum and must
        // not trigger redaction (avoids stripping arbitrary prose).
        let input = "sk-foo and sk-bar in the prose";
        let out = scrub(input);
        assert_eq!(out, input);
    }
}
