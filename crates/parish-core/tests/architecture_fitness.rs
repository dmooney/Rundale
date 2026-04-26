//! Architecture-fitness sensors for the Parish workspace.
//!
//! These tests enforce the structural rules in `CLAUDE.md` /
//! `AGENTS.md` and `docs/agent/architecture.md` mechanically rather
//! than by convention. They run as part of `cargo test` (which `just
//! check` and CI both invoke) so any drift fails the gate locally
//! and in CI.
//!
//! Lessons applied from OpenAI's harness-engineering post:
//!
//! - **Computational sensors are cheap and fast** — these are textual
//!   checks against `Cargo.toml` and `src/` trees. They run in
//!   milliseconds and never call out to the network.
//! - **Custom error messages carry the self-correction hint** — every
//!   `assert!` message names the offending file, cites the rule (with
//!   the doc section the agent should consult), and gives the
//!   canonical fix.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    // tests run with `CARGO_MANIFEST_DIR` = parish-core's crate root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root is two levels above crate root")
        .to_path_buf()
}

/// Crates that must remain backend-agnostic — they may not directly
/// depend on any web/desktop/UI runtime crate. Adding a runtime dep
/// here breaks `mode parity`: parish-server (web), parish-tauri
/// (desktop), and parish-cli (headless) must all consume the same
/// game logic.
const BACKEND_AGNOSTIC: &[&str] = &[
    "parish-types",
    "parish-config",
    "parish-input",
    "parish-world",
    "parish-palette",
    "parish-npc",
    "parish-inference",
    "parish-persistence",
    "parish-core",
];

/// Dependency names that imply a particular runtime and therefore
/// must not appear in any `BACKEND_AGNOSTIC` crate. Only the wrapper
/// crates (`parish-server`, `parish-tauri`) are allowed to pull these.
const FORBIDDEN_FOR_BACKEND_AGNOSTIC: &[&str] = &[
    // Tauri (desktop)
    "tauri",
    "tauri-build",
    "wry",
    "tao",
    // Axum / Tower (web)
    "axum",
    "tower",
    "tower-http",
    "hyper",
    "hyper-util",
    // Frontend frameworks (none today, but reserve the slot)
    "leptos",
    "yew",
    "dioxus",
];

#[test]
fn backend_agnostic_crates_do_not_pull_runtime_deps() {
    let ws = workspace_root();
    let mut violations: Vec<String> = Vec::new();

    for crate_name in BACKEND_AGNOSTIC {
        let cargo_toml = ws.join("crates").join(crate_name).join("Cargo.toml");
        let body = fs::read_to_string(&cargo_toml)
            .unwrap_or_else(|e| panic!("read {}: {e}", cargo_toml.display()));
        let parsed: toml::Value = toml::from_str(&body).expect("parse Cargo.toml");

        for section in ["dependencies", "build-dependencies"] {
            let Some(deps) = parsed.get(section).and_then(|v| v.as_table()) else {
                continue;
            };
            for dep_name in deps.keys() {
                if FORBIDDEN_FOR_BACKEND_AGNOSTIC.contains(&dep_name.as_str()) {
                    violations.push(format!(
                        "{crate_name}/Cargo.toml [{section}] = `{dep_name}`",
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Architecture violation — backend-agnostic crates must not depend on \
         web/desktop runtime crates:\n  - {}\n\n\
         FIX: move the dependency (and the code that needs it) into \
         `parish-server` (web) or `parish-tauri` (desktop). The leaf logic \
         crates and `parish-core` must compose without binding to a runtime. \
         See CLAUDE.md §Mode parity and docs/agent/architecture.md.",
        violations.join("\n  - "),
    );
}

#[test]
fn parish_cli_does_not_duplicate_parish_core_modules() {
    let ws = workspace_root();

    // Top-level modules `parish-cli/src/` is allowed to define — these
    // are binary-specific glue, not shared logic.
    const CLI_ONLY: &[&str] = &[
        "main", "lib", "app", "config", "debug", "headless", "testing",
    ];

    let core_mods = list_top_level_modules(&ws.join("crates/parish-core/src"));
    let cli_mods = list_top_level_modules(&ws.join("crates/parish-cli/src"));

    let mut violations: Vec<String> = Vec::new();
    for m in &cli_mods {
        if CLI_ONLY.contains(&m.as_str()) {
            continue;
        }
        if core_mods.contains(m) {
            violations.push(m.clone());
        }
    }

    assert!(
        violations.is_empty(),
        "Module ownership violation — parish-cli/src/{{{}}} duplicate(s) of \
         module(s) under parish-core/src/.\n\n\
         FIX: extend the leaf crate (parish-config / parish-inference / \
         parish-input / parish-npc / parish-persistence / parish-world / \
         parish-types) or parish-core itself, then rely on \
         `pub use parish_core::*` in parish-cli/src/lib.rs. See CLAUDE.md \
         §Module ownership and docs/agent/architecture.md.",
        violations.join(", "),
    );
}

#[test]
fn no_orphaned_source_files() {
    let ws = workspace_root();
    let mut violations: Vec<String> = Vec::new();

    for entry in fs::read_dir(ws.join("crates")).expect("read crates/") {
        let crate_dir = entry.expect("entry").path();
        let src = crate_dir.join("src");
        if !src.is_dir() {
            continue;
        }

        // Build the set of `mod NAME` declarations that exist anywhere
        // in this crate's `src/`. A file's stem must appear in that set
        // for the file to be reachable from the build.
        let declared = collect_mod_declarations(&src);

        let mut files = Vec::new();
        walk_rs_files(&src, &mut files);
        for f in files {
            let stem = f
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            // Entry points are reachable by definition.
            if matches!(stem.as_str(), "lib" | "main" | "build" | "mod") {
                continue;
            }
            // `bin/*.rs` are declared as separate `[[bin]]` targets in
            // Cargo.toml, not via `mod` — exempt them.
            if f.components().any(|c| c.as_os_str() == "bin") {
                continue;
            }
            if !declared.contains(&stem) {
                let pretty = f
                    .strip_prefix(&ws)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| f.display().to_string());
                violations.push(pretty);
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Orphaned source file(s) — present on disk but not declared as `mod` \
         anywhere in their crate's src/ tree:\n  - {}\n\n\
         FIX: either add `mod NAME;` (or `pub mod NAME;`) in the parent \
         (lib.rs / main.rs / mod.rs / parent.rs) so the file is reachable, \
         or delete the file. Stale files commonly appear after extracting a \
         module into its own crate but forgetting to remove the original. \
         See CLAUDE.md §Module ownership.",
        violations.join("\n  - "),
    );
}

fn list_top_level_modules(src: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if !src.is_dir() {
        return out;
    }
    for entry in fs::read_dir(src).expect("read src/") {
        let path = entry.expect("entry").path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.is_file() {
            if let Some(stem) = name.strip_suffix(".rs")
                && !matches!(stem, "lib" | "main" | "build")
            {
                out.insert(stem.to_string());
            }
        } else if path.is_dir() {
            if matches!(name, "bin" | "tests" | "examples" | "benches") {
                continue;
            }
            out.insert(name.to_string());
        }
    }
    out
}

fn collect_mod_declarations(src: &Path) -> BTreeSet<String> {
    // Matches `mod NAME;`, `pub mod NAME;`, `pub(crate) mod NAME {`, etc.
    // The `\bmod\s+` anchor avoids false positives on identifiers that
    // happen to contain "mod" (e.g. `let modify = ...;`).
    let re = regex::Regex::new(r"\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*[{;]")
        .expect("static regex compiles");
    let mut out = BTreeSet::new();
    let mut files = Vec::new();
    walk_rs_files(src, &mut files);
    for f in files {
        let body = fs::read_to_string(&f).unwrap_or_default();
        for raw_line in body.lines() {
            // Strip line comments — they're the common false-positive source.
            let line = raw_line.split("//").next().unwrap_or("");
            for cap in re.captures_iter(line) {
                out.insert(cap[1].to_string());
            }
        }
    }
    out
}

fn walk_rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        if root.extension().is_some_and(|e| e == "rs") {
            out.push(root.to_path_buf());
        }
        return;
    }
    if !root.is_dir() {
        return;
    }
    for entry in fs::read_dir(root).expect("read dir") {
        walk_rs_files(&entry.expect("entry").path(), out);
    }
}
