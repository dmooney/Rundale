//! Malformed-input tests for mod artefact loaders.
//!
//! Closes issue #730: every artefact type loaded by `GameMod::load()` and
//! the adjacent `WorldGraph::load_from_str` / `load_npcs_from_str` loaders
//! must return a descriptive `Err` when handed invalid data rather than
//! panicking or silently succeeding.
//!
//! Test strategy
//! - One test per artefact loader.
//! - Each test builds a valid minimal mod directory in a `TempDir`, replaces
//!   the artefact under test with a deliberately malformed version, and
//!   asserts that the loader returns `Err` (not `Ok`, not a panic).
//! - Where the error message is inspectable the test also asserts that it is
//!   non-empty and describes the problem (i.e. is useful to a mod author).

use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a minimal-but-valid mod into a `TempDir`.
///
/// Every file is valid. Individual tests then overwrite specific artefacts
/// with malformed content before loading.
fn minimal_valid_mod() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::create_dir_all(root.join("prompts")).unwrap();
    fs::write(root.join("prompts/tier1_system.txt"), "Tier-1 system.").unwrap();
    fs::write(root.join("prompts/tier1_context.txt"), "Tier-1 context.").unwrap();
    fs::write(root.join("prompts/tier2_system.txt"), "Tier-2 system.").unwrap();

    // world.json — two locations with bidirectional connections
    fs::write(
        root.join("world.json"),
        r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "The Crossroads",
                    "description_template": "A crossroads {time}.",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 2, "path_description": "A narrow boreen."}]
                },
                {
                    "id": 2,
                    "name": "The Mill",
                    "description_template": "A mill {time}.",
                    "indoor": true,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "Back to the crossroads."}]
                }
            ]
        }"#,
    )
    .unwrap();

    // npcs.json — minimal valid roster (single NPC, no relationships)
    fs::write(
        root.join("npcs.json"),
        r#"{"npcs": [{"id": 1, "name": "Séamas", "age": 45, "occupation": "Farmer",
                       "personality": "Quiet", "home": 1, "workplace": null, "mood": "neutral",
                       "relationships": []}]}"#,
    )
    .unwrap();

    // anachronisms.json
    fs::write(
        root.join("anachronisms.json"),
        r#"{"context_alert_prefix": "NOTE:", "context_alert_suffix": "END", "terms": []}"#,
    )
    .unwrap();

    // festivals.json
    fs::write(root.join("festivals.json"), "[]").unwrap();

    // encounters.json
    fs::write(
        root.join("encounters.json"),
        r#"{"morning": "A farmer nods."}"#,
    )
    .unwrap();

    // loading.toml
    fs::write(
        root.join("loading.toml"),
        "spinner_frames = [\"|\"]\nspinner_colors = [[200, 180, 100]]\nphrases = [\"Loading...\"]\n",
    )
    .unwrap();

    // ui.toml
    fs::write(root.join("ui.toml"), "").unwrap();

    // pronunciations.json
    fs::write(root.join("pronunciations.json"), r#"{"names": []}"#).unwrap();

    // transport.toml
    fs::write(
        root.join("transport.toml"),
        "default = \"walking\"\n\n[[modes]]\nid = \"walking\"\nlabel = \"on foot\"\nspeed_m_per_s = 1.25\n",
    )
    .unwrap();

    // mod.toml
    fs::write(
        root.join("mod.toml"),
        r#"
[mod]
name = "Bad-Mod Test"
id = "bad-mod-test"
version = "0.1.0"
description = "A minimal mod for malformed-input testing."

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 1
period_year = 1820

[files]
world = "world.json"
npcs = "npcs.json"
anachronisms = "anachronisms.json"
festivals = "festivals.json"
encounters = "encounters.json"
loading = "loading.toml"
ui = "ui.toml"
pronunciations = "pronunciations.json"
transport = "transport.toml"

[prompts]
tier1_system = "prompts/tier1_system.txt"
tier1_context = "prompts/tier1_context.txt"
tier2_system = "prompts/tier2_system.txt"
"#,
    )
    .unwrap();

    tmp
}

// ---------------------------------------------------------------------------
// 1. mod.toml — missing required `name` field
// ---------------------------------------------------------------------------

#[test]
fn mod_toml_missing_name_returns_err() {
    let tmp = minimal_valid_mod();
    fs::write(
        tmp.path().join("mod.toml"),
        r#"
[mod]
id = "bad"
version = "1.0.0"
description = "Missing name"

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 1
period_year = 1820

[files]
world = "world.json"
npcs = "npcs.json"
anachronisms = "anachronisms.json"
festivals = "festivals.json"
encounters = "encounters.json"
loading = "loading.toml"
ui = "ui.toml"

[prompts]
tier1_system = "prompts/tier1_system.txt"
tier1_context = "prompts/tier1_context.txt"
tier2_system = "prompts/tier2_system.txt"
"#,
    )
    .unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(
        result.is_err(),
        "mod.toml missing `name` should fail to load"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 2. mod.toml — invalid TOML syntax
// ---------------------------------------------------------------------------

#[test]
fn mod_toml_invalid_toml_returns_err() {
    let tmp = minimal_valid_mod();
    fs::write(tmp.path().join("mod.toml"), "this is {{{{ not valid toml").unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(result.is_err(), "invalid mod.toml TOML should fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 3. world.json — orphan location (no connections)
// ---------------------------------------------------------------------------
//
// WorldGraph::validate() rejects locations with no connections because a
// player can never reach or leave them — they are always a bug.

#[test]
fn world_json_orphan_location_returns_err() {
    let json = r#"{"locations": [
        {"id": 1, "name": "Nowhere", "description_template": "{time}", "indoor": false, "public": true, "connections": []}
    ]}"#;
    let result = parish_world::graph::WorldGraph::load_from_str(json);
    assert!(result.is_err(), "orphan location should fail validation");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("orphan") || msg.contains("connections") || msg.contains("Nowhere"),
        "error should mention the orphan; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 4. world.json — dangling connection target (non-existent location id)
// ---------------------------------------------------------------------------

#[test]
fn world_json_dangling_target_returns_err() {
    let json = r#"{"locations": [
        {
            "id": 1, "name": "The Crossroads",
            "description_template": "{time}", "indoor": false, "public": true,
            "connections": [{"target": 999, "path_description": "Nowhere."}]
        }
    ]}"#;
    let result = parish_world::graph::WorldGraph::load_from_str(json);
    assert!(result.is_err(), "dangling connection target should fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 5. npcs.json — dangling relationship target
// ---------------------------------------------------------------------------
//
// load_npcs_from_str() validates referential integrity: every relationship
// target must be present in the same file.

#[test]
fn npcs_json_dangling_relationship_returns_err() {
    let json = r#"{"npcs": [
        {
            "id": 1, "name": "Séamas", "age": 45, "occupation": "Farmer",
            "personality": "Quiet", "home": 1, "workplace": null, "mood": "neutral",
            "relationships": [{"target_id": 999, "kind": "Friend", "strength": 0.5}]
        }
    ]}"#;
    let result = parish_npc::data::load_npcs_from_str(json);
    assert!(
        result.is_err(),
        "dangling NPC relationship target should fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 6. npcs.json — missing required field (`name`)
// ---------------------------------------------------------------------------

#[test]
fn npcs_json_missing_name_returns_err() {
    // `name` has no serde(default) in NpcFileEntry; omitting it must error.
    let json = r#"{"npcs": [
        {
            "id": 1, "age": 45, "occupation": "Farmer",
            "personality": "Quiet", "home": 1, "workplace": null, "mood": "neutral",
            "relationships": []
        }
    ]}"#;
    let result = parish_npc::data::load_npcs_from_str(json);
    assert!(result.is_err(), "NPC missing `name` should fail to parse");
}

// ---------------------------------------------------------------------------
// 7. prompts/*.txt — (via GameMod::load) prompt file is missing entirely
// ---------------------------------------------------------------------------
//
// A missing prompt file must return Err, not panic.

#[test]
fn prompt_file_missing_returns_err() {
    let tmp = minimal_valid_mod();
    // Remove the tier1_system prompt; the file reference remains in mod.toml
    fs::remove_file(tmp.path().join("prompts/tier1_system.txt")).unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(
        result.is_err(),
        "missing prompt file should cause load failure"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 8. anachronisms.json — missing required `context_alert_prefix` field
// ---------------------------------------------------------------------------

#[test]
fn anachronisms_json_missing_prefix_returns_err() {
    let tmp = minimal_valid_mod();
    // Write anachronisms.json without `context_alert_prefix`
    fs::write(
        tmp.path().join("anachronisms.json"),
        r#"{"context_alert_suffix": "END", "terms": []}"#,
    )
    .unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(
        result.is_err(),
        "anachronisms.json missing `context_alert_prefix` should fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 9. festivals.json — type mismatch (`month` as string instead of u32)
// ---------------------------------------------------------------------------

#[test]
fn festivals_json_type_mismatch_returns_err() {
    let tmp = minimal_valid_mod();
    fs::write(
        tmp.path().join("festivals.json"),
        r#"[{"name": "Imbolc", "month": "February", "day": 1, "description": "Spring."}]"#,
    )
    .unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(
        result.is_err(),
        "festivals.json with string `month` should fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 10. encounters.json — wrong shape (array instead of object)
// ---------------------------------------------------------------------------

#[test]
fn encounters_json_wrong_shape_returns_err() {
    let tmp = minimal_valid_mod();
    // EncounterTable uses #[serde(flatten)] on a BTreeMap, which requires an
    // object at the top level. Giving it a JSON array must fail.
    fs::write(
        tmp.path().join("encounters.json"),
        r#"["A farmer nods.", "An owl hoots."]"#,
    )
    .unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(
        result.is_err(),
        "encounters.json as array instead of object should fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 11. loading.toml — missing required `phrases` field
// ---------------------------------------------------------------------------

#[test]
fn loading_toml_missing_phrases_returns_err() {
    let tmp = minimal_valid_mod();
    fs::write(
        tmp.path().join("loading.toml"),
        "spinner_frames = [\"|\"]\nspinner_colors = [[200, 180, 100]]\n",
    )
    .unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(
        result.is_err(),
        "loading.toml missing `phrases` should fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 12. ui.toml — invalid TOML syntax
// ---------------------------------------------------------------------------
//
// Note: UiConfig is entirely `#[serde(default)]`, so the only malformed-input
// path reachable without fixing the loader is a syntax error in the TOML
// itself (a missing field silently succeeds with defaults).

#[test]
fn ui_toml_invalid_syntax_returns_err() {
    let tmp = minimal_valid_mod();
    fs::write(
        tmp.path().join("ui.toml"),
        "[sidebar\nhints_label = \"Missing closing bracket\"\n",
    )
    .unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(result.is_err(), "ui.toml with invalid syntax should fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 13. pronunciations.json — missing required `word` field
// ---------------------------------------------------------------------------

#[test]
fn pronunciations_json_missing_word_returns_err() {
    let tmp = minimal_valid_mod();
    fs::write(
        tmp.path().join("pronunciations.json"),
        r#"{"names": [{"pronunciation": "NEEV", "meaning": "brightness"}]}"#,
    )
    .unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(
        result.is_err(),
        "pronunciations.json missing `word` should fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 14. transport.toml — type mismatch (`speed_m_per_s` as string)
// ---------------------------------------------------------------------------

#[test]
fn transport_toml_speed_wrong_type_returns_err() {
    let tmp = minimal_valid_mod();
    fs::write(
        tmp.path().join("transport.toml"),
        "default = \"walking\"\n\n[[modes]]\nid = \"walking\"\nlabel = \"on foot\"\nspeed_m_per_s = \"fast\"\n",
    )
    .unwrap();

    let result = parish_core::game_mod::GameMod::load(tmp.path());
    assert!(
        result.is_err(),
        "transport.toml with string speed should fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        !msg.is_empty(),
        "error message should be non-empty; got: {msg}"
    );
}
