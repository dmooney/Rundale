//! Real-loop integration tests for the runaway-generation guard chain
//! (#1487, #1488, #1489).
//!
//! These tests drive the **real** `parish_core::game_loop` (`execute_via_real_loop`)
//! with a mock inference client that injects adversarial / degenerate model
//! responses, then assert the post-generation guards in `run_npc_turn` produce
//! clean player-visible output.
//!
//! # Why `execute_via_real_loop` and not a plain unit test
//!
//! The guards for #1487 (`collapse_degenerate_phrase_loop`) and #1489
//! (`cap_word_count`) are called from `guard_verbosity_runons`, which is
//! invoked at line 382 of `parish_core::game_loop::npc_turn::run_npc_turn`.
//! The guard for #1488 (`known_person_names` now includes all parish NPCs) is
//! set up in `prepare_npc_conversation_turn` (also called from `run_npc_turn`).
//!
//! `parish-engine --script` uses the LEGACY `execute()` path which bypasses
//! `game_loop/npc_turn`, so a plain `--script` fixture would NOT exercise these
//! guards. `GameTestHarness::execute_via_real_loop` routes through the real
//! `handle_game_input` → `handle_npc_conversation` → `run_npc_turn` path, so
//! the guards ARE exercised.

use parish_core::npc::types::NpcState;
use parish_core::world::events::GameEvent;
use parish_engine::testing::GameTestHarness;

/// Helper: drain all events from a broadcast receiver.
fn drain(rx: &mut tokio::sync::broadcast::Receiver<GameEvent>) -> Vec<GameEvent> {
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(ev) => out.push(ev),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
    out
}

/// Sets up a harness with exactly one NPC co-located with the player, clears
/// other NPCs from the player location, and marks the NPC as introduced so
/// dialogue routing resolves to them deterministically.
///
/// Returns `(harness, npc_name)`.
fn harness_with_one_npc() -> (GameTestHarness, String) {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    // Pick the first NPC.
    let speaker_id = h
        .app
        .npc_manager
        .all_npcs()
        .map(|n| n.id)
        .next()
        .expect("harness loads at least one NPC");

    let speaker_name = {
        let npc = h
            .app
            .npc_manager
            .get_mut(speaker_id)
            .expect("speaker exists");
        npc.location = player_loc;
        npc.state = NpcState::Present;
        npc.name.clone()
    };
    h.app.npc_manager.mark_introduced(speaker_id);

    // Move every other NPC away so routing is deterministic.
    let other_loc = h
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|l| *l != player_loc)
        .expect("graph has at least two locations");
    let others: Vec<_> = h
        .app
        .npc_manager
        .all_npcs()
        .filter(|n| n.location == player_loc && n.id != speaker_id)
        .map(|n| n.id)
        .collect();
    for id in others {
        if let Some(n) = h.app.npc_manager.get_mut(id) {
            n.location = other_loc;
        }
    }

    (h, speaker_name)
}

// ── #1487 — degenerate phrase-loop guard ─────────────────────────────────────

/// AC-1 (#1487, real-loop): When the mock model emits a short phrase repeated
/// 15× (the priest runaway repro), the player-visible dialogue must NOT contain
/// that repeated phrase more than once. The guard fires inside `run_npc_turn`
/// via `guard_verbosity_runons`.
#[test]
fn real_loop_degenerate_phrase_loop_is_collapsed() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // Adversarial mock response: "in His time and His purpose" repeated 15×.
    // Preamble is kept so the NPC isn't silent after guard fires.
    let preamble = "God's blessings upon ye.";
    let loop_phrase = "in His time and His purpose";
    let loop_part = std::iter::repeat_n(loop_phrase, 15)
        .collect::<Vec<_>>()
        .join(", ");
    let runaway = format!("{preamble} {loop_part}");

    h.mock().push_for(&speaker_name, runaway.clone());
    let mut rx = h.app.world.event_bus.subscribe();
    let events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));

    let dialogue_events = drain(&mut rx);
    let dialogue_texts: Vec<String> = dialogue_events
        .iter()
        .filter_map(|ev| match ev {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.clone(),
            _ => None,
        })
        .collect();

    // At least one DialogueOccurred event must have been published.
    assert!(
        !dialogue_texts.is_empty(),
        "expected DialogueOccurred for the NPC turn; got events: {events:?}"
    );

    // The player-visible text must NOT contain the runaway phrase loop.
    // At most one occurrence is permitted (the first, kept by the guard).
    let joined = dialogue_texts.join(" ");
    let phrase_count = joined
        .to_lowercase()
        .matches("in his time and his purpose")
        .count();
    assert!(
        phrase_count <= 1,
        "degenerate phrase loop must be collapsed to ≤1 occurrence via real game loop; \
         got {phrase_count}: {joined:?}"
    );

    // The preamble sentence must survive in the dialogue event.
    assert!(
        joined.contains("God's blessings"),
        "preamble must survive the guard: {joined:?}"
    );
}

// ── #1489 — word-count cap guard ─────────────────────────────────────────────

/// AC-1 (#1489, real-loop): When the mock model emits a reply over 80 words,
/// the `cap_word_count` guard (called from `guard_verbosity_runons` inside
/// `run_npc_turn`) must produce a player-visible response of ≤ 80 words.
#[test]
fn real_loop_overlong_reply_is_word_capped() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // Build a response that is 3 sentences but ~100 words. Deliberately avoid
    // title-cased words beyond the first position to prevent false-positive
    // person-name extraction.
    let s1 = "A fine morning to ye, friend.";
    let s2 = "The roads have been fierce hard on everyone this past fortnight and the river \
        rose three whole feet after the rains came down on tuesday and wednesday and the \
        landlord sent his agent round to collect the rents before the harvest was even in \
        which caused great hardship to all the families in the townland and the market at \
        the crossroads was cancelled on account of the weather as well.";
    let s3 = "But the sun is shining today and the prices are somewhat better than they were \
        last year which is a true blessing for the parish and all who live here.";
    let overlong = format!("{s1} {s2} {s3}");
    let word_count = overlong.split_whitespace().count();
    assert!(
        word_count > 80,
        "test input must be >80 words, got {word_count}"
    );

    h.mock().push_for(&speaker_name, overlong);
    let mut rx = h.app.world.event_bus.subscribe();
    let events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));
    let _ = events; // events captured; we inspect via event bus

    let dialogue_events = drain(&mut rx);
    let dialogue_texts: Vec<String> = dialogue_events
        .iter()
        .filter_map(|ev| match ev {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.clone(),
            _ => None,
        })
        .collect();

    assert!(
        !dialogue_texts.is_empty(),
        "expected DialogueOccurred for the NPC turn"
    );

    let shown = dialogue_texts.join(" ");
    let result_words = shown.split_whitespace().count();
    assert!(
        result_words <= 80,
        "player-visible dialogue must be ≤80 words after real-loop word-cap guard; \
         got {result_words} words: {shown:?}"
    );

    // The first sentence must survive.
    assert!(
        shown.contains("fine morning"),
        "first sentence must survive the cap: {shown:?}"
    );
}

// ── #1488 — no false-denial of real roster NPC ───────────────────────────────

/// AC-1 (#1488, real-loop): When an NPC gives a correct description of a REAL
/// parish NPC (one who is not in the speaking NPC's personal relationship
/// roster but IS in the parish registry), the `guard_fabricated_person_confirmation`
/// guard must NOT fire and must NOT replace the response with a denial.
///
/// Previously the guard fired because `known_person_names` only included the
/// speaking NPC's personal roster. The fix extends it to include all parish NPCs.
#[test]
fn real_loop_real_npc_description_not_denied() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // Find the name of a SECOND real parish NPC (the one we moved away).
    let other_name: String = {
        let player_loc = h.app.world.player_location;
        h.app
            .npc_manager
            .all_npcs()
            .find(|n| n.location != player_loc)
            .map(|n| n.name.clone())
            .expect("there must be a second NPC in the parish")
    };

    // The model accurately describes the other NPC, who is real but NOT in
    // the speaker's personal roster. Before the fix, this would be replaced
    // with "I know no one by that name in these parts."
    let good_reply = format!(
        "{other_name} is a fine person in this parish. You'll find them at their usual spot."
    );

    h.mock().push_for(&speaker_name, good_reply.clone());
    let mut rx = h.app.world.event_bus.subscribe();
    let events = h.execute_via_real_loop(&format!("talk to {speaker_name} about {other_name}"));
    let _ = events;

    let dialogue_events = drain(&mut rx);
    let shown: Vec<String> = dialogue_events
        .iter()
        .filter_map(|ev| match ev {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.clone(),
            _ => None,
        })
        .collect();

    assert!(
        !shown.is_empty(),
        "expected DialogueOccurred for the NPC turn"
    );

    let joined = shown.join(" ");

    // The guard MUST NOT have replaced the reply with a false denial.
    assert!(
        !joined.to_lowercase().contains("know no one by that name"),
        "real parish NPC description must NOT be replaced with a false denial (#1488); \
         got: {joined:?}"
    );
    assert!(
        !joined.to_lowercase().contains("not known to me"),
        "real parish NPC description must NOT be replaced with a false denial (#1488); \
         got: {joined:?}"
    );
    assert!(
        !joined.to_lowercase().contains("never heard of"),
        "real parish NPC description must NOT be replaced with a false denial (#1488); \
         got: {joined:?}"
    );

    // The good reply should have reached the player (or a truncated version of it).
    // The mock reply opens with the full name, so it survives any word-cap; assert
    // on the FULL name (not just the first name, which could be a common word like
    // "Father" or "Mary") to avoid a false positive (gemini review #1500).
    assert!(
        joined.contains(&other_name),
        "the real NPC's name must survive in the player-visible output (#1488); \
         got: {joined:?}"
    );
}
