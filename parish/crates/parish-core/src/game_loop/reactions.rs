//! Shared reaction-pipeline helpers — extracted from all backends (#696 third slice).
//!
//! # What is here
//!
//! - [`is_snippet_injection_char`] — security validation shared by the
//!   `react_to_message` endpoint on every runtime so injection protection
//!   (#498 / #687) is enforced uniformly.
//!
//! # What stays per-runtime
//!
//! `emit_npc_reactions` itself cannot be extracted here because it spawns a
//! background task that needs an `Arc`-clone of the entire `AppState` (so it
//! can re-acquire locks after the caller returns). The server and Tauri runtimes
//! store state as `Mutex<T>` fields inside `Arc<AppState>`, not as individually
//! `Arc`-wrapped mutexes, so there is no portable way to pass them to a shared
//! background spawner without restructuring `AppState`.  Each backend therefore
//! keeps its own `emit_npc_reactions` but both now call
//! [`is_snippet_injection_char`] from this shared module for security parity.

/// Returns `true` for characters that are banned from `message_snippet` values
/// to prevent NPC system-prompt injection (#498 / #687).
///
/// Banned set: double-quote, backslash, Unicode line/paragraph separators
/// (U+0085 NEL, U+2028, U+2029), and all Unicode control characters
/// (including `\n`, `\r`, `\t`). This broadened filter covers every sibling
/// glyph attackers might reach for without enumerating them one at a time.
///
/// # Cross-mode parity
///
/// `react_to_message` on both the web server (`parish-server/src/routes.rs`)
/// and the Tauri desktop (`parish-tauri/src/commands.rs`) delegate their
/// snippet validation here, guaranteeing identical rejection behaviour.
pub fn is_snippet_injection_char(c: char) -> bool {
    c == '"' || c == '\\' || c == '\u{2028}' || c == '\u{2029}' || c.is_control()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_newline() {
        assert!(is_snippet_injection_char('\n'));
    }

    #[test]
    fn blocks_carriage_return() {
        assert!(is_snippet_injection_char('\r'));
    }

    #[test]
    fn blocks_double_quote() {
        assert!(is_snippet_injection_char('"'));
    }

    #[test]
    fn blocks_backslash() {
        assert!(is_snippet_injection_char('\\'));
    }

    #[test]
    fn blocks_unicode_line_sep() {
        assert!(is_snippet_injection_char('\u{2028}'));
    }

    #[test]
    fn blocks_unicode_para_sep() {
        assert!(is_snippet_injection_char('\u{2029}'));
    }

    #[test]
    fn allows_normal_text() {
        for c in "Hello, world! It's a grand day.".chars() {
            assert!(
                !is_snippet_injection_char(c),
                "char {:?} should be allowed",
                c
            );
        }
    }

    #[test]
    fn blocks_nel_control() {
        // U+0085 NEL — a Unicode control character
        assert!(is_snippet_injection_char('\u{0085}'));
    }
}
