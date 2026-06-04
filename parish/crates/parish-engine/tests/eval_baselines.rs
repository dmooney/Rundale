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

use parish_engine::npc::manager::TierTransition;
use parish_engine::npc::types::CogTier;
use parish_engine::testing::{ActionResult, GameTestHarness, ScriptResult, run_script_captured};
use parish_engine::world::LocationId;
use parish_types::events::GameEvent;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

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
    "test_address_absent_npc",
];

/// Cache of fixture results, populated once on first access.
/// Each fixture runs exactly once regardless of how many tests reference it.
static FIXTURE_CACHE: LazyLock<HashMap<&'static str, Vec<ScriptResult>>> = LazyLock::new(|| {
    BASELINED_FIXTURES
        .iter()
        .map(|&name| {
            let path = fixture_path(name);
            assert!(
                path.exists(),
                "fixture {} not found at {}",
                name,
                path.display()
            );
            let results = run_script_captured(&path).expect("script execution");
            (name, results)
        })
        .collect()
});

fn fixture_path(name: &str) -> PathBuf {
    Path::new("../../testing/fixtures").join(format!("{name}.txt"))
}

fn baseline_path(name: &str) -> PathBuf {
    Path::new("../../testing/evals/baselines").join(format!("{name}.json"))
}

fn capture(name: &str) -> &'static [ScriptResult] {
    FIXTURE_CACHE
        .get(name)
        .unwrap_or_else(|| panic!("fixture {name} not in FIXTURE_CACHE"))
        .as_slice()
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

/// Regression coverage for #985 — addressing an absent NPC must surface a
/// system "{name} is not here." message rather than letting a different
/// co-located NPC speak as a silent fallback.
#[test]
fn baseline_test_address_absent_npc() {
    check_or_update_baseline("test_address_absent_npc");
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

// ============================================================
// Text-quality rubrics — apply parish_npc::quality detectors to every
// baselined fixture. Three detectors are safe to assert against fixture
// output (player commands are scripted, NPC dialogue comes from the
// simulator Markov backend in fixtures):
//
// - `detect_json_envelope_leak`: fixtures never feed JSON-wrapped text;
//   firing means a regression in `extract_action_from_response`.
// - `detect_template_tokens`: simulator output has no template variables;
//   firing means a prompt template leaked into rendered text.
// - `detect_hallucinated_gaelic`: simulator English corpus has no fada
//   chars; firing means hallucinated Gaelic reached user-visible output.
//
// Skipped: simulator-corpus-overlap (would always fire on simulator
// output) and modern-register (soft signal, runtime WARN only).
// ============================================================

#[test]
fn rubric_no_json_envelope_leak_in_fixtures() {
    use parish_engine::npc::quality::detect_json_envelope_leak;
    for name in BASELINED_FIXTURES {
        let results = capture(name);
        for (i, r) in results.iter().enumerate() {
            let cmd_issues = detect_json_envelope_leak(&r.command);
            assert!(
                cmd_issues.is_empty(),
                "{name}.txt step {i}: player command leaks JSON envelope.\n\
                 Command: `{}`\n\
                 Issues: {:?}\n\
                 FIX: see parish_tauri::commands::extract_action_from_response.",
                r.command,
                cmd_issues,
            );
            if let ActionResult::NpcResponse { dialogue, npc, .. } = &r.result {
                let dlg_issues = detect_json_envelope_leak(dialogue);
                assert!(
                    dlg_issues.is_empty(),
                    "{name}.txt step {i}: NPC `{npc}` dialogue leaks JSON envelope.\n\
                     Dialogue: `{dialogue}`\n\
                     Issues: {:?}\n\
                     FIX: see parish_core::npc::parse_npc_stream_response.",
                    dlg_issues,
                );
            }
        }
    }
}

#[test]
fn rubric_no_template_tokens_in_fixtures() {
    use parish_engine::npc::quality::detect_template_tokens;
    for name in BASELINED_FIXTURES {
        let results = capture(name);
        for (i, r) in results.iter().enumerate() {
            let cmd_issues = detect_template_tokens(&r.command);
            assert!(
                cmd_issues.is_empty(),
                "{name}.txt step {i}: player command contains unfilled template tokens.\n\
                 Command: `{}`\n\
                 Issues: {:?}",
                r.command,
                cmd_issues,
            );
            if let ActionResult::NpcResponse { dialogue, npc, .. } = &r.result {
                let dlg_issues = detect_template_tokens(dialogue);
                assert!(
                    dlg_issues.is_empty(),
                    "{name}.txt step {i}: NPC `{npc}` dialogue contains unfilled template tokens.\n\
                     Dialogue: `{dialogue}`\n\
                     Issues: {:?}\n\
                     FIX: see mods/rundale/prompts/ for the template source.",
                    dlg_issues,
                );
            }
        }
    }
}

#[test]
fn rubric_no_hallucinated_gaelic_in_fixtures() {
    use parish_engine::npc::quality::detect_hallucinated_gaelic;
    for name in BASELINED_FIXTURES {
        let results = capture(name);
        for (i, r) in results.iter().enumerate() {
            if let ActionResult::NpcResponse { dialogue, npc, .. } = &r.result {
                let issues = detect_hallucinated_gaelic(dialogue);
                assert!(
                    issues.is_empty(),
                    "{name}.txt step {i}: NPC `{npc}` dialogue contains fada-bearing \
                     words not on the vetted Gaelic vocab list.\n\
                     Dialogue: `{dialogue}`\n\
                     Issues: {:?}\n\
                     FIX: add to parish_npc::quality::GAELIC_VOCAB if legitimate.",
                    issues,
                );
            }
        }
    }
}

// ============================================================
// Gameplay rubrics — Tier 4 CPU rules engine (#722)
// ============================================================

/// Asserts that at least one Tier 4 life event surfaces after advancing game
/// time past the 90-day tick threshold onto a festival date.
///
/// Strategy:
/// - Load the full Rundale world (real NPCs, real world graph).
/// - Subscribe to the event bus *before* advancing time so no events are lost.
/// - Advance exactly to Lughnasa (Aug 1, 1820): 134 days * 24 * 60 = 192 960
///   minutes from start (1820-03-20).  134 days > the 90-day tier4 threshold,
///   so the tick fires exactly once with `game_date = 1820-08-01`.
///   `Festival::check(Aug 1)` returns `Some(Lughnasa)` unconditionally —
///   no RNG involved — so at least one event is guaranteed.
/// - Assert the tier4 tick was recorded, the debug log shows it, and the
///   event bus received at least one GameEvent.
///
/// Fixture: `parish/testing/fixtures/test_tier4_far_npcs.txt`
#[test]
fn rubric_tier4_events_appear_in_journal() {
    let mut h = GameTestHarness::new();

    let tier4_count_before = h
        .app
        .npc_manager
        .npcs_in_tier(parish_engine::npc::types::CogTier::Tier4)
        .len();

    // Subscribe to the event bus BEFORE advancing time.
    let mut rx = h.app.world.event_bus.subscribe();

    // Advance exactly to Lughnasa (Aug 1): 134 days * 24 * 60 = 192 960 min.
    // 134 days > 90-day tier4 threshold from a None baseline, so the tick
    // fires with game_date = 1820-08-01 = Lughnasa.  Festival::check is pure
    // date math — no RNG — guaranteeing at least one FestivalDetected event.
    const MINUTES_TO_LUGHNASA: i64 = 134 * 24 * 60; // 192 960
    h.advance_time(MINUTES_TO_LUGHNASA);

    // Check 1: tier4 tick was recorded (last_tier4_game_time is Some).
    assert!(
        h.app.npc_manager.last_tier4_game_time().is_some(),
        "test_tier4_far_npcs: tier4 tick should have fired after advancing \
         {MINUTES_TO_LUGHNASA} game-minutes (~134 days to Lughnasa).\n\
         FIX: check NpcManager::needs_tier4_tick and GameTestHarness::advance_time \
         in parish/crates/parish-engine/src/testing.rs.\n\
         Tier 4 NPC count at test start: {tier4_count_before}."
    );

    // Check 2: the debug log contains at least one `[tier4]` entry.
    let tier4_log: Vec<&str> = h
        .debug_log()
        .into_iter()
        .filter(|line| line.contains("[tier4]"))
        .collect();
    assert!(
        !tier4_log.is_empty(),
        "test_tier4_far_npcs: expected at least one '[tier4] N events' debug entry \
         after the tick interval elapsed, but the debug log contained none.\n\
         Full debug log: {:?}",
        h.debug_log()
    );

    // Check 3: a FestivalStarted(Lughnasa) GameEvent was published on the bus.
    // tick_tier4 calls Festival::check(1820-08-01) = Some(Lughnasa) and emits
    // FestivalDetected; apply_tier4_events converts it to FestivalStarted.
    let mut lughnasa_fired = false;
    while let Ok(evt) = rx.try_recv() {
        if let GameEvent::FestivalStarted { name, .. } = &evt
            && name == "Lughnasa"
        {
            lughnasa_fired = true;
        }
    }
    assert!(
        lughnasa_fired,
        "test_tier4_far_npcs: expected a FestivalStarted(\"Lughnasa\") GameEvent \
         published on the tier4 tick with game_date = 1820-08-01, but it was absent.\n\
         Debug log (tier4 entries): {tier4_log:?}\n\
         FIX: verify tick_tier4 calls Festival::check(game_date) and \
         apply_tier4_events emits GameEvent::FestivalStarted for FestivalDetected events."
    );
}

// ============================================================
// Gameplay rubrics — Festival fixture (#720)
// ============================================================

/// Asserts that a `FestivalStarted { name: "Samhain" }` `GameEvent` is
/// published when the game clock is advanced to exactly November 1, 1820.
///
/// Strategy:
/// - Load the full Rundale world.
/// - Subscribe to the event bus *before* advancing time.
/// - Advance exactly to Samhain (Nov 1, 1820): 226 days * 24 * 60 = 325 440
///   minutes from start (1820-03-20).  226 days > 90-day tier4 threshold
///   (last_tier4 = None), so the tick fires once with `game_date = 1820-11-01`.
///   `Festival::check(Nov 1)` returns `Some(Samhain)` unconditionally.
/// - Drain the event bus and assert that a `FestivalStarted { "Samhain" }`
///   event was received.
///
/// Fixture: `parish/testing/fixtures/test_festival_samhain.txt`
#[test]
fn rubric_festival_event_published_on_festival_date() {
    let mut h = GameTestHarness::new();

    // Subscribe BEFORE advancing so no events are lost.
    let mut rx = h.app.world.event_bus.subscribe();

    // Advance exactly to Samhain (Nov 1): 226 days * 24 * 60 = 325 440 min.
    // 226 days > 90-day tier4 threshold from a None baseline, so the tick
    // fires with game_date = 1820-11-01 = Samhain.
    const MINUTES_TO_SAMHAIN: i64 = 226 * 24 * 60; // 325 440
    h.advance_time(MINUTES_TO_SAMHAIN);

    // Drain the bus and collect FestivalStarted events.
    let mut samhain_fired = false;
    while let Ok(evt) = rx.try_recv() {
        if let GameEvent::FestivalStarted { name, .. } = &evt
            && name == "Samhain"
        {
            samhain_fired = true;
        }
    }

    assert!(
        samhain_fired,
        "test_festival_samhain: expected a FestivalStarted(\"Samhain\") GameEvent \
         after advancing {MINUTES_TO_SAMHAIN} game-minutes (226 days) to land on \
         Samhain (Nov 1, 1820), but none was received.\n\
         FIX: verify that tick_tier4 calls Festival::check(game_date) where \
         game_date is the current clock date when the tier4 tick fires, that \
         apply_tier4_events converts FestivalDetected → GameEvent::FestivalStarted, \
         and that GameTestHarness::advance_time publishes tier4 events on \
         world.event_bus.\n\
         See: parish/crates/parish-npc/src/tier4.rs (tick_tier4) and \
         parish/crates/parish-npc/src/manager.rs (apply_tier4_events)."
    );
}

// ============================================================
// Gameplay rubrics — Tier promotion/demotion on proximity (#721)
// ============================================================

/// Asserts that NPCs promote to Tier 1 when the player moves co-located and
/// demote when the player moves away.
///
/// Strategy:
/// - Load the full Rundale world (real NPCs, real world graph).
/// - Player starts at Kilteevan Village (id=15).  The Walsh family (Eamon,
///   Kathleen, Ciaran — NpcIds 15/16/17) home at Boatman's Cottage (id=20),
///   BFS-distance 3 from Kilteevan.  Distance 3 <= tier3_max_distance(5), so
///   they start at Tier 3.  (The vanilla Rundale world has max graph-distance
///   4, so no NPC ever reaches Tier 4 from the default start.)
/// - Teleport the player to Boatman's Cottage by directly writing
///   `world.player_location`, then call `assign_tiers` to recompute distances.
///   Distance becomes 0 → tier1_max_distance(0), so Walsh NPCs promote to
///   Tier 1.  Assert at least one `TierTransition` carries `new_tier=Tier1`.
/// - Teleport back to Kilteevan Village and call `assign_tiers` again.
///   Distance becomes 3 → Tier 3 again.  Assert that the NPCs that promoted
///   now have a demotion transition back to their original tier.
///
/// This test uses direct field assignment + `assign_tiers` rather than script
/// harness movement so that NPC schedule ticks cannot relocate the Walsh
/// family between the assertion steps.
///
/// Fixture: `parish/testing/fixtures/test_tier_promotion.txt`
#[test]
fn rubric_tier_promotion_on_proximity() {
    let mut h = GameTestHarness::new();

    // --- 1. Verify starting state -----------------------------------------------

    // Player starts at Kilteevan Village.
    assert_eq!(
        h.app.world.player_location,
        LocationId(15),
        "rubric_tier_promotion_on_proximity: expected player to start at \
         Kilteevan Village (id=15).  Check the default mod start location."
    );

    // Walsh family (NpcIds 15, 16, 17) home at Boatman's Cottage (id=20),
    // BFS-distance 3 from Kilteevan.  Tier 3 range is d <= 5.
    let walsh_ids = [
        parish_engine::npc::NpcId(15), // Eamon Walsh
        parish_engine::npc::NpcId(16), // Kathleen Walsh
        parish_engine::npc::NpcId(17), // Ciaran Walsh
    ];

    for id in walsh_ids {
        let tier = h.app.npc_manager.tier_of(id).unwrap_or(CogTier::Tier4);
        assert!(
            matches!(tier, CogTier::Tier3 | CogTier::Tier4),
            "rubric_tier_promotion_on_proximity: NpcId({}) expected Tier3/Tier4 \
             at start (distance 3 from Kilteevan) but got {:?}.\n\
             FIX: verify initial tier assignment in GameTestHarness::new() and \
             that the Walsh NPCs' home location (id=20) is reachable from \
             Kilteevan (id=15) at the expected distance.",
            id.0,
            tier
        );
    }

    // --- 2. Teleport to Boatman's Cottage and check promotion -------------------

    // Move player to Boatman's Cottage (id=20, distance=0 from itself).
    // Distance 0 <= tier1_max_distance(0), so Walsh NPCs must promote to Tier 1.
    h.app.world.player_location = LocationId(20);
    let promotion_transitions = h.app.npc_manager.assign_tiers(&h.app.world, &[]);

    // At least one Walsh NPC must have a Tier1 promotion transition.
    let promoted_to_tier1: Vec<&TierTransition> = promotion_transitions
        .iter()
        .filter(|t| walsh_ids.contains(&t.npc_id) && t.new_tier == CogTier::Tier1 && t.promoted)
        .collect();

    assert!(
        !promoted_to_tier1.is_empty(),
        "rubric_tier_promotion_on_proximity: expected at least one Walsh NPC \
         (NpcIds 15/16/17) to promote to Tier 1 after player moved to \
         Boatman's Cottage (id=20), but no Tier1 promotion was returned by \
         assign_tiers.\n\
         Transitions returned: {:?}\n\
         FIX: check NpcManager::assign_tiers BFS logic in \
         parish/crates/parish-npc/src/manager.rs.  tier1_max_distance=0 means \
         distance 0 (same location) must map to CogTier::Tier1.",
        promotion_transitions
            .iter()
            .map(|t| format!("{:?} {:?}->{:?}", t.npc_id, t.old_tier, t.new_tier))
            .collect::<Vec<_>>()
    );

    // Confirm the tier map reflects Tier 1 after assign_tiers.
    for id in walsh_ids {
        let tier = h.app.npc_manager.tier_of(id).unwrap_or(CogTier::Tier4);
        assert_eq!(
            tier,
            CogTier::Tier1,
            "rubric_tier_promotion_on_proximity: NpcId({}) tier_of() reports {:?} \
             after player co-location; expected Tier1.\n\
             FIX: NpcManager::assign_tiers must update tier_assignments map for \
             every NPC, not just those with transitions.",
            id.0,
            tier
        );
    }

    // --- 3. Teleport back to Kilteevan and check demotion -----------------------

    h.app.world.player_location = LocationId(15);
    let demotion_transitions = h.app.npc_manager.assign_tiers(&h.app.world, &[]);

    // At least one Walsh NPC must demote (new_tier != Tier1, promoted=false).
    let demoted_from_tier1: Vec<&TierTransition> = demotion_transitions
        .iter()
        .filter(|t| {
            walsh_ids.contains(&t.npc_id)
                && t.old_tier == CogTier::Tier1
                && t.new_tier != CogTier::Tier1
                && !t.promoted
        })
        .collect();

    assert!(
        !demoted_from_tier1.is_empty(),
        "rubric_tier_promotion_on_proximity: expected at least one Walsh NPC \
         (NpcIds 15/16/17) to demote from Tier 1 after player returned to \
         Kilteevan Village (id=15), but no demotion transition was returned.\n\
         Transitions returned: {:?}\n\
         FIX: check NpcManager::assign_tiers demotion branch.  BFS distance \
         from Kilteevan (id=15) to Boatman's Cottage (id=20) is 3, which is \
         > tier1_max_distance(0) and <= tier3_max_distance(5).",
        demotion_transitions
            .iter()
            .map(|t| format!("{:?} {:?}->{:?}", t.npc_id, t.old_tier, t.new_tier))
            .collect::<Vec<_>>()
    );

    // Confirm the tier map reflects Tier 3 (not Tier 1) after demotion.
    for id in walsh_ids {
        let tier = h.app.npc_manager.tier_of(id).unwrap_or(CogTier::Tier4);
        assert!(
            tier != CogTier::Tier1,
            "rubric_tier_promotion_on_proximity: NpcId({}) tier_of() still \
             reports Tier1 after player moved back to Kilteevan — demotion did \
             not take effect.  Got {:?}.",
            id.0,
            tier
        );
    }
}

// ============================================================
// Demo-audit regression rubrics (issue #1175). The same demo-audit finding
// categories were fixed repeatedly instead of guarded: auto-player-never-moves
// (#1129/#1139/#1140), mid-conversation farewells, and mood→emoji mis-mapping.
// These three rubrics turn each finding into a deterministic guard.
// ============================================================

/// Negative moods must never render as a positive-affect emoji. Guards the
/// recurring mood→emoji mis-mapping — e.g. a positive-emoji branch shadowing a
/// negative mood after a reorder in `parish-npc/src/mood.rs`.
#[test]
fn rubric_mood_emoji_sign() {
    use parish_engine::npc::mood::mood_emoji;

    // Unambiguously positive-affect emoji produced by `mood_emoji`.
    const POSITIVE_EMOJI: &[&str] = &["😄", "😊", "🤗", "😆", "🔥"];
    // Unambiguously negative moods the LLM commonly emits.
    const NEGATIVE_MOODS: &[&str] = &[
        "bitter",
        "sharp",
        "angry",
        "furious",
        "afraid",
        "terrified",
        "anxious",
        "worried",
        "sad",
        "mournful",
        "irritated",
        "frustrated",
        "resentful",
        "grumpy",
        "sour",
        "suspicious",
        "caustic",
        "embittered",
    ];

    for mood in NEGATIVE_MOODS {
        let emoji = mood_emoji(mood);
        assert!(
            !POSITIVE_EMOJI.contains(&emoji),
            "rubric_mood_emoji_sign: negative mood `{mood}` maps to positive \
             emoji {emoji}.\n\
             FIX: in parish-npc/src/mood.rs, negative-mood branches must \
             precede any positive branch whose keywords overlap — a positive \
             branch is shadowing this mood."
        );
    }
}

/// The player can and does move. Guards the recurring auto-player-never-moves
/// finding (#1129/#1139/#1140): movement must be possible (the start location
/// has neighbours) AND a scripted multi-turn walkthrough must visit more than
/// one distinct location.
#[test]
fn rubric_demo_player_moves() {
    // 1. Movement is possible: the player's start has at least one neighbour.
    let h = GameTestHarness::new();
    let start = h.app.world.player_location;
    let neighbours = h.app.world.graph.neighbors(start);
    assert!(
        !neighbours.is_empty(),
        "rubric_demo_player_moves: player start location {start:?} has no \
         adjacent locations — movement is impossible.\n\
         FIX: check the active mod's world graph edges (mods/rundale/world.json)."
    );

    // 2. Movement actually happens: over a multi-turn walkthrough the player
    //    visits more than one distinct location.
    let results = capture("test_all_locations");
    let distinct: std::collections::HashSet<&str> =
        results.iter().map(|r| r.location.as_str()).collect();
    assert!(
        distinct.len() >= 2,
        "rubric_demo_player_moves: across {} turns of test_all_locations the \
         player never left `{}` ({} distinct location(s)).\n\
         FIX: movement is silently failing — check \
         parish_core::game_session::apply_movement and the `go`/`go to` parser path.",
        results.len(),
        results.first().map(|r| r.location.as_str()).unwrap_or("?"),
        distinct.len(),
    );
}

/// No NPC says a farewell before the conversation's final turn. Guards the
/// recurring mid-conversation-farewell finding: within each baselined fixture,
/// a farewell token may appear only in an NPC's last dialogue turn.
#[test]
fn rubric_no_midconversation_farewell() {
    fn is_farewell(text: &str) -> bool {
        let t = text.to_lowercase();
        ["slán abhaile", "slán leat", "goodbye", "farewell"]
            .iter()
            .any(|f| t.contains(f))
    }

    for name in BASELINED_FIXTURES {
        let results = capture(name);
        // Group NPC dialogue turns by speaker, preserving turn order.
        let mut by_npc: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
        for (i, r) in results.iter().enumerate() {
            if let ActionResult::NpcResponse { dialogue, npc, .. } = &r.result {
                by_npc
                    .entry(npc.as_str())
                    .or_default()
                    .push((i, dialogue.as_str()));
            }
        }
        for (npc, turns) in &by_npc {
            if turns.len() < 2 {
                continue; // a single-turn exchange may legitimately close.
            }
            // Every turn except the last must NOT be a farewell.
            for (i, dialogue) in &turns[..turns.len() - 1] {
                assert!(
                    !is_farewell(dialogue),
                    "rubric_no_midconversation_farewell: {name}.txt step {i}: \
                     NPC `{npc}` says a farewell before the conversation's \
                     final turn.\n  Dialogue: `{dialogue}`\n\
                     FIX: an NPC must not sign off mid-conversation. Check the \
                     dialogue prompt template (mods/rundale/prompts/) and the \
                     per-turn seam parish_core::game_session::apply_npc_dialogue_turn."
                );
            }
        }
    }
}
