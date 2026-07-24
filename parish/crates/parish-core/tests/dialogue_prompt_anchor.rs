//! End-to-end check that the dialogue prompt-builder injects the
//! name-anchor directives (TODO #11 / #24 / #35).
//!
//! The bug: in cycles 7+ of the demo-audit runs, NPCs at new locations
//! addressed the player by names from prior-location dialogue history
//! (Roisin called the player "Nora" because Nora Duffy was upstream in
//! the recent-events buffer), and NPCs addressed absent characters as
//! if they were present. Root cause: the dialogue prompt named the
//! player and listed co-located NPCs but did not forbid the LLM from
//! using any other name found in the history.
//!
//! These integration tests load Rundale, set up the documented
//! scenarios, and assert that the assembled context contains the
//! anchor strings that constrain naming. Backs the AC in
//! `.proofs/todo-name-anchor/acceptance-criteria.md`.

use std::path::{Path, PathBuf};

use parish_core::game_loop::save::load_fresh_world_and_npcs;
use parish_core::game_mod::GameMod;
use parish_core::ipc::prepare_npc_conversation_turn;
use parish_core::npc::{LanguageSettings, NpcId};

fn rundale_mod_dir() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.join("../../../mods/rundale")
}

/// Loads Rundale and returns (npc_manager, world) so a test can drive
/// `prepare_npc_conversation_turn` more than once against the same
/// fixture (e.g. turn 1 vs turn 2 for the introduced-anchor test).
fn fresh_rundale_world_and_npcs() -> (
    parish_core::world::WorldState,
    parish_core::npc::manager::NpcManager,
) {
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).expect("load rundale mod");
    load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).expect("load fresh world")
}

/// Loads Rundale, picks the first NPC co-located with the player at
/// fresh save, optionally marks the NPC as knowing the player's name,
/// and returns the assembled dialogue context.
fn build_dialogue_context_with_first_npc(
    player_name: Option<&str>,
    teach_player_name: bool,
) -> String {
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).expect("load rundale mod");
    let (mut world, mut npc_manager) =
        load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).expect("load fresh world");

    if let Some(name) = player_name {
        world.player_name = Some(name.to_string());
    }

    let speaker_id: NpcId = npc_manager
        .npcs_at(world.player_location)
        .first()
        .map(|n| n.id)
        .expect("Rundale fresh save should co-locate at least one NPC with the player");

    if teach_player_name {
        npc_manager.teach_player_name(speaker_id);
    }

    let setup = prepare_npc_conversation_turn(
        &world,
        &mut npc_manager,
        "hello there",
        speaker_id,
        &[],
        false,
        &LanguageSettings::english_only(),
        &parish_core::config::NpcConfig::default(),
    )
    .expect("prepare_npc_conversation_turn returned None");

    setup.context
}

/// AC1: when the NPC knows the player's name, the assembled context
/// must contain a directive instructing the model to use only that
/// name — closing the leak in TODO #35 where Roisin called the player
/// "Nora" with no anchor to push back.
#[test]
fn dialogue_context_pins_player_name_when_known() {
    let context = build_dialogue_context_with_first_npc(Some("Aiden Carney"), true);

    assert!(
        context.contains("Aiden Carney"),
        "context must mention player name:\n{context}"
    );
    assert!(
        context.contains("Address them by the name 'Aiden Carney' only"),
        "name-anchor directive missing:\n{context}"
    );
    assert!(
        context.contains("recent conversation history"),
        "history-leak guard missing:\n{context}"
    );
}

/// AC4: when the NPC does NOT know the player's name, the assembled
/// context must still anchor naming so the LLM does not borrow a name
/// from the recent-events buffer (the upstream form of the #35 leak).
#[test]
fn dialogue_context_forbids_borrowed_name_when_unknown() {
    let context = build_dialogue_context_with_first_npc(None, false);

    assert!(
        context.contains("A newcomer to the parish"),
        "newcomer label missing:\n{context}"
    );
    assert!(
        context.contains("do not invent a name"),
        "invent-name guard missing:\n{context}"
    );
    assert!(
        context.contains("do not borrow a name"),
        "borrow-name guard missing:\n{context}"
    );
}

/// TODO #39: assembled context must carry an "already introduced"
/// anchor on the second and later turns with an NPC, but NOT on the
/// first turn (so the NPC can naturally introduce themselves once).
/// The bug: Roisin recited "Roisin Connolly, of Connolly's Shop"
/// mid-reply on turn 7 because the dialogue prompt did not flag her
/// as already introduced.
#[test]
fn dialogue_context_introduced_anchor_fires_only_after_first_turn() {
    use parish_core::npc::LanguageSettings;

    let (mut world, mut npc_manager) = fresh_rundale_world_and_npcs();
    let speaker_id: NpcId = npc_manager
        .npcs_at(world.player_location)
        .first()
        .map(|n| n.id)
        .expect("Rundale fresh save should co-locate at least one NPC");
    world.player_name = Some("Aiden Carney".to_string());

    // Turn 1 — NPC has never been met before; the anchor must NOT
    // appear so the NPC can introduce themselves.
    let setup1 = prepare_npc_conversation_turn(
        &world,
        &mut npc_manager,
        "hello there",
        speaker_id,
        &[],
        false,
        &LanguageSettings::english_only(),
        &parish_core::config::NpcConfig::default(),
    )
    .expect("turn 1 setup");
    assert!(
        !setup1
            .context
            .contains("You have already introduced yourself"),
        "anchor must not render on first contact:\n{}",
        setup1.context
    );

    // A delivered self-identification is what marks identity known. Prompt
    // preparation alone no longer does so (#1776); simulate the shared apply
    // seam accepting the NPC's spoken full name before building turn 2.
    npc_manager.mark_introduced(speaker_id);

    // Turn 2 — now the anchor must appear and name the NPC + occupation.
    let setup2 = prepare_npc_conversation_turn(
        &world,
        &mut npc_manager,
        "what brings ye to the parish today?",
        speaker_id,
        &[],
        false,
        &LanguageSettings::english_only(),
        &parish_core::config::NpcConfig::default(),
    )
    .expect("turn 2 setup");
    assert!(
        setup2
            .context
            .contains("You have already introduced yourself"),
        "anchor must render on follow-up turns:\n{}",
        setup2.context
    );
    assert!(
        setup2
            .context
            .contains("Do not recite your full name and occupation"),
        "anchor missing recitation guard:\n{}",
        setup2.context
    );
}

/// TODO #13: assembled context must carry an explicit time-of-day
/// cue naming both the bucket (Morning/Dusk/Night/...) and HH:MM, so
/// the model has an unambiguous signal for greeting register. The bug:
/// NPCs at Dusk were greeting with "good mornin'" because the only
/// time signal was the bare 17:30 timestamp.
#[test]
fn dialogue_context_carries_time_of_day_cue() {
    let context = build_dialogue_context_with_first_npc(Some("Aiden Carney"), true);

    assert!(
        context.contains("Time of day:"),
        "missing Time of day cue:\n{context}"
    );
    assert!(
        context.contains("greet and refer to the time of day accordingly"),
        "missing greeting-register directive:\n{context}"
    );
    // Rundale fresh save loads at 08:00 → Morning bucket.
    assert!(
        context.contains("Morning"),
        "time-of-day label missing:\n{context}"
    );
    assert!(
        context.contains("(08:"),
        "HH:MM must accompany the cue:\n{context}"
    );
}

/// TODO #55: when the player input contains a modern-register
/// phrase (e.g. "taking in the sights"), the assembled context must
/// carry an alert telling the NPC not to echo it back. Pre-fix the
/// alert path only fired for technology/slang anachronisms, so
/// modern-register phrases reached the NPC unchallenged and tripped
/// a post-reply WARN once the LLM mirrored them.
#[test]
fn dialogue_context_alerts_on_player_modern_register_echo() {
    use parish_core::npc::LanguageSettings;

    let (mut world, mut npc_manager) = fresh_rundale_world_and_npcs();
    let speaker_id: NpcId = npc_manager
        .npcs_at(world.player_location)
        .first()
        .map(|n| n.id)
        .expect("Rundale fresh save should co-locate at least one NPC");
    world.player_name = Some("Aiden Carney".to_string());

    let setup = prepare_npc_conversation_turn(
        &world,
        &mut npc_manager,
        "I'm just taking in the sights",
        speaker_id,
        &[],
        false,
        &LanguageSettings::english_only(),
        &parish_core::config::NpcConfig::default(),
    )
    .expect("turn setup");
    assert!(
        setup.context.contains("MODERN-REGISTER ALERT"),
        "missing alert header:\n{}",
        setup.context
    );
    assert!(
        setup.context.contains("taking in the sights"),
        "alert must surface the matched phrase:\n{}",
        setup.context
    );
    assert!(
        setup.context.contains("Do NOT echo"),
        "missing echo guard:\n{}",
        setup.context
    );
}

/// AC2: no register alert when the player input is clean.
#[test]
fn dialogue_context_no_register_alert_on_clean_input() {
    use parish_core::npc::LanguageSettings;

    let (mut world, mut npc_manager) = fresh_rundale_world_and_npcs();
    let speaker_id: NpcId = npc_manager
        .npcs_at(world.player_location)
        .first()
        .map(|n| n.id)
        .expect("Rundale fresh save should co-locate at least one NPC");
    world.player_name = Some("Aiden Carney".to_string());

    let setup = prepare_npc_conversation_turn(
        &world,
        &mut npc_manager,
        "good morning to ye",
        speaker_id,
        &[],
        false,
        &LanguageSettings::english_only(),
        &parish_core::config::NpcConfig::default(),
    )
    .expect("turn setup");
    assert!(
        !setup.context.contains("MODERN-REGISTER ALERT"),
        "alert must not fire on clean input:\n{}",
        setup.context
    );
}

/// TODO #14: the assembled context must instruct the NPC to address
/// only one person per reply. Brendan was mixing multiple Slán*
/// farewells in a single message because the prompt tail invited
/// "may answer the player or another nearby NPC" with no upper limit.
#[test]
fn dialogue_context_carries_single_addressee_anchor() {
    let context = build_dialogue_context_with_first_npc(Some("Aiden Carney"), true);

    assert!(
        context.contains("Address ONLY ONE person"),
        "missing single-addressee directive:\n{context}"
    );
    assert!(
        context.contains("do NOT mix farewells with ongoing chat"),
        "missing mixed-farewell guard:\n{context}"
    );
    assert!(
        context.contains("Do NOT say goodbye to one person"),
        "missing goodbye-then-continue guard:\n{context}"
    );
}

/// TODO #21: the assembled context must pin the NPC to the player's
/// current location with a directive forbidding substitution of any
/// nearby canonical settlement. The bug surfaced at The Mill near
/// Kilteevan when Cormac/Nora kept saying "here in Curraghboy".
#[test]
fn dialogue_context_pins_current_location() {
    let context = build_dialogue_context_with_first_npc(Some("Aiden Carney"), true);

    assert!(
        context.contains("WHERE YOU ARE RIGHT NOW"),
        "location anchor header missing:\n{context}"
    );
    assert!(
        context.contains("no other settlement"),
        "no-other-settlement directive missing:\n{context}"
    );
    assert!(
        context.contains("you are NOT at any of them"),
        "not-elsewhere guard missing:\n{context}"
    );
}

/// AC2/AC3: the assembled context must always carry a present-only
/// anchor — either listing co-located NPCs as the exclusive set or
/// declaring nobody else is here. TODO #11 captured the failure mode
/// where Brendan addressed absent "Nora" mid-reply because the prompt
/// did not forbid invoking off-stage characters by name.
#[test]
fn dialogue_context_includes_present_or_solo_anchor() {
    let context = build_dialogue_context_with_first_npc(Some("Aiden Carney"), true);

    let has_present_anchor = context.contains("these are the only other people");
    let has_solo_anchor = context.contains("No one else is here");
    assert!(
        has_present_anchor || has_solo_anchor,
        "expected either present-anchor or solo-anchor; got neither in:\n{context}"
    );
    if has_solo_anchor {
        assert!(
            context.contains("never as if they can hear you now"),
            "solo anchor must forbid addressing absent NPCs as present:\n{context}"
        );
    }
}

/// AC-4 (#1394): when grounding is enabled, the system prompt must contain
/// a `PLACES IN THIS PARISH` block listing real parish location names.
#[test]
fn dialogue_system_prompt_contains_location_grounding_block() {
    use parish_core::npc::LanguageSettings;

    let (mut world, mut npc_manager) = fresh_rundale_world_and_npcs();
    let speaker_id: NpcId = npc_manager
        .npcs_at(world.player_location)
        .first()
        .map(|n| n.id)
        .expect("Rundale fresh save should co-locate at least one NPC");
    world.player_name = Some("Aiden Carney".to_string());

    let setup = prepare_npc_conversation_turn(
        &world,
        &mut npc_manager,
        "hello there",
        speaker_id,
        &[],
        false,
        &LanguageSettings::english_only(),
        &parish_core::config::NpcConfig {
            grounding_enabled: true,
            ..parish_core::config::NpcConfig::default()
        },
    )
    .expect("setup must succeed");

    // AC-4: PLACES IN THIS PARISH block is present
    assert!(
        setup.system_prompt.contains("PLACES IN THIS PARISH"),
        "system prompt must contain PLACES IN THIS PARISH block:\n{}",
        setup.system_prompt
    );
    // Rundale has Kilteevan Village as a canonical location — must be listed
    assert!(
        setup.system_prompt.contains("Kilteevan"),
        "system prompt must list at least one real location name:\n{}",
        setup.system_prompt
    );
}

/// AC-5 (#1394): when grounding is enabled, the system prompt must contain
/// an explicit instruction to deflect/refuse confirmation of unknown places
/// or people.
#[test]
fn dialogue_system_prompt_contains_grounding_deflection_instruction() {
    use parish_core::npc::LanguageSettings;

    let (world, mut npc_manager) = fresh_rundale_world_and_npcs();
    let speaker_id: NpcId = npc_manager
        .npcs_at(world.player_location)
        .first()
        .map(|n| n.id)
        .expect("Rundale fresh save should co-locate at least one NPC");

    let setup = prepare_npc_conversation_turn(
        &world,
        &mut npc_manager,
        "hello",
        speaker_id,
        &[],
        false,
        &LanguageSettings::english_only(),
        &parish_core::config::NpcConfig {
            grounding_enabled: true,
            ..parish_core::config::NpcConfig::default()
        },
    )
    .expect("setup must succeed");

    // AC-5: deflection instruction is present
    assert!(
        setup.system_prompt.contains("do not confirm"),
        "system prompt must instruct NPC not to confirm unknown places/people:\n{}",
        setup.system_prompt
    );
    assert!(
        setup.system_prompt.contains("Politely correct or deflect"),
        "system prompt must include deflection instruction:\n{}",
        setup.system_prompt
    );
}

/// Inverse: when grounding is disabled, the PLACES block must be absent.
#[test]
fn dialogue_system_prompt_omits_location_block_when_grounding_disabled() {
    use parish_core::npc::LanguageSettings;

    let (world, mut npc_manager) = fresh_rundale_world_and_npcs();
    let speaker_id: NpcId = npc_manager
        .npcs_at(world.player_location)
        .first()
        .map(|n| n.id)
        .expect("Rundale fresh save should co-locate at least one NPC");

    let setup = prepare_npc_conversation_turn(
        &world,
        &mut npc_manager,
        "hello",
        speaker_id,
        &[],
        false,
        &LanguageSettings::english_only(),
        &parish_core::config::NpcConfig {
            grounding_enabled: false,
            ..parish_core::config::NpcConfig::default()
        },
    )
    .expect("setup must succeed");

    assert!(
        !setup.system_prompt.contains("PLACES IN THIS PARISH"),
        "PLACES block must be absent when grounding disabled:\n{}",
        setup.system_prompt
    );
}
