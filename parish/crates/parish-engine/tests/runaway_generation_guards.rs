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

    // Pick a stable NPC. `NpcManager::all_npcs()` is HashMap-backed, so raw
    // iterator order varies across test worker threads.
    let mut npc_ids: Vec<_> = h.app.npc_manager.all_npcs().map(|n| n.id).collect();
    npc_ids.sort_unstable();
    let speaker_id = npc_ids
        .first()
        .copied()
        .expect("harness loads at least one NPC");

    let speaker_name = {
        let npc = h
            .app
            .npc_manager
            .get_mut(speaker_id)
            .expect("speaker exists");
        npc.set_location_and_state(player_loc, NpcState::Present);
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
        .filter(|n| n.location() == player_loc && n.id != speaker_id)
        .map(|n| n.id)
        .collect();
    for id in others {
        if let Some(n) = h.app.npc_manager.get_mut(id) {
            n.set_location(other_loc);
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
        let mut others: Vec<_> = h
            .app
            .npc_manager
            .all_npcs()
            .filter(|n| n.location() != player_loc)
            .collect();
        others.sort_by_key(|n| n.id);
        others
            .first()
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

// ── guard-override-false-denial — honorific-prefix false-denial regression ────
//
// Quality-harness run 16 (#1500): the person-confirmation guard over-fires when
// the player's question contains a name with a spelled-out honorific
// ("Father Declan Tierney") while the roster stores the abbreviated form
// ("Fr. Declan Tierney"). The guard treated the spelled-out form as a fabricated
// full name and replaced the NPC's correct reply with a non-recognition decline.
//
// These tests drive the real game loop via `execute_via_real_loop` with a mock
// model so the guard in `run_npc_turn` is exercised deterministically.

/// AC-2 (guard-override-false-denial, game-loop tier): when the mock model
/// returns a correct NPC self-introduction ("I am <NPC full name>, at your
/// service."), the player-visible dialogue must NOT be replaced with the
/// non-recognition decline. This is the Turn 5 repro from the harness log.
#[test]
fn real_loop_self_introduction_not_replaced_by_guard() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // The mock model returns a self-introduction using the NPC's own name.
    // This is the canonical correct model behaviour for "Would you tell me
    // your name?" — the guard must never suppress it.
    let self_intro = format!("I am {speaker_name}, at your service.");

    h.mock().push_for(&speaker_name, self_intro.clone());
    let mut rx = h.app.world.event_bus.subscribe();
    let events = h.execute_via_real_loop("Would you tell me your name?");
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
        "expected DialogueOccurred for the NPC self-introduction turn"
    );

    let joined = shown.join(" ");

    // The guard must NOT have fired — the self-introduction must reach the player.
    let decline_phrases = [
        "know no one by that name",
        "not known to me",
        "never heard of",
        "no one by that name",
        "wrong parish",
    ];
    for phrase in &decline_phrases {
        assert!(
            !joined.to_lowercase().contains(phrase),
            "self-introduction must NOT be replaced by a non-recognition decline \
             (guard-override-false-denial, #1500); decline phrase {phrase:?} found in: {joined:?}"
        );
    }

    // The NPC's own name must appear in the player-visible output — the
    // self-introduction reached the player intact (or was trimmed at a
    // word-count boundary, but the name still survives as the first element).
    assert!(
        joined.contains(&speaker_name),
        "NPC's own name must survive in the player-visible self-introduction (#1500); \
         got: {joined:?}"
    );
}

/// AC-3 (guard-override-false-denial, game-loop tier): when the player's
/// question contains a real roster member's name using the spelled-out
/// honorific ("Father <Name>") while the roster stores the abbreviated form
/// ("Fr. <Name>"), the NPC's correct reply about that person must NOT be
/// replaced with a denial. This is the Turn 13/15 repro from the harness log.
#[test]
fn real_loop_spelled_out_honorific_of_roster_member_not_denied() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // Find a real priest / Fr. NPC in the roster if one exists.
    // If none, use the speaker NPC itself with a "Fr." prefix to simulate
    // the exact condition: player uses "Father X", roster stores "Fr. X".
    //
    // We inject the abbreviation into the speaker's name for the duration
    // of this test so the honorific mismatch is guaranteed regardless of
    // which NPC the harness picked.
    let original_name = speaker_name.clone();
    let abbrev_name = format!("Fr. {original_name}");
    {
        // Temporarily rename the speaker to the abbreviated form so
        // `known_person_names` in `prepare_npc_conversation_turn` will
        // contain the abbreviated name.
        let speaker_id = h
            .app
            .npc_manager
            .all_npcs()
            .find(|n| n.name == speaker_name)
            .map(|n| n.id)
            .expect("speaker must exist");
        if let Some(npc) = h.app.npc_manager.get_mut(speaker_id) {
            npc.name = abbrev_name.clone();
        }
    }

    // The mock model correctly names the NPC using the spelled-out honorific —
    // exactly what a model would do when "Father X" appears in the player's
    // question. The guard must not treat "Father X" as fabricated.
    let correct_reply = format!(
        "'Tis a good laugh, but I'm no priest. Father {original_name} is a man of the cloth."
    );
    h.mock().push_for(&abbrev_name, correct_reply.clone());

    let mut rx = h.app.world.event_bus.subscribe();
    // Player input contains "Father <original_name>" — the spelled-out form.
    let player_input = format!("Surely Father {original_name} is the one I mean?");
    let events = h.execute_via_real_loop(&player_input);
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
    // The guard must NOT have fired — the correct reply must reach the player.
    let decline_phrases = [
        "know no one by that name",
        "not known to me",
        "never heard of",
        "no one by that name",
        "wrong parish",
    ];
    for phrase in &decline_phrases {
        assert!(
            !joined.to_lowercase().contains(phrase),
            "NPC correctly naming a real roster member (spelled-out honorific) must NOT \
             be replaced by a denial (guard-override-false-denial, #1500); \
             decline phrase {phrase:?} found in: {joined:?}"
        );
    }
}

// ── #1526 — CJK/JSON scaffolding sanitizer (real-loop) ───────────────────────

/// AC-1 (#1526, real-loop): When the mock model emits CJK meta-reasoning as a
/// suffix inside the `dialogue` field, the `sanitize_scaffolding_leak` guard
/// (called from `guard_verbosity_runons`) must strip it. The clean prefix before
/// the scaffold must survive in the player-visible DialogueOccurred event.
#[test]
fn real_loop_cjk_scaffolding_sanitized() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // Mock response: clean Irish dialogue followed by CJK scaffold text.
    // U+4E2D (中) is in the CJK Unified Ideographs block — unmistakeable scaffold.
    let clean_part = "A fine morning to ye.";
    let cjk_scaffold = "\u{4E2D}\u{6587}\u{5185}\u{5BB9}\u{6CE8}\u{91CA}";
    let raw_model_output = format!("{clean_part}{cjk_scaffold}");

    h.mock().push_for(&speaker_name, raw_model_output);
    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));

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

    let joined = dialogue_texts.join(" ");

    // CJK scaffold must not appear in player-visible output.
    assert!(
        !joined.contains('\u{4E2D}'),
        "CJK scaffold must be stripped by the real-loop guard (#1526); got: {joined:?}"
    );

    // The clean prefix must survive.
    assert!(
        joined.contains("fine morning"),
        "clean dialogue prefix must survive CJK scaffold removal (#1526); got: {joined:?}"
    );
}

// ── #1527/#1528 — false denial of roster NPC (real-loop) ─────────────────────

/// AC-1 (#1527/#1528, real-loop): When the mock model denies knowing a real
/// roster NPC by name, the `guard_false_denial_of_roster_person` guard must
/// replace the false denial with a grounded acknowledgement.
#[test]
fn real_loop_false_denial_of_roster_npc_corrected() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // Find a second real NPC in the parish (the one we moved away).
    let other_name: String = {
        let player_loc = h.app.world.player_location;
        let mut others: Vec<_> = h
            .app
            .npc_manager
            .all_npcs()
            .filter(|n| n.location() != player_loc)
            .collect();
        others.sort_by_key(|n| n.id);
        others
            .first()
            .map(|n| n.name.clone())
            .expect("there must be a second NPC in the parish")
    };

    // The mock model wrongly denies knowing this real roster member.
    let false_denial = format!("That name is not known to me hereabouts. {other_name}, ye say?");

    h.mock().push_for(&speaker_name, false_denial.clone());
    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(&format!("Do you know {other_name}?"));

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

    // The false denial must have been replaced with a grounded acknowledgement.
    // The guard replaces the entire dialogue, so the original denial phrase
    // must NOT appear in the output.
    assert!(
        !joined.to_lowercase().contains("not known to me"),
        "false denial of roster NPC must be corrected (#1527/#1528); \
         denial phrase still present in: {joined:?}"
    );
}

// ── #1530 — invented place confirmation guard (real-loop) ────────────────────

/// AC-1 (#1530, real-loop): When the mock model affirms an invented place not
/// in the world's location list, the `guard_invented_place_confirmation` guard
/// must replace the affirmation with a place-decline.
#[test]
fn real_loop_invented_place_confirmation_declined() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // The invented place "Ballyfantasy" is not in the Rundale world graph.
    // The mock model confirms it — the guard must intercept.
    let invented_affirmation = "'Tis in Ballyfantasy, over the hill to the east.";

    h.mock()
        .push_for(&speaker_name, invented_affirmation.to_string());
    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop("Where is Ballyfantasy?");

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

    // The invented place affirmation must have been replaced with a decline.
    assert!(
        !joined.to_lowercase().contains("ballyfantasy"),
        "invented place affirmation must be replaced by the guard (#1530); \
         invented place name still present in: {joined:?}"
    );
}

// ── #1531 — physical action narration in populated scene (real-loop) ──────────

/// AC-1 (#1531, real-loop): A first-person physical action ("I pick up …")
/// submitted while an NPC is co-located must produce a `text-log` event with
/// subtype "action" containing the narrated player action, rather than being
/// silently dropped.
///
/// The `is_interact` branch in `handle_game_input` calls `handle_interact`,
/// which emits the You-line. This test verifies the full path fires via the
/// real game loop with a co-located NPC present.
#[test]
fn real_loop_physical_action_produces_you_line() {
    let (mut h, _speaker_name) = harness_with_one_npc();

    // `handle_interact` is synchronous and emits the text-log before any NPC
    // inference is attempted. No mock model response is needed.
    let events = h.execute_via_real_loop("I pick up one of the horseshoes from the hook.");

    // We expect at least one `text-log` event with source "action" that says
    // "You pick up …". (`handle_interact` sets `source = "action"`.)
    let action_events: Vec<_> = events
        .iter()
        .filter(|(name, payload)| {
            name == "text-log" && payload.get("source").and_then(|s| s.as_str()) == Some("action")
        })
        .collect();

    assert!(
        !action_events.is_empty(),
        "physical action must produce a text-log event with source='action' (#1531); \
         got events: {events:?}"
    );

    // The content must narrate the pick-up.
    let content = action_events[0]
        .1
        .get("content")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_lowercase();
    assert!(
        content.contains("pick up"),
        "You-line must narrate the player's pick-up action (#1531); \
        got content: {content:?}"
    );
}

// ── #1561 — ordinary answers must not truncate to opener ─────────────────────

/// AC-1 (#1561, real-loop): a short, distinct multi-sentence answer that
/// contains a harmless repeated phrase ("work for a") must not be trimmed back
/// to only its greeting by the post-generation verbosity guards.
#[test]
fn real_loop_cooper_work_answer_is_not_truncated_to_greeting() {
    let (mut h, speaker_name) = harness_with_one_npc();

    let cooper_answer = "Good morning, Aiden Carney. Work for a cooper? \
                         Aye, there's always work for a man with that skill. \
                         This place needs barrels for ale and salt, surely. \
                         Ye know yer trade?";

    h.mock().push_for(&speaker_name, cooper_answer.to_string());
    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(
        "I am Aiden Carney, a cooper newly arrived in Kilteevan. Might there be work here?",
    );

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
        "expected DialogueOccurred for the cooper-work turn"
    );

    let joined = shown.join(" ");
    assert_ne!(
        joined, "Good morning, Aiden Carney.",
        "cooper-work answer must not be truncated to only the greeting"
    );
    for phrase in [
        "Work for a cooper?",
        "there's always work",
        "barrels for ale and salt",
    ] {
        assert!(
            joined.contains(phrase),
            "cooper-work answer lost phrase {phrase:?}: {joined:?}"
        );
    }
    assert!(
        !joined.contains("Ye know yer trade?"),
        "the fourth sentence must be dropped by the shared three-sentence cap: {joined:?}"
    );
}

// ── #1566 — watchful sacred-place run-on must be terse ──────────────────────

/// AC-1 (#1566, real-loop): when the NPC's canonical mood is watchful and the
/// mock model emits the raw sacred-place loop, the mood-aware verbosity guard
/// inside `run_npc_turn` must clip it before the repeated question/tail reaches
/// `DialogueOccurred`, even if model metadata claims a friendlier mood (#1779).
#[test]
fn real_loop_watchful_sacred_place_runon_is_clipped() {
    let (mut h, speaker_name) = harness_with_one_npc();
    let speaker_id = h
        .app
        .npc_manager
        .all_npcs()
        .find(|npc| npc.name == speaker_name)
        .map(|npc| npc.id)
        .expect("speaker exists");
    h.app
        .npc_manager
        .get_mut(speaker_id)
        .expect("speaker exists")
        .mood = "watchful".to_string();

    let raw_dialogue = "Aye, 'tis said the sidhe live in the mounds and the forts. \
        But the power here at the well, that's a different matter. \
        A blessing, mayhap, but not just for those who seek it out. \
        What do ye seek, Colm Brennan, is it for yerself or for another that \
        troubles yer thoughts this morning, aye, and brings ye to this place \
        of old magic and healing water, so it is indeed. \
        What troubles yer mind, if ye care to speak of it, and I'll do what I \
        can to ease it, if I may. \
        Ye'll not be the first to find comfort here, nor the last. \
        What brings ye to Kilteevan, and why the holy well, do ye ask, if not \
        simply to see the sights and hear the tales, aye, but to seek a deeper \
        truth or a healing hand, so it seems. \
        Tell me, and I'll listen, and if I can, I'll guide ye. \
        What do ye seek, Colm Brennan, aye, what troubles yer heart and mind \
        this mornin' so bold, aye, and brings ye here to the well, and not \
        elsewhere in the parish, if not for the sake of yer soul and the \
        whispers of the old ones, so it is indeed. \
        What do ye seek, Colm Brennan, aye, and what brings ye here to the \
        well, so it is indeed?";
    let json_reply = serde_json::json!({
        "dialogue": raw_dialogue,
        "action": "watches carefully",
        "mood": "friendly",
        "internal_thought": null,
        "language_hints": []
    })
    .to_string();

    h.mock().push_json_for(&speaker_name, json_reply);
    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(
        "I heard there is a fairy fort called Cnoc na Si on Darcy land where the cure is strongest. Is it true?",
    );

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
        "expected DialogueOccurred for the watchful run-on turn"
    );

    let joined = shown.join(" ");
    let lower = joined.to_lowercase();
    let sentence_count = joined
        .split(['.', '!', '?'])
        .filter(|s| !s.trim().is_empty())
        .count();

    assert!(
        sentence_count <= 2,
        "watchful run-on must be clipped to a terse reply; got {sentence_count}: {joined:?}"
    );
    assert!(
        joined.contains("mounds and the forts") && joined.contains("power here at the well"),
        "grounded opening must survive (#1566): {joined:?}"
    );
    assert!(
        !lower.contains("what do ye seek"),
        "repeated question loop must not reach DialogueOccurred (#1566): {joined:?}"
    );
    assert!(
        !lower.contains("brings ye here to the well"),
        "later repeated loop tail must not reach DialogueOccurred (#1566): {joined:?}"
    );
}

// ── #1565 — invented titled landlord must be denied ─────────────────────────

/// AC-1 (#1565, real-loop): an NPC reply that confirms and elaborates on the
/// fabricated titled entity "Lord Fitzwilliam of Castlemore" must be rejected
/// by the fabricated-person guard before the dialogue reaches the transcript.
#[test]
fn real_loop_invented_titled_landlord_hearsay_is_declined() {
    let (mut h, speaker_name) = harness_with_one_npc();

    let fabricated_landlord = "Aye, I've heard the talk of Lord Fitzwilliam. \
                              'Tis said he owns most of the land round hereabouts. \
                              Ye'll need to be careful with yer words when ye speak \
                              of him, 'tis a mighty man he is.";

    h.mock()
        .push_for(&speaker_name, fabricated_landlord.to_string());
    let mut rx = h.app.world.event_bus.subscribe();
    let events = h.execute_via_real_loop(
        "Have you heard of Lord Fitzwilliam of Castlemore? I hear he is the \
         great landlord hereabouts",
    );

    let dialogue_events = drain(&mut rx);
    let shown: Vec<String> = dialogue_events
        .iter()
        .filter_map(|ev| match ev {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.clone(),
            _ => None,
        })
        .collect();

    assert!(shown.is_empty(), "rejection must publish no dialogue event");
    let terminal = events
        .iter()
        .find(|(name, _)| name == "stream-turn-end")
        .map(|(_, payload)| payload)
        .expect("rejection should terminate the stream");
    assert_eq!(
        terminal.get("status").and_then(serde_json::Value::as_str),
        Some("failed")
    );
    let serialized = serde_json::to_string(&events).unwrap();
    for phrase in [
        "heard the talk of Lord Fitzwilliam",
        "owns most of the land",
        "mighty man",
    ] {
        assert!(!serialized.contains(phrase));
    }
}

// ── #1569 — known place history must not trigger person denial ───────────────

/// AC-1 (#1569, real-loop): when the player asks about the history of a known
/// place whose short name looks like a person bigram ("Lough Ree"), the
/// fabricated-person guard must not replace the model's valid place-history
/// answer with a canned "no such person" decline.
#[test]
fn real_loop_known_place_history_not_replaced_by_person_denial() {
    let (mut h, speaker_name) = harness_with_one_npc();

    let lake_history = "Ah, the history of Lough Ree is a tale as grand as the lake itself. \
                        Folk say it was formed by the great flood, and it is said to be home \
                        to the Lough Ree wurm.";

    h.mock().push_for(&speaker_name, lake_history.to_string());
    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(
        "Aoife, I never saw a lake this grand. What is the history of Lough Ree?",
    );

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
        "expected DialogueOccurred for the Lough Ree history turn"
    );

    let joined = shown.join(" ");
    let lower = joined.to_lowercase();
    assert!(
        joined.contains("history of Lough Ree"),
        "valid place-history answer must reach the transcript (#1569); got: {joined:?}"
    );
    for phrase in [
        "no such person",
        "know of no such person",
        "know no one by that name",
        "wrong parish",
    ] {
        assert!(
            !lower.contains(phrase),
            "known place-history answer must not be replaced by a person decline \
             (#1569); decline phrase {phrase:?} found in: {joined:?}"
        );
    }
}

// ── #1563 — real parish entities must not be falsely denied ─────────────────

/// AC-1/AC-4 (#1563, real-loop): when the mock model generically denies a real
/// place from the world graph, the shared `run_npc_turn` guard chain must
/// replace that denial before it reaches `DialogueOccurred`.
#[test]
fn real_loop_known_place_generic_denial_is_corrected() {
    let (mut h, speaker_name) = harness_with_one_npc();

    assert!(
        h.app
            .world
            .graph
            .location_ids()
            .into_iter()
            .filter_map(|id| h.app.world.graph.get(id))
            .any(|location| location.name == "Darcy's Pub"),
        "Rundale fixture must include Darcy's Pub"
    );

    h.mock().push_for(
        &speaker_name,
        "I cannae guide ye to a place that doesn't exist.".to_string(),
    );
    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(&format!(
        "talk to {speaker_name} about Where is Darcy's Pub?"
    ));

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
        "expected DialogueOccurred for known-place false-denial turn"
    );

    let joined = shown.join(" ");
    let lower = joined.to_lowercase();
    assert!(
        !lower.contains("doesn't exist") && !lower.contains("does not exist"),
        "known place must not reach transcript as nonexistent (#1563); got: {joined:?}"
    );
    assert!(
        lower.contains("place")
            && (lower.contains("know") || lower.contains("known") || lower.contains("real")),
        "known-place denial should become a grounded acknowledgement; got: {joined:?}"
    );
}

/// AC-2/AC-4 (#1563, real-loop): when the mock model says "I know no one by
/// that name" after the player asks about a real parish NPC, the shared
/// `run_npc_turn` guard chain must replace the false denial even though the
/// dialogue did not repeat the full name.
#[test]
fn real_loop_known_person_generic_denial_is_corrected() {
    let (mut h, speaker_name) = harness_with_one_npc();

    assert!(
        h.app
            .npc_manager
            .all_npcs()
            .any(|npc| npc.name == "Padraig Darcy"),
        "Rundale fixture must include Padraig Darcy"
    );

    h.mock().push_for(
        &speaker_name,
        "I know no one by that name in these parts.".to_string(),
    );
    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(&format!(
        "talk to {speaker_name} about Where is Padraig Darcy?"
    ));

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
        "expected DialogueOccurred for known-person false-denial turn"
    );

    let joined = shown.join(" ");
    let lower = joined.to_lowercase();
    assert!(
        !lower.contains("no one by that name") && !lower.contains("no such person"),
        "known person must not reach transcript as nonexistent (#1563); got: {joined:?}"
    );
    assert!(
        lower.contains("name") || lower.contains("parish"),
        "known-person denial should become a grounded acknowledgement; got: {joined:?}"
    );
}

// ── #1553 — player self-introduction not denied (real-loop) ──────────────────

/// AC-1 (#1553, real-loop): When the player introduces themselves ("I am Aiden
/// Carney") and the NPC mock model replies with a warm welcome that contains
/// the player's name alongside affirmation markers, `guard_fabricated_person_confirmation`
/// (called from `run_npc_turn`) must NOT replace the warm welcome with a canned
/// denial. The player's own name must be exempt from the fabricated-person guard
/// via `setup.player_name` threaded from `NpcConversationSetup`.
#[test]
fn real_loop_player_self_introduction_not_denied() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // The mock model replies with a warm welcome that contains the player's name
    // and affirmation markers that would previously trigger the guard.
    let warm_welcome = "'Tis a fine mornin', Aiden Carney! Welcome to Kilteevan, indeed! \
                        A cooper is just what we need in these parts.";

    h.mock().push_for(&speaker_name, warm_welcome.to_string());
    let mut rx = h.app.world.event_bus.subscribe();
    // Player introduces themselves — detect_and_record_player_name fires first.
    let _events = h.execute_via_real_loop("I am Aiden Carney, a cooper newly come to Kilteevan.");

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

    // The warm welcome must NOT have been replaced with a canned denial.
    // Canned denials contain "not known to me", "know no one by that name", etc.
    assert!(
        !joined.to_lowercase().contains("not known to me"),
        "player self-introduction must not trigger the denial guard (#1553); \
         warm welcome was replaced with a canned denial in: {joined:?}"
    );
    assert!(
        !joined.to_lowercase().contains("know no one by that name"),
        "player self-introduction must not trigger the denial guard (#1553); \
         wrong canned decline in: {joined:?}"
    );

    // The warm welcome must survive intact.
    assert!(
        joined.contains("Aiden Carney"),
        "player name must survive in the NPC's reply (#1553); got: {joined:?}"
    );
}
