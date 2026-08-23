//! Tests for the command handler module.

use super::*;
use crate::config::InferenceCategory;
use crate::input::{Command, FlagSubcommand};
use crate::ipc::GameConfig;
use crate::npc::manager::NpcManager;
use crate::world::{LocationId, Weather, WorldState};
use chrono::Timelike;
use std::path::Path;

fn default_state() -> (WorldState, NpcManager, GameConfig) {
    (WorldState::new(), NpcManager::new(), GameConfig::default())
}

#[test]
fn pause_command() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Pause, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("stand still"));
    assert!(world.clock.is_paused());
}

#[test]
fn resume_command() {
    let (mut world, mut npc, mut config) = default_state();
    world.clock.pause();
    let result = handle_command(Command::Resume, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("stirs again"));
    assert!(!world.clock.is_paused());
}

#[test]
fn redundant_pause_is_silent_no_edge() {
    // TODO #6 / #31: a second /pause while already paused must not re-emit the
    // "stand still" line. The clock stays paused; the response is empty.
    let (mut world, mut npc, mut config) = default_state();
    let first = handle_command(Command::Pause, &mut world, &mut npc, &mut config);
    assert!(first.response.contains("stand still"));
    assert!(world.clock.is_paused());

    let second = handle_command(Command::Pause, &mut world, &mut npc, &mut config);
    assert!(
        second.response.is_empty(),
        "redundant pause must emit no text; got {:?}",
        second.response
    );
    assert!(world.clock.is_paused());
}

#[test]
fn redundant_resume_is_silent_no_edge() {
    // TODO #6 / #31: /resume while already running must not emit "stirs again".
    // This is the back-to-back duplicate the demo audit captured.
    let (mut world, mut npc, mut config) = default_state();
    assert!(!world.clock.is_paused());
    let result = handle_command(Command::Resume, &mut world, &mut npc, &mut config);
    assert!(
        result.response.is_empty(),
        "redundant resume must emit no text; got {:?}",
        result.response
    );
    assert!(!world.clock.is_paused());
}

#[test]
fn pause_resume_sequence_emits_each_edge_once() {
    // /pause /pause /resume /resume → exactly one edge message each direction.
    let (mut world, mut npc, mut config) = default_state();
    let p1 = handle_command(Command::Pause, &mut world, &mut npc, &mut config);
    let p2 = handle_command(Command::Pause, &mut world, &mut npc, &mut config);
    let r1 = handle_command(Command::Resume, &mut world, &mut npc, &mut config);
    let r2 = handle_command(Command::Resume, &mut world, &mut npc, &mut config);
    assert!(p1.response.contains("stand still"));
    assert!(p2.response.is_empty());
    assert!(r1.response.contains("stirs again"));
    assert!(r2.response.is_empty());
    assert!(!world.clock.is_paused());
}

#[test]
fn status_command() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Status, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("Location:"));
}

#[test]
fn wait_one_minute_uses_singular_narration() {
    // #1163: `/wait 1` must read "You wait for 1 minute...", not the
    // hardcoded plural "1 minutes". The pluralization lives in
    // `minute_word`, but this guards the Wait handler's call site so a
    // future edit can't silently re-hardcode the plural.
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Wait(1), &mut world, &mut npc, &mut config);
    assert!(
        result.response.contains("You wait for 1 minute..."),
        "expected singular minute, got {:?}",
        result.response
    );
    assert!(
        !result.response.contains("1 minutes"),
        "1-minute wait must not use the plural, got {:?}",
        result.response
    );
}

#[test]
fn wait_multiple_minutes_uses_plural_narration() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Wait(5), &mut world, &mut npc, &mut config);
    assert!(
        result.response.contains("You wait for 5 minutes..."),
        "expected plural minutes, got {:?}",
        result.response
    );
}

#[test]
fn toggle_improv() {
    let (mut world, mut npc, mut config) = default_state();
    assert!(!config.improv_enabled);
    let result = handle_command(Command::ToggleImprov, &mut world, &mut npc, &mut config);
    assert!(config.improv_enabled);
    assert!(result.response.contains("improv"));
}

#[test]
fn set_provider_is_removed_without_rebuild() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetProvider("openrouter".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert_ne!(config.provider_name, "openrouter");
}

#[test]
fn set_key_is_removed_without_storing_secret() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetKey("sk-test12345678".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert!(config.api_key.is_none());
}

#[test]
fn show_model_auto_detect() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::ShowModel, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("auto-detect"));
}

#[test]
fn quit_returns_effect() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Quit, &mut world, &mut npc, &mut config);
    assert!(result.response.is_empty());
    assert!(result.effects.contains(&CommandEffect::Quit));
}

#[test]
fn npcs_here_empty() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::NpcsHere, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("No one"));
}

#[test]
fn time_command() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Time, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("Weather:"));
    assert!(result.response.contains("Speed:"));
}

#[test]
fn category_provider_inherits_base() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::ShowCategoryProvider(InferenceCategory::Dialogue),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("inherits base"));
}

#[test]
fn render_look_text_basic() {
    let world = WorldState::new();
    let npc = NpcManager::new();
    let text = render_look_text(&world, &npc, 1.25, "on foot", true);
    assert!(!text.is_empty());
}

// ── Place listening ──────────────────────────────────────────────────────

fn rundale_world_at(location: LocationId) -> WorldState {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale/world.json");
    let mut world =
        WorldState::from_parish_file(&path, location).expect("load the bundled Rundale world");
    // Freeze the accelerated clock so determinism assertions cannot cross a
    // time-of-day boundary while the test is running.
    world.clock.pause();
    world
}

fn response_for(world: &WorldState, config: &GameConfig, command: Command) -> CommandResult {
    let mut world = world.clone();
    let mut npc = NpcManager::new();
    let mut config = config.clone();
    handle_command(command, &mut world, &mut npc, &mut config)
}

fn current_vignette(world: &WorldState) -> crate::world::folklore::ListeningVignette {
    let location = world.current_location_data().unwrap();
    let now = world.clock.now();
    let date = now.date_naive();
    let time = crate::world::time::time_of_day_from_hour(now.hour());
    crate::world::folklore::listen_at_location(
        location,
        time,
        crate::world::time::Season::from_date(date),
        world.weather,
        crate::world::folklore::listening_seed(date, location.id, time),
    )
}

#[test]
fn three_atmosphere_actions_are_distinct_and_component_scoped() {
    let world = rundale_world_at(LocationId(1));
    let config = GameConfig::default();
    let vignette = current_vignette(&world);
    let ambient = vignette.ambient;
    let echo = vignette.echo.expect("the Crossroads has an authored echo");
    let lore = vignette.lore.expect("the Crossroads has authored folklore");

    let listen = response_for(&world, &config, Command::Listen);
    let omen = response_for(&world, &config, Command::Omen);
    let folklore = response_for(&world, &config, Command::Folklore);

    assert_eq!(
        listen.response,
        format!("You stand still and listen.\n\n{ambient}")
    );
    assert_eq!(omen.response, format!("You watch for an omen.\n\n{echo}"));
    assert_eq!(
        folklore.response,
        format!("You call to mind what is said of this place.\n\n{lore}")
    );
    assert_ne!(listen.response, omen.response);
    assert_ne!(listen.response, folklore.response);
    assert_ne!(omen.response, folklore.response);
    assert!(listen.effects.is_empty());
    assert!(omen.effects.is_empty());
    assert!(folklore.effects.is_empty());
}

#[test]
fn ordinary_place_has_grounded_fallbacks_without_lore_or_an_omen() {
    // The Hurling Green has no mythological_significance in the authored world.
    let world = rundale_world_at(LocationId(5));
    let config = GameConfig::default();
    let ambient = current_vignette(&world).ambient;

    assert_eq!(
        response_for(&world, &config, Command::Listen).response,
        format!("You stand still and listen.\n\n{ambient}")
    );
    assert_eq!(
        response_for(&world, &config, Command::Omen).response,
        "You watch for an omen.\n\nNothing in the place sets itself apart from the ordinary."
    );
    assert_eq!(
        response_for(&world, &config, Command::Folklore).response,
        "You call to mind what is said of this place.\n\nNo old account of this place comes readily to mind."
    );
}

#[test]
fn supplemental_atmosphere_is_the_matching_component_without_an_action_intro() {
    let world = rundale_world_at(LocationId(1));
    let config = GameConfig::default();
    let vignette = current_vignette(&world);

    assert_eq!(
        render_place_atmosphere(
            &world,
            &config,
            crate::input::AtmosphericTopic::Listen,
            AtmospherePresentation::Supplemental,
        ),
        Some(vignette.ambient)
    );
    assert_eq!(
        render_place_atmosphere(
            &world,
            &config,
            crate::input::AtmosphericTopic::Omen,
            AtmospherePresentation::Supplemental,
        ),
        vignette.echo
    );
    assert_eq!(
        render_place_atmosphere(
            &world,
            &config,
            crate::input::AtmosphericTopic::Folklore,
            AtmospherePresentation::Supplemental,
        ),
        vignette.lore
    );
}

#[test]
fn place_listening_flag_distinguishes_standalone_from_supplemental_text() {
    let world = rundale_world_at(LocationId(1));
    let mut config = GameConfig::default();
    config.flags.disable("place-listening");

    for (topic, command) in [
        (crate::input::AtmosphericTopic::Listen, Command::Listen),
        (crate::input::AtmosphericTopic::Omen, Command::Omen),
        (crate::input::AtmosphericTopic::Folklore, Command::Folklore),
    ] {
        assert_eq!(
            response_for(&world, &config, command).response,
            "Listening to places is currently disabled."
        );
        assert_eq!(
            render_place_atmosphere(&world, &config, topic, AtmospherePresentation::Supplemental,),
            None
        );
    }
}

#[test]
fn all_three_atmosphere_actions_are_deterministic_for_the_same_scene() {
    let world = rundale_world_at(LocationId(1));
    let config = GameConfig::default();

    for command in [Command::Listen, Command::Omen, Command::Folklore] {
        let first = response_for(&world, &config, command.clone());
        let second = response_for(&world, &config, command);
        assert_eq!(first.response, second.response);
    }
}

#[test]
fn all_seven_bundled_rundale_traditions_are_player_ready() {
    let mut world = rundale_world_at(LocationId(1));
    let mut locations: Vec<_> = world
        .graph
        .location_ids()
        .into_iter()
        .filter_map(|id| world.graph.get(id))
        .filter(|location| {
            location
                .mythological_significance
                .as_deref()
                .is_some_and(|lore| !lore.trim().is_empty())
        })
        .cloned()
        .collect();
    locations.sort_by_key(|location| location.id);

    let names: Vec<_> = locations
        .iter()
        .map(|location| location.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "The Crossroads",
            "St. Brigid's Church",
            "Lough Ree Shore",
            "The Fairy Fort",
            "The Bog Road",
            "Kilteevan Village",
            "The Holy Well",
        ],
        "the documented seven-place contract must track the shipped mod"
    );

    let config = GameConfig::default();

    for location in locations {
        let authored = location.mythological_significance.as_deref().unwrap();
        world.player_location = location.id;
        let vignette = current_vignette(&world);
        assert_eq!(
            vignette.lore.as_deref(),
            Some(authored),
            "{} did not preserve its exact authored account",
            location.name
        );
        assert!(
            vignette.echo.is_some(),
            "{} did not receive a sensory echo",
            location.name
        );

        let lower = authored.to_lowercase();
        for forbidden in ["folklore", "nessie", "goddess-turned-saint"] {
            assert!(
                !lower.contains(forbidden),
                "{} exposed an anachronistic authoring term: {forbidden}",
                location.name
            );
        }
        assert!(
            !authored.contains("gave Cill Taobháin its name"),
            "the well account must not contradict the village's place-name account"
        );

        let listen = response_for(&world, &config, Command::Listen).response;
        let omen = response_for(&world, &config, Command::Omen).response;
        let folklore = response_for(&world, &config, Command::Folklore).response;
        assert_eq!(
            folklore,
            format!("You call to mind what is said of this place.\n\n{authored}"),
            "{} did not preserve its exact account",
            location.name
        );
        assert_eq!(
            omen,
            format!(
                "You watch for an omen.\n\n{}",
                vignette.echo.as_deref().unwrap()
            ),
            "{} did not render only its cautious sensory echo",
            location.name
        );
        assert_eq!(
            listen,
            format!("You stand still and listen.\n\n{}", vignette.ambient),
            "{} did not render only its ordinary soundscape",
            location.name
        );
    }
}

#[test]
fn listen_ambience_varies_with_weather() {
    let mut world = rundale_world_at(LocationId(5));
    let config = GameConfig::default();

    world.weather = Weather::Clear;
    let clear = response_for(&world, &config, Command::Listen);
    world.weather = Weather::Storm;
    let storm = response_for(&world, &config, Command::Listen);

    assert_ne!(clear.response, storm.response);
}

// ── Silent pause / resume (focus-switch) — fix #1277 ───────────────────

#[test]
fn pause_silent_pauses_clock_with_no_message() {
    // AC1 + AC3: /pause-silent freezes the clock but emits no text.
    let (mut world, mut npc, mut config) = default_state();
    assert!(!world.clock.is_paused());
    let result = handle_command(Command::PauseSilent, &mut world, &mut npc, &mut config);
    assert!(
        result.response.is_empty(),
        "PauseSilent must not emit any text; got {:?}",
        result.response
    );
    assert!(
        world.clock.is_paused(),
        "clock must be paused after PauseSilent"
    );
    assert!(
        result.effects.is_empty(),
        "PauseSilent must not produce side effects"
    );
}

#[test]
fn resume_silent_resumes_clock_with_no_message() {
    // AC2 + AC3: /resume-silent restarts the clock but emits no text.
    let (mut world, mut npc, mut config) = default_state();
    world.clock.pause();
    let result = handle_command(Command::ResumeSilent, &mut world, &mut npc, &mut config);
    assert!(
        result.response.is_empty(),
        "ResumeSilent must not emit any text; got {:?}",
        result.response
    );
    assert!(
        !world.clock.is_paused(),
        "clock must be running after ResumeSilent"
    );
    assert!(result.effects.is_empty());
}

#[test]
fn pause_command_still_emits_message() {
    // AC4: user-typed /pause still shows the full message.
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Pause, &mut world, &mut npc, &mut config);
    assert!(
        result.response.contains("stand still"),
        "explicit /pause must still emit the user-visible message"
    );
}

#[test]
fn resume_command_still_emits_message() {
    // AC4: user-typed /resume still shows the full message.
    let (mut world, mut npc, mut config) = default_state();
    world.clock.pause();
    let result = handle_command(Command::Resume, &mut world, &mut npc, &mut config);
    assert!(
        result.response.contains("stirs again"),
        "explicit /resume must still emit the user-visible message"
    );
}

#[test]
fn redundant_pause_silent_is_noop() {
    // AC6: PauseSilent while already paused is empty-response, no state change.
    let (mut world, mut npc, mut config) = default_state();
    world.clock.pause();
    let result = handle_command(Command::PauseSilent, &mut world, &mut npc, &mut config);
    assert!(result.response.is_empty());
    assert!(world.clock.is_paused());
}

#[test]
fn redundant_resume_silent_is_noop() {
    // AC6: ResumeSilent while already running is empty-response, no state change.
    let (mut world, mut npc, mut config) = default_state();
    assert!(!world.clock.is_paused());
    let result = handle_command(Command::ResumeSilent, &mut world, &mut npc, &mut config);
    assert!(result.response.is_empty());
    assert!(!world.clock.is_paused());
}

// ── focus-auto-pause flag (fix #1357) ────────────────────────────────────

#[test]
fn focus_auto_pause_flag_off_pause_silent_is_noop() {
    // AC-2: when focus-auto-pause is explicitly disabled, PauseSilent must
    // not mutate the clock.
    let (mut world, mut npc, mut config) = default_state();
    config.flags.disable("focus-auto-pause");
    assert!(!world.clock.is_paused());
    let result = handle_command(Command::PauseSilent, &mut world, &mut npc, &mut config);
    assert!(
        result.response.is_empty(),
        "PauseSilent must not emit text even when flag is off"
    );
    assert!(
        !world.clock.is_paused(),
        "clock must remain running when focus-auto-pause is disabled"
    );
}

#[test]
fn focus_auto_pause_flag_off_resume_silent_is_noop() {
    // AC-3: when focus-auto-pause is explicitly disabled, ResumeSilent must
    // not mutate the clock.
    let (mut world, mut npc, mut config) = default_state();
    config.flags.disable("focus-auto-pause");
    world.clock.pause();
    assert!(world.clock.is_paused());
    let result = handle_command(Command::ResumeSilent, &mut world, &mut npc, &mut config);
    assert!(
        result.response.is_empty(),
        "ResumeSilent must not emit text even when flag is off"
    );
    assert!(
        world.clock.is_paused(),
        "clock must remain paused when focus-auto-pause is disabled"
    );
}

#[test]
fn focus_auto_pause_flag_on_pause_silent_still_pauses() {
    // AC-1 / AC-5: flag explicitly ON behaves identically to the default
    // (unknown flag also runs the feature; default-on is implemented via
    // is_disabled, not is_enabled).
    let (mut world, mut npc, mut config) = default_state();
    config.flags.enable("focus-auto-pause");
    assert!(!world.clock.is_paused());
    let result = handle_command(Command::PauseSilent, &mut world, &mut npc, &mut config);
    assert!(result.response.is_empty());
    assert!(world.clock.is_paused(), "clock must pause when flag is on");
}

#[test]
fn focus_auto_pause_flag_explicit_pause_resume_unaffected() {
    // AC-4: /pause and /resume are not gated — the flag only controls
    // the focus-driven silent variants.
    let (mut world, mut npc, mut config) = default_state();
    config.flags.disable("focus-auto-pause");
    let r = handle_command(Command::Pause, &mut world, &mut npc, &mut config);
    assert!(
        r.response.contains("stand still"),
        "explicit /pause must work with flag off"
    );
    assert!(world.clock.is_paused());

    let r = handle_command(Command::Resume, &mut world, &mut npc, &mut config);
    assert!(
        r.response.contains("stirs again"),
        "explicit /resume must work with flag off"
    );
    assert!(!world.clock.is_paused());
}

// ── Additional coverage for previously untested Command variants ─────────

#[test]
fn about_command_returns_game_blurb() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::About, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("Parish"));
    assert!(result.response.contains("/help"));
    assert!(result.effects.is_empty());
}

#[test]
fn help_command_lists_commands() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Help, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("/help"));
    assert!(result.response.contains("/save"));
    assert!(result.response.contains("/pause"));
    let about_pos = result
        .response
        .find("/about")
        .expect("help text should include /about");
    let help_pos = result
        .response
        .find("/help")
        .expect("help text should include /help");
    let time_pos = result
        .response
        .find("/time")
        .expect("help text should include /time");
    assert!(about_pos < help_pos);
    assert!(help_pos < time_pos);
    assert!(result.effects.is_empty());
}

#[test]
fn wait_command_advances_clock() {
    let (mut world, mut npc, mut config) = default_state();
    let start = world.clock.now();
    let result = handle_command(Command::Wait(30), &mut world, &mut npc, &mut config);
    let end = world.clock.now();
    let delta = (end - start).num_minutes();
    assert_eq!(delta, 30);
    assert!(result.response.contains("30 minutes"));
}

#[test]
fn tick_command_with_empty_roster() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Tick, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("No NPC activity"));
}

#[test]
fn show_speed_reports_current_speed() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::ShowSpeed, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("Speed:"));
}

#[test]
fn set_speed_updates_clock() {
    use parish_types::time::GameSpeed;
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetSpeed(GameSpeed::Fast),
        &mut world,
        &mut npc,
        &mut config,
    );
    // Activation message should be non-empty; speed should be Fast.
    assert!(!result.response.is_empty());
    assert_eq!(world.clock.current_speed(), Some(GameSpeed::Fast));
}

#[test]
fn invalid_speed_reports_hint() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::InvalidSpeed("warp".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("warp"));
    assert!(result.response.contains("slow"));
}

#[test]
fn invalid_branch_name_returns_msg() {
    let (mut world, mut npc, mut config) = default_state();
    let msg = "Branch name too long.".to_string();
    let result = handle_command(
        Command::InvalidBranchName(msg.clone()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert_eq!(result.response, msg);
}

#[test]
fn invalid_flag_name_returns_msg() {
    let (mut world, mut npc, mut config) = default_state();
    let msg = "Flag name cannot be empty.".to_string();
    let result = handle_command(
        Command::InvalidFlagName(msg.clone()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert_eq!(result.response, msg);
}

#[test]
fn invalid_system_command_returns_helpful_error() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::InvalidSystemCommand("/not-a-command".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert_eq!(
        result.response,
        "Unknown system command: /not-a-command. Use /help to list available commands."
    );
}

#[test]
fn toggle_sidebar_returns_message() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::ToggleSidebar, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("sidebar"));
}

#[test]
fn set_model_is_removed_without_mutation() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetModel("qwen3:14b".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(config.model_name.is_empty());
    assert!(result.response.contains("schema v2"));
}

#[test]
fn show_key_not_set() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::ShowKey, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("not set"));
}

#[test]
fn show_key_masks_when_set() {
    let (mut world, mut npc, mut config) = default_state();
    config.api_key = Some("sk-abcdefghijklmnop".to_string());
    let result = handle_command(Command::ShowKey, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("API key"));
    // Full key must not leak.
    assert!(!result.response.contains("abcdefghijklmnop"));
}

#[test]
fn show_provider_reflects_config() {
    let (mut world, mut npc, mut config) = default_state();
    config.provider_name = "lmstudio".to_string();
    let result = handle_command(Command::ShowProvider, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("lmstudio"));
}

#[test]
fn set_provider_invalid_returns_error() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetProvider("bogus".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    // Invalid provider should not trigger a rebuild.
    assert!(!result.effects.contains(&CommandEffect::RebuildInference));
}

// ── Cloud provider commands ──────────────────────────────────────────────

#[test]
fn show_cloud_not_configured() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::ShowCloud, &mut world, &mut npc, &mut config);
    assert!(
        result
            .response
            .contains("removed by configuration schema v2")
    );
    assert!(result.effects.is_empty());
}

#[test]
fn show_cloud_configured() {
    let (mut world, mut npc, mut config) = default_state();
    config.cloud_provider_name = Some("openrouter".to_string());
    config.cloud_model_name = Some("anthropic/claude-3-haiku".to_string());
    let result = handle_command(Command::ShowCloud, &mut world, &mut npc, &mut config);
    assert!(
        result
            .response
            .contains("removed by configuration schema v2")
    );
    assert!(result.effects.is_empty());
}

#[test]
fn legacy_cloud_commands_are_hard_rejected_without_mutation() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetCloudProvider("openrouter".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert!(config.cloud_provider_name.is_none());
}

// ── Category-specific commands ───────────────────────────────────────────

#[test]
fn set_category_provider_is_removed_without_mutation() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetCategoryProvider(InferenceCategory::Dialogue, "openrouter".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert!(
        !config
            .category_provider
            .contains_key(&InferenceCategory::Dialogue)
    );
}

#[test]
fn set_category_model_is_removed_without_mutation() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetCategoryModel(InferenceCategory::Simulation, "mini-model".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert!(
        !config
            .category_model
            .contains_key(&InferenceCategory::Simulation)
    );
}

#[test]
fn show_category_model_inherits_base() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::ShowCategoryModel(InferenceCategory::Intent),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("inherits base"));
}

#[test]
fn show_category_key_not_set() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::ShowCategoryKey(InferenceCategory::Reaction),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("not set"));
}

#[test]
fn set_category_key_is_removed_without_storing_secret() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetCategoryKey(InferenceCategory::Dialogue, "sk-cat-key".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert!(
        !config
            .category_api_key
            .contains_key(&InferenceCategory::Dialogue)
    );
}

// ── Provider presets ────────────────────────────────────────────────────

#[test]
fn apply_preset_is_removed_without_mutating_routes() {
    let (mut world, mut npc, mut config) = default_state();
    let prior_provider = config.provider_name.clone();
    let result = handle_command(
        Command::ApplyPreset("anthropic".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert_eq!(config.provider_name, prior_provider);
    assert!(config.category_provider.is_empty());
    assert!(config.category_model.is_empty());
    assert!(config.category_base_url.is_empty());
}

#[test]
fn apply_preset_does_not_overwrite_existing_category_models() {
    let (mut world, mut npc, mut config) = default_state();
    config.category_model.insert(
        InferenceCategory::Dialogue,
        "old-dialogue-model".to_string(),
    );

    let result = handle_command(
        Command::ApplyPreset("ollama".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert_eq!(
        config
            .category_model
            .get(&InferenceCategory::Dialogue)
            .map(String::as_str),
        Some("old-dialogue-model")
    );
}

#[test]
fn apply_preset_does_not_touch_api_keys() {
    let (mut world, mut npc, mut config) = default_state();
    config.api_key = Some("sk-existing".to_string());
    config
        .category_api_key
        .insert(InferenceCategory::Dialogue, "sk-cat".to_string());

    handle_command(
        Command::ApplyPreset("anthropic".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert_eq!(config.api_key.as_deref(), Some("sk-existing"));
    assert_eq!(
        config
            .category_api_key
            .get(&InferenceCategory::Dialogue)
            .map(String::as_str),
        Some("sk-cat")
    );
}

#[test]
fn removed_preset_does_not_offer_legacy_api_key_hint() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::ApplyPreset("openai".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(!result.response.contains("API key"));
    assert!(result.effects.is_empty());
}

#[test]
fn apply_preset_no_hint_for_keyless_provider() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::ApplyPreset("ollama".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(!result.response.contains("API key"));
}

#[test]
fn apply_preset_unknown_provider_returns_error() {
    let (mut world, mut npc, mut config) = default_state();
    let prior_provider = config.provider_name.clone();
    let result = handle_command(
        Command::ApplyPreset("not-a-provider".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(!result.effects.contains(&CommandEffect::RebuildInference));
    // Config should not have been mutated on error.
    assert_eq!(config.provider_name, prior_provider);
}

#[test]
fn apply_preset_custom_is_removed_without_mutation() {
    let (mut world, mut npc, mut config) = default_state();
    let prior_provider = config.provider_name.clone();
    let result = handle_command(
        Command::ApplyPreset("custom".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert_eq!(config.provider_name, prior_provider);
}

#[test]
fn removed_set_provider_does_not_fill_models_from_preset() {
    let (mut world, mut npc, mut config) = default_state();
    let prior_provider = config.provider_name.clone();
    let result = handle_command(
        Command::SetProvider("anthropic".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert_eq!(config.provider_name, prior_provider);
    assert!(config.category_model.is_empty());
}

#[test]
fn set_provider_does_not_overwrite_existing_model() {
    let mut config = GameConfig {
        model_name: "preferred-model".to_string(),
        ..GameConfig::default()
    };
    let mut world = WorldState::new();
    let mut npc = NpcManager::new();
    handle_command(
        Command::SetProvider("anthropic".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert_eq!(config.model_name, "preferred-model");
}

#[test]
fn removed_set_category_provider_does_not_fill_model_from_preset() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::SetCategoryProvider(InferenceCategory::Intent, "anthropic".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert!(
        !config
            .category_model
            .contains_key(&InferenceCategory::Intent)
    );
}

#[test]
fn removed_ollama_preset_preserves_auto_setup_without_mutating_routes() {
    let (mut world, mut npc, mut config) = default_state();
    let prior_provider = config.provider_name.clone();
    config.auto_setup_model = Some("gemma4:e4b".to_string());
    let result = handle_command(
        Command::ApplyPreset("ollama".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert_eq!(config.provider_name, prior_provider);
    assert!(config.category_model.is_empty());
    assert_eq!(config.auto_setup_model.as_deref(), Some("gemma4:e4b"));
}

#[test]
fn removed_ollama_preset_does_not_install_static_fallbacks() {
    let (mut world, mut npc, mut config) = default_state();
    let prior_provider = config.provider_name.clone();
    config.auto_setup_model = None;
    let result = handle_command(
        Command::ApplyPreset("ollama".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("schema v2"));
    assert!(result.effects.is_empty());
    assert_eq!(config.provider_name, prior_provider);
    assert!(config.category_model.is_empty());
}

#[test]
fn show_preset_lists_providers() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::ShowPreset, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("anthropic"));
    assert!(result.response.contains("ollama"));
}

// ── Feature flags ────────────────────────────────────────────────────────

#[test]
fn flag_list_empty() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Flag(FlagSubcommand::List),
        &mut world,
        &mut npc,
        &mut config,
    );
    // Either empty-state message or flag header — depends on default flags.
    assert!(
        result.response.contains("No feature flags") || result.response.contains("Feature flags")
    );
}

#[test]
fn flag_enable_triggers_save() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Flag(FlagSubcommand::Enable("my-feature".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.effects.contains(&CommandEffect::SaveFlags));
    assert!(result.response.contains("my-feature"));
    assert!(result.response.contains("enabled"));
}

#[test]
fn flag_disable_triggers_save() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Flag(FlagSubcommand::Disable("my-feature".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.effects.contains(&CommandEffect::SaveFlags));
    assert!(result.response.contains("disabled"));
}

#[test]
fn flag_disable_reveal_unexplored_clears_active_reveal_state() {
    let (mut world, mut npc, mut config) = default_state();
    // Simulate: reveal mode is active (e.g. player ran `/unexplored reveal`).
    config.reveal_unexplored_locations = true;
    // Operator runs `/flag disable reveal-unexplored` — this must immediately
    // clear the cached reveal state, not wait for the next `/unexplored` call.
    let result = handle_command(
        Command::Flag(FlagSubcommand::Disable("reveal-unexplored".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.effects.contains(&CommandEffect::SaveFlags));
    assert!(result.response.contains("disabled"));
    assert!(
        !config.reveal_unexplored_locations,
        "reveal_unexplored_locations must be cleared immediately when the flag is disabled"
    );
}

#[test]
fn flags_alias_matches_list() {
    let (mut world, mut npc, mut config) = default_state();
    let flags_result = handle_command(Command::Flags, &mut world, &mut npc, &mut config);
    let list_result = handle_command(
        Command::Flag(FlagSubcommand::List),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert_eq!(flags_result.response, list_result.response);
}

// ── Effect-only commands ─────────────────────────────────────────────────

#[test]
fn save_returns_save_effect() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Save, &mut world, &mut npc, &mut config);
    assert!(result.response.is_empty());
    assert!(result.effects.contains(&CommandEffect::SaveGame));
}

#[test]
fn fork_returns_fork_effect() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Fork("experiment".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(
        result
            .effects
            .contains(&CommandEffect::ForkBranch("experiment".to_string()))
    );
}

#[test]
fn load_returns_load_effect() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Load("main".to_string()),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(
        result
            .effects
            .contains(&CommandEffect::LoadBranch("main".to_string()))
    );
}

#[test]
fn branches_returns_list_effect() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Branches, &mut world, &mut npc, &mut config);
    assert!(result.effects.contains(&CommandEffect::ListBranches));
}

#[test]
fn log_returns_show_log_effect() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Log, &mut world, &mut npc, &mut config);
    assert!(result.effects.contains(&CommandEffect::ShowLog));
}

#[test]
fn new_game_returns_effect() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::NewGame, &mut world, &mut npc, &mut config);
    assert!(result.effects.contains(&CommandEffect::NewGame));
}

#[test]
fn spinner_returns_effect_with_seconds() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Spinner(5), &mut world, &mut npc, &mut config);
    assert!(result.effects.contains(&CommandEffect::ShowSpinner(5)));
}

#[test]
fn debug_returns_effect_with_subcommand() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Debug(Some("schedule".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(
        result
            .effects
            .contains(&CommandEffect::Debug(Some("schedule".to_string())))
    );
}

#[test]
fn debug_no_subcommand() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Debug(None), &mut world, &mut npc, &mut config);
    assert!(result.effects.contains(&CommandEffect::Debug(None)));
}

// ── Theme ────────────────────────────────────────────────────────────────

#[test]
fn theme_no_arg_lists_available() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Theme(None), &mut world, &mut npc, &mut config);
    assert!(result.response.contains("default"));
    assert!(result.response.contains("solarized"));
    assert!(result.effects.is_empty());
}

#[test]
fn theme_default_applies_default() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Theme(Some("default".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.effects.iter().any(|e| matches!(
        e,
        CommandEffect::ApplyTheme(name, _) if name == "default"
    )));
}

#[test]
fn theme_solarized_defaults_to_auto() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Theme(Some("solarized".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.effects.iter().any(|e| matches!(
        e,
        CommandEffect::ApplyTheme(name, mode) if name == "solarized" && mode == "auto"
    )));
}

#[test]
fn theme_solarized_with_explicit_mode() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Theme(Some("solarized dark".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.effects.iter().any(|e| matches!(
        e,
        CommandEffect::ApplyTheme(name, mode) if name == "solarized" && mode == "dark"
    )));
}

#[test]
fn theme_unknown_name_returns_error() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Theme(Some("neon".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("neon"));
    assert!(result.effects.is_empty());
}

#[test]
fn theme_solarized_invalid_mode() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Theme(Some("solarized taupe".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("taupe"));
    assert!(result.effects.is_empty());
}

// ── NpcsHere with population ─────────────────────────────────────────────

#[test]
fn npcs_here_lists_present_npcs() {
    // Use the full GameTestHarness via the default state + direct roster inspection.
    // We can't cheaply populate an NpcManager from scratch here, so we only assert
    // the branch is reachable via the empty path; the populated path is covered by
    // integration tests in crates/parish-engine/tests/.
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::NpcsHere, &mut world, &mut npc, &mut config);
    // Falls through to the "No one else is here." branch.
    assert!(result.response.contains("No one"));
}

// ── /map command ───────────────────────────────────────────────────────

fn seed_tile_sources(config: &mut GameConfig) {
    config.tile_sources = vec![
        ("osm".to_string(), "OpenStreetMap".to_string()),
        (
            "historic".to_string(),
            "Historic 6\" OS Ireland (1st ed., via NLS)".to_string(),
        ),
    ];
    config.active_tile_source = "osm".to_string();
}

#[test]
fn map_list_when_no_arg() {
    let (mut world, mut npc, mut config) = default_state();
    seed_tile_sources(&mut config);
    let result = handle_command(Command::Map(None), &mut world, &mut npc, &mut config);
    assert!(result.response.contains("osm"));
    assert!(result.response.contains("historic"));
    assert!(result.response.contains("(active)"));
    assert!(result.effects.is_empty());
}

#[test]
fn map_list_empty_registry() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Map(None), &mut world, &mut npc, &mut config);
    assert!(result.response.contains("No tile sources configured"));
    assert!(result.effects.is_empty());
}

#[test]
fn map_switch_sets_config_and_emits_effect() {
    let (mut world, mut npc, mut config) = default_state();
    seed_tile_sources(&mut config);
    let result = handle_command(
        Command::Map(Some("historic".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert_eq!(config.active_tile_source, "historic");
    assert!(result.response.contains("Switched"));
    assert_eq!(
        result.effects,
        vec![CommandEffect::ApplyTiles("historic".to_string())]
    );
}

#[test]
fn map_switch_is_case_insensitive() {
    let (mut world, mut npc, mut config) = default_state();
    seed_tile_sources(&mut config);
    let result = handle_command(
        Command::Map(Some("OSM".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert_eq!(config.active_tile_source, "osm");
    assert_eq!(
        result.effects,
        vec![CommandEffect::ApplyTiles("osm".to_string())]
    );
}

#[test]
fn map_unknown_id_returns_error_text() {
    let (mut world, mut npc, mut config) = default_state();
    seed_tile_sources(&mut config);
    let result = handle_command(
        Command::Map(Some("made-up".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("Unknown"));
    assert!(result.response.contains("made-up"));
    assert!(result.response.contains("osm"));
    assert!(result.effects.is_empty());
    assert_eq!(config.active_tile_source, "osm", "active unchanged");
}

#[test]
fn map_disabled_flag_returns_refusal() {
    let (mut world, mut npc, mut config) = default_state();
    seed_tile_sources(&mut config);
    config.flags.disable("period-map-tiles");
    let result = handle_command(
        Command::Map(Some("historic".to_string())),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("/flag enable"));
    assert!(result.effects.is_empty());
    assert_eq!(config.active_tile_source, "osm", "active unchanged");
}

#[test]
fn map_help_lists_command() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Help, &mut world, &mut npc, &mut config);
    assert!(result.response.contains("/map"));
    assert!(result.response.contains("/unexplored"));
}

#[test]
fn unexplored_reveal_updates_config() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(
        Command::Unexplored(Some(true)),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(config.reveal_unexplored_locations);
    assert!(result.response.contains("revealed"));
    assert!(result.effects.is_empty());
}

#[test]
fn unexplored_hide_updates_config() {
    let (mut world, mut npc, mut config) = default_state();
    config.reveal_unexplored_locations = true;
    let result = handle_command(
        Command::Unexplored(Some(false)),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(!config.reveal_unexplored_locations);
    assert!(result.response.contains("hidden"));
    assert!(result.effects.is_empty());
}

#[test]
fn unexplored_none_reports_status_and_usage() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Unexplored(None), &mut world, &mut npc, &mut config);
    assert!(result.response.contains("currently hidden"));
    assert!(result.response.contains("/unexplored reveal|hide"));
    assert!(result.effects.is_empty());
}

#[test]
fn unexplored_disabled_flag_returns_refusal() {
    let (mut world, mut npc, mut config) = default_state();
    config.flags.disable("reveal-unexplored");
    let result = handle_command(
        Command::Unexplored(Some(true)),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("/flag enable"));
    assert!(result.effects.is_empty());
    assert!(!config.reveal_unexplored_locations);
}

/// Codex P1: disabling the flag while reveal is already active must clear
/// `reveal_unexplored_locations`, making the kill-switch effective.
/// Previously the early return left the boolean true, so map rendering
/// continued to show unexplored areas even though the feature flag was off.
#[test]
fn unexplored_disabled_flag_clears_active_reveal_state() {
    let (mut world, mut npc, mut config) = default_state();
    // Simulate: player ran `/unexplored reveal` while flag was enabled.
    config.reveal_unexplored_locations = true;
    // Now an operator disables the feature flag.
    config.flags.disable("reveal-unexplored");
    // Any attempt to use /unexplored should clear reveal state, not just refuse.
    let result = handle_command(
        Command::Unexplored(Some(true)),
        &mut world,
        &mut npc,
        &mut config,
    );
    assert!(result.response.contains("/flag enable"));
    assert!(result.effects.is_empty());
    // Kill-switch must be complete: reveal state cleared even though we
    // could not execute the command.
    assert!(
        !config.reveal_unexplored_locations,
        "reveal_unexplored_locations must be false when the feature flag is disabled"
    );
}

#[test]
fn unexplored_disabled_flag_clears_active_reveal() {
    let (mut world, mut npc, mut config) = default_state();
    config.reveal_unexplored_locations = true;
    config.flags.disable("reveal-unexplored");
    let result = handle_command(Command::Unexplored(None), &mut world, &mut npc, &mut config);
    assert!(result.response.contains("/flag enable"));
    assert!(
        !config.reveal_unexplored_locations,
        "should clear reveal state when flag is disabled"
    );
}

#[test]
fn help_output_is_tabular_and_column_aligned() {
    let (mut world, mut npc, mut config) = default_state();
    let result = handle_command(Command::Help, &mut world, &mut npc, &mut config);

    assert_eq!(result.presentation, TextPresentation::Tabular);

    // Every row after the "Available commands:" header must contain
    // exactly one em-dash separator, and all em-dashes must share the
    // same character column — that's what makes the list tabular in a
    // monospace font.
    let mut dash_col: Option<usize> = None;
    for line in result.response.lines().skip(1) {
        let matches: Vec<usize> = line.match_indices('—').map(|(i, _)| i).collect();
        assert_eq!(
            matches.len(),
            1,
            "help row should contain exactly one em-dash: {:?}",
            line
        );
        let col = line[..matches[0]].chars().count();
        match dash_col {
            None => dash_col = Some(col),
            Some(expected) => {
                assert_eq!(col, expected, "em-dash column mismatch on row: {:?}", line)
            }
        }
    }
    assert!(dash_col.is_some(), "help body had no rows");
}
