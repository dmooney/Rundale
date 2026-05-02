/// Tests for submit_input validation in the Tauri command path (#752).
///
/// Mode-parity mirror of the `addressed_to` tests in
/// `parish-server/tests/isolation.rs`.  Both code paths enforce identical
/// limits (max 10 entries, max 100 chars per name) so clients see consistent
/// error behaviour regardless of which backend they target.
use parish_tauri_lib::commands::validate_addressed_to;

// ── #752 — addressed_to length cap (Tauri path) ──────────────────────────────

/// Empty addressee list is always valid.
#[test]
fn addressed_to_empty_list_passes() {
    assert_eq!(validate_addressed_to(&[]), Ok(()));
}

/// Up to 10 entries with short names is valid.
#[test]
fn addressed_to_ten_entries_passes() {
    let names: Vec<String> = (0..10).map(|i| format!("npc{i}")).collect();
    assert_eq!(validate_addressed_to(&names), Ok(()));
}

/// 11 entries must be rejected (exceeds the 10-entry cap).
#[test]
fn addressed_to_eleven_entries_is_rejected() {
    let names: Vec<String> = (0..11).map(|i| format!("npc{i}")).collect();
    assert!(
        validate_addressed_to(&names).is_err(),
        "11 addressees should be rejected"
    );
}

/// A single name longer than 100 characters must be rejected.
#[test]
fn addressed_to_name_over_100_chars_is_rejected() {
    let long_name = "a".repeat(101);
    assert!(
        validate_addressed_to(&[long_name]).is_err(),
        "name > 100 chars should be rejected"
    );
}

/// A name of exactly 100 characters must be accepted.
#[test]
fn addressed_to_name_exactly_100_chars_passes() {
    let name = "a".repeat(100);
    assert_eq!(validate_addressed_to(&[name]), Ok(()));
}

/// Mix: valid count but one name is oversized — still rejected.
#[test]
fn addressed_to_valid_count_but_oversized_name_is_rejected() {
    let mut names: Vec<String> = (0..5).map(|i| format!("npc{i}")).collect();
    names.push("x".repeat(101));
    assert!(
        validate_addressed_to(&names).is_err(),
        "oversized name within valid count should be rejected"
    );
}
