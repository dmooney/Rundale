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
//! ## CLI coverage (issue #1174)
//!
//! The CLI (`parish-engine`) runs the same `parish-core` game loop and exposes
//! game commands as slash-commands typed at the prompt rather than as HTTP
//! routes or Tauri IPC calls, so it is not a third *route* registry to diff.
//!
//! What it DOES share with the other two entry points is the
//! [`parish_core::game_loop::system_command::SystemCommandHost`] trait — the
//! backend-specific dispatcher for every `CommandEffect`. All three production
//! hosts (`AppStateCommandHost`, `TauriCommandHost`, `CliCommandHost`) must
//! implement every effect handler, or a backend silently inherits a trait
//! default no-op and drifts. `system_command_host_parity` (below) enforces
//! that by static source-scan, closing the CLI/headless gap.

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

#[test]
fn advertised_system_commands_are_recognized_before_gameplay_dispatch() {
    for input in ["/load main", "/irish", "/new-game"] {
        assert!(
            matches!(
                parish_core::input::classify_input(input),
                parish_core::input::InputResult::SystemCommand(_)
            ),
            "advertised command {input} must never fall through to NPC inference"
        );
    }
}

#[test]
fn gui_load_command_adapters_route_named_and_bare_forms() {
    let ws = workspace_root();
    for (runtime, relative_path, delegate) in [
        (
            "server",
            "crates/parish-server/src/command_host.rs",
            "do_load_named_branch_inner",
        ),
        (
            "Tauri",
            "crates/parish-tauri/src/command_host.rs",
            "do_load_named_branch",
        ),
    ] {
        let source = fs::read_to_string(ws.join(relative_path)).unwrap();
        assert!(
            source.contains("if name.trim().is_empty()"),
            "{runtime} must preserve bare /load as the save picker"
        );
        assert!(
            source.contains(delegate),
            "{runtime} must delegate named /load to its transactional loader"
        );
    }
}

// ── SystemCommandHost wiring parity (issue #1174) ───────────────────────────────
//
// The CLI/headless path has no HTTP/IPC registry to diff, but it shares the
// `SystemCommandHost` trait with the server and Tauri. Two of that trait's
// effect handlers carry default impls (`reset_byok`, `inference_log_toggle`),
// so a backend can silently inherit a no-op and drift. This test asserts each
// production host overrides every effect handler, with an explicit allowlist
// for intentional default-reliance.

/// Effect-handler methods every production `SystemCommandHost` is expected to
/// override. Excludes `run_command` (the dispatcher entry point, not a
/// per-effect handler). Kept in sync with the trait by
/// `host_method_set_matches_trait` below — adding/removing a trait method
/// without updating this list fails that test.
const EXPECTED_HOST_METHODS: &[&str] = &[
    "quit",
    "rebuild_inference",
    "toggle_map",
    "open_designer",
    "save_game",
    "fork_branch",
    "load_branch",
    "list_branches",
    "show_log",
    "show_spinner",
    "new_game",
    "save_flags",
    "apply_theme",
    "apply_tiles",
    "handle_debug",
    "reset_byok",
    "inference_log_toggle",
    "emit_text_log",
    "emit_world_update",
    "emit_command_echo",
    "is_echo_commands_disabled",
];

/// The three production entry-point hosts (crate label, source path relative to
/// the workspace root). The `MockHost` test double in `system_command.rs` is
/// intentionally excluded — it is not an entry point.
const PRODUCTION_HOSTS: &[(&str, &str)] = &[
    ("parish-server", "crates/parish-server/src/command_host.rs"),
    ("parish-tauri", "crates/parish-tauri/src/command_host.rs"),
    ("parish-engine", "crates/parish-engine/src/command_host.rs"),
];

/// `(crate, method)` pairs that intentionally inherit the trait's default
/// implementation instead of overriding it. Every entry needs a comment
/// justifying why the default is correct for that backend. Anything NOT listed
/// here must be overridden; anything listed here that IS overridden is flagged
/// as a stale exception (see `system_command_host_parity`).
const DEFAULT_RELIANT_ALLOWLIST: &[(&str, &str)] = &[
    // `reset_byok` re-opens the BYOK provider picker. Only the Tauri desktop
    // has a provider-picker overlay to re-open, so it is the sole override.
    // The web server and headless CLI currently inherit the no-op default;
    // wiring real BYOK-reset there is a runtime feature tracked separately
    // (it needs its own live proof). Listing them here makes the gap an
    // explicit, reviewed exception rather than silent drift.
    ("parish-server", "reset_byok"),
    ("parish-engine", "reset_byok"),
    // `emit_command_echo` and `is_echo_commands_disabled` support the
    // #1423 command-echo feature (display slash commands in the transcript).
    // The headless CLI REPL already prints the raw input at the prompt, so
    // echoing into a text-log event would double-print; the default no-op is
    // the correct behaviour there. GUI backends (Tauri, server) emit the
    // player+command payload and check the flag — both override these methods.
    ("parish-engine", "emit_command_echo"),
    ("parish-engine", "is_echo_commands_disabled"),
];

/// Extract the `fn <name>` declarations inside the `SystemCommandHost` trait
/// block (brace-matched), excluding `run_command`.
fn system_command_host_trait_methods(src: &str) -> BTreeSet<String> {
    let mut methods = BTreeSet::new();
    let Some(start) = src.find("trait SystemCommandHost") else {
        return methods;
    };
    let mut depth: i32 = 0;
    let mut entered = false;
    for raw_line in src[start..].lines() {
        // Strip line/doc comments first so braces or a `fn ...` mentioned
        // inside `//`/`///` can't corrupt depth tracking or yield a phantom
        // method name.
        let line = raw_line.split("//").next().unwrap_or("");
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    entered = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        // Only collect `fn` decls at trait-body depth (1).
        if entered
            && depth >= 1
            && let Some(name) = fn_name_on_line(line)
            && name != "run_command"
        {
            methods.insert(name);
        }
        if entered && depth <= 0 {
            break; // end of the trait block
        }
    }
    methods
}

/// If `line` declares a function, return its name. Matches `fn <name>(` and
/// `fn <name> (`; ignores the rest of the signature (which may wrap).
fn fn_name_on_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after_fn = trimmed
        .strip_prefix("fn ")
        .or_else(|| trimmed.strip_prefix("pub fn "))
        .or_else(|| trimmed.strip_prefix("async fn "))?;
    let name: String = after_fn
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

/// True if `src` (an impl file) overrides `method` (declares `fn <method>(`).
///
/// Line comments are stripped first so a commented-out or merely-mentioned
/// `// fn <method>(` is not mistaken for a real override.
fn impl_overrides(src: &str, method: &str) -> bool {
    let needle_a = format!("fn {method}(");
    let needle_b = format!("fn {method} (");
    src.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .any(|l| l.contains(&needle_a) || l.contains(&needle_b))
}

#[test]
fn host_method_set_matches_trait() {
    let ws = workspace_root();
    let trait_src = ws.join("crates/parish-core/src/game_loop/system_command.rs");
    let src = fs::read_to_string(&trait_src)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", trait_src.display()));

    let actual = system_command_host_trait_methods(&src);
    let expected: BTreeSet<String> = EXPECTED_HOST_METHODS
        .iter()
        .map(|s| s.to_string())
        .collect();

    let missing: Vec<&String> = expected.difference(&actual).collect();
    let extra: Vec<&String> = actual.difference(&expected).collect();

    assert!(
        missing.is_empty() && extra.is_empty(),
        "EXPECTED_HOST_METHODS is out of sync with the SystemCommandHost trait \
         (issue #1174).\n  In the list but not the trait: {missing:?}\n  \
         In the trait but not the list: {extra:?}\n\n\
         FIX: update EXPECTED_HOST_METHODS in wiring_parity.rs to match the \
         trait's effect-handler methods (everything except `run_command`). If \
         you added a handler, also wire it into all three production hosts \
         (or add a justified DEFAULT_RELIANT_ALLOWLIST entry).",
    );
}

#[test]
fn system_command_host_parity() {
    let ws = workspace_root();
    let allow: BTreeSet<(&str, &str)> = DEFAULT_RELIANT_ALLOWLIST.iter().copied().collect();

    let mut violations: Vec<String> = Vec::new();
    let mut stale_allowlist: Vec<String> = Vec::new();

    for (krate, rel_path) in PRODUCTION_HOSTS {
        let path = ws.join(rel_path);
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));

        for method in EXPECTED_HOST_METHODS {
            let overridden = impl_overrides(&src, method);
            let allowlisted = allow.contains(&(*krate, *method));

            if !overridden && !allowlisted {
                violations.push(format!(
                    "{krate} does not override `SystemCommandHost::{method}` \
                     — it silently inherits the trait default.\n        FIX: \
                     add `fn {method}(...)` to {rel_path}, or add \
                     (\"{krate}\", \"{method}\") to DEFAULT_RELIANT_ALLOWLIST \
                     with a justifying comment."
                ));
            }
            if overridden && allowlisted {
                stale_allowlist.push(format!(
                    "({krate}, {method}) is in DEFAULT_RELIANT_ALLOWLIST but \
                     {krate} DOES override it — remove the stale exception."
                ));
            }
        }
    }

    let mut msg = String::new();
    if !violations.is_empty() {
        msg.push_str(&format!(
            "Wiring-parity violation — a SystemCommandHost effect is not wired \
             from every entry point (issue #1174):\n    - {}\n",
            violations.join("\n    - ")
        ));
    }
    if !stale_allowlist.is_empty() {
        msg.push_str(&format!(
            "\nStale allowlist entries:\n    - {}\n",
            stale_allowlist.join("\n    - ")
        ));
    }
    assert!(
        violations.is_empty() && stale_allowlist.is_empty(),
        "{msg}\nEvery CommandEffect must be handled identically across the web \
         server, Tauri desktop, and headless CLI. See CLAUDE.md §Mode parity \
         and docs/agent/architecture.md.",
    );
}

#[test]
fn fn_name_on_line_extracts_names() {
    assert_eq!(
        fn_name_on_line("    fn quit(&self) -> Foo {").as_deref(),
        Some("quit")
    );
    assert_eq!(
        fn_name_on_line("    fn inference_log_toggle(").as_deref(),
        Some("inference_log_toggle")
    );
    assert_eq!(fn_name_on_line("    // not a fn").as_deref(), None);
    assert_eq!(
        fn_name_on_line("pub fn helper() {").as_deref(),
        Some("helper")
    );
}

#[test]
fn impl_overrides_detects_declarations() {
    let src = "impl X {\n    fn save_game(&self) -> String { String::new() }\n}";
    assert!(impl_overrides(src, "save_game"));
    assert!(!impl_overrides(src, "reset_byok"));
    // A commented-out or merely-mentioned fn must NOT count as an override.
    assert!(!impl_overrides(
        "impl X {\n    // fn save_game(&self) {}\n}",
        "save_game"
    ));
}
