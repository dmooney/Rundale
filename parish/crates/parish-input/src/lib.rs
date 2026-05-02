//! Player input parsing and command detection.
//!
//! System commands use `/` prefix (e.g., `/quit`, `/save`).
//! All other input is natural language sent to the LLM for
//! intent parsing (move, talk, look, interact, examine).

mod commands;
mod intent_llm;
mod intent_local;
mod intent_types;
mod mention;
mod parser;

pub use commands::{Command, FlagSubcommand};
pub use intent_llm::parse_intent;
pub use intent_local::parse_intent_local;
pub use intent_types::{InputResult, IntentKind, PlayerIntent};
pub use mention::{MentionExtraction, extract_mention};
pub use parser::{classify_input, parse_system_command};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::validate_branch_name;
    use crate::intent_llm::IntentResponse;
    use parish_config::InferenceCategory;
    use parish_types::GameSpeed;

    #[test]
    fn test_parse_quit() {
        assert_eq!(parse_system_command("/quit"), Some(Command::Quit));
        assert_eq!(parse_system_command("/QUIT"), Some(Command::Quit));
        assert_eq!(parse_system_command("  /quit  "), Some(Command::Quit));
    }

    #[test]
    fn test_parse_fork() {
        assert_eq!(
            parse_system_command("/fork main"),
            Some(Command::Fork("main".to_string()))
        );
        assert_eq!(
            parse_system_command("/fork  my save "),
            Some(Command::Fork("my save".to_string()))
        );
    }

    #[test]
    fn test_parse_load() {
        assert_eq!(
            parse_system_command("/load main"),
            Some(Command::Load("main".to_string()))
        );
    }

    #[test]
    fn test_parse_all_commands() {
        assert_eq!(parse_system_command("/pause"), Some(Command::Pause));
        assert_eq!(parse_system_command("/resume"), Some(Command::Resume));
        assert_eq!(parse_system_command("/save"), Some(Command::Save));
        assert_eq!(parse_system_command("/branches"), Some(Command::Branches));
        assert_eq!(parse_system_command("/log"), Some(Command::Log));
        assert_eq!(parse_system_command("/status"), Some(Command::Status));
        assert_eq!(parse_system_command("/help"), Some(Command::Help));
    }

    #[test]
    fn test_parse_unknown_command() {
        assert_eq!(parse_system_command("/unknown"), None);
        assert_eq!(parse_system_command("quit"), None);
        assert_eq!(parse_system_command("go to pub"), None);
    }

    #[test]
    fn test_parse_fork_empty_name() {
        // Bare /fork with only whitespace returns Help command
        assert_eq!(parse_system_command("/fork "), Some(Command::Help));
        assert_eq!(parse_system_command("/fork   "), Some(Command::Help));
    }

    #[test]
    fn test_classify_system_command() {
        assert_eq!(
            classify_input("/quit"),
            InputResult::SystemCommand(Command::Quit)
        );
        assert_eq!(
            classify_input("/fork main"),
            InputResult::SystemCommand(Command::Fork("main".to_string()))
        );
    }

    #[test]
    fn test_classify_game_input() {
        assert_eq!(
            classify_input("go to the pub"),
            InputResult::GameInput("go to the pub".to_string())
        );
        assert_eq!(
            classify_input("tell Mary hello"),
            InputResult::GameInput("tell Mary hello".to_string())
        );
    }

    #[test]
    fn test_classify_unknown_slash_command() {
        // Unknown /commands fall through as game input
        assert_eq!(
            classify_input("/dance"),
            InputResult::GameInput("/dance".to_string())
        );
    }

    #[test]
    fn test_intent_kind_deserialize() {
        let json = r#""move""#;
        let kind: IntentKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, IntentKind::Move);

        let json = r#""talk""#;
        let kind: IntentKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, IntentKind::Talk);

        let json = r#""unknown""#;
        let kind: IntentKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, IntentKind::Unknown);
    }

    #[test]
    fn test_intent_response_deserialize() {
        let json = r#"{"intent": "move", "target": "the pub", "dialogue": null}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.intent, Some(IntentKind::Move));
        assert_eq!(resp.target, Some("the pub".to_string()));
        assert!(resp.dialogue.is_none());
    }

    #[test]
    fn test_intent_response_empty() {
        let json = r#"{}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert!(resp.intent.is_none());
        assert!(resp.target.is_none());
        assert!(resp.dialogue.is_none());
    }

    #[test]
    fn test_classify_whitespace() {
        assert_eq!(
            classify_input("  /quit  "),
            InputResult::SystemCommand(Command::Quit)
        );
        assert_eq!(
            classify_input("  hello  "),
            InputResult::GameInput("hello".to_string())
        );
    }

    #[test]
    fn test_local_parse_go_to() {
        let intent = parse_intent_local("go to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));
    }

    #[test]
    fn test_local_parse_walk_to() {
        let intent = parse_intent_local("walk to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));
    }

    #[test]
    fn test_local_parse_go_shorthand() {
        let intent = parse_intent_local("go pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }

    #[test]
    fn test_local_parse_head_to() {
        let intent = parse_intent_local("head to Murphy's Farm").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("murphy's farm".to_string()));
    }

    #[test]
    fn test_local_parse_visit() {
        let intent = parse_intent_local("visit the fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the fairy fort".to_string()));
    }

    #[test]
    fn test_local_parse_look() {
        let intent = parse_intent_local("look").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);

        let intent = parse_intent_local("look around").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);

        let intent = parse_intent_local("l").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);
    }

    #[test]
    fn test_local_parse_case_insensitive() {
        let intent = parse_intent_local("GO TO THE PUB").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("LOOK").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);
    }

    #[test]
    fn test_local_parse_no_match() {
        assert!(parse_intent_local("tell Mary hello").is_none());
        assert!(parse_intent_local("pick up the stone").is_none());
        assert!(parse_intent_local("hello there").is_none());
    }

    #[test]
    fn test_local_parse_first_person_narrative_is_talk() {
        // First-person statements that mention place names must not be
        // interpreted as move commands (regression: "I came from the coast"
        // was triggering navigation to Lough Ree Shore).
        let intent = parse_intent_local("I came from the coast").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);
        assert_eq!(intent.target, None);
        assert_eq!(intent.dialogue, Some("I came from the coast".to_string()));

        let intent = parse_intent_local("I was at the shore yesterday").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);

        let intent = parse_intent_local("I'm not from around here").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);

        let intent = parse_intent_local("I've been to the pub before").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);

        // Bare "I" with no continuation is also talk
        let intent = parse_intent_local("I").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);
    }

    #[test]
    fn test_local_parse_empty_target() {
        // "go to " with nothing after should match "go " prefix with target "to",
        // which is fine — the world graph won't find "to" and will say not found.
        // But bare "go" or "walk" with no target should not match.
        assert!(parse_intent_local("go").is_none());
        assert!(parse_intent_local("walk").is_none());
    }

    #[test]
    fn test_local_parse_saunter() {
        let intent = parse_intent_local("saunter to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("saunter pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }

    #[test]
    fn test_local_parse_mosey() {
        let intent = parse_intent_local("mosey to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("mosey church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("church".to_string()));
    }

    #[test]
    fn test_local_parse_wander() {
        let intent = parse_intent_local("wander to the crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the crossroads".to_string()));

        let intent = parse_intent_local("wander crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("crossroads".to_string()));
    }

    #[test]
    fn test_local_parse_stroll() {
        let intent = parse_intent_local("stroll to the fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the fairy fort".to_string()));

        let intent = parse_intent_local("stroll fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("fairy fort".to_string()));
    }

    #[test]
    fn test_local_parse_amble() {
        let intent = parse_intent_local("amble to the village green").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the village green".to_string()));

        let intent = parse_intent_local("amble village green").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("village green".to_string()));
    }

    #[test]
    fn test_local_parse_trek_and_hike() {
        let intent = parse_intent_local("trek to the bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the bog".to_string()));

        let intent = parse_intent_local("hike to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));

        let intent = parse_intent_local("trek bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("bog".to_string()));
    }

    #[test]
    fn test_local_parse_run_jog_dash() {
        let intent = parse_intent_local("run to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("jog to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("dash to the crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the crossroads".to_string()));

        // Without "to"
        let intent = parse_intent_local("run pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }

    #[test]
    fn test_local_parse_hurry_rush() {
        let intent = parse_intent_local("hurry to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("rush to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("hurry pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }

    #[test]
    fn test_local_parse_proceed() {
        let intent = parse_intent_local("proceed to the town square").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the town square".to_string()));

        let intent = parse_intent_local("proceed town square").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("town square".to_string()));
    }

    #[test]
    fn test_local_parse_multi_word_phrases() {
        let intent = parse_intent_local("make my way to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("make my way pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));

        let intent = parse_intent_local("head over to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("head over church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("church".to_string()));

        let intent = parse_intent_local("pop over to the shop").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the shop".to_string()));

        let intent = parse_intent_local("pop over shop").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("shop".to_string()));

        let intent = parse_intent_local("nip to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("swing by the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));
    }

    #[test]
    fn test_local_parse_sprint_march_traipse() {
        let intent = parse_intent_local("sprint to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("march to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("traipse to the bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the bog".to_string()));
    }

    #[test]
    fn test_local_parse_meander_trot_stride() {
        let intent = parse_intent_local("meander to the river").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the river".to_string()));

        let intent = parse_intent_local("trot to the farm").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the farm".to_string()));

        let intent = parse_intent_local("stride to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));
    }

    #[test]
    fn test_local_parse_creep_sneak_bolt_scramble() {
        let intent = parse_intent_local("creep to the graveyard").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the graveyard".to_string()));

        let intent = parse_intent_local("sneak to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("bolt to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("scramble to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));
    }

    #[test]
    fn test_local_parse_unusual_verbs_case_insensitive() {
        let intent = parse_intent_local("SAUNTER TO THE PUB").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("Mosey To The Church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("WANDER crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("crossroads".to_string()));
    }

    #[test]
    fn test_local_parse_bare_unusual_verbs_no_target() {
        // Bare verbs without a target should not match
        assert!(parse_intent_local("saunter").is_none());
        assert!(parse_intent_local("mosey").is_none());
        assert!(parse_intent_local("wander").is_none());
        assert!(parse_intent_local("stroll").is_none());
        assert!(parse_intent_local("amble").is_none());
        assert!(parse_intent_local("run").is_none());
        assert!(parse_intent_local("dash").is_none());
    }

    #[test]
    fn test_parse_irish_command() {
        let cmd = parse_system_command("/irish");
        assert_eq!(cmd, Some(Command::ToggleSidebar));
    }

    #[test]
    fn test_parse_irish_command_case_insensitive() {
        let cmd = parse_system_command("/IRISH");
        assert_eq!(cmd, Some(Command::ToggleSidebar));
    }

    #[test]
    fn test_classify_irish_command() {
        let result = classify_input("/irish");
        assert_eq!(result, InputResult::SystemCommand(Command::ToggleSidebar));
    }

    #[test]
    fn test_parse_improv_command() {
        let cmd = parse_system_command("/improv");
        assert_eq!(cmd, Some(Command::ToggleImprov));
    }

    #[test]
    fn test_parse_improv_command_case_insensitive() {
        let cmd = parse_system_command("/IMPROV");
        assert_eq!(cmd, Some(Command::ToggleImprov));
    }

    #[test]
    fn test_classify_improv_command() {
        let result = classify_input("/improv");
        assert_eq!(result, InputResult::SystemCommand(Command::ToggleImprov));
    }

    #[test]
    fn test_parse_about_command() {
        assert_eq!(parse_system_command("/about"), Some(Command::About));
    }

    #[test]
    fn test_parse_about_command_case_insensitive() {
        assert_eq!(parse_system_command("/ABOUT"), Some(Command::About));
    }

    #[test]
    fn test_parse_map_command() {
        assert_eq!(parse_system_command("/map"), Some(Command::Map(None)));
        assert_eq!(parse_system_command("/map   "), Some(Command::Map(None)));
        assert_eq!(
            parse_system_command("/map osm"),
            Some(Command::Map(Some("osm".to_string())))
        );
        assert_eq!(
            parse_system_command("/map historic"),
            Some(Command::Map(Some("historic".to_string())))
        );
    }

    #[test]
    fn test_parse_map_command_case_insensitive() {
        assert_eq!(parse_system_command("/MAP"), Some(Command::Map(None)));
        assert_eq!(
            parse_system_command("/MAP OSM"),
            Some(Command::Map(Some("OSM".to_string())))
        );
    }

    #[test]
    fn test_classify_map_command() {
        let result = classify_input("/map");
        assert_eq!(result, InputResult::SystemCommand(Command::Map(None)));
    }

    #[test]
    fn test_parse_designer_command() {
        assert_eq!(parse_system_command("/designer"), Some(Command::Designer));
        assert_eq!(parse_system_command("/DESIGNER"), Some(Command::Designer));
        assert_eq!(
            parse_system_command("  /designer  "),
            Some(Command::Designer)
        );
    }

    #[test]
    fn test_parse_npcs_command() {
        assert_eq!(parse_system_command("/npcs"), Some(Command::NpcsHere));
    }

    #[test]
    fn test_parse_time_command() {
        assert_eq!(parse_system_command("/time"), Some(Command::Time));
    }

    #[test]
    fn test_parse_where_command() {
        assert_eq!(parse_system_command("/where"), Some(Command::Status));
    }

    #[test]
    fn test_parse_wait_command() {
        assert_eq!(parse_system_command("/wait"), Some(Command::Wait(15)));
        assert_eq!(parse_system_command("/wait 60"), Some(Command::Wait(60)));
        assert_eq!(parse_system_command("/wait abc"), Some(Command::Wait(15)));
    }

    #[test]
    fn test_parse_new_command() {
        assert_eq!(parse_system_command("/new"), Some(Command::NewGame));
    }

    #[test]
    fn test_parse_tick_command() {
        assert_eq!(parse_system_command("/tick"), Some(Command::Tick));
    }

    #[test]
    fn test_parse_theme_command() {
        assert_eq!(parse_system_command("/theme"), Some(Command::Theme(None)));
        assert_eq!(
            parse_system_command("/theme default"),
            Some(Command::Theme(Some("default".to_string())))
        );
        assert_eq!(
            parse_system_command("/theme solarized"),
            Some(Command::Theme(Some("solarized".to_string())))
        );
        assert_eq!(
            parse_system_command("/theme solarized light"),
            Some(Command::Theme(Some("solarized light".to_string())))
        );
        assert_eq!(
            parse_system_command("/theme solarized dark"),
            Some(Command::Theme(Some("solarized dark".to_string())))
        );
        assert_eq!(
            parse_system_command("/theme solarized auto"),
            Some(Command::Theme(Some("solarized auto".to_string())))
        );
        assert_eq!(
            parse_system_command("/THEME Solarized Dark"),
            Some(Command::Theme(Some("Solarized Dark".to_string())))
        );
    }

    #[test]
    fn test_parse_unexplored_command() {
        assert_eq!(
            parse_system_command("/unexplored"),
            Some(Command::Unexplored(None))
        );
        assert_eq!(
            parse_system_command("/unexplored reveal"),
            Some(Command::Unexplored(Some(true)))
        );
        assert_eq!(
            parse_system_command("/unexplored hide"),
            Some(Command::Unexplored(Some(false)))
        );
        assert_eq!(
            parse_system_command("/unexplored on"),
            Some(Command::Unexplored(Some(true)))
        );
        assert_eq!(
            parse_system_command("/unexplored off"),
            Some(Command::Unexplored(Some(false)))
        );
        assert_eq!(
            parse_system_command("/unexplored whatever"),
            Some(Command::Unexplored(None))
        );
    }

    #[test]
    fn test_parse_provider_show() {
        assert_eq!(
            parse_system_command("/provider"),
            Some(Command::ShowProvider)
        );
        assert_eq!(
            parse_system_command("/provider   "),
            Some(Command::ShowProvider)
        );
    }

    #[test]
    fn test_parse_provider_set() {
        assert_eq!(
            parse_system_command("/provider openrouter"),
            Some(Command::SetProvider("openrouter".to_string()))
        );
        assert_eq!(
            parse_system_command("/provider  ollama "),
            Some(Command::SetProvider("ollama".to_string()))
        );
    }

    #[test]
    fn test_parse_model_show() {
        assert_eq!(parse_system_command("/model"), Some(Command::ShowModel));
    }

    #[test]
    fn test_parse_model_set() {
        assert_eq!(
            parse_system_command("/model google/gemma-3-1b-it:free"),
            Some(Command::SetModel("google/gemma-3-1b-it:free".to_string()))
        );
    }

    #[test]
    fn test_parse_key_show() {
        assert_eq!(parse_system_command("/key"), Some(Command::ShowKey));
    }

    #[test]
    fn test_parse_key_set() {
        assert_eq!(
            parse_system_command("/key sk-or-v1-abc123"),
            Some(Command::SetKey("sk-or-v1-abc123".to_string()))
        );
    }

    #[test]
    fn test_parse_preset_show_bare() {
        assert_eq!(parse_system_command("/preset"), Some(Command::ShowPreset));
        assert_eq!(
            parse_system_command("/preset   "),
            Some(Command::ShowPreset)
        );
    }

    #[test]
    fn test_parse_preset_apply() {
        assert_eq!(
            parse_system_command("/preset anthropic"),
            Some(Command::ApplyPreset("anthropic".to_string()))
        );
        assert_eq!(
            parse_system_command("/preset  ollama "),
            Some(Command::ApplyPreset("ollama".to_string()))
        );
    }

    #[test]
    fn test_parse_preset_case_insensitive() {
        // The /preset prefix is matched case-insensitively, but the argument
        // is preserved verbatim — Provider::from_str_loose handles casing.
        assert_eq!(
            parse_system_command("/PRESET Anthropic"),
            Some(Command::ApplyPreset("Anthropic".to_string()))
        );
    }

    #[test]
    fn test_parse_provider_case_insensitive() {
        assert_eq!(
            parse_system_command("/PROVIDER"),
            Some(Command::ShowProvider)
        );
        assert_eq!(
            parse_system_command("/Provider OpenRouter"),
            Some(Command::SetProvider("OpenRouter".to_string()))
        );
    }

    #[test]
    fn test_parse_cloud_show() {
        assert_eq!(parse_system_command("/cloud"), Some(Command::ShowCloud));
    }

    #[test]
    fn test_parse_cloud_provider_set() {
        assert_eq!(
            parse_system_command("/cloud provider openrouter"),
            Some(Command::SetCloudProvider("openrouter".to_string()))
        );
    }

    #[test]
    fn test_parse_cloud_model_show() {
        assert_eq!(
            parse_system_command("/cloud model"),
            Some(Command::ShowCloudModel)
        );
    }

    #[test]
    fn test_parse_cloud_model_set() {
        assert_eq!(
            parse_system_command("/cloud model anthropic/claude-sonnet-4-20250514"),
            Some(Command::SetCloudModel(
                "anthropic/claude-sonnet-4-20250514".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_cloud_key_show() {
        assert_eq!(
            parse_system_command("/cloud key"),
            Some(Command::ShowCloudKey)
        );
    }

    #[test]
    fn test_parse_cloud_key_set() {
        assert_eq!(
            parse_system_command("/cloud key sk-test-key"),
            Some(Command::SetCloudKey("sk-test-key".to_string()))
        );
    }

    #[test]
    fn test_parse_cloud_unknown_subcommand() {
        // Unknown subcommands show cloud status
        assert_eq!(
            parse_system_command("/cloud foobar"),
            Some(Command::ShowCloud)
        );
    }

    #[test]
    fn test_parse_speed_show() {
        assert_eq!(parse_system_command("/speed"), Some(Command::ShowSpeed));
    }

    #[test]
    fn test_parse_speed_set_variants() {
        assert_eq!(
            parse_system_command("/speed slow"),
            Some(Command::SetSpeed(GameSpeed::Slow))
        );
        assert_eq!(
            parse_system_command("/speed normal"),
            Some(Command::SetSpeed(GameSpeed::Normal))
        );
        assert_eq!(
            parse_system_command("/speed fast"),
            Some(Command::SetSpeed(GameSpeed::Fast))
        );
        assert_eq!(
            parse_system_command("/speed fastest"),
            Some(Command::SetSpeed(GameSpeed::Fastest))
        );
    }

    #[test]
    fn test_parse_speed_case_insensitive() {
        assert_eq!(
            parse_system_command("/speed FAST"),
            Some(Command::SetSpeed(GameSpeed::Fast))
        );
        assert_eq!(
            parse_system_command("/speed Slow"),
            Some(Command::SetSpeed(GameSpeed::Slow))
        );
        assert_eq!(
            parse_system_command("/SPEED normal"),
            Some(Command::SetSpeed(GameSpeed::Normal))
        );
    }

    #[test]
    fn test_parse_speed_invalid_shows_error() {
        assert_eq!(
            parse_system_command("/speed bogus"),
            Some(Command::InvalidSpeed("bogus".to_string()))
        );
    }

    #[test]
    fn test_parse_speed_whitespace_shows_current() {
        assert_eq!(parse_system_command("/speed   "), Some(Command::ShowSpeed));
    }

    // --- extract_mention tests ---

    #[test]
    fn test_extract_mention_simple_name() {
        let result = extract_mention("@Padraig hello there").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello there");
    }

    #[test]
    fn test_extract_mention_full_name() {
        let result = extract_mention("@Padraig Darcy hello").unwrap();
        assert_eq!(result.name, "Padraig Darcy");
        assert_eq!(result.remaining, "hello");
    }

    #[test]
    fn test_extract_mention_name_only() {
        let result = extract_mention("@Padraig").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "");
    }

    #[test]
    fn test_extract_mention_no_at() {
        assert!(extract_mention("hello there").is_none());
    }

    #[test]
    fn test_extract_mention_at_mid_input() {
        let result = extract_mention("hello @Padraig").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello");
    }

    #[test]
    fn test_extract_mention_at_not_after_space() {
        assert!(extract_mention("email@Padraig").is_none());
    }

    #[test]
    fn test_extract_mention_bare_at() {
        assert!(extract_mention("@").is_none());
    }

    #[test]
    fn test_extract_mention_at_space() {
        assert!(extract_mention("@ hello").is_none());
    }

    #[test]
    fn test_extract_mention_with_sentence() {
        let result = extract_mention("@Siobhan how are you today?").unwrap();
        assert_eq!(result.name, "Siobhan");
        assert_eq!(result.remaining, "how are you today?");
    }

    #[test]
    fn test_extract_mention_whitespace_trimmed() {
        let result = extract_mention("  @Padraig  hello  ").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello");
    }

    #[test]
    fn test_extract_mention_mid_with_rest() {
        let result = extract_mention("hello @Padraig how are you").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello how are you");
    }

    #[test]
    fn test_validate_branch_name_valid() {
        assert!(validate_branch_name("my-save").is_ok());
        assert!(validate_branch_name("save_1").is_ok());
        assert!(validate_branch_name("My Save Game").is_ok());
    }

    #[test]
    fn test_validate_branch_name_too_long() {
        let long_name = "a".repeat(256);
        assert!(validate_branch_name(&long_name).is_err());
    }

    #[test]
    fn test_validate_branch_name_invalid_chars() {
        assert!(validate_branch_name("save/game").is_err());
        assert!(validate_branch_name("save;drop").is_err());
        assert!(validate_branch_name("../../etc").is_err());
    }

    #[test]
    fn test_fork_with_invalid_branch_name() {
        assert_eq!(
            parse_system_command("/fork ../../etc"),
            Some(Command::InvalidBranchName(
                "Branch names may only contain letters, numbers, spaces, underscores, and hyphens."
                    .to_string()
            ))
        );
    }

    // --- /debug command tests ---

    #[test]
    fn test_parse_debug_bare() {
        assert_eq!(parse_system_command("/debug"), Some(Command::Debug(None)));
    }

    #[test]
    fn test_parse_debug_with_subcommand() {
        assert_eq!(
            parse_system_command("/debug npcs"),
            Some(Command::Debug(Some("npcs".to_string())))
        );
        assert_eq!(
            parse_system_command("/debug memory Padraig"),
            Some(Command::Debug(Some("memory Padraig".to_string())))
        );
    }

    #[test]
    fn test_parse_debug_with_empty_trailing_space() {
        assert_eq!(
            parse_system_command("/debug   "),
            Some(Command::Debug(None))
        );
    }

    #[test]
    fn test_parse_debug_case_insensitive() {
        assert_eq!(parse_system_command("/DEBUG"), Some(Command::Debug(None)));
        assert_eq!(
            parse_system_command("/DEBUG npcs"),
            Some(Command::Debug(Some("npcs".to_string())))
        );
    }

    // --- /spinner command tests ---

    #[test]
    fn test_parse_spinner_bare() {
        assert_eq!(parse_system_command("/spinner"), Some(Command::Spinner(30)));
    }

    #[test]
    fn test_parse_spinner_with_duration() {
        assert_eq!(
            parse_system_command("/spinner 10"),
            Some(Command::Spinner(10))
        );
        assert_eq!(
            parse_system_command("/spinner 120"),
            Some(Command::Spinner(120))
        );
    }

    #[test]
    fn test_parse_spinner_invalid_duration() {
        // Non-numeric falls back to 30
        assert_eq!(
            parse_system_command("/spinner abc"),
            Some(Command::Spinner(30))
        );
    }

    // --- category command tests ---
    //
    // Table-driven: all four InferenceCategory variants × three verbs (model, provider, key)
    // × two operations (show, set).  If a new category is added to InferenceCategory::ALL the
    // compiler will NOT remind you to add tests here — keep the ALL_CATS slice in sync manually.

    #[test]
    fn test_parse_category_all_show_and_set() {
        // (category name slug, InferenceCategory variant, show/set examples)
        type ShowFn = fn(InferenceCategory) -> Command;
        type SetFn = fn(InferenceCategory, String) -> Command;

        struct Case {
            slug: &'static str,
            cat: InferenceCategory,
        }
        let cases = [
            Case {
                slug: "dialogue",
                cat: InferenceCategory::Dialogue,
            },
            Case {
                slug: "simulation",
                cat: InferenceCategory::Simulation,
            },
            Case {
                slug: "intent",
                cat: InferenceCategory::Intent,
            },
            Case {
                slug: "reaction",
                cat: InferenceCategory::Reaction,
            },
        ];

        let verbs: &[(&str, ShowFn, SetFn)] = &[
            (
                "model",
                Command::ShowCategoryModel as ShowFn,
                Command::SetCategoryModel as SetFn,
            ),
            (
                "provider",
                Command::ShowCategoryProvider as ShowFn,
                Command::SetCategoryProvider as SetFn,
            ),
            (
                "key",
                Command::ShowCategoryKey as ShowFn,
                Command::SetCategoryKey as SetFn,
            ),
        ];

        for case in &cases {
            for (verb, show_fn, set_fn) in verbs {
                // show (bare command)
                let show_input = format!("/{}.{}", verb, case.slug);
                assert_eq!(
                    parse_system_command(&show_input),
                    Some(show_fn(case.cat)),
                    "show failed for {}.{}",
                    verb,
                    case.slug
                );

                // set (command with argument)
                let set_input = format!("/{}.{} test-value", verb, case.slug);
                assert_eq!(
                    parse_system_command(&set_input),
                    Some(set_fn(case.cat, "test-value".to_string())),
                    "set failed for {}.{}",
                    verb,
                    case.slug
                );
            }
        }
    }

    #[test]
    fn test_parse_category_invalid_category_returns_none() {
        // Invalid category name should not match
        assert_eq!(parse_system_command("/model.bogus"), None);
        assert_eq!(parse_system_command("/provider.bogus"), None);
        assert_eq!(parse_system_command("/key.bogus"), None);
    }

    // --- /load edge cases ---

    #[test]
    fn test_parse_load_empty_shows_picker() {
        assert_eq!(
            parse_system_command("/load"),
            Some(Command::Load(String::new()))
        );
        assert_eq!(
            parse_system_command("/load  "),
            Some(Command::Load(String::new()))
        );
    }

    #[test]
    fn test_load_with_invalid_branch_name() {
        assert_eq!(
            parse_system_command("/load ../../etc"),
            Some(Command::InvalidBranchName(
                "Branch names may only contain letters, numbers, spaces, underscores, and hyphens."
                    .to_string()
            ))
        );
    }

    // --- /cloud edge cases ---

    #[test]
    fn test_parse_cloud_provider_show_bare() {
        // "/cloud provider" without a name shows cloud info
        assert_eq!(
            parse_system_command("/cloud provider"),
            Some(Command::ShowCloud)
        );
    }

    #[test]
    fn test_parse_cloud_provider_empty_name() {
        // "/cloud provider  " with only whitespace shows cloud info
        assert_eq!(
            parse_system_command("/cloud provider  "),
            Some(Command::ShowCloud)
        );
    }

    #[test]
    fn test_parse_cloud_model_empty_name() {
        assert_eq!(
            parse_system_command("/cloud model  "),
            Some(Command::ShowCloudModel)
        );
    }

    #[test]
    fn test_parse_cloud_key_empty_name() {
        assert_eq!(
            parse_system_command("/cloud key  "),
            Some(Command::ShowCloudKey)
        );
    }

    // --- validate_branch_name edge cases ---

    #[test]
    fn test_validate_branch_name_at_max_length() {
        let name = "a".repeat(255);
        assert!(validate_branch_name(&name).is_ok());
    }

    #[test]
    fn test_validate_branch_name_just_over_max() {
        let name = "a".repeat(256);
        let err = validate_branch_name(&name).unwrap_err();
        assert!(err.contains("max 255"));
    }

    #[test]
    fn test_validate_branch_name_with_special_chars() {
        assert!(validate_branch_name("save!game").is_err());
        assert!(validate_branch_name("save@game").is_err());
        assert!(validate_branch_name("save#game").is_err());
        assert!(validate_branch_name("save.game").is_err());
    }

    // --- /speed ludicrous ---

    #[test]
    fn test_parse_speed_ludicrous() {
        assert_eq!(
            parse_system_command("/speed ludicrous"),
            Some(Command::SetSpeed(GameSpeed::Ludicrous))
        );
    }
}
