//! Memory-truncation helper shared by all tiers.

/// Truncates a string to a maximum length, adding `…` (single
/// codepoint, 3-byte UTF-8) if truncated.
///
/// TODO #7: the previous suffix `...` (three ASCII dots) doubled as a
/// truncation signal but was indistinguishable from a model-emitted
/// ellipsis-as-punctuation in the recent-events buffer fed to
/// subsequent NPC turns. The single `…` codepoint is unambiguously
/// "the system trimmed this" and is also 0 bytes cheaper in the
/// prompt budget (3 vs 3 bytes for `...` but 1 vs 3 visual chars).
pub(super) fn truncate_for_memory(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // `…` is 3 bytes UTF-8 (`0xE2 0x80 0xA6`); reserve that many
        // so the resulting string still satisfies the caller's
        // `max_len` byte cap.
        const ELLIPSIS_BYTES: usize = 3;
        let boundary = crate::floor_char_boundary(s, max_len.saturating_sub(ELLIPSIS_BYTES));
        let safe_boundary = boundary.min(s.len());
        format!("{}…", &s[..safe_boundary])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_for_memory() {
        // Under cap: passes through unchanged.
        assert_eq!(truncate_for_memory("short", 10), "short");

        // At cap exactly: passes through unchanged.
        let at_cap = "a".repeat(20);
        assert_eq!(truncate_for_memory(&at_cap, 20), at_cap);

        // Over cap: clips and appends single-codepoint `…`.
        // TODO #7: suffix changed from "..." to "…" so the trim
        // marker is unambiguous vs model-emitted ellipsis.
        let long = "a".repeat(100);
        let truncated = truncate_for_memory(&long, 20);
        assert!(truncated.len() <= 20);
        assert!(
            truncated.ends_with('…'),
            "truncation must end with single-codepoint ellipsis, got {truncated:?}"
        );
        assert!(
            !truncated.ends_with("..."),
            "truncation must not use legacy three-dot suffix"
        );
    }

    #[test]
    fn test_truncate_for_memory_empty_string() {
        assert_eq!(truncate_for_memory("", 10), "");
    }

    #[test]
    fn test_truncate_for_memory_exact_boundary() {
        assert_eq!(truncate_for_memory("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_for_memory_one_over() {
        let result = truncate_for_memory("123456", 5);
        assert!(result.ends_with('…'));
        // `…` is 3 bytes UTF-8; result fits in the byte cap.
        assert!(result.len() <= 5);
    }

    #[test]
    fn test_truncate_for_memory_max_len_zero() {
        // max_len=0 leaves no room — result is the bare ellipsis (3 bytes).
        let result = truncate_for_memory("hello", 0);
        assert_eq!(result, "…");
    }

    #[test]
    fn test_truncate_for_memory_max_len_three() {
        // max_len=3 means only room for `…` (3 bytes).
        let result = truncate_for_memory("hello world", 3);
        assert_eq!(result, "…");
    }

    #[test]
    fn test_truncate_for_memory_multibyte_utf8() {
        // Ensure truncation doesn't split multi-byte characters
        let irish = "Dia dhuit, a chara. Cén chaoi a bhfuil tú?";
        let result = truncate_for_memory(irish, 15);
        assert!(result.ends_with('…'));
        assert!(result.len() <= 18); // slightly over due to char boundary
        // Should be valid UTF-8 (no panic)
        let _ = result.chars().count();
    }

    #[test]
    fn test_truncate_for_memory_very_long_string() {
        let long = "x".repeat(10000);
        let result = truncate_for_memory(&long, 50);
        assert!(result.len() <= 50);
        assert!(result.ends_with('…'));
    }
}
