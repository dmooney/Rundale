//! Inferential sensors for gameplay behavior.
//!
//! Two complementary checks per fixture:
//!
//! 1. **Snapshot baseline.** Run the fixture through the script harness,
//!    serialize the captured `Vec<ScriptResult>` to JSON, and diff against
//!    a stored baseline at `testing/evals/baselines/<fixture>.json`. Catches
//!    silent regressions where output drifts.
//! 2. **Structural rubric.** Apply invariants every fixture must satisfy
//!    (anachronisms empty, movement minutes positive, look descriptions
//!    non-empty). Catches whole categories of regression at once.
//!
//! Run `just baselines` (which sets `UPDATE_BASELINES=1`) to regenerate
//! baselines after an intentional gameplay change. The error message a
//! failing baseline emits walks you through the same flow.
//!
//! Lessons applied from OpenAI's harness-engineering post:
//!
//! - **Capture-on-green, diff-on-red.** Baselines are cheap, deterministic
//!   regression sensors that don't need an LLM judge.
//! - **Custom error messages carry the self-correction hint.** Each
//!   `assert!` names the fixture, the step, the rule that fired, and the
//!   canonical fix.

use parish::testing::{ActionResult, ScriptResult, run_script_captured};
use std::fs;
use std::path::{Path, PathBuf};

/// Fixtures whose structured output is deterministic enough to baseline.
/// Add new entries cautiously — non-deterministic NPC dialogue, weather
/// drift, or `/debug`-style HashMap-iteration ordering will produce flaky
/// failures. (Example: `test_grand_tour.txt` is great as a smoke test but
/// includes `/debug` which surfaces NPCs in HashMap order, so it's not a
/// stable baseline candidate today.)
const BASELINED_FIXTURES: &[&str] = &[
    "test_movement_errors",
    "test_walkthrough",
    "test_all_locations",
];

fn fixture_path(name: &str) -> PathBuf {
    Path::new("../../testing/fixtures").join(format!("{name}.txt"))
}

fn baseline_path(name: &str) -> PathBuf {
    Path::new("../../testing/evals/baselines").join(format!("{name}.json"))
}

fn capture(name: &str) -> Vec<ScriptResult> {
    let path = fixture_path(name);
    assert!(
        path.exists(),
        "fixture {} not found at {}",
        name,
        path.display()
    );
    run_script_captured(&path).expect("script execution")
}

fn updating_baselines() -> bool {
    matches!(std::env::var("UPDATE_BASELINES").as_deref(), Ok("1"))
}

fn check_or_update_baseline(name: &str) {
    let live = capture(name);
    let live_json = serde_json::to_string_pretty(&live).expect("serialize") + "\n";
    let baseline = baseline_path(name);

    if updating_baselines() {
        if let Some(parent) = baseline.parent() {
            fs::create_dir_all(parent).expect("create baselines/");
        }
        fs::write(&baseline, &live_json).expect("write baseline");
        eprintln!("baseline updated: {}", baseline.display());
        return;
    }

    let stored = fs::read_to_string(&baseline).unwrap_or_else(|_| {
        panic!(
            "missing baseline at {}.\n\nFIX: run `just baselines` (sets \
             UPDATE_BASELINES=1) after confirming the new fixture is correct.",
            baseline.display(),
        )
    });

    if stored != live_json {
        panic!(
            "fixture {name}: live output does not match baseline.\n\
             baseline: {}\n\n\
             FIX: if the drift is intentional (a deliberate gameplay change), \
             regenerate with `just baselines`. Otherwise, locate the \
             regression in your change. See docs/design/testing.md \
             §Eval baselines.\n\n\
             First diff window (live | baseline):\n{}",
            baseline.display(),
            first_diff_window(&live_json, &stored, 4),
        );
    }
}

fn first_diff_window(live: &str, baseline: &str, ctx: usize) -> String {
    let l: Vec<&str> = live.lines().collect();
    let b: Vec<&str> = baseline.lines().collect();
    for (i, (ll, bl)) in l.iter().zip(b.iter()).enumerate() {
        if ll != bl {
            let lo = i.saturating_sub(ctx);
            let mut out = String::new();
            for j in lo..i {
                out.push_str(&format!("  L{j}: {} | {}\n", l[j], b[j]));
            }
            out.push_str(&format!("> L{i}: {ll} | {bl}\n"));
            for j in (i + 1)..(i + 1 + ctx).min(l.len()).min(b.len()) {
                out.push_str(&format!("  L{j}: {} | {}\n", l[j], b[j]));
            }
            return out;
        }
    }
    if l.len() != b.len() {
        format!(
            "(line count differs: live={}, baseline={})",
            l.len(),
            b.len()
        )
    } else {
        "(no per-line diff found)".to_string()
    }
}

// ============================================================
// Snapshot baselines
// ============================================================

#[test]
fn baseline_test_movement_errors() {
    check_or_update_baseline("test_movement_errors");
}

#[test]
fn baseline_test_walkthrough() {
    check_or_update_baseline("test_walkthrough");
}

#[test]
fn baseline_test_all_locations() {
    check_or_update_baseline("test_all_locations");
}

// ============================================================
// Structural rubrics — apply to every baselined fixture
// ============================================================

#[test]
fn rubric_anachronisms_are_empty() {
    for name in BASELINED_FIXTURES {
        let results = capture(name);
        for (i, r) in results.iter().enumerate() {
            if let ActionResult::NpcResponse { anachronisms, .. } = &r.result {
                assert!(
                    anachronisms.is_empty(),
                    "{name}.txt step {i} (`{}`): anachronisms detected: {:?}\n\
                     FIX: rephrase the fixture command to avoid out-of-period \
                     words. See mods/rundale/anachronisms.json for the active \
                     dictionary.",
                    r.command,
                    anachronisms,
                );
            }
        }
    }
}

#[test]
fn rubric_movement_minutes_are_positive() {
    for name in BASELINED_FIXTURES {
        let results = capture(name);
        for (i, r) in results.iter().enumerate() {
            if let ActionResult::Moved { minutes, to, .. } = &r.result {
                assert!(
                    *minutes > 0,
                    "{name}.txt step {i} (`{}`): Moved to {to} with minutes=0.\n\
                     FIX: travel time must be positive. Check world graph \
                     edge weights and parish_world::movement::resolve_movement().",
                    r.command,
                );
            }
        }
    }
}

#[test]
fn rubric_look_descriptions_are_non_empty() {
    for name in BASELINED_FIXTURES {
        let results = capture(name);
        for (i, r) in results.iter().enumerate() {
            if let ActionResult::Looked { description } = &r.result {
                assert!(
                    !description.trim().is_empty(),
                    "{name}.txt step {i} (`{}`): empty Looked description.\n\
                     FIX: parish_world::description::render_description() is \
                     silently producing empty text — check the active mod's \
                     location data and the renderer's fallback path.",
                    r.command,
                );
            }
        }
    }
}
