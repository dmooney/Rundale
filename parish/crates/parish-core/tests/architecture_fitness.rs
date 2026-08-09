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
/// (desktop), and parish-engine (headless) must all consume the same
/// game logic.
const BACKEND_AGNOSTIC: &[&str] = &[
    "parish-types",
    "parish-config",
    "parish-input",
    "parish-world",
    "parish-palette",
    "parish-npc",
    "parish-mod",
    "parish-editor",
    "parish-chronicle",
    "parish-diagnostics",
    "parish-providers",
    "parish-setup",
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
    // OS keychain — desktop-only secret storage. Backend-agnostic crates use
    // the `parish_core::secret_store::SecretStore` trait; the keyring-backed
    // impl lives in `parish-tauri`.
    "keyring",
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
fn parish_engine_does_not_duplicate_parish_core_modules() {
    let ws = workspace_root();

    // Top-level modules `parish-engine/src/` is allowed to define — these
    // are binary-specific glue, not shared logic.
    const CLI_ONLY: &[&str] = &[
        "main", "lib", "app", "config", "debug", "headless", "testing",
    ];

    let core_mods = list_top_level_modules(&ws.join("crates/parish-core/src"));
    let cli_mods = list_top_level_modules(&ws.join("crates/parish-engine/src"));

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
        "Module ownership violation — parish-engine/src/{{{}}} duplicate(s) of \
         module(s) under parish-core/src/.\n\n\
         FIX: extend the leaf crate (parish-config / parish-inference / \
         parish-input / parish-npc / parish-persistence / parish-world / \
         parish-types) or parish-core itself, then rely on \
         `pub use parish_core::*` in parish-engine/src/lib.rs. See CLAUDE.md \
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

/// Keep regression inputs distinct from one-off proof transcripts. A file in
/// `testing/fixtures` is swept by CI and therefore claims to be a regression;
/// legacy regressions use the `test_*.txt` convention, while new coverage must
/// use the asserted YAML schema under `testing/scenarios`.
#[test]
fn gameplay_test_corpus_separates_regressions_from_proofs() {
    let ws = workspace_root();
    let fixtures = ws.join("testing/fixtures");
    let mut misplaced = fs::read_dir(&fixtures)
        .expect("read testing/fixtures")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("test_"))
        })
        .collect::<Vec<_>>();
    misplaced.sort();

    assert!(
        misplaced.is_empty(),
        "Truthful test automation violation — non-regression scripts are in \
         testing/fixtures:\n  - {}\n\n\
         FIX: new machine-asserted coverage belongs in testing/scenarios/*.yaml; \
         one-off play/proof scripts belong in testing/proofs/. See AGENTS.md \
         rule #10 and parish/testing/AGENTS.md.",
        misplaced
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}

/// Sensor for the narration-drift class behind #1156.
///
/// Player-facing minute counts must be pluralized through the shared
/// helper `parish_types::minute_word` (or, for the dependency-free
/// `parish-client`, its local equivalent) rather than hand-written as a
/// `format!("... {} minutes ...")` literal with no singular branch. The
/// original bug shipped *four* independent hand-rolled copies of
/// "{} minutes" — one of them in `parish-engine`'s headless harness, a
/// copy that had silently diverged from the shared `/wait` command. Each
/// copy is a place a "1 minutes on foot" defect can reappear, exactly the
/// copy-paste drift CLAUDE.md rule #12 forbids.
///
/// The check is a cheap textual sensor: it flags any format-placeholder
/// (`{}`, `{:>2}`, `{n}`, …) immediately followed by the word
/// `minute`/`minutes` in a workspace `src/` file. Routing the count
/// through a `minute_word(n)`-style helper (which emits `{} {}`) clears it.
#[test]
fn minute_counts_pluralize_through_helper() {
    let ws = workspace_root();

    // A format placeholder directly followed by the bare unit word — the
    // hand-rolled-pluralization smell. `{}min` (the abbreviated exits
    // hint) and `minute_word` itself are deliberately not matched.
    let re = regex::Regex::new(r"\{[^{}]*\}\s+minutes?\b").expect("static regex compiles");

    let mut violations: Vec<String> = Vec::new();
    for entry in fs::read_dir(ws.join("crates")).expect("read crates/") {
        let src = entry.expect("entry").path().join("src");
        if !src.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        walk_rs_files(&src, &mut files);
        for f in files {
            let body = fs::read_to_string(&f).unwrap_or_default();
            for (i, raw_line) in body.lines().enumerate() {
                // Strip line/doc comments — the common false-positive source.
                let line = raw_line.split("//").next().unwrap_or("");
                if re.is_match(line) {
                    let pretty = f
                        .strip_prefix(&ws)
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| f.display().to_string());
                    violations.push(format!("{pretty}:{}", i + 1));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Hand-rolled minute pluralization — `{{}} minutes` literal(s) with no \
         singular branch (the #1156 defect class):\n  - {}\n\n\
         FIX: pluralize through `parish_types::minute_word(n)` (re-exported as \
         `parish_core::world::time::minute_word`), which returns \"minute\" for \
         1 and \"minutes\" otherwise — emit `({{}} {{}})` with the count and \
         `minute_word(count)`. `parish-client` is dependency-free by design and \
         carries its own private `minute_word`. Duplicating the format literal \
         instead lets a \"1 minutes on foot\" bug reappear in each copy. See \
         CLAUDE.md rule #12 (no copy-pasted narration across entry points).",
        violations.join("\n  - "),
    );
}

/// `(src-relative path, Command variant)` pairs that are knowingly handled
/// inside an entry-point crate instead of delegating to the shared
/// `parish_core::ipc::commands::handle_command`. Every entry MUST cite why
/// the local handling exists and a tracking issue for its removal.
///
/// Empty since #1159: the world-advance pump (weather tick, schedule→narration,
/// banshee, gossip, tier-4) that every runtime drives now lives in exactly one
/// place — `parish_core::game_loop::advance_world`. The synchronous
/// CLI/script/test harness runs it once per turn from `GameTestHarness::execute`
/// and delegates `Wait`/`Tick` to the shared `handle_command`, so no
/// entry-point crate re-implements a command's orchestration body.
const ALLOWED_LOCAL_COMMAND_HANDLERS: &[(&str, &str)] = &[];

/// Rule #12 structural sensor: cross-runtime orchestration belongs in
/// `parish-core`, parameterized over runtime concerns via traits — entry-point
/// crates are limited to thin wiring.
///
/// The canonical player-command dispatcher is
/// `parish_core::ipc::commands::handle_command`, which exhaustively matches
/// every `Command` variant once. Any `Command::<Variant> =>` match arm in an
/// entry-point crate (`parish-engine`, `parish-server`, `parish-tauri`) is a
/// second, divergent implementation of an orchestration body — the exact
/// copy-paste drift that let #1156's "1 minutes" survive in the headless
/// harness and that #687/#696 warn about. Entry points must call the shared
/// handler and dispatch its returned effects, not re-handle commands inline.
#[test]
fn entry_point_crates_do_not_reimplement_shared_commands() {
    let ws = workspace_root();
    const ENTRY_POINTS: &[&str] = &["parish-engine", "parish-server", "parish-tauri"];

    // `Command::Variant` optionally with a tuple/struct payload, then `=>` on
    // the same arm — i.e. a match arm, not a `Command::Wait(60)` construction
    // (those terminate in `;`/`,`/`)` before any `=>`).
    let re = regex::Regex::new(r"Command::([A-Z][A-Za-z0-9_]*)[^;\n]*=>")
        .expect("static regex compiles");

    let mut violations: Vec<String> = Vec::new();
    for crate_name in ENTRY_POINTS {
        let src = ws.join("crates").join(crate_name).join("src");
        let mut files = Vec::new();
        walk_rs_files(&src, &mut files);
        for f in files {
            let rel = f
                .strip_prefix(&ws)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| f.display().to_string());
            let body = fs::read_to_string(&f).unwrap_or_default();
            for (i, raw_line) in body.lines().enumerate() {
                let line = raw_line.split("//").next().unwrap_or("");
                for cap in re.captures_iter(line) {
                    let variant = &cap[1];
                    if ALLOWED_LOCAL_COMMAND_HANDLERS
                        .iter()
                        .any(|(p, v)| rel.replace('\\', "/") == *p && v == &variant)
                    {
                        continue;
                    }
                    violations.push(format!("{rel}:{} — Command::{variant}", i + 1));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Rule #12 violation — entry-point crate(s) re-implement a shared \
         command handler instead of delegating:\n  - {}\n\n\
         FIX: route the command through \
         `parish_core::ipc::commands::handle_command` and dispatch the returned \
         `CommandEffect`s; put any shared orchestration body, constant, or \
         payload struct in `parish-core` parameterized over `EventEmitter`. \
         Copy-pasting an orchestration arm into a second entry point produces \
         invisible drift (#687, #696; it is how #1156 reached the headless \
         harness). If the local handling is genuinely unavoidable, add it to \
         ALLOWED_LOCAL_COMMAND_HANDLERS with a justification and a tracking \
         issue. See CLAUDE.md rule #12 and docs/agent/architecture.md.",
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
