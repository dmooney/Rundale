//! Unit tests for the parish-npc crate root logic.

use super::*;

#[test]
fn test_npc_test_npc() {
    let npc = Npc::new_test_npc();
    assert_eq!(npc.name, "Padraig O'Brien");
    assert_eq!(npc.age, 58);
    assert_eq!(npc.occupation, "Publican");
    assert_eq!(npc.location(), LocationId(1));
}

#[test]
fn grounding_sensitive_setters_advance_lineage_on_location_state_and_schedule_changes() {
    use crate::types::{ScheduleEntry, ScheduleVariant};
    use chrono::{TimeZone, Utc};

    let mut npc = Npc::new_test_npc();
    let initial = npc.grounding_revision();

    npc.set_location(npc.location());
    assert_eq!(
        npc.grounding_revision(),
        initial,
        "reapplying the same location must be a no-op"
    );
    npc.set_location(LocationId(2));
    let after_location = npc.grounding_revision();
    assert!(after_location > initial);

    npc.set_state(NpcState::InTransit {
        from: LocationId(2),
        to: LocationId(3),
        arrives_at: Utc.with_ymd_and_hms(1820, 3, 20, 11, 0, 0).unwrap(),
        activity: None,
    });
    let after_state = npc.grounding_revision();
    assert!(after_state > after_location);

    let schedule = SeasonalSchedule {
        variants: vec![ScheduleVariant {
            season: None,
            day_type: None,
            entries: vec![ScheduleEntry {
                start_hour: 10,
                end_hour: 12,
                location: LocationId(3),
                activity: "mending nets".to_string(),
                cuaird: false,
            }],
        }],
    };
    npc.set_schedule(Some(schedule.clone()));
    let after_schedule = npc.grounding_revision();
    assert!(after_schedule > after_state);

    npc.set_schedule(Some(schedule));
    assert_eq!(
        npc.grounding_revision(),
        after_schedule,
        "reapplying identical authored schedule data must be a no-op"
    );
}

#[test]
fn test_display_name_before_introduction() {
    let npc = Npc::new_test_npc();
    assert_eq!(npc.display_name(false), "an older man behind the bar");
}

#[test]
fn test_display_name_after_introduction() {
    let npc = Npc::new_test_npc();
    assert_eq!(npc.display_name(true), "Padraig O'Brien");
}

#[test]
fn test_build_system_prompt() {
    let npc = Npc::new_test_npc();
    let lang = LanguageSettings::english_only();
    let prompt = build_tier1_system_prompt(&npc, false, &lang);
    assert!(prompt.contains("Padraig O'Brien"));
    assert!(prompt.contains("58-year-old"));
    assert!(prompt.contains("Publican"));
    // Mood is NOT in the system prompt — it lives in the dynamic context
    // so that mood changes never bust the stable system-prompt prefix that
    // the model-runtime prefix cache (vllm-mlx --enable-prefix-cache) depends on.
    assert!(
        !prompt.contains("content"),
        "mood must not appear in the static system prompt"
    );
    assert!(
        prompt.contains("JSON object"),
        "prompt should instruct JSON object response format"
    );
    assert!(
        prompt.contains("\"dialogue\""),
        "prompt should mention the dialogue field"
    );
    assert!(
        prompt.contains("1820"),
        "prompt should specify the year 1820"
    );
    assert!(
        prompt.contains("Acts of Union"),
        "prompt should mention Acts of Union"
    );
    assert!(
        prompt.contains("CULTURAL GUIDELINES"),
        "prompt should include cultural guidelines"
    );
    assert!(
        prompt.contains("language_hints"),
        "prompt should instruct about language_hints metadata"
    );
}

#[test]
fn tier1_system_prompt_has_cross_npc_cacheable_prefix() {
    let first = Npc::new_test_npc();
    let mut second = Npc::new_test_npc();
    second.name = "Una Malone".to_string();
    second.age = 48;
    second.occupation = "Weaver".to_string();
    second.personality = "Quietly observant and exacting about her work.".to_string();
    let lang = LanguageSettings::english_only();
    let first_prompt = build_tier1_system_prompt(&first, false, &lang);
    let second_prompt = build_tier1_system_prompt(&second, false, &lang);
    let marker = "CHARACTER IDENTITY:";
    let first_prefix = first_prompt
        .split_once(marker)
        .expect("identity marker in first prompt")
        .0;
    let second_prefix = second_prompt
        .split_once(marker)
        .expect("identity marker in second prompt")
        .0;

    assert_eq!(
        first_prefix, second_prefix,
        "NPC-invariant instructions must precede identity for cross-NPC prefix-cache reuse"
    );
    assert!(
        first_prefix.len() > 2_000,
        "the shared cacheable prefix must include the substantive global contract"
    );
    assert!(first_prompt.contains("You are Padraig O'Brien"));
    assert!(second_prompt.contains("You are Una Malone"));
}

#[test]
fn test_build_context() {
    let world = WorldState::new();
    let context = build_tier1_context(&world);
    assert!(context.contains("The Crossroads"));
    assert!(context.contains("Spring"));
    assert!(context.contains("1820"));
    assert!(context.contains("Your Location:"));
    assert!(context.contains("Date and time:"));
    // Regression (fixed: #13) — explicit time-of-day cue with both the bucket
    // label and HH:MM so the model picks the right greeting
    // register (NPCs were saying "good morning" at Dusk because
    // the only time signal was 17:30 with no English label).
    assert!(
        context.contains("Time of day:"),
        "missing time-of-day greeting cue:\n{context}"
    );
    // WorldState::new() starts at 08:00 → TimeOfDay::Morning
    assert!(
        context.contains("Time of day: Morning (08:00)"),
        "time-of-day cue must include both label and HH:MM:\n{context}"
    );
    assert!(
        context.contains("greet and refer to the time of day accordingly"),
        "missing greeting-register directive:\n{context}"
    );
    assert!(!context.contains("is here"));
    assert!(!context.contains("\nWeather:"));
    // Old "\nTime:" / "\nSeason:" standalone lines must stay absent.
    // (The new "\nTime of day:" line is allowed — note the space.)
    assert!(!context.contains("\nTime:"));
    assert!(!context.contains("\nSeason:"));
}

#[test]
fn test_build_named_action_line_emote() {
    let line = build_named_action_line("*tips hat*", None);
    assert!(
        line.contains("The newcomer performs an action: tips hat"),
        "emote should strip asterisks and use action phrasing"
    );
    assert!(
        line.contains("emoting rather than speaking"),
        "emote should include action-mode instruction"
    );
}

#[test]
fn test_build_named_action_line_normal_input() {
    let line = build_named_action_line("hello there", None);
    assert!(line.contains("The newcomer says: \"hello there\""));
    assert!(!line.contains("performs an action"));
}

#[test]
fn test_build_named_action_line_partial_asterisks() {
    let line = build_named_action_line("*incomplete", None);
    assert!(line.contains("The newcomer says: \"*incomplete\""));
}

#[test]
fn test_build_named_action_line_with_name() {
    let line = build_named_action_line("hello", Some("Ciaran"));
    assert_eq!(line, "Ciaran says: \"hello\"");
}

#[test]
fn test_build_named_action_line_without_name() {
    let line = build_named_action_line("hello", None);
    assert_eq!(line, "The newcomer says: \"hello\"");
}

#[test]
fn test_build_named_action_line_emote_with_name() {
    let line = build_named_action_line("*tips hat*", Some("Ciaran"));
    assert!(line.contains("Ciaran performs an action: tips hat"));
}

#[test]
fn test_detect_player_name_my_name_is() {
    assert_eq!(
        detect_player_name("My name is Ciaran"),
        Some("Ciaran".to_string())
    );
}

#[test]
fn test_detect_player_name_im() {
    assert_eq!(
        detect_player_name("I'm Padraig O'Brien"),
        Some("Padraig O'Brien".to_string())
    );
}

#[test]
fn test_detect_player_name_call_me() {
    assert_eq!(detect_player_name("Call me Sean"), Some("Sean".to_string()));
}

#[test]
fn test_detect_player_name_no_match() {
    assert_eq!(detect_player_name("hello there"), None);
    assert_eq!(detect_player_name("what is your name?"), None);
}

#[test]
fn test_detect_player_name_in_sentence() {
    assert_eq!(
        detect_player_name("Well, my name is Maeve if you must know"),
        Some("Maeve".to_string())
    );
}

#[test]
fn test_validate_mentioned_people_known() {
    let roster = vec![
        (
            NpcId(1),
            "Padraig Darcy".to_string(),
            "publican".to_string(),
        ),
        (
            NpcId(2),
            "Mary O'Sullivan".to_string(),
            "weaver".to_string(),
        ),
    ];
    let mentioned = vec!["Padraig".to_string()];
    let hallucinated = validate_mentioned_people(&mentioned, &roster, None);
    assert!(hallucinated.is_empty());
}

#[test]
fn test_validate_mentioned_people_hallucinated() {
    let roster = vec![(
        NpcId(1),
        "Padraig Darcy".to_string(),
        "publican".to_string(),
    )];
    let mentioned = vec!["Padraig".to_string(), "Seamus".to_string()];
    let hallucinated = validate_mentioned_people(&mentioned, &roster, None);
    assert_eq!(hallucinated, vec!["Seamus".to_string()]);
}

#[test]
fn test_validate_mentioned_people_player_name() {
    let roster = vec![];
    let mentioned = vec!["Ciaran".to_string()];
    let hallucinated = validate_mentioned_people(&mentioned, &roster, Some("Ciaran"));
    assert!(hallucinated.is_empty());
}

#[test]
fn test_validate_mentioned_people_empty() {
    let roster = vec![];
    let hallucinated = validate_mentioned_people(&[], &roster, None);
    assert!(hallucinated.is_empty());
}

#[test]
fn test_npc_json_response_deserialize_full() {
    let json = r#"{
            "dialogue": "Ah, good morning to ye!",
            "action": "speaks",
            "mood": "friendly",
            "internal_thought": "Haven't seen this one before.",
            "irish_words": [{"word": "Dia dhuit", "pronunciation": "DEE-ah gwit", "meaning": "Hello"}],
            "assigned_task": "Dig over the potato patch."
        }"#;
    let resp: NpcJsonResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.dialogue, "Ah, good morning to ye!");
    assert_eq!(resp.action, "speaks");
    assert_eq!(resp.mood, "friendly");
    assert_eq!(
        resp.internal_thought,
        Some("Haven't seen this one before.".to_string())
    );
    assert_eq!(resp.language_hints.len(), 1);
    assert_eq!(
        resp.assigned_task.as_deref(),
        Some("Dig over the potato patch.")
    );
}

#[test]
fn test_npc_json_response_deserialize_minimal() {
    let json = r#"{"dialogue": "Hello!"}"#;
    let resp: NpcJsonResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.dialogue, "Hello!");
    assert_eq!(resp.action, "");
    assert_eq!(resp.mood, "");
    assert!(resp.internal_thought.is_none());
    assert!(resp.language_hints.is_empty());
    assert!(resp.assigned_task.is_none());
}

#[test]
fn test_parse_npc_stream_response_json() {
    let text = r#"{"dialogue": "(Looks up) Ah, good morning to ye!", "action": "speaks", "mood": "friendly"}"#;
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, "(Looks up) Ah, good morning to ye!");
    let meta = parsed.metadata.unwrap();
    assert_eq!(meta.action, "speaks");
    assert_eq!(meta.mood, "friendly");
    assert!(meta.assigned_task.is_none());
}

#[test]
fn test_parse_npc_stream_response_bounds_assigned_task_description() {
    let oversized = "p".repeat(parish_types::MAX_TASK_DESCRIPTION_CHARS + 17);
    let text = serde_json::json!({
        "dialogue": "Start with the potato patch.",
        "assigned_task": format!("  {oversized}  ")
    })
    .to_string();

    let parsed = parse_npc_stream_response(&text);
    let assigned_task = parsed.metadata.unwrap().assigned_task.unwrap();

    assert_eq!(
        assigned_task.chars().count(),
        parish_types::MAX_TASK_DESCRIPTION_CHARS
    );
    assert!(!assigned_task.starts_with(char::is_whitespace));
}

#[test]
fn test_parse_npc_stream_response_drops_blank_assigned_task() {
    let parsed = parse_npc_stream_response(
        r#"{"dialogue":"There is no work for ye today.","assigned_task":" \n\t "}"#,
    );

    assert!(parsed.metadata.unwrap().assigned_task.is_none());
}

#[test]
fn test_parse_npc_stream_response_plain_text_fallback() {
    let text = "Well hello there, stranger!";
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, "Well hello there, stranger!");
    assert!(parsed.metadata.is_none());
}

#[test]
fn test_parse_npc_stream_response_empty() {
    let parsed = parse_npc_stream_response("");
    assert_eq!(parsed.dialogue, "");
    assert!(parsed.metadata.is_none());
}

#[test]
fn test_parse_npc_stream_response_truncated_json() {
    // Live demo (2026-05-17) — Brendan Duffy at The Mill. JSON stream
    // ran out of tokens before the closing brace; previous behaviour
    // surfaced the raw `{"dialogue": "..."}` wrapper as user-visible
    // dialogue text.
    let text = r#"{"dialogue": "Aye, the process of milling, is it? 'Tis a simple enough thing, so it is. Ye bring yer grain, we grind it"#;
    let parsed = parse_npc_stream_response(text);
    assert_eq!(
        parsed.dialogue,
        "Aye, the process of milling, is it? 'Tis a simple enough thing, so it is. Ye bring yer grain, we grind it"
    );
    assert!(parsed.metadata.is_none());
}

#[test]
fn test_parse_npc_stream_response_truncated_at_escape() {
    // Defensive: stream ends mid backslash-escape.
    let text = r#"{"dialogue": "Aye \"good"#;
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, "Aye \"good");
}

#[test]
fn test_parse_npc_stream_response_truncated_empty_dialogue() {
    // `dialogue: ""` should fall through to raw text instead of
    // surfacing an empty bubble.
    let text = r#"{"dialogue": ""#;
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, text);
}

#[test]
fn test_parse_npc_stream_response_single_quoted_with_inner_double_quote() {
    // Bot review (PR #990, codex P2): the heuristic accepted `'` as
    // an opening quote but only stopped at `"`. For pseudo-JSON like
    // `{'dialogue':'Aye, "good", said he'}` the inner double quote
    // must NOT terminate the body; we should keep going until the
    // matching single-quote closer.
    let text = r#"{'dialogue':'Aye, "good", said he'}"#;
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, r#"Aye, "good", said he"#);
}

#[test]
fn test_parse_npc_stream_response_double_quoted_with_inner_single_quote() {
    // Mirror case: standard `"dialogue": "ye'll see"` body contains
    // an inner apostrophe; the opener `"` must drive the terminator
    // so we don't break early on the apostrophe.
    let text = r#"{"dialogue": "ye'll see, lad"}"#;
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, "ye'll see, lad");
}

#[test]
fn test_parse_npc_stream_response_invalid_json() {
    let text = "{not valid json at all";
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, "{not valid json at all");
    assert!(parsed.metadata.is_none());
}

#[test]
fn test_parse_npc_stream_response_empty_json() {
    let text = "{}";
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, "");
    let meta = parsed.metadata.unwrap();
    assert_eq!(meta.action, "");
    assert_eq!(meta.mood, "");
    assert!(meta.internal_thought.is_none());
    assert!(meta.language_hints.is_empty());
    assert!(meta.assigned_task.is_none());
}

#[test]
fn test_parse_npc_stream_response_fenced_json() {
    let text = "```json\n{\"dialogue\": \"Hello there!\", \"mood\": \"friendly\"}\n```";
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, "Hello there!");
    let meta = parsed.metadata.unwrap();
    assert_eq!(meta.mood, "friendly");
}

#[test]
fn test_parse_npc_stream_response_fenced_json_untagged() {
    let text = "```\n{\"dialogue\": \"Good day!\", \"action\": \"waves\"}\n```";
    let parsed = parse_npc_stream_response(text);
    assert_eq!(parsed.dialogue, "Good day!");
    let meta = parsed.metadata.unwrap();
    assert_eq!(meta.action, "waves");
}

#[test]
fn test_strip_json_fence_plain() {
    assert_eq!(strip_json_fence(r#"{"a":1}"#), r#"{"a":1}"#);
}

#[test]
fn test_strip_json_fence_markdown() {
    assert_eq!(strip_json_fence("```json\n{\"a\":1}\n```"), r#"{"a":1}"#);
}

// ── Issue #731 — prompt template placeholder interpolation ───────────────

/// Tier 1 system prompt: every `{placeholder}` must be substituted.
///
/// Uses the canonical test NPC fixture so any new placeholder added to the
/// template without a matching format-argument will cause a compile error
/// or leave a literal `{key}` in the output that this test catches.
///
/// Note: the prompt embeds a JSON example block whose keys use single braces
/// (e.g. `{"action": ...}`). The regex `\{[a-z_]+\}` matches only
/// lower-case-word placeholders and skips those JSON key-value pairs, so
/// false positives from the example block are not possible.
#[test]
fn test_tier1_system_no_unsubstituted_placeholders() {
    let re = regex::Regex::new(r"\{[a-z_]+\}").unwrap();
    let npc = Npc::new_test_npc();
    let lang = LanguageSettings::english_only();
    let prompt = build_tier1_system_prompt(&npc, false, &lang);

    // No word-placeholder should survive substitution.
    assert!(
        !re.is_match(&prompt),
        "Unsubstituted placeholder found in tier1 system prompt: {:?}",
        re.find(&prompt).map(|m| m.as_str()),
    );

    // Known values must appear.
    assert!(prompt.contains("Padraig O'Brien"), "NPC name missing");
    assert!(prompt.contains("58"), "NPC age missing");
    assert!(prompt.contains("Publican"), "NPC occupation missing");
    // Mood is NOT in the system prompt; it is injected in the dynamic context
    // so mood changes do not bust the stable prefix-cache prefix.
    assert!(
        !prompt.contains("content"),
        "mood must not appear in the static system prompt"
    );

    // Anachronism and cultural guidelines are part of the contract; a
    // future edit that removes them will trip this test intentionally.
    assert!(
        prompt.contains("Acts of Union"),
        "historical context missing"
    );
    assert!(
        prompt.contains("CULTURAL GUIDELINES"),
        "cultural guidelines missing"
    );

    // Lane-keeping clause (issue: midwife-replied-as-tracker, grok 2/5).
    assert!(
        prompt.contains("STAY IN YOUR LANE"),
        "lane-keeping clause missing"
    );

    // 1820 fact preamble (issue: Aoife claimed Irish-language teaching
    // was outlawed in 1820, but the Penal Laws were repealed in 1782).
    assert!(
        prompt.contains("Penal Laws"),
        "1820 fact preamble missing — Penal Laws clause"
    );
    assert!(
        prompt.contains("1782"),
        "1820 fact preamble missing — repeal date"
    );

    // Allowed-Irish-phrases whitelist (issue: "go connachtú"
    // hallucinated past the known "Slán abhaile").
    assert!(
        prompt.contains("ALLOWED IRISH PHRASES"),
        "Irish-phrase whitelist clause missing"
    );
    assert!(
        prompt.contains("Slán abhaile"),
        "anchor phrase missing from whitelist"
    );
    assert!(
        prompt.contains("Do NOT invent or extend Irish phrases"),
        "Irish-grammar improvisation guard missing"
    );

    // Modern-register blacklist (issue: "healing properties" in midwife
    // reply scored 2/5).
    assert!(
        prompt.contains("REGISTER:"),
        "modern-register guard missing"
    );
    assert!(
        prompt.contains("healing properties"),
        "modern-register negative example missing"
    );

    // Anti-farewell directive (fixed: #4 — Cormac signed off with
    // "Slán abhaile" mid-conversation in cycle 1 of the demo audit).
    assert!(
        prompt.contains("NEVER FAREWELL MID-CONVERSATION"),
        "anti-farewell directive header missing"
    );
    for tok in ["Slán abhaile", "Slán leat", "Goodbye", "Farewell"] {
        assert!(
            prompt.contains(tok),
            "anti-farewell directive missing gated token {tok:?}"
        );
    }
}

/// Regression guard: the Tier 1 system prompt must be byte-identical across
/// two calls that change only the NPC's `mood` field between them.
///
/// This protects the model-runtime prefix cache contract (vllm-mlx
/// `--enable-prefix-cache`): the system prompt is the stable prefix shared
/// across turns, and any field that mutates between turns must NOT appear in
/// it. If mood is accidentally re-added, this test fails immediately.
#[test]
fn tier1_system_prompt_stable_across_mood_change() {
    let mut npc = Npc::new_test_npc();
    let lang = LanguageSettings::english_only();

    npc.mood = "cheerful".to_string();
    let prompt_before = build_tier1_system_prompt(&npc, false, &lang);

    npc.mood = "melancholy".to_string();
    let prompt_after = build_tier1_system_prompt(&npc, false, &lang);

    assert_eq!(
        prompt_before, prompt_after,
        "system prompt must be byte-identical across mood changes — \
             mood must live in the dynamic context, not the static system prompt"
    );
}

/// Tier 1 context prompt: every `{placeholder}` must be substituted.
///
/// Uses a world backed by a real `WorldGraph` containing a
/// `description_template` with `{time}`, `{weather}`, and `{npcs_present}`
/// so that the `render_description` path is exercised — the one place where
/// silent leakage can actually occur at runtime (`.replace()` is not
/// compile-checked).
#[test]
fn test_tier1_context_no_unsubstituted_placeholders() {
    use parish_world::{WorldState, graph::WorldGraph};

    let re = regex::Regex::new(r"\{[a-z_]+\}").unwrap();

    // Build a world whose description_template contains all three dynamic
    // placeholders, so render_description must replace each of them.
    let graph_json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "The Crossroads",
                    "description_template": "A crossroads at {time}. The sky is {weather}. {npcs_present} stand nearby.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.618,
                    "lon": -8.095,
                    "connections": [{"target": 2, "path_description": "a lane"}],
                    "associated_npcs": []
                },
                {
                    "id": 2,
                    "name": "The Church",
                    "description_template": "The church at {time}.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.620,
                    "lon": -8.097,
                    "connections": [{"target": 1, "path_description": "back"}],
                    "associated_npcs": []
                }
            ]
        }"#;

    let mut world = WorldState::new();
    world.graph = WorldGraph::load_from_str(graph_json).unwrap();

    let context = build_tier1_context(&world);

    // No word-placeholder should survive substitution.
    assert!(
        !re.is_match(&context),
        "Unsubstituted placeholder found in tier1 context: {:?} — full output: {context}",
        re.find(&context).map(|m| m.as_str()),
    );

    // Known values must appear in the rendered output.
    assert!(context.contains("The Crossroads"), "location name missing");
    // WorldState::new() starts at 08:00 → TimeOfDay::Morning → "morning"
    assert!(
        context.contains("morning"),
        "time-of-day substitution missing"
    );
    // WorldState::new() sets Weather::Clear → weather_display produces "clear"
    assert!(context.contains("clear"), "weather substitution missing");
    // Date / season from WorldState::new(): 20 March 1820, Spring
    assert!(context.contains("1820"), "year missing from context");
    assert!(context.contains("Spring"), "season missing from context");
}

/// Helper: a world whose current location (id 1) carries the given
/// indoor/public flags, optional mythological note, and a single hazard
/// connection to "The Church" (id 2). `player_location` defaults to 1.
#[cfg(test)]
fn world_with_location_details(
    indoor: bool,
    public: bool,
    myth: Option<&str>,
    hazard: Option<&str>,
    weather: parish_world::Weather,
) -> parish_world::WorldState {
    use parish_world::graph::WorldGraph;

    let myth_field = match myth {
        Some(s) => format!(r#""mythological_significance": "{s}","#),
        None => String::new(),
    };
    let hazard_field = match hazard {
        Some(h) => format!(r#", "hazard": "{h}""#),
        None => String::new(),
    };
    let graph_json = format!(
        r#"{{
                "locations": [
                    {{
                        "id": 1,
                        "name": "The Crossroads",
                        "description_template": "A crossroads.",
                        "indoor": {indoor},
                        "public": {public},
                        {myth_field}
                        "lat": 53.618,
                        "lon": -8.095,
                        "connections": [{{"target": 2, "path_description": "a narrow boreen lined with hawthorn"{hazard_field}}}],
                        "associated_npcs": []
                    }},
                    {{
                        "id": 2,
                        "name": "St. Brigid's Church",
                        "description_template": "The church.",
                        "indoor": false,
                        "public": true,
                        "lat": 53.640,
                        "lon": -8.120,
                        "connections": [{{"target": 1, "path_description": "back the way you came"}}],
                        "associated_npcs": []
                    }}
                ]
            }}"#
    );

    let mut world = parish_world::WorldState::new();
    world.graph = WorldGraph::load_from_str(&graph_json).unwrap();
    world.player_location = LocationId(1);
    world.weather = weather;
    world
}

#[test]
fn tier1_context_lists_connections_with_path_and_walk_time() {
    let world = world_with_location_details(false, true, None, None, parish_world::Weather::Clear);
    let context = build_tier1_context(&world);

    assert!(
        context.contains("Paths from here:"),
        "paths block missing: {context}"
    );
    assert!(
        context.contains("St. Brigid's Church"),
        "connected neighbour name missing: {context}"
    );
    assert!(
        context.contains("a narrow boreen lined with hawthorn"),
        "path description missing: {context}"
    );
    assert!(
        context.contains("min on foot"),
        "on-foot travel estimate missing: {context}"
    );
}

#[test]
fn tier1_context_states_indoor_and_public_framing() {
    let indoor_private =
        world_with_location_details(true, false, None, None, parish_world::Weather::Clear);
    let ctx = build_tier1_context(&indoor_private);
    assert!(
        ctx.contains("an enclosed indoor space"),
        "indoor framing missing: {ctx}"
    );
    assert!(
        ctx.contains("a private place"),
        "private framing missing: {ctx}"
    );

    let outdoor_public =
        world_with_location_details(false, true, None, None, parish_world::Weather::Clear);
    let ctx = build_tier1_context(&outdoor_public);
    assert!(
        ctx.contains("an open outdoor place"),
        "outdoor framing missing: {ctx}"
    );
    assert!(ctx.contains("open to all"), "public framing missing: {ctx}");
}

#[test]
fn tier1_context_includes_mythological_note_only_when_present() {
    let with_myth = world_with_location_details(
        false,
        true,
        Some("a fairy fort the old people will not disturb"),
        None,
        parish_world::Weather::Clear,
    );
    let ctx = build_tier1_context(&with_myth);
    assert!(
        ctx.contains("Of local note: a fairy fort the old people will not disturb"),
        "mythological note missing when present: {ctx}"
    );

    let without_myth =
        world_with_location_details(false, true, None, None, parish_world::Weather::Clear);
    let ctx = build_tier1_context(&without_myth);
    assert!(
        !ctx.contains("Of local note:"),
        "mythological note present when it should be absent: {ctx}"
    );
}

#[test]
fn tier1_context_flags_weather_hazard_on_connections() {
    // A flood-hazard edge is impassable in a Storm.
    let stormy = world_with_location_details(
        false,
        true,
        None,
        Some("flood"),
        parish_world::Weather::Storm,
    );
    let ctx = build_tier1_context(&stormy);
    assert!(
        ctx.contains("impassable in this weather"),
        "hazard note missing under storm: {ctx}"
    );

    // Same edge under clear weather carries no hazard note.
    let clear = world_with_location_details(
        false,
        true,
        None,
        Some("flood"),
        parish_world::Weather::Clear,
    );
    let ctx = build_tier1_context(&clear);
    assert!(
        !ctx.contains("impassable in this weather"),
        "hazard note present under clear weather: {ctx}"
    );
    assert!(
        !ctx.contains("slow going today"),
        "slowed note present under clear weather: {ctx}"
    );
}

#[test]
fn tier1_context_slowed_edge_quotes_weather_adjusted_travel_time() {
    // A flood edge under heavy rain is Slowed (factor 0.6), so the quoted
    // on-foot time must rise above the clear-weather estimate — the NPC
    // can't say "slow going today" while quoting the dry-road time.
    fn quoted_minutes(ctx: &str) -> u16 {
        let marker = "(about ";
        let start = ctx.find(marker).expect("travel estimate present") + marker.len();
        let rest = &ctx[start..];
        let end = rest.find(" min on foot").expect("minutes label present");
        rest[..end].trim().parse().expect("minutes parse")
    }

    let clear = world_with_location_details(
        false,
        true,
        None,
        Some("flood"),
        parish_world::Weather::Clear,
    );
    let clear_minutes = quoted_minutes(&build_tier1_context(&clear));

    let rainy = world_with_location_details(
        false,
        true,
        None,
        Some("flood"),
        parish_world::Weather::HeavyRain,
    );
    let rainy_ctx = build_tier1_context(&rainy);
    assert!(
        rainy_ctx.contains("slow going today"),
        "slowed note missing under heavy rain: {rainy_ctx}"
    );
    assert!(
        quoted_minutes(&rainy_ctx) > clear_minutes,
        "slowed travel time should exceed clear-weather time: clear={clear_minutes}, ctx={rainy_ctx}"
    );
}

#[test]
fn tier1_context_falls_back_cleanly_when_location_absent_from_graph() {
    use parish_world::graph::WorldGraph;

    // Graph contains only ids 998/999, so the player's location (1) is
    // absent from the graph: current_location_data() is None.
    let graph_json = r#"{
            "locations": [
                {
                    "id": 998,
                    "name": "Nowhere",
                    "description_template": "Empty.",
                    "indoor": false,
                    "public": true,
                    "lat": 0.0,
                    "lon": 0.0,
                    "connections": [{"target": 999, "path_description": "a track"}],
                    "associated_npcs": []
                },
                {
                    "id": 999,
                    "name": "Elsewhere",
                    "description_template": "Empty.",
                    "indoor": false,
                    "public": true,
                    "lat": 0.1,
                    "lon": 0.1,
                    "connections": [{"target": 998, "path_description": "a track"}],
                    "associated_npcs": []
                }
            ]
        }"#;

    let mut world = parish_world::WorldState::new();
    world.graph = WorldGraph::load_from_str(graph_json).unwrap();
    world.player_location = LocationId(1);

    // Must not panic, and must emit no paths block / setting line.
    let ctx = build_tier1_context(&world);
    assert!(
        !ctx.contains("Paths from here:"),
        "paths block leaked: {ctx}"
    );
    assert!(
        !ctx.contains("There are no paths leading away"),
        "no-paths sentinel leaked: {ctx}"
    );
    assert!(
        !ctx.contains("This is an"),
        "setting line leaked without graph data: {ctx}"
    );
    assert!(
        ctx.contains("Your Location:"),
        "legacy location line missing: {ctx}"
    );
}

/// Tier 2 system prompt: every `{placeholder}` must be substituted.
///
/// `build_tier2_prompt` is a pure `format!()` call, so a new placeholder
/// added without a matching argument will cause a compile error.  This test
/// guards the runtime values: location name, time, weather, and at least one
/// NPC name must all appear in the final output.
#[test]
fn test_tier2_system_no_unsubstituted_placeholders() {
    use crate::ticks::{NpcSnapshot, Tier2Group, build_tier2_prompt};
    use parish_types::{LocationId, NpcId};

    let re = regex::Regex::new(r"\{[a-z_]+\}").unwrap();

    let group = Tier2Group {
        location: LocationId(2),
        location_name: "Darcy's Pub".to_string(),
        other_location_names: vec!["The Mill".to_string()],
        npcs: vec![
            NpcSnapshot {
                id: NpcId(1),
                name: "Brigid Murphy".to_string(),
                occupation: "Weaver".to_string(),
                personality: "Steady and observant".to_string(),
                pronouns: "she/her".to_string(),
                intelligence_prose: "Sharp-minded and perceptive.".to_string(),
                mood: "thoughtful".to_string(),
                relationship_summary: String::new(),
                current_activity: Some("weaving by the hearth".to_string()),
                grounding_revision: 1,
                activity_fingerprint: "interval-brigid".to_string(),
            },
            NpcSnapshot {
                id: NpcId(7),
                name: "Seamus Fahey".to_string(),
                occupation: "Blacksmith".to_string(),
                personality: "Blunt and loyal".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: "Plain-spoken and blunt.".to_string(),
                mood: "tired".to_string(),
                relationship_summary: String::new(),
                current_activity: Some("waiting for a drink".to_string()),
                grounding_revision: 2,
                activity_fingerprint: "interval-seamus".to_string(),
            },
        ],
    };

    let lang = LanguageSettings::english_only();
    let prompt = build_tier2_prompt(&group, "Evening", "Overcast", &lang);

    // No word-placeholder should survive substitution.
    assert!(
        !re.is_match(&prompt),
        "Unsubstituted placeholder found in tier2 system prompt: {:?}",
        re.find(&prompt).map(|m| m.as_str()),
    );

    // Known values from the fixture must appear.
    assert!(prompt.contains("Darcy's Pub"), "location name missing");
    assert!(prompt.contains("Evening"), "time missing");
    assert!(prompt.contains("Overcast"), "weather missing");
    assert!(prompt.contains("Brigid Murphy"), "NPC name 1 missing");
    assert!(prompt.contains("Seamus Fahey"), "NPC name 2 missing");
    assert!(prompt.contains("Weaver"), "occupation missing");
    assert!(prompt.contains("thoughtful"), "mood missing");
}

// ── language_directive tests ───────────────────────────────────────────────

#[test]
fn language_directive_en_ie_with_native_ga_ie() {
    let lang = LanguageSettings::new("en-IE".to_string(), Some("ga-IE".to_string()));
    let directive = language_directive(&lang);
    assert!(
        directive.contains("en-IE"),
        "directive should name the player locale"
    );
    assert!(
        directive.contains("en-US"),
        "directive should warn against en-US spellings for non-en-US English"
    );
    assert!(
        directive.contains("ga-IE"),
        "directive should name the native language"
    );
    assert!(
        directive.contains("language_hints"),
        "directive should mention the language_hints metadata field"
    );
    assert!(
        directive.contains("Use ONLY en-IE and ga-IE"),
        "directive should name an explicit two-language allow-list"
    );
    // Should NOT tell the NPC to stay in one language when a native is given
    assert!(
        !directive.contains("do not invent or import other languages"),
        "mono-language restriction must not appear when native language is set"
    );
}

#[test]
fn language_directive_includes_ga_ie_phrase_guide_for_irish_native() {
    let lang = LanguageSettings::new("en-IE", Some("ga-IE".into()));
    let directive = language_directive(&lang);
    assert!(
        directive.contains("Preferred ga-IE phrases"),
        "ga-IE native should append the curated phrase guide"
    );
    assert!(
        directive.contains("\"Dia dhuit\""),
        "phrase guide should include canonical greetings"
    );
    assert!(
        directive.contains("\"seanchaí\""),
        "phrase guide should include period-appropriate concept words"
    );
}

#[test]
fn language_directive_omits_ga_ie_phrase_guide_for_non_irish_native() {
    let lang = LanguageSettings::new("en-US", Some("fr-FR".into()));
    let directive = language_directive(&lang);
    assert!(
        !directive.contains("Preferred ga-IE phrases"),
        "ga-IE phrase guide must only fire for Irish native"
    );
}

#[test]
fn language_directive_includes_pig_lat_guide() {
    let lang = LanguageSettings::new("en", Some("x-pig-lat".into()));
    let directive = language_directive(&lang);
    assert!(
        directive.contains("x-pig-lat"),
        "directive should name the pig Latin code"
    );
    assert!(
        directive.contains("Pig Latin rules"),
        "x-pig-lat native should append the pig Latin phrase guide"
    );
    assert!(
        directive.contains("igpay"),
        "phrase guide should include a canonical example"
    );
    assert!(
        !directive.contains("Preferred ga-IE phrases"),
        "ga-IE phrase guide must not appear for pig Latin native"
    );
}

#[test]
fn language_directive_forbids_non_latin_scripts() {
    // Character-set guard must appear regardless of locale config so the
    // multilingual-model drift (Qwen → Han / Cyrillic mid-sentence) is
    // disciplined at the prompt boundary.
    for lang in [
        LanguageSettings::new("en-IE", Some("ga-IE".into())),
        LanguageSettings::english_only(),
        LanguageSettings::new("fr-FR", None),
    ] {
        let directive = language_directive(&lang);
        assert!(
            directive.contains("Latin script"),
            "directive should allow-list Latin script"
        );
        for forbidden in [
            "Cyrillic",
            "Han",
            "Hiragana",
            "Hangul",
            "Arabic",
            "Hebrew",
            "Greek",
            "Devanagari",
        ] {
            assert!(
                directive.contains(forbidden),
                "directive should explicitly forbid {forbidden}: {directive:?}"
            );
        }
    }
}

#[test]
fn language_directive_en_us_no_native() {
    let lang = LanguageSettings::new("en-US".to_string(), None);
    let directive = language_directive(&lang);
    assert!(
        directive.contains("en-US"),
        "directive should name the locale"
    );
    // en-US should NOT get the anti-en-US-spelling warning
    assert!(
        !directive.contains("Never use en-US spellings"),
        "en-US locale must not warn against itself"
    );
    assert!(
        directive.contains("do not invent or import other languages"),
        "mono-language restriction should appear when no native language is set"
    );
}

#[test]
fn language_directive_fr_fr_no_native() {
    let lang = LanguageSettings::new("fr-FR".to_string(), None);
    let directive = language_directive(&lang);
    assert!(
        directive.contains("fr-FR"),
        "directive should name the locale"
    );
    // Non-English locale should NOT get the en-US spelling warning
    assert!(
        !directive.contains("en-US spellings"),
        "en-US spelling warning must not appear for non-English locale"
    );
    assert!(
        directive.contains("do not invent or import other languages"),
        "mono-language restriction should appear when no native language is set"
    );
}

#[test]
fn tier1_prompt_contains_language_directive() {
    let npc = Npc::new_test_npc();
    let lang = LanguageSettings::new("en-IE".to_string(), Some("ga-IE".to_string()));
    let prompt = build_tier1_system_prompt(&npc, false, &lang);
    assert!(
        prompt.contains("LANGUAGE:"),
        "tier1 system prompt should embed the language directive"
    );
    assert!(
        prompt.contains("en-IE"),
        "tier1 system prompt should name the player locale"
    );
    assert!(
        prompt.contains("ga-IE"),
        "tier1 system prompt should name the native language"
    );
}

// ── #1225 — time-of-day greeting guard ───────────────────────────────────

/// AC-1/AC-2 (fix-1224-1225): `build_tier1_context` must carry an explicit
/// time-of-day label and HH:MM for every non-morning bucket, so small
/// models that ignore the plain HH:MM timestamp still see an unambiguous
/// English cue for greeting register.
#[test]
fn build_tier1_context_carries_dusk_label_and_time() {
    use chrono::TimeZone;
    let mut world = WorldState::new();
    // Advance the clock to 17:30 — Dusk bucket.
    world.clock = parish_types::GameClock::new(
        chrono::Utc
            .with_ymd_and_hms(1820, 3, 20, 17, 30, 0)
            .unwrap(),
    );
    let context = build_tier1_context(&world);

    assert!(
        context.contains("Time of day:"),
        "AC-1: time-of-day cue missing at Dusk:\n{context}"
    );
    assert!(
        context.contains("Dusk"),
        "AC-2: time-of-day label must be 'Dusk' at 17:30:\n{context}"
    );
    assert!(
        context.contains("17:"),
        "AC-2: HH:MM must accompany the Dusk label:\n{context}"
    );
    assert!(
        context.contains("greet and refer to the time of day accordingly"),
        "AC-2: greeting-register directive missing:\n{context}"
    );
}

/// AC-3 (fix-1224-1225): when the world clock is Dusk, the context must
/// include a negative directive forbidding "good morning" greetings so
/// small models cannot override the positive cue with training-data bias.
#[test]
fn build_tier1_context_includes_forbidden_morning_greeting_at_dusk() {
    use chrono::TimeZone;
    let mut world = WorldState::new();
    world.clock = parish_types::GameClock::new(
        chrono::Utc
            .with_ymd_and_hms(1820, 3, 20, 17, 30, 0)
            .unwrap(),
    );
    let context = build_tier1_context(&world);

    assert!(
        context.contains("Do NOT say 'good morning'"),
        "AC-3: forbidden-morning-greeting directive missing at Dusk:\n{context}"
    );
}

// ── #1451: season grounding directive ────────────────────────────────────

/// AC (#1451): build_tier1_context must emit a CURRENT SEASON directive with
/// an explicit prohibition on referencing other seasons, so small models cannot
/// substitute summer in spring (or any other season mismatch).
#[test]
fn build_tier1_context_carries_season_directive() {
    // WorldState::new() defaults to a fixed date — check what season it produces.
    let world = WorldState::new();
    let season_str = world.clock.season().to_string();
    let context = build_tier1_context(&world);

    assert!(
        context.contains("CURRENT SEASON:"),
        "context must carry a CURRENT SEASON directive (#1451):\n{context}"
    );
    assert!(
        context.contains(&season_str),
        "CURRENT SEASON directive must name the actual season ({season_str}) (#1451):\n{context}"
    );
    assert!(
        context.contains("Do not reference any other season"),
        "CURRENT SEASON directive must prohibit referencing other seasons (#1451):\n{context}"
    );
}

/// Corollary to AC-3: at Morning the forbidden-greeting directive must NOT
/// appear (a morning greeting is appropriate and the directive would confuse
/// the model into avoiding a correct response).
#[test]
fn build_tier1_context_no_forbidden_greeting_at_morning() {
    // WorldState::new() starts at 08:00 — Morning bucket.
    let world = WorldState::new();
    let context = build_tier1_context(&world);

    assert!(
        !context.contains("Do NOT say 'good morning'"),
        "forbidden-greeting directive must not appear at Morning:\n{context}"
    );
}

/// Spot-check `forbidden_greeting_directive` for each bucket.
#[test]
fn forbidden_greeting_directive_covers_non_morning_buckets() {
    // Non-morning buckets must produce a directive.
    for tod in [
        TimeOfDay::Dawn,
        TimeOfDay::Afternoon,
        TimeOfDay::Dusk,
        TimeOfDay::Night,
        TimeOfDay::Midnight,
    ] {
        assert!(
            forbidden_greeting_directive(tod).is_some(),
            "expected a directive for {tod:?}"
        );
    }

    // Morning/Midday must produce None — "good morning"/"good day" are correct.
    for tod in [TimeOfDay::Morning, TimeOfDay::Midday] {
        assert!(
            forbidden_greeting_directive(tod).is_none(),
            "unexpected directive for {tod:?}"
        );
    }
}

// ── #1224 — length instruction in system prompt (AC-4) ────────────────────

/// AC-4 (fix-1224-1225, updated fix-1373-1374): the Tier 1 system prompt must contain a length
/// instruction so the model has an explicit sentence-count target. Updated to 2-3 sentences
/// with a single-question cap to also address #1374 (run-on / stacked questions).
#[test]
fn build_tier1_system_prompt_contains_length_guidance() {
    let npc = Npc::new_test_npc();
    let lang = LanguageSettings::english_only();
    let prompt = build_tier1_system_prompt(&npc, false, &lang);
    assert!(
        prompt.contains("2-3 sentences") || prompt.contains("2–3 sentences"),
        "AC-4: length guidance missing from system prompt:\n{prompt}"
    );
    assert!(
        prompt.contains("AT MOST ONE question") || prompt.contains("at most one question"),
        "AC-3 (#1374): single-question cap missing from system prompt:\n{prompt}"
    );
}

// ── #1228 — anti-repetition guard ─────────────────────────────────────────

/// AC-1: a degenerate loop of one clause repeated many times collapses to a
/// single instance. This is the dominant #1228 failure mode.
#[test]
fn collapse_repeated_sentences_collapses_runaway_loop() {
    let clause = "Speak yer mind, and we'll see what be in it, m'friend.";
    let runaway = clause.repeat(20);
    let collapsed = collapse_repeated_sentences(&runaway);
    // The clause must appear exactly once after collapse.
    let occurrences = collapsed.matches("Speak yer mind").count();
    assert_eq!(
        occurrences, 1,
        "AC-1: runaway clause must collapse to one instance, got {occurrences}:\n{collapsed}"
    );
}

/// AC-1: non-repeating multi-sentence dialogue is preserved (no false
/// collapse). Distinct adjacent sentences must all survive.
#[test]
fn collapse_repeated_sentences_preserves_distinct_sentences() {
    let line = "Good evening to ye. The harvest looks fair this year. Will ye have a pint?";
    let collapsed = collapse_repeated_sentences(line);
    assert!(collapsed.contains("Good evening"), "got: {collapsed}");
    assert!(collapsed.contains("harvest looks fair"), "got: {collapsed}");
    assert!(
        collapsed.contains("Will ye have a pint"),
        "got: {collapsed}"
    );
}

/// AC-2: identical text, case/whitespace/punctuation-only differences, and
/// high-overlap text are all flagged near-identical; distinct lines are not.
#[test]
fn is_near_identical_normalizes_and_thresholds() {
    let a = "Aye, the rents be cruel this season.";
    // Exact (always near-identical regardless of threshold).
    assert!(is_near_identical(a, a, 1.0));
    // Case / whitespace / trailing punctuation only.
    assert!(is_near_identical(
        a,
        "  AYE,   the rents be cruel this season  ",
        1.0
    ));
    // Clearly distinct content must NOT be flagged at a high threshold.
    assert!(!is_near_identical(
        a,
        "The fiddler will play a reel at the céilí tonight.",
        0.92
    ));
}

/// AC-3: when the new line is near-identical to the NPC's previous line, the
/// guard substitutes a varied, non-empty fallback that differs from both the
/// previous line and a verbatim repeat.
#[test]
fn guard_against_repetition_varies_cross_turn_repeat() {
    let prev = "Sure, the talk 'round here be always of the weather and the rents, m'friend.";
    // Model echoes its own previous line near-verbatim (only trailing
    // punctuation changed).
    let new_line = "Sure, the talk 'round here be always of the weather and the rents, m'friend!";
    let out = guard_against_repetition(new_line, Some(prev), 0.92, 42, &[]);
    assert!(!out.trim().is_empty(), "AC-3: fallback must be non-empty");
    assert!(
        !is_near_identical(&out, prev, 0.92),
        "AC-3: guard must vary the repeated line, got:\n{out}"
    );
}

/// AC-4: a clearly different new line passes through unchanged (no false
/// positive). Only intra-line collapse may touch it, and here there is none.
#[test]
fn guard_against_repetition_passes_distinct_line() {
    let prev = "Good evening to ye, traveller.";
    let new_line = "The miller's wife claims she saw a boat on the stream by night.";
    let out = guard_against_repetition(new_line, Some(prev), 0.92, 7, &[]);
    assert_eq!(
        out, new_line,
        "AC-4: distinct line must pass through unchanged"
    );
}

/// AC-3 + AC-1 combined: the worst-case #1228 input (a previous line plus a
/// new line that is BOTH an internal loop AND a repeat of the previous line)
/// yields a short, varied, non-degenerate line.
#[test]
fn guard_against_repetition_handles_loop_that_echoes_previous() {
    let prev = "Speak yer mind, and we'll see what be in it, m'friend.";
    let new_line = "Speak yer mind, and we'll see what be in it, m'friend. ".repeat(15);
    let out = guard_against_repetition(&new_line, Some(prev), 0.92, 3, &[]);
    assert!(out.matches("Speak yer mind").count() <= 1, "got:\n{out}");
    assert!(
        !is_near_identical(&out, prev, 0.92),
        "must not echo the previous line, got:\n{out}"
    );
}

// ── AC-2: double farewell token dedup (#1387) ────────────────────────────────

/// AC-2 (#1387): a reply containing "Slán abhaile" twice (non-consecutively)
/// must have the second occurrence removed by `dedup_farewell_tokens`.
#[test]
fn dedup_farewell_tokens_removes_second_slan_abhaile() {
    let input = "Come back when ye've a mind to. \
                 Slán abhaile to ye for now, then. \
                 Safe journey to ye, stranger. Slán abhaile";
    let out = dedup_farewell_tokens(input);
    // Only one "Slán abhaile" must survive.
    let lower = out.to_lowercase();
    let count = lower.matches("slán abhaile").count();
    assert_eq!(
        count, 1,
        "double 'Slán abhaile' must be reduced to one, got:\n{out}"
    );
    // The text before the first farewell must be preserved.
    assert!(
        out.contains("Come back when ye've a mind to"),
        "preceding text must be preserved:\n{out}"
    );
}

/// Single farewell token is left untouched.
#[test]
fn dedup_farewell_tokens_single_farewell_unchanged() {
    let input = "Good luck to ye. Slán abhaile.";
    let out = dedup_farewell_tokens(input);
    assert!(
        out.contains("Slán abhaile"),
        "single farewell must remain:\n{out}"
    );
}

/// No farewell tokens: text passes through unchanged.
#[test]
fn dedup_farewell_tokens_no_farewell_unchanged() {
    let input = "A fine day for the harvest, to be sure.";
    let out = dedup_farewell_tokens(input);
    assert_eq!(
        out,
        input.trim(),
        "text without farewells must pass through:\n{out}"
    );
}

/// `guard_against_repetition` integrates `dedup_farewell_tokens` so a
/// double-farewell reply is cleaned before reaching the player.
#[test]
fn guard_against_repetition_deduplicates_farewell_tokens() {
    let double = "May yer trade flourish. Slán abhaile to ye. Safe journey. Slán abhaile";
    let out = guard_against_repetition(double, None, 0.92, 0, &[]);
    let lower = out.to_lowercase();
    let count = lower.matches("slán abhaile").count();
    assert_eq!(count, 1, "guard must dedup double farewell, got:\n{out}");
}

/// A threshold of 0.0 disables the cross-turn check, but intra-line collapse
/// still runs (runaway loops are always undesirable).
#[test]
fn guard_against_repetition_threshold_zero_disables_cross_turn_only() {
    let prev = "Aye, as I said before.";
    let new_line = "Aye, as I said before.";
    let out = guard_against_repetition(new_line, Some(prev), 0.0, 1, &[]);
    // Cross-turn check disabled: the line is NOT replaced by a fallback.
    assert_eq!(out, new_line);
    // But a runaway loop is still collapsed even with the check disabled.
    let loop_line = "Same clause here. ".repeat(10);
    let collapsed = guard_against_repetition(&loop_line, None, 0.0, 1, &[]);
    assert_eq!(collapsed.matches("Same clause here").count(), 1);
}
