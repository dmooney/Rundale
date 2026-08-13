//! Tests for the NPC arrival reaction system.

use super::selection::{generate_arrival_reactions, is_negative_mood, reaction_threshold};
use super::templates::{ReactionTemplates, substitute_placeholders};
use super::types::{ArrivalContext, ReactionKind};
use crate::NpcId;
use parish_config::ReactionConfig;
use parish_types::LocationId;
use parish_types::dice::{DiceRoll, fixed_n};
use parish_world::graph::GeoKind;
use parish_world::graph::LocationData;
use parish_world::time::TimeOfDay;
use std::collections::HashSet;

use super::prompt::build_reaction_prompt;
use crate::LanguageSettings;
use crate::test_helpers::make_named_occupation_npc as test_npc;

fn test_location(id: u32, indoor: bool) -> LocationData {
    LocationData {
        id: LocationId(id),
        name: "Test Location".to_string(),
        description_template: String::new(),
        landmarks: vec![],
        indoor,
        public: true,
        connections: vec![],
        lat: 0.0,
        lon: 0.0,
        associated_npcs: vec![],
        mythological_significance: None,
        aliases: vec![],
        geo_kind: GeoKind::Fictional,
        relative_to: None,
        geo_source: None,
    }
}

#[test]
fn test_publican_at_pub_reacts_with_low_roll() {
    let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
    let loc = test_location(2, true);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.1]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].kind, ReactionKind::Introduction);
    assert!(reactions[0].introduces);
    assert!(reactions[0].use_llm);
    assert!(reactions[0].canned_text.contains("Padraig"));
}

#[test]
fn test_introduced_publican_at_pub_gives_welcome() {
    let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
    let loc = test_location(2, true);
    let mut introduced = HashSet::new();
    introduced.insert(NpcId(1));
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.5]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Afternoon,
        weather: "overcast",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].kind, ReactionKind::Welcome);
    assert!(!reactions[0].introduces);
    assert!(reactions[0].use_llm);
}

#[test]
fn test_high_roll_no_reaction() {
    let npc = test_npc(1, "Siobhan", "Farmer", None);
    let loc = test_location(1, false);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.99, 0.5]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert!(reactions.is_empty());
}

#[test]
fn test_priest_gives_blessing_greeting() {
    let npc = test_npc(
        3,
        "Fr. Declan Tierney",
        "Parish Priest",
        Some(LocationId(3)),
    );
    let loc = test_location(1, false);
    let mut introduced = HashSet::new();
    introduced.insert(NpcId(3));
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.3]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].kind, ReactionKind::Greeting);
    let text = &reactions[0].canned_text;
    assert!(
        text.contains("God")
            || text.contains("Dia")
            || text.contains("peace")
            || text.contains("bless")
            || text.contains("Lord"),
        "Priest greeting should have religious content, got: {}",
        text
    );
}

#[test]
fn test_night_reduces_reaction_chance() {
    let npc = test_npc(1, "Siobhan", "Farmer", None);
    let loc = test_location(1, false);
    let _introduced: HashSet<NpcId> = HashSet::new();
    let config = ReactionConfig::default();

    let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Night, &config);
    assert!((threshold - 0.40).abs() < 0.01);

    let threshold_morning = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
    assert!((threshold_morning - 0.55).abs() < 0.01);
}

#[test]
fn test_workplace_bonus() {
    let npc = test_npc(1, "Padraig", "Publican", Some(LocationId(2)));
    let loc = test_location(2, true);
    let config = ReactionConfig::default();

    let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
    assert!((threshold - 1.0).abs() < 0.01);
}

#[test]
fn test_negative_mood_penalty() {
    let mut npc = test_npc(1, "Siobhan", "Farmer", None);
    npc.mood = "angry and frustrated".to_string();
    let loc = test_location(1, false);
    let config = ReactionConfig::default();

    let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
    assert!((threshold - 0.35).abs() < 0.01);
}

#[test]
fn test_empty_npc_list() {
    let loc = test_location(1, false);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice: Vec<DiceRoll> = vec![];

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[], &introduced, &ctx, &dice);

    assert!(reactions.is_empty());
}

#[test]
fn test_multiple_npcs() {
    let npc1 = test_npc(1, "Padraig", "Publican", Some(LocationId(2)));
    let npc2 = test_npc(2, "Siobhan", "Farmer", None);
    let loc = test_location(2, true);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.1, 0.99, 0.5]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc1, &npc2], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].npc_id, NpcId(1));
}

#[test]
fn test_unintroduced_npc_gesture() {
    let npc = test_npc(1, "Siobhan Murphy", "Farmer", None);
    let loc = test_location(1, false);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.5]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].kind, ReactionKind::Gesture);
    assert!(!reactions[0].introduces);
    assert!(!reactions[0].use_llm);
    assert!(reactions[0].npc_display_name.contains("farmer"));
}

#[test]
fn test_unintroduced_npc_casual_introduction() {
    let npc = test_npc(1, "Siobhan Murphy", "Farmer", None);
    let loc = test_location(1, false);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.1]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].kind, ReactionKind::Introduction);
    assert!(reactions[0].introduces);
    assert!(reactions[0].use_llm);
    assert_eq!(reactions[0].npc_display_name, "Siobhan Murphy");
}

#[test]
fn test_calculating_workplace_introduction_has_appraising_edge() {
    let mut npc = test_npc(12, "Cormac Duffy", "Miller", Some(LocationId(18)));
    npc.mood = "calculating".to_string();
    npc.personality =
        "Shrewd, cunning with weights and measures, and always looking to turn a profit."
            .to_string();
    let loc = test_location(18, true);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.1]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].kind, ReactionKind::Introduction);
    assert!(reactions[0].introduces);
    assert!(reactions[0].use_llm);
    let text = reactions[0].canned_text.to_lowercase();
    assert!(
        ["measure", "weigh", "business", "terms", "price"]
            .iter()
            .any(|cue| text.contains(cue)),
        "calculating introduction must carry appraising/business tone: {text}"
    );
    assert!(
        !text.contains("warmly") && !text.contains("friendly"),
        "calculating first contact must not collapse to generic warmth: {text}"
    );
}

#[test]
fn test_negated_cunning_does_not_make_workplace_intro_calculating() {
    let mut npc = test_npc(14, "Brendan Duffy", "Miller's Son", Some(LocationId(18)));
    npc.mood = "dutiful".to_string();
    npc.personality =
        "Honest and hardworking, but lacks his father's cunning. He speaks little.".to_string();
    let loc = test_location(18, true);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.1]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    let text = reactions[0].canned_text.to_lowercase();
    assert!(
        !["measure", "weighing", "business", "terms", "price"]
            .iter()
            .any(|cue| text.contains(cue)),
        "negated cunning must not select calculating intro: {text}"
    );
}

#[test]
fn test_calculating_intro_does_not_duplicate_mononymous_name() {
    let mut npc = test_npc(21, "Aoife", "Trader", Some(LocationId(4)));
    npc.mood = "calculating".to_string();
    let loc = test_location(4, true);
    let introduced: HashSet<NpcId> = HashSet::new();
    let templates = ReactionTemplates::default();
    let config = ReactionConfig::default();
    let dice = fixed_n(&[0.0, 0.4]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert!(
        !reactions[0].canned_text.contains("Aoife Aoife"),
        "mononymous names must not render twice: {}",
        reactions[0].canned_text
    );
}

#[test]
fn test_max_reactions_cap() {
    let npc1 = test_npc(1, "Aoife", "Farmer", None);
    let npc2 = test_npc(2, "Brigid", "Farmer", None);
    let npc3 = test_npc(3, "Cormac", "Farmer", None);
    let npc4 = test_npc(4, "Donal", "Farmer", None);
    let loc = test_location(1, false);
    let mut introduced = HashSet::new();
    for id in [1u32, 2, 3, 4] {
        introduced.insert(NpcId(id));
    }
    let templates = ReactionTemplates::default();
    let config = ReactionConfig {
        max_reactions: 2,
        ..Default::default()
    };
    let dice = fixed_n(&[0.0, 0.3, 0.0, 0.3, 0.0, 0.3, 0.0, 0.3]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions =
        generate_arrival_reactions(&[&npc1, &npc2, &npc3, &npc4], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 2);
}

#[test]
fn test_max_reactions_priority_keeps_introduction() {
    let npc1 = test_npc(1, "Eoin", "Farmer", None);
    let npc2 = test_npc(2, "Fiona Murphy", "Farmer", None);
    let loc = test_location(1, false);
    let mut introduced = HashSet::new();
    introduced.insert(NpcId(1));
    let templates = ReactionTemplates::default();
    let config = ReactionConfig {
        max_reactions: 1,
        ..Default::default()
    };
    let dice = fixed_n(&[0.0, 0.6, 0.0, 0.1]);

    let ctx = ArrivalContext {
        location: &loc,
        time_of_day: TimeOfDay::Morning,
        weather: "clear",
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&[&npc1, &npc2], &introduced, &ctx, &dice);

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].kind, ReactionKind::Introduction);
    assert_eq!(reactions[0].npc_id, NpcId(2));
}

#[test]
fn test_placeholder_substitution() {
    let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
    let result = substitute_placeholders(
        "\"Welcome, says {name}. It's {time}, {weather}.\"",
        &npc,
        "Padraig Darcy",
        TimeOfDay::Morning,
        "overcast",
    );
    assert_eq!(
        result,
        "\"Welcome, says Padraig Darcy. It's morning, overcast.\""
    );
}

#[test]
fn test_reaction_templates_default_has_content() {
    let t = ReactionTemplates::default();
    assert!(t.gestures.len() >= 20);
    assert!(t.greetings.morning.len() >= 10);
    assert!(t.greetings.afternoon.len() >= 10);
    assert!(t.greetings.evening.len() >= 10);
    assert!(t.greetings.any.len() >= 10);
    assert!(t.welcomes.publican.len() >= 10);
    assert!(t.welcomes.shopkeeper.len() >= 10);
    assert!(t.welcomes.priest.len() >= 8);
    assert!(t.introductions.workplace.len() >= 8);
    assert!(t.introductions.casual.len() >= 8);
    assert!(t.introductions.calculating.len() >= 4);
    assert!(t.occupation_greetings.priest.len() >= 8);
}

#[test]
fn test_is_negative_mood() {
    assert!(is_negative_mood("angry"));
    assert!(is_negative_mood("frustrated and bitter"));
    assert!(is_negative_mood("anxious"));
    assert!(!is_negative_mood("content"));
    assert!(!is_negative_mood("cheerful"));
    assert!(!is_negative_mood("contemplative"));
}

#[test]
fn test_high_emotional_intelligence_bonus() {
    let mut npc = test_npc(1, "Padraig", "Publican", None);
    npc.intelligence.emotional = 5;
    let loc = test_location(1, false);
    let config = ReactionConfig::default();

    let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
    assert!((threshold - 0.60).abs() < 0.01);
}

#[test]
fn test_build_reaction_prompt_not_introduced() {
    let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
    let lang = LanguageSettings::english_only();
    let (system, context) = build_reaction_prompt(
        &npc,
        "Darcy's Pub",
        TimeOfDay::Morning,
        "overcast",
        false,
        true,
        &lang,
    );
    assert!(system.contains("Padraig Darcy"));
    assert!(system.contains("Publican"));
    assert!(context.contains("Darcy's Pub"));
    assert!(context.contains("morning"));
    // Greeting must be directed AT the newcomer, not a self-introduction monologue.
    assert!(
        context.contains("newcomer") || context.contains("stranger"),
        "context should frame the task as greeting the newcomer, got: {context}"
    );
    // The old self-focused wording must not appear.
    assert!(
        !context.contains("Introduce yourself briefly"),
        "self-introduction-only wording must not appear in context, got: {context}"
    );
}

/// Arrival greeting prompt instructs the NPC to address the player, not narrate themselves.
///
/// Regression guard for #1431 item 1: the model was producing self-directed greetings
/// ("I'm Padraig. I work here.") because the prompt said "Introduce yourself briefly"
/// with no instruction to address the newcomer.
#[test]
fn test_arrival_greeting_prompt_addresses_newcomer_not_self() {
    // Unintroduced NPC at their workplace — the path that produced self-greetings.
    let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
    let lang = LanguageSettings::english_only();
    let (system, context) = build_reaction_prompt(
        &npc,
        "Darcy's Pub",
        TimeOfDay::Morning,
        "overcast",
        /*is_introduced=*/ false,
        /*at_workplace=*/ true,
        &lang,
    );
    // System prompt must direct the NPC to speak TO the newcomer.
    assert!(
        system.contains("Address the newcomer directly") || system.contains("speak TO them"),
        "system prompt must instruct outward address, got: {system}"
    );
    // Context must frame the task as welcoming/greeting, not pure self-introduction.
    assert!(
        context.contains("newcomer") || context.contains("stranger"),
        "context must frame arrival as greeting the newcomer, got: {context}"
    );
    // Must not use the self-greeting trigger phrase.
    assert!(
        !context.contains("Introduce yourself briefly"),
        "self-introduction-only wording must be absent, got: {context}"
    );

    // Same check for unintroduced NPC NOT at workplace.
    let (system2, context2) = build_reaction_prompt(
        &npc,
        "The Road",
        TimeOfDay::Afternoon,
        "clear",
        /*is_introduced=*/ false,
        /*at_workplace=*/ false,
        &lang,
    );
    assert!(
        context2.contains("newcomer") || context2.contains("stranger"),
        "non-workplace context must also frame the greeting as addressing the newcomer, got: {context2}"
    );
    assert!(
        !context2.contains("Introduce yourself"),
        "self-introduction-only wording must be absent in non-workplace case, got: {context2}"
    );
    // System prompt is shared — same outward-address instruction.
    assert!(
        system2.contains("Address the newcomer directly") || system2.contains("speak TO them"),
        "system prompt must instruct outward address (non-workplace), got: {system2}"
    );
}

#[test]
fn test_arrival_greeting_prompt_calculating_mood_overrides_warm_default() {
    let mut npc = test_npc(12, "Cormac Duffy", "Miller", Some(LocationId(18)));
    npc.mood = "calculating".to_string();
    npc.personality = "A shrewd miller who is hard in his dealings.".to_string();
    let lang = LanguageSettings::english_only();
    let (system, context) = build_reaction_prompt(
        &npc,
        "The Mill",
        TimeOfDay::Morning,
        "clear",
        /*is_introduced=*/ false,
        /*at_workplace=*/ true,
        &lang,
    );

    assert!(
        system.contains("calculating mood must override"),
        "prompt must make calculating mood stronger than the warm default:\n{system}"
    );
    assert!(
        system.contains("measured")
            && system.contains("appraising")
            && system.contains("business-minded"),
        "prompt must name the desired calculating register:\n{system}"
    );
    assert!(
        context.contains("A newcomer has just arrived at The Mill"),
        "context should still describe the arrival target:\n{context}"
    );
}

#[test]
fn test_build_reaction_prompt_introduced() {
    let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
    let lang = LanguageSettings::english_only();
    let (_, context) = build_reaction_prompt(
        &npc,
        "Darcy's Pub",
        TimeOfDay::Afternoon,
        "clear",
        true,
        true,
        &lang,
    );
    assert!(context.contains("You know this person"));
}
