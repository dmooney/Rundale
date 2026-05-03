//! Wiring-parity fitness sensor — closes GitHub issue #732.
//!
//! Asserts that the Tauri command surface (`parish-tauri`) and the Axum HTTP
//! API surface (`parish-server`) expose the same logical set of IPC commands.
//! A handler that exists in one backend but not the other ships silently
//! without this gate.
//!
//! ## How it works
//!
//! Both backends declare a canonical list in their own source tree:
//!
//! - `parish-tauri/src/command_registry.rs` — `EXPECTED_COMMANDS: &[&str]`
//! - `parish-server/src/route_registry.rs`  — `EXPECTED_HTTP_ROUTES: &[&str]`
//!
//! This test reads both files at *test runtime* (not compile time) using
//! `CARGO_MANIFEST_DIR` to navigate from the `parish-core` crate root to
//! the workspace root, then applies a normalisation step:
//!
//! ```text
//! Tauri:  "get_world_snapshot"      →  canonical: "get_world_snapshot"
//! Server: "/api/world-snapshot"     →  canonical: "get_world_snapshot"
//!         "/api/submit-input"       →  canonical: "submit_input"
//!         "/api/editor-list-mods"   →  canonical: "editor_list_mods"
//! ```
//!
//! The mapping rule is purely textual:
//!   1. Strip the `/api/` prefix from the HTTP path.
//!   2. Replace `-` with `_`.
//!   3. Prefix with the HTTP method as a label only when ambiguity would
//!      arise — currently all IPC routes are unambiguous so no prefix is
//!      needed; the rule exists for documentation.
//!
//! After normalisation both sets must be identical.  Any asymmetry is
//! reported with the canonical fix message.
//!
//! ## CLI coverage
//!
//! The CLI (`parish-cli`) runs the same `parish-core` game loop and exposes
//! game commands as slash-commands typed at the prompt rather than as HTTP
//! routes or Tauri IPC calls.  It is therefore not a third registry to
//! diff here — its coverage is verified indirectly by the shared `parish-core`
//! tests.  A separate tracking issue will add CLI-specific parity if a
//! dedicated command enumeration is introduced.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Returns the workspace root (two levels above `parish-core`'s crate root).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root is two levels above crate root")
        .to_path_buf()
}

/// Parse a `pub const NAME: &[&str] = &[ ... ];` literal from a Rust source
/// file and return the individual string values.
///
/// The parser is intentionally simple: it looks for lines that contain a
/// bare string literal (one or more `"..."` on the line, ignoring comments
/// and whitespace) between the opening `&[` and the closing `];`.  This is
/// robust enough for the well-structured registry files in this workspace.
fn parse_str_array_const(src: &str) -> Vec<String> {
    let mut inside = false;
    let mut values = Vec::new();

    for raw_line in src.lines() {
        // Strip trailing line comments.
        let line = raw_line.split("//").next().unwrap_or("").trim();

        if line.contains("&[") {
            inside = true;
        }
        if inside {
            // Extract all `"..."` string literals from this line.
            let mut rest = line;
            while let Some(start) = rest.find('"') {
                rest = &rest[start + 1..];
                if let Some(end) = rest.find('"') {
                    let val = &rest[..end];
                    if !val.is_empty() {
                        values.push(val.to_string());
                    }
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
            }
        }
        if inside && line.contains("];") {
            break;
        }
    }
    values
}

/// Convert a Tauri snake_case command name to the canonical logical name used
/// for comparison.
///
/// Tauri read-commands conventionally use a `get_` prefix which the HTTP
/// server drops from the route path (e.g. `get_world_snapshot` →
/// `/api/world-snapshot`).  Stripping the `get_` prefix here lets us
/// compare the two without special-casing every getter.
fn tauri_to_canonical(tauri_name: &str) -> String {
    tauri_name
        .strip_prefix("get_")
        .unwrap_or(tauri_name)
        .to_string()
}

/// Convert an HTTP `/api/kebab-case-path` route to the canonical snake_case
/// logical name used for comparison.
///
/// Rule: strip `/api/` prefix, then replace `-` with `_`.
fn http_to_canonical(http_path: &str) -> Option<String> {
    let stripped = http_path.strip_prefix("/api/")?;
    Some(stripped.replace('-', "_"))
}

#[test]
fn tauri_and_server_expose_the_same_ipc_commands() {
    let ws = workspace_root();

    // ── Read Tauri registry ───────────────────────────────────────────────
    let tauri_registry = ws.join("crates/parish-tauri/src/command_registry.rs");
    let tauri_src = fs::read_to_string(&tauri_registry).unwrap_or_else(|e| {
        panic!(
            "could not read {}: {e}\n\nFIX: ensure parish-tauri/src/command_registry.rs exists \
             and declares `pub const EXPECTED_COMMANDS: &[&str]`.",
            tauri_registry.display()
        )
    });
    let tauri_raw = parse_str_array_const(&tauri_src);
    assert!(
        !tauri_raw.is_empty(),
        "parish-tauri/src/command_registry.rs parsed zero entries — \
         EXPECTED_COMMANDS may be empty or the file format changed."
    );

    // ── Read server registry ──────────────────────────────────────────────
    let server_registry = ws.join("crates/parish-server/src/route_registry.rs");
    let server_src = fs::read_to_string(&server_registry).unwrap_or_else(|e| {
        panic!(
            "could not read {}: {e}\n\nFIX: ensure parish-server/src/route_registry.rs exists \
             and declares `pub const EXPECTED_HTTP_ROUTES: &[&str]`.",
            server_registry.display()
        )
    });
    let server_raw = parse_str_array_const(&server_src);
    assert!(
        !server_raw.is_empty(),
        "parish-server/src/route_registry.rs parsed zero entries — \
         EXPECTED_HTTP_ROUTES may be empty or the file format changed."
    );

    // ── Normalise to canonical names ──────────────────────────────────────
    let tauri_canonical: BTreeSet<String> =
        tauri_raw.iter().map(|s| tauri_to_canonical(s)).collect();

    let server_canonical: BTreeSet<String> = server_raw
        .iter()
        .filter_map(|s| http_to_canonical(s))
        .collect();

    // ── Compare ───────────────────────────────────────────────────────────
    let tauri_only: Vec<&String> = tauri_canonical.difference(&server_canonical).collect();
    let server_only: Vec<&String> = server_canonical.difference(&tauri_canonical).collect();

    let mut violations: Vec<String> = Vec::new();
    if !tauri_only.is_empty() {
        violations.push(format!(
            "Commands in Tauri but MISSING from server ({} total):\n    - {}",
            tauri_only.len(),
            tauri_only
                .iter()
                .map(|n| format!(
                    "{n}  →  add /api/{} to parish-server/src/lib.rs and route_registry.rs",
                    n.replace('_', "-")
                ))
                .collect::<Vec<_>>()
                .join("\n    - "),
        ));
    }
    if !server_only.is_empty() {
        violations.push(format!(
            "Routes in server but MISSING from Tauri ({} total):\n    - {}",
            server_only.len(),
            server_only
                .iter()
                .map(|n| format!(
                    "{n}  →  add \"{n}\" to parish-tauri/src/command_registry.rs and lib.rs",
                ))
                .collect::<Vec<_>>()
                .join("\n    - "),
        ));
    }

    assert!(
        violations.is_empty(),
        "Wiring-parity violation — Tauri and HTTP server expose different IPC \
         command sets (issue #732):\n\n{}\n\n\
         Every user-facing command must be registered in BOTH backends so that \
         the web and desktop experiences stay in sync.  See CLAUDE.md §Mode \
         parity and docs/agent/architecture.md.",
        violations.join("\n\n"),
    );
}

// ── Unit tests for the helpers ────────────────────────────────────────────────

#[test]
fn http_to_canonical_strips_prefix_and_replaces_hyphens() {
    assert_eq!(
        http_to_canonical("/api/world-snapshot").as_deref(),
        Some("world_snapshot")
    );
    assert_eq!(
        http_to_canonical("/api/editor-list-mods").as_deref(),
        Some("editor_list_mods")
    );
    assert_eq!(
        http_to_canonical("/api/submit-input").as_deref(),
        Some("submit_input")
    );
}

#[test]
fn http_to_canonical_rejects_non_api_paths() {
    assert!(http_to_canonical("/api/health").is_some()); // infra — excluded at registry level
    assert!(http_to_canonical("/auth/login").is_none()); // no /api/ prefix
    assert!(http_to_canonical("/metrics").is_none());
}

#[test]
fn tauri_to_canonical_strips_get_prefix() {
    assert_eq!(tauri_to_canonical("get_world_snapshot"), "world_snapshot");
    assert_eq!(tauri_to_canonical("get_save_state"), "save_state");
    // Non-getter commands are unchanged.
    assert_eq!(tauri_to_canonical("submit_input"), "submit_input");
    assert_eq!(tauri_to_canonical("editor_list_mods"), "editor_list_mods");
    assert_eq!(tauri_to_canonical("new_game"), "new_game");
}

#[test]
fn parse_str_array_const_extracts_values() {
    let src = r#"
pub const FOO: &[&str] = &[
    "alpha",
    "beta_gamma",   // comment
    "delta-epsilon",
];
"#;
    let vals = parse_str_array_const(src);
    assert_eq!(vals, vec!["alpha", "beta_gamma", "delta-epsilon"]);
}

#[test]
fn parse_str_array_const_handles_inline_comments() {
    let src = r#"
pub const BAR: &[&str] = &[
    // -- section header --
    "one",
    "two", // trailing comment
];
"#;
    let vals = parse_str_array_const(src);
    assert_eq!(vals, vec!["one", "two"]);
}
