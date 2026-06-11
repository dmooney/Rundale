//! Lock-order fitness sensor for `parish-server`'s `AppState` (#483, #1366 §2,
//! Rule 11).
//!
//! `AppState` holds many independent coordination `Mutex`es whose acquisition
//! order is the deadlock-avoidance invariant documented as the
//! [`parish_server::state::LOCK_ORDER`] const (single source of truth). This
//! test is the mechanical guard that the invariant is actually upheld in the
//! server handler / session / route bodies — the same harness-engineering
//! shape as `parish-core/tests/architecture_fitness.rs`:
//!
//! - **Cheap, fast, textual** — it greps `src/` line-by-line; no compilation,
//!   no runtime, no network.
//! - **The error message carries the fix** — every violation names the file,
//!   the offending pair, cites the rule (#483 / Rule 11), and points at
//!   `LOCK_ORDER` in `state.rs`.
//!
//! ## What counts as a violation
//!
//! A *nested* acquisition taken out of order: a lock guard that is still
//! **held** (bound with `let … = X.lock()…;` and not yet `drop`-ed or fallen
//! out of scope) when a **later** acquisition takes a lock that sorts
//! *earlier* in `LOCK_ORDER`. That is the pattern that can deadlock against a
//! path taking the two in canonical order.
//!
//! Take-and-release statements (`*X.lock().await = …;`,
//! `let v = X.lock().await.clone();`) acquire a *temporary* that is released
//! at the end of the statement, so they never nest and are not compared
//! against each other — only against guards that outlive them. This is why
//! the sensor is drop-aware rather than a naive line-adjacency check: the real
//! handlers legitimately do e.g. `*save_lock.lock() = …; *save_path.lock() =
//! …;` (two temporaries, out of `LOCK_ORDER` index order but never held
//! simultaneously) and `let c = config.lock(); …; drop(c); let g =
//! cloud_client.lock();` (acquire / drop / acquire), neither of which is a
//! deadlock hazard.
//!
//! Dissolved members of a group map to their group node before comparison: a
//! `client` / `cloud_client` / `inference_queue` reference maps to
//! `inference`; a `save_path` / `current_branch_id` / `current_branch_name`
//! reference maps to `save_identity` (#1366 §2).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use parish_server::state::LOCK_ORDER;

fn server_src() -> PathBuf {
    // `CARGO_MANIFEST_DIR` = parish-server's crate root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Maps a raw `AppState` field name appearing at a `.lock()` site to the
/// `LOCK_ORDER` node that governs its ordering. Dissolved group members map to
/// their group node; everything else maps to itself.
fn group_of(field: &str) -> &str {
    match field {
        "client" | "cloud_client" | "inference_queue" => "inference",
        "save_path" | "current_branch_id" | "current_branch_name" => "save_identity",
        other => other,
    }
}

/// The position of a node in the canonical chain, or `None` if the node is not
/// part of the ordered chain (and therefore not compared).
fn order_index(node: &str) -> Option<usize> {
    LOCK_ORDER.iter().position(|n| *n == node)
}

/// Field names the sensor recognises at a `.lock()` / `.try_lock()` site —
/// the chain nodes plus the dissolved group members. A lock on any other field
/// (e.g. `command_lock`, `setup_status`) is ignored.
fn known_lock_fields() -> BTreeSet<&'static str> {
    let mut s: BTreeSet<&'static str> = LOCK_ORDER.iter().copied().collect();
    for member in [
        "client",
        "cloud_client",
        "inference_queue",
        "save_path",
        "current_branch_id",
        "current_branch_name",
    ] {
        s.insert(member);
    }
    s
}

/// A single recognised lock acquisition found on a source line.
struct Acquire {
    line_no: usize,
    /// `LOCK_ORDER` node governing this acquisition.
    node: &'static str,
    /// `true` when the guard is bound by a `let … = …` that keeps it alive;
    /// `false` for a temporary (take-and-release) acquisition.
    bound: bool,
    /// `Some(binding)` when the guard is `let`-bound; used to honour an
    /// explicit `drop(binding)`.
    binding: Option<String>,
}

/// Extracts the recognised lock acquisitions on one source line, in order.
///
/// Recognises `<expr>.<field>.lock()` and `.try_lock()` where `<field>` is a
/// known chain node or dissolved member. Determines whether the guard is bound
/// (held) by looking for a leading `let <name> = …`. Multiple acquisitions on
/// the same physical line are returned left-to-right.
fn acquisitions_on_line(
    line_no: usize,
    line: &str,
    known: &BTreeSet<&'static str>,
) -> Vec<Acquire> {
    let mut out = Vec::new();

    // Each `.lock(` / `.try_lock(` occurrence is one acquisition. We find the
    // field name immediately preceding the `.` that introduces the call, and
    // the text after the call's `(` to classify held-vs-temporary.
    let mut search_from = 0usize;
    while search_from < line.len() {
        // Locate the next lock call and the byte index just past its `(`.
        let Some((dot_at, after_paren)) = next_lock_call(line, search_from) else {
            break;
        };
        // The field name is the last identifier segment before `dot_at`.
        let field = line[..dot_at]
            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
            .find(|seg| !seg.is_empty())
            .unwrap_or("");

        if let Some(&node_raw) = known.get(field) {
            let node = group_of(node_raw);
            // Node may be a dissolved member's group; only order chain nodes.
            if order_index(node).is_some() {
                // The acquisition is a *held* guard only when the binding IS
                // the guard: a `let <name> = <expr>.lock().await;` whose tail
                // (after the lock call, up to the statement `;`) is empty.
                // Anything that consumes the guard in the same statement —
                // `.lock().await.clone()`, `.is_some()`, `.unwrap_or(1)`,
                // `*x.lock().await = …`, `foo(x.lock().await)` — yields a
                // temporary released at the `;` and never nests.
                let binding = held_guard_binding(line, &line[after_paren..]);
                out.push(Acquire {
                    line_no,
                    node,
                    bound: binding.is_some(),
                    binding,
                });
            }
        }
        search_from = after_paren;
    }

    out
}

/// Finds the next `.lock(` / `.try_lock(` call at or after `from`. Returns
/// `(dot_index, index_just_past_open_paren)`, where `dot_index` is the `.`
/// that introduces the call (so the field name is the identifier ending there).
fn next_lock_call(line: &str, from: usize) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for token in [".lock(", ".try_lock("] {
        if let Some(rel) = line[from..].find(token) {
            let dot = from + rel;
            let past = dot + token.len();
            if best.is_none_or(|(bd, _)| dot < bd) {
                best = Some((dot, past));
            }
        }
    }
    best
}

/// Returns the guard binding name when this acquisition is *held* — i.e. the
/// line is `let <name> = <expr>.lock()[.await];` with nothing consuming the
/// guard before the `;`. `after` is the text following the opening `.lock(`.
///
/// Returns `None` for temporaries (the guard is dropped at statement end):
/// take-and-read (`.clone()`, `.is_some()`, `.unwrap_or(…)`), take-and-assign
/// (`*x.lock() = …`), or a lock passed into a call.
fn held_guard_binding(line: &str, after: &str) -> Option<String> {
    // Must be a `let [mut] <name> = …` statement.
    let t = line.trim_start();
    let rest = t.strip_prefix("let ")?;
    let rest = rest.strip_prefix("mut ").unwrap_or(rest);
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }

    // A `let x = *<expr>.lock()…;` deref-copies the guarded value out (the
    // guarded type is `Copy`, e.g. `Option<i64>`); the binding is the copied
    // value, not the guard, which is released at the `;`. Detect the `*` that
    // immediately follows the `=`.
    if let Some(eq) = rest.find('=')
        && rest[eq + 1..].trim_start().starts_with('*')
    {
        return None;
    }

    // The `.lock(` we matched is closed by its `)`. The tail after that close
    // paren must be only `.await` and whitespace before the `;` — otherwise the
    // guard is consumed (method call / field access) and released immediately.
    let close = after.find(')')?;
    let mut tail = after[close + 1..].trim();
    if let Some(stripped) = tail.strip_prefix(".await") {
        tail = stripped.trim_start();
    }
    let tail = tail.strip_suffix(';').unwrap_or(tail).trim();
    if tail.is_empty() { Some(name) } else { None }
}

/// Scans one file for nested out-of-order acquisitions. Returns violation
/// strings. `rel` is the display path used in messages.
fn scan_source(rel: &str, body: &str, known: &BTreeSet<&'static str>) -> Vec<String> {
    let mut violations = Vec::new();
    // Held `let`-bound guards, each tagged with the brace-nesting depth at
    // which it was acquired. A guard is alive until execution leaves the block
    // that introduced it — i.e. until the brace depth drops *below* its
    // acquisition depth. Modelling Rust's lexical drop this way is what keeps
    // legitimately-scoped reads (`let s = { let world = …; … };`) from looking
    // like cross-scope nested holds.
    struct Held {
        binding: String,
        node: &'static str,
        line: usize,
        depth: usize,
    }
    let mut held: Vec<Held> = Vec::new();
    // Brace depth at the *start* of the current line.
    let mut depth: usize = 0;

    for (idx, raw_line) in body.lines().enumerate() {
        let line_no = idx + 1;
        // Drop line comments — the usual false-positive source. (Lock idioms
        // never put `.lock()` after `//` in this codebase.)
        let code = raw_line.split("//").next().unwrap_or("");

        // 1. Explicit `drop(name)` releases first so a drop+reacquire on later
        //    lines doesn't look like a nested inversion.
        if let Some(dropped) = explicit_drop_target(code) {
            held.retain(|h| h.binding != dropped);
        }

        // 2. Examine acquisitions on this line against currently-held guards,
        //    then record any new `let`-bound guard at the current depth.
        for acq in acquisitions_on_line(line_no, code, known) {
            let new_idx = order_index(acq.node).expect("checked in acquisitions_on_line");
            for h in &held {
                let held_idx = order_index(h.node).expect("held nodes are chain nodes");
                if held_idx > new_idx {
                    violations.push(format!(
                        "{rel}: holds `{}` (acquired line {}, guard `{}`) then acquires \
                         `{}` at line {} — inverts the canonical chain. FIX: acquire \
                         \"{}\" before \"{}\"; see `LOCK_ORDER` in state.rs (Rule 11 / #483).",
                        h.node, h.line, h.binding, acq.node, acq.line_no, acq.node, h.node,
                    ));
                }
            }
            if acq.bound {
                held.push(Held {
                    binding: acq.binding.clone().unwrap_or_default(),
                    node: acq.node,
                    line: acq.line_no,
                    depth,
                });
            }
        }

        // 3. Update brace depth for the next line and evict guards whose
        //    introducing block has closed. We compute the running depth char by
        //    char so a `}` that closes a guard's block on the *same* line drops
        //    it before the next line's acquisitions are compared.
        for ch in code.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    held.retain(|h| h.depth <= depth);
                }
                _ => {}
            }
        }
    }

    violations
}

/// Returns the binding name in a `drop(<name>);` statement, if any.
fn explicit_drop_target(code: &str) -> Option<String> {
    let t = code.trim_start();
    let rest = t.strip_prefix("drop(")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
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

/// C4: the sensor is green on the clean tree.
#[test]
fn appstate_locks_acquired_in_canonical_order() {
    let known = known_lock_fields();
    let src = server_src();
    let mut files = Vec::new();
    walk_rs_files(&src, &mut files);

    let mut violations = Vec::new();
    for f in files {
        // The struct/group definitions in state.rs are not acquisition sites.
        let rel = f
            .strip_prefix(src.parent().unwrap())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| f.display().to_string());
        let body = fs::read_to_string(&f).unwrap_or_default();
        violations.extend(scan_source(&rel, &body, &known));
    }

    assert!(
        violations.is_empty(),
        "Lock-order violation(s) — a held `AppState` guard is still alive when a \
         lock earlier in the canonical chain is acquired (the #483 deadlock \
         hazard):\n  - {}\n\n\
         The canonical order is the `LOCK_ORDER` const in \
         `parish-server/src/state.rs`. Acquire chain locks in that order and \
         drop each guard before taking one that sorts earlier. The inference \
         group (`client`/`cloud_client`/`inference_queue`) and the \
         save-identity group (`save_path`/`current_branch_id`/\
         `current_branch_name`) are each one node in the chain (#1366 §2).",
        violations.join("\n  - "),
    );
}

/// C5 (negative proof / permanent regression guard): the sensor *flags* an
/// inverted nested acquisition. We run the scanner against a hardcoded bad
/// snippet so the negative case is guarded forever without leaving a real
/// inversion in the tree.
#[test]
fn sensor_rejects_inverted_pair() {
    let known = known_lock_fields();

    // `save_identity` (index 10) held while `world` (index 0) is acquired —
    // a textbook inversion of the canonical chain.
    let bad = r#"
async fn bad_handler(state: &AppState) {
    let id_guard = state.save_identity.current_branch_id.lock().await;
    let world = state.world.lock().await;
    let _ = (id_guard, world);
}
"#;
    let violations = scan_source("FIXTURE/bad_handler.rs", bad, &known);
    assert!(
        !violations.is_empty(),
        "sensor must flag the inverted save_identity→world acquisition; got no violations",
    );
    let msg = &violations[0];
    assert!(
        msg.contains("save_identity") && msg.contains("world"),
        "violation must name both offending nodes: {msg}",
    );
    assert!(
        msg.contains("LOCK_ORDER") && (msg.contains("#483") || msg.contains("Rule 11")),
        "violation must cite the canonical-fix hint (LOCK_ORDER + rule): {msg}",
    );

    // And a *dissolved member* of the save-identity group maps to the same
    // node, so a member-level inversion is caught too.
    let bad_member = r#"
async fn bad_member(state: &AppState) {
    let p = state.save_path.lock().await;
    let cfg = state.config.lock().await;
    let _ = (p, cfg);
}
"#;
    let v2 = scan_source("FIXTURE/bad_member.rs", bad_member, &known);
    assert!(
        v2.iter()
            .any(|m| m.contains("save_identity") && m.contains("config")),
        "member-level inversion (save_path held, config acquired) must be caught: {v2:?}",
    );

    // Drop-awareness: a textually later-then-earlier acquisition is NOT an
    // inversion when the first guard is explicitly dropped before the second
    // is taken. `inference` (index 4) acquired, dropped, then `config` (index
    // 3) — without the `drop` this would be held(4)→acquire(3) and flagged;
    // with it the two are never held together. This pins the drop tracking
    // that keeps the real `drop(config); cloud_client.lock()` idiom green.
    let good_drop = r#"
async fn good_drop(state: &AppState) {
    let g = state.inference.cloud_client.lock().await;
    drop(g);
    let cfg = state.config.lock().await;
    let _ = cfg;
}
"#;
    let v3 = scan_source("FIXTURE/good_drop.rs", good_drop, &known);
    assert!(
        v3.is_empty(),
        "drop-then-reacquire (inference dropped before config) is not an inversion: {v3:?}",
    );

    // Control: the SAME two acquisitions *without* the drop ARE flagged — this
    // proves the previous case passes because of drop-awareness, not because
    // the pair is harmless.
    let bad_no_drop = r#"
async fn bad_no_drop(state: &AppState) {
    let g = state.inference.cloud_client.lock().await;
    let cfg = state.config.lock().await;
    let _ = (g, cfg);
}
"#;
    let v3b = scan_source("FIXTURE/bad_no_drop.rs", bad_no_drop, &known);
    assert!(
        v3b.iter()
            .any(|m| m.contains("inference") && m.contains("config")),
        "holding `inference` while acquiring `config` (no drop) must be flagged: {v3b:?}",
    );

    // Two temporaries out of textual index order but never held together must
    // NOT be flagged (the real `*save_lock.lock()=…; *save_path.lock()=…;`
    // idiom in inference_setup.rs).
    let good_temps = r#"
async fn good_temps(state: &AppState) {
    *state.save_lock.lock().await = None;
    *state.save_identity.save_path.lock().await = None;
}
"#;
    let v4 = scan_source("FIXTURE/good_temps.rs", good_temps, &known);
    assert!(
        v4.is_empty(),
        "two release-at-statement-end temporaries never nest: {v4:?}",
    );
}
