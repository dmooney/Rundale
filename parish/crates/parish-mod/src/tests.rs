//! Tests for mod loading, discovery, pronunciations, and related functionality.

use super::*;
use std::fs;
use tempfile::TempDir;

use parish_types::AnachronismEntry;

/// Build a minimal mod directory inside a tempdir and return it.
fn create_test_mod() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // prompts/
    fs::create_dir_all(root.join("prompts")).unwrap();
    fs::write(root.join("prompts/tier1_system.txt"), "You are tier1.").unwrap();
    fs::write(root.join("prompts/tier1_context.txt"), "Context here.").unwrap();
    fs::write(root.join("prompts/tier2_system.txt"), "You are tier2.").unwrap();

    // world.json (content not parsed by GameMod, just path referenced)
    fs::write(root.join("world.json"), "{}").unwrap();

    // npcs.json
    fs::write(root.join("npcs.json"), "[]").unwrap();

    // anachronisms.json
    fs::write(
        root.join("anachronisms.json"),
        r#"{
            "context_alert_prefix": "NOTE:",
            "context_alert_suffix": "END",
            "terms": [
                {"term": "internet", "reason": "not invented until the 20th century"}
            ]
        }"#,
    )
    .unwrap();

    // festivals.json
    fs::write(
        root.join("festivals.json"),
        r#"[
            {"name": "St Patrick's Day", "month": 3, "day": 17, "description": "Patron saint feast."},
            {"name": "May Day", "month": 5, "day": 1, "description": "Start of summer."}
        ]"#,
    )
    .unwrap();

    // encounters.json
    fs::write(
        root.join("encounters.json"),
        r#"{"morning": "A farmer waves.", "night": "An owl hoots."}"#,
    )
    .unwrap();

    // loading.toml
    fs::write(
        root.join("loading.toml"),
        r#"
spinner_frames = ["|", "/", "-", "\\"]
spinner_colors = [[200, 180, 100], [100, 200, 100]]
phrases = ["Loading...", "Please wait..."]
"#,
    )
    .unwrap();

    // ui.toml
    fs::write(
        root.join("ui.toml"),
        r##"
[sidebar]
hints_label = "Focail"

[theme.palette]
accent = "#aabbcc"
"##,
    )
    .unwrap();

    // mod.toml
    fs::write(
        root.join("mod.toml"),
        r#"
[mod]
name = "Test Mod"
id = "test-mod"
version = "0.1.0"
description = "A test mod."

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 15
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

    tmp
}

#[test]
fn test_mod_meta_app_name_falls_back_to_name() {
    let meta = ModMeta {
        name: "Rundale".to_string(),
        title: None,
        id: "rundale".to_string(),
        save_root: None,
        version: "0.1".to_string(),
        description: String::new(),
        kind: ModKind::default(),
        dependencies: vec![],
        optional_dependencies: vec![],
        conflicts: vec![],
    };
    assert_eq!(meta.app_name(), "Rundale");
}

#[test]
fn test_mod_meta_app_name_treats_blank_save_root_as_unset() {
    let meta = ModMeta {
        name: "Rundale".to_string(),
        title: None,
        id: "rundale".to_string(),
        save_root: Some("   ".to_string()),
        version: "0.1".to_string(),
        description: String::new(),
        kind: ModKind::default(),
        dependencies: vec![],
        optional_dependencies: vec![],
        conflicts: vec![],
    };
    // Whitespace-only save_root must fall back to `name`, not collapse
    // saves into the bare user-data root.
    assert_eq!(meta.app_name(), "Rundale");
}

#[test]
fn test_sanitize_app_name_strips_traversal_and_separators() {
    // Basename extraction handles "../etc" and absolute paths.
    assert_eq!(sanitize_app_name("Rundale"), Some("Rundale".to_string()));
    assert_eq!(
        sanitize_app_name("  Rundale  "),
        Some("Rundale".to_string())
    );
    assert_eq!(sanitize_app_name("../etc"), Some("etc".to_string()));
    assert_eq!(
        sanitize_app_name("/abs/Rundale"),
        Some("Rundale".to_string())
    );
    // Pure traversal / dot / empty all reject.
    assert_eq!(sanitize_app_name(".."), None);
    assert_eq!(sanitize_app_name("."), None);
    assert_eq!(sanitize_app_name(""), None);
    assert_eq!(sanitize_app_name("   "), None);
    // Trailing separator: file_name on "foo/" returns Some("foo").
    assert_eq!(sanitize_app_name("Rundale/"), Some("Rundale".to_string()));
}

#[test]
fn test_app_name_from_mod_engine_fallback_when_none() {
    let resolved = app_name_from_mod(&None);
    assert_eq!(resolved, parish_persistence::paths::DEFAULT_APP_NAME);
}

#[test]
fn test_app_name_from_mod_falls_through_to_name_when_save_root_invalid() {
    // Build a real GameMod via the test fixture, then mutate save_root
    // to an invalid value to verify the resolver falls back to `name`
    // rather than to the engine default.
    let tmp = create_test_mod();
    let mut gm = GameMod::load(tmp.path()).unwrap();
    gm.manifest.meta.save_root = Some("..".to_string());
    let resolved = app_name_from_mod(&Some(gm));
    // Invalid save_root rejected, falls back to sanitised `name`
    // (the fixture's `name = "Test Mod"` → basename "Test Mod").
    assert_eq!(resolved, "Test Mod");
}

#[test]
fn test_mod_meta_app_name_uses_save_root_when_set() {
    let meta = ModMeta {
        name: "Rundale".to_string(),
        title: None,
        id: "rundale".to_string(),
        save_root: Some("Rundale-Beta".to_string()),
        version: "0.1".to_string(),
        description: String::new(),
        kind: ModKind::default(),
        dependencies: vec![],
        optional_dependencies: vec![],
        conflicts: vec![],
    };
    assert_eq!(meta.app_name(), "Rundale-Beta");
}

#[test]
fn test_load_mod_from_directory() {
    let tmp = create_test_mod();
    let gm = GameMod::load(tmp.path()).expect("should load test mod");
    assert_eq!(gm.manifest.meta.id, "test-mod");
    assert_eq!(gm.manifest.meta.name, "Test Mod");
    // Schema-additive: test fixture omits `save_root`, so it must
    // round-trip as None and `app_name()` falls back to `name`.
    assert!(gm.manifest.meta.save_root.is_none());
    assert_eq!(gm.manifest.meta.app_name(), "Test Mod");
    assert_eq!(gm.prompts.tier1_system, "You are tier1.");
    assert_eq!(gm.anachronisms.terms.len(), 1);
    assert_eq!(gm.festivals.len(), 2);
    assert_eq!(gm.loading.spinner_frames.len(), 4);
    assert!(gm.scenes.is_none());
    // No pronunciations file referenced → empty vec
    assert!(gm.pronunciations.is_empty());
    // No transport.toml in test mod — should default to walking
    assert_eq!(gm.transport.default, "walking");
    assert_eq!(gm.transport.modes.len(), 1);
    assert_eq!(gm.transport.default_mode().id, "walking");
}

fn create_test_mod_with_scenes() -> TempDir {
    let tmp = create_test_mod();
    let root = tmp.path();
    fs::create_dir_all(root.join("assets/scenes/test-location")).unwrap();
    fs::create_dir_all(root.join("assets/scenes/sprites")).unwrap();
    fs::write(root.join("assets/scenes/test-location/plate.png"), b"plate").unwrap();
    fs::write(
        root.join("assets/scenes/sprites/generic-villager.png"),
        b"sprite",
    )
    .unwrap();
    fs::write(
        root.join("scenes.json"),
        r#"{
            "scenes": [
                {
                    "location_id": 15,
                    "slug": "test-location",
                    "plate": "assets/scenes/test-location/plate.png",
                    "hotspots": [
                        {
                            "id": "lane",
                            "shape": { "rect": [10.0, 20.0, 30.0, 40.0] },
                            "label": "A lane",
                            "action": { "travel_to": 1 }
                        }
                    ],
                    "slots": [
                        { "id": "visitor", "x": 50.0, "y": 70.0 }
                    ]
                }
            ],
            "fallback_sprites": {
                "default": "assets/scenes/sprites/generic-villager.png"
            }
        }"#,
    )
    .unwrap();

    let manifest = fs::read_to_string(root.join("mod.toml")).unwrap();
    fs::write(
        root.join("mod.toml"),
        manifest.replace(
            "ui = \"ui.toml\"",
            "ui = \"ui.toml\"\nscenes = \"scenes.json\"",
        ),
    )
    .unwrap();

    tmp
}

#[test]
fn test_load_mod_with_optional_scenes() {
    let tmp = create_test_mod_with_scenes();
    let gm = GameMod::load(tmp.path()).expect("should load mod with scenes");
    let scenes = gm.scenes.as_ref().expect("scenes index should load");

    assert_eq!(scenes.scenes.len(), 1);
    assert_eq!(scenes.sprite_asset_count(), 1);
    assert!(scenes.scene_for(parish_types::LocationId(15)).is_some());
    assert_eq!(
        gm.scene_load_summary().as_deref(),
        Some("scenes.json loaded: 1 scenes, 0 layers, 0 assets, 1 sprites")
    );
}

#[test]
fn test_mod_world_path() {
    let tmp = create_test_mod();
    let gm = GameMod::load(tmp.path()).unwrap();
    assert!(gm.world_path().ends_with("world.json"));
    assert!(gm.world_path().is_absolute());
}

#[test]
fn test_mod_npcs_path() {
    let tmp = create_test_mod();
    let gm = GameMod::load(tmp.path()).unwrap();
    assert!(gm.npcs_path().ends_with("npcs.json"));
    assert!(gm.npcs_path().is_absolute());
}

#[test]
fn test_mod_icon_paths_resolve_under_assets() {
    let tmp = create_test_mod();
    let icons = tmp.path().join("assets/icons/app");
    fs::create_dir_all(&icons).unwrap();
    fs::write(icons.join("icon-512.png"), b"icon").unwrap();
    fs::write(icons.join("favicon-32.png"), b"favicon").unwrap();
    fs::write(
        tmp.path().join("ui.toml"),
        r##"
[branding]
app_icon = "assets/icons/app/icon-512.png"
favicon = "assets/icons/app/favicon-32.png"

[sidebar]
hints_label = "Focail"

[theme.palette]
accent = "#aabbcc"
"##,
    )
    .unwrap();

    let gm = GameMod::load(tmp.path()).unwrap();
    assert!(gm.app_icon_path().unwrap().ends_with("icon-512.png"));
    assert!(gm.favicon_path().unwrap().ends_with("favicon-32.png"));
}

#[test]
fn test_mod_icon_paths_must_stay_under_assets() {
    let tmp = create_test_mod();
    fs::write(
        tmp.path().join("ui.toml"),
        r##"
[branding]
app_icon = "../icon.png"
"##,
    )
    .unwrap();

    let err = GameMod::load(tmp.path()).expect_err("escaping icon path should fail");
    let msg = format!("{err:?}");
    assert!(msg.contains("ui.branding.app_icon"));
}

#[test]
fn test_encounter_text_lookup() {
    let tmp = create_test_mod();
    let gm = GameMod::load(tmp.path()).unwrap();
    assert_eq!(gm.encounter_text("morning"), Some("A farmer waves."));
    assert_eq!(gm.encounter_text("night"), Some("An owl hoots."));
    assert_eq!(gm.encounter_text("afternoon"), None);
}

#[test]
fn test_check_festival() {
    let tmp = create_test_mod();
    let gm = GameMod::load(tmp.path()).unwrap();
    let fest = gm
        .check_festival(3, 17)
        .expect("should find St Patrick's Day");
    assert_eq!(fest.name, "St Patrick's Day");
    assert!(gm.check_festival(12, 25).is_none());
}

/// #1366 §7 — a manifest without `schema_version` defaults to 1 so every
/// pre-versioning mod keeps loading unchanged.
#[test]
fn test_schema_version_defaults_to_one_when_absent() {
    let tmp = create_test_mod();
    let gm = GameMod::load(tmp.path()).unwrap();
    assert_eq!(gm.manifest.schema_version, 1);
}

/// #1366 §7 — an explicit, supported `schema_version` parses and loads.
#[test]
fn test_schema_version_explicit_supported_value_loads() {
    let tmp = create_test_mod();
    let manifest = fs::read_to_string(tmp.path().join("mod.toml")).unwrap();
    fs::write(
        tmp.path().join("mod.toml"),
        format!("schema_version = 1\n{manifest}"),
    )
    .unwrap();
    let gm = GameMod::load(tmp.path()).unwrap();
    assert_eq!(gm.manifest.schema_version, 1);
}

/// #1366 §7 — a mod authored against a future schema is rejected with an
/// actionable message, not half-parsed.
#[test]
fn test_schema_version_future_value_is_rejected() {
    let tmp = create_test_mod();
    let manifest = fs::read_to_string(tmp.path().join("mod.toml")).unwrap();
    fs::write(
        tmp.path().join("mod.toml"),
        format!("schema_version = 2\n{manifest}"),
    )
    .unwrap();
    let err = GameMod::load(tmp.path()).unwrap_err().to_string();
    assert!(
        err.contains("schema_version = 2") && err.contains("supports up to 1"),
        "got: {err}"
    );
}

#[test]
fn test_load_nonexistent_dir() {
    let result = GameMod::load(Path::new("/tmp/nonexistent_parish_mod_dir_12345"));
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("mod directory not found"), "got: {err}");
}

/// #741 — a malicious mod.toml with a `..` path must be rejected, not
/// allowed to read files outside the mod directory.
#[test]
fn test_load_rejects_directory_traversal_in_manifest() {
    let outer = TempDir::new().unwrap();
    // Sensitive file outside the mod directory.
    fs::write(outer.path().join("secret.txt"), "TOP SECRET").unwrap();

    // Build a real mod inside outer/, then rewrite mod.toml so the
    // `world` path traverses out to secret.txt.
    let mod_dir = outer.path().join("mod");
    fs::create_dir_all(&mod_dir).unwrap();

    // Re-use the canonical fixture's contents but point a manifest path at a traversal.
    let inner_tmp = create_test_mod();
    for entry in fs::read_dir(inner_tmp.path()).unwrap() {
        let entry = entry.unwrap();
        let dest = mod_dir.join(entry.file_name());
        if entry.path().is_dir() {
            fs::create_dir_all(&dest).unwrap();
            for sub in fs::read_dir(entry.path()).unwrap() {
                let sub = sub.unwrap();
                fs::copy(sub.path(), dest.join(sub.file_name())).unwrap();
            }
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }

    // Overwrite mod.toml with a malicious anachronisms path. (anachronisms
    // is read via read_text during load, unlike world/npcs which are
    // resolved lazily by world_path()/npcs_path().)
    let manifest = fs::read_to_string(mod_dir.join("mod.toml")).unwrap();
    let evil = manifest.replace(
        "anachronisms = \"anachronisms.json\"",
        "anachronisms = \"../secret.txt\"",
    );
    fs::write(mod_dir.join("mod.toml"), evil).unwrap();

    let result = GameMod::load(&mod_dir);
    assert!(result.is_err(), "expected traversal to be rejected");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("escapes mod directory"),
        "expected escape error, got: {err}"
    );
}

#[test]
fn test_festival_def_deserialize() {
    let json = r#"{"name":"Lughnasa","month":8,"day":1,"description":"Harvest festival."}"#;
    let f: FestivalDef = serde_json::from_str(json).unwrap();
    assert_eq!(f.name, "Lughnasa");
    assert_eq!(f.month, 8);
    assert_eq!(f.day, 1);
}

#[test]
fn test_loading_config_deserialize() {
    let toml_str = r#"
spinner_frames = ["a", "b"]
spinner_colors = [[255, 0, 0]]
phrases = ["Loading"]
"#;
    let lc: LoadingConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(lc.spinner_frames, vec!["a", "b"]);
    assert_eq!(lc.spinner_colors, vec![[255, 0, 0]]);
    assert_eq!(lc.phrases, vec!["Loading"]);
}

#[test]
fn test_ui_config_defaults() {
    let toml_str = "";
    let ui: UiConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(ui.sidebar.hints_label, "Language Hints");
    // Defaults are now neutral grey — the engine ships no aesthetic of
    // its own. Mods supply colours via `ui.toml`.
    assert_eq!(ui.theme.palette.bg, "#18181a");
    assert_eq!(ui.theme.palette.accent, "#8c8c96");
}

#[test]
fn test_ui_config_custom() {
    let toml_str = r##"
[sidebar]
hints_label = "Custom"

[theme.palette]
accent = "#ff0000"
bg = "#010203"
"##;
    let ui: UiConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(ui.sidebar.hints_label, "Custom");
    assert_eq!(ui.theme.palette.bg, "#010203");
    assert_eq!(ui.theme.palette.accent, "#ff0000");
    assert_eq!(ui.theme.palette.fg, "#dcdce0");
}

#[test]
fn test_ui_config_legacy_default_accent() {
    let toml_str = r##"
[theme]
default_accent = "#112233"
"##;
    let ui: UiConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(ui.theme.resolved_palette().accent, "#112233");
    assert_eq!(ui.theme.resolved_palette().bg, "#18181a");
}

#[test]
fn test_anachronism_entry_deserialize() {
    // JSON with the current format (note, category, origin_year)
    let json = r#"{"term":"telephone","category":"technology","origin_year":1876,"note":"invented by Bell in 1876"}"#;
    let e: AnachronismEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.term, "telephone");
    assert_eq!(e.note, "invented by Bell in 1876");
    assert_eq!(e.category.as_deref(), Some("technology"));
    assert_eq!(e.origin_year, Some(1876));
}

#[test]
fn test_anachronism_entry_deserialize_legacy_reason() {
    // Backward compatible: accepts "reason" alias for "note"
    let json = r#"{"term":"telephone","reason":"invented 1876"}"#;
    let e: AnachronismEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.term, "telephone");
    assert_eq!(e.note, "invented 1876");
}

// -- Pronunciation tests --------------------------------------------------

/// Build a test mod that includes a pronunciations.json file.
fn create_test_mod_with_pronunciations() -> TempDir {
    let tmp = create_test_mod();
    let root = tmp.path();

    fs::write(
        root.join("pronunciations.json"),
        r#"{
            "names": [
                {"word": "Niamh", "pronunciation": "NEEV", "meaning": "brightness", "matches": ["Niamh"]},
                {"word": "Siobhán", "pronunciation": "shiv-AWN", "meaning": "Irish form of Joan", "matches": ["Siobhan"]},
                {"word": "Kilteevan", "pronunciation": "kill-TEE-van", "meaning": "Cill Taobháin — Teevan's Church", "matches": ["Kilteevan"]}
            ]
        }"#,
    )
    .unwrap();

    // Rewrite mod.toml to include pronunciations
    fs::write(
        root.join("mod.toml"),
        r#"
[mod]
name = "Test Mod"
id = "test-mod"
version = "0.1.0"
description = "A test mod."

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 15
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

[prompts]
tier1_system = "prompts/tier1_system.txt"
tier1_context = "prompts/tier1_context.txt"
tier2_system = "prompts/tier2_system.txt"
"#,
    )
    .unwrap();

    tmp
}

#[test]
fn test_load_mod_with_pronunciations() {
    let tmp = create_test_mod_with_pronunciations();
    let gm = GameMod::load(tmp.path()).expect("should load mod with pronunciations");
    assert_eq!(gm.pronunciations.len(), 3);
    assert_eq!(gm.pronunciations[0].word, "Niamh");
    assert_eq!(gm.pronunciations[0].pronunciation, "NEEV");
}

#[test]
fn test_name_hints_for_matching() {
    let tmp = create_test_mod_with_pronunciations();
    let gm = GameMod::load(tmp.path()).unwrap();

    // Match NPC name containing "Niamh"
    let hints = gm.name_hints_for(&["Niamh Darcy"]);
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].word, "Niamh");
    assert_eq!(hints[0].pronunciation, "NEEV");

    // Match location name
    let hints = gm.name_hints_for(&["Kilteevan Village"]);
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].word, "Kilteevan");

    // Multiple matches
    let hints = gm.name_hints_for(&["Niamh Darcy", "Kilteevan Village"]);
    assert_eq!(hints.len(), 2);

    // No match
    let hints = gm.name_hints_for(&["Tommy O'Brien"]);
    assert!(hints.is_empty());
}

#[test]
fn test_name_hints_case_insensitive() {
    let tmp = create_test_mod_with_pronunciations();
    let gm = GameMod::load(tmp.path()).unwrap();

    let hints = gm.name_hints_for(&["niamh darcy"]);
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].word, "Niamh");
}

#[test]
fn test_pronunciation_entry_deserialize() {
    let json = r#"{"word":"Aoife","pronunciation":"EE-fa","meaning":"beauty","matches":["Aoife"]}"#;
    let e: PronunciationEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.word, "Aoife");
    assert_eq!(e.pronunciation, "EE-fa");
    assert_eq!(e.meaning, Some("beauty".to_string()));
    assert_eq!(e.matches, vec!["Aoife"]);
}

#[test]
fn test_pronunciation_entry_matches_via_word_fallback() {
    let json = r#"{"word":"Aoife","pronunciation":"EE-fa"}"#;
    let e: PronunciationEntry = serde_json::from_str(json).unwrap();
    // No explicit matches → falls back to matching the word itself
    assert!(e.matches_any(&["Aoife Brennan"]));
    assert!(!e.matches_any(&["Tommy O'Brien"]));
}

// -- Integration test against the real mod directory (skipped in CI) ----

#[test]
fn test_load_real_default_mod() {
    let rundale_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
    if rundale_dir.exists() {
        let gm = GameMod::load(&rundale_dir).expect("should load rundale mod");
        assert!(!gm.manifest.meta.name.is_empty());
        assert!(gm.world_path().is_absolute());
        assert!(gm.npcs_path().is_absolute());
        // The rundale mod should have pronunciation data
        assert!(
            !gm.pronunciations.is_empty(),
            "rundale mod should have pronunciation entries"
        );
    }
}

#[test]
fn test_real_mod_npc_name_hints() {
    let rundale_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
    if rundale_dir.exists() {
        let gm = GameMod::load(&rundale_dir).expect("should load rundale mod");

        // Each NPC with an Irish name should produce a hint
        let hints = gm.name_hints_for(&["Padraig Darcy"]);
        assert_eq!(hints.len(), 1, "Padraig should match");
        assert_eq!(hints[0].word, "Pádraig");

        let hints = gm.name_hints_for(&["Siobhan Murphy"]);
        assert_eq!(hints.len(), 1, "Siobhan should match");
        assert_eq!(hints[0].word, "Siobhán");

        let hints = gm.name_hints_for(&["Niamh Darcy"]);
        assert_eq!(hints.len(), 1, "Niamh should match");

        let hints = gm.name_hints_for(&["Aoife Brennan"]);
        assert_eq!(hints.len(), 1, "Aoife should match");

        let hints = gm.name_hints_for(&["Roisin Connolly"]);
        assert_eq!(hints.len(), 1, "Roisin should match");

        // Location + NPC combined
        let hints = gm.name_hints_for(&["Kilteevan Village", "Padraig Darcy", "Niamh Darcy"]);
        assert_eq!(hints.len(), 3, "should match location + both NPCs");
    }
}

fn write_manifest(dir: &Path, id: &str, kind: Option<&str>) {
    fs::create_dir_all(dir).unwrap();
    let kind_line = kind
        .map(|k| format!("kind = \"{k}\"\n"))
        .unwrap_or_default();
    let body = format!(
        "[mod]\nname = \"{id}\"\nid = \"{id}\"\nversion = \"0.0.0\"\ndescription = \"x\"\n{kind_line}"
    );
    fs::write(dir.join("mod.toml"), body).unwrap();
}

#[test]
fn discover_mods_finds_setting_and_auxiliary_in_lex_order() {
    let tmp = TempDir::new().unwrap();
    let mods = tmp.path().join("mods");
    write_manifest(&mods.join("rundale"), "rundale", Some("base"));
    write_manifest(&mods.join("solarized"), "solarized", Some("asset"));
    write_manifest(&mods.join("aurora"), "aurora", Some("asset"));

    let discovered = discover_mods_in(&mods).expect("discovery succeeds");
    assert!(discovered.base.ends_with("rundale"));
    let aux_ids: Vec<_> = discovered.auxiliary.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(aux_ids, vec!["aurora", "solarized"]);
    assert_eq!(discovered.auxiliary[0].kind, ModKind::Asset);
}

#[test]
fn discover_mods_treats_missing_kind_as_base() {
    let tmp = TempDir::new().unwrap();
    let mods = tmp.path().join("mods");
    write_manifest(&mods.join("rundale"), "rundale", None);
    let discovered = discover_mods_in(&mods).expect("discovery succeeds");
    assert!(discovered.base.ends_with("rundale"));
    assert!(discovered.auxiliary.is_empty());
}

#[test]
fn discover_mods_rejects_two_bases() {
    let tmp = TempDir::new().unwrap();
    let mods = tmp.path().join("mods");
    write_manifest(&mods.join("rundale"), "rundale", Some("base"));
    write_manifest(&mods.join("hokkaido"), "hokkaido", Some("base"));
    let err = discover_mods_in(&mods).expect_err("two bases without mod-list is a hard error");
    let msg = format!("{err:?}");
    assert!(msg.contains("Multiple base mods"));
}

#[test]
fn discover_mods_selects_via_mod_list() {
    let tmp = TempDir::new().unwrap();
    let mods = tmp.path().join("mods");
    write_manifest(&mods.join("rundale"), "rundale", Some("base"));
    write_manifest(&mods.join("testbed"), "testbed", Some("base"));
    fs::write(mods.join("mod-list.toml"), "active_setting = \"testbed\"\n").unwrap();
    let discovered = discover_mods_in(&mods).expect("mod-list.toml selection succeeds");
    assert!(
        discovered.base.ends_with("testbed"),
        "should select the testbed mod"
    );
}

#[test]
fn checked_in_mod_list_selects_rundale_by_default() {
    let mods = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods");
    if mods.exists() {
        let discovered = discover_mods_in(&mods).expect("repo mod discovery succeeds");
        assert!(
            discovered.base.ends_with("rundale"),
            "checked-in mods/mod-list.toml should select rundale (the shipped base mod) by default"
        );
    }
}

#[test]
fn discover_mods_errors_when_active_setting_missing() {
    let tmp = TempDir::new().unwrap();
    let mods = tmp.path().join("mods");
    write_manifest(&mods.join("rundale"), "rundale", Some("base"));
    fs::write(
        mods.join("mod-list.toml"),
        "active_setting = \"nonexistent\"\n",
    )
    .unwrap();
    let err = discover_mods_in(&mods).expect_err("unknown active_setting is a hard error");
    let msg = format!("{err:?}");
    assert!(msg.contains("nonexistent"));
}

#[test]
fn discover_mods_requires_a_base() {
    let tmp = TempDir::new().unwrap();
    let mods = tmp.path().join("mods");
    write_manifest(&mods.join("solarized"), "solarized", Some("asset"));
    let err = discover_mods_in(&mods).expect_err("no base mod is fatal");
    let msg = format!("{err:?}");
    assert!(msg.contains("No base mod"));
}

#[test]
fn discover_mods_classifies_providers_kind() {
    let tmp = TempDir::new().unwrap();
    let mods = tmp.path().join("mods");
    write_manifest(&mods.join("rundale"), "rundale", Some("base"));
    write_manifest(&mods.join("anthropic"), "anthropic", Some("providers"));
    let discovered = discover_mods_in(&mods).expect("discovery succeeds");
    assert_eq!(discovered.auxiliary.len(), 1);
    assert_eq!(discovered.auxiliary[0].kind, ModKind::Providers);
    assert_eq!(discovered.auxiliary[0].id, "anthropic");
}

fn write_provider_toml(dir: &Path, id: &str) {
    let body = format!(
        r#"
id = "{id}"
display_name = "{id} Provider"
kind = "openai-compat"
default_base_url = "https://api.{id}.example/v1"
requires_api_key = true
requires_model = true
api_key_env_var = "{KEY}"
featured = false
"#,
        id = id,
        KEY = id.to_uppercase().replace('-', "_") + "_API_KEY",
    );
    fs::write(dir.join(format!("{id}.toml")), body).unwrap();
}

#[test]
fn load_providers_from_mod_parses_multiple_tomls_in_lex_order() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = tmp.path().join("multi-providers");
    fs::create_dir_all(mod_dir.join("providers")).unwrap();
    // Write three files in an unsorted order. The loader must return them sorted.
    write_provider_toml(&mod_dir.join("providers"), "zeta");
    write_provider_toml(&mod_dir.join("providers"), "alpha");
    write_provider_toml(&mod_dir.join("providers"), "mu");

    let providers = load_providers_from_mod(&mod_dir).expect("load succeeds");
    let ids: Vec<_> = providers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["alpha", "mu", "zeta"]);
}

#[test]
fn load_providers_from_mod_empty_when_directory_missing() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = tmp.path().join("no-providers");
    fs::create_dir_all(&mod_dir).unwrap();
    let providers = load_providers_from_mod(&mod_dir).expect("load succeeds");
    assert!(providers.is_empty());
}

#[test]
fn load_providers_from_mod_rejects_symlink_traversal() {
    // Skip on platforms without symlinks (Windows in CI). Unix-only.
    #[cfg(unix)]
    {
        let tmp = TempDir::new().unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        write_provider_toml(&outside, "evil");

        let mod_dir = tmp.path().join("mod-with-symlink");
        fs::create_dir_all(mod_dir.join("providers")).unwrap();
        // Symlink the evil TOML into the mod's providers/ dir.
        std::os::unix::fs::symlink(
            outside.join("evil.toml"),
            mod_dir.join("providers/evil.toml"),
        )
        .unwrap();

        let err = load_providers_from_mod(&mod_dir).expect_err("traversal must be rejected");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("escapes mod directory"),
            "expected traversal error, got: {msg}"
        );
    }
}

#[test]
fn load_providers_from_mod_rejects_duplicate_ids_within_one_mod() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = tmp.path().join("dup-providers");
    fs::create_dir_all(mod_dir.join("providers")).unwrap();
    // Write two files containing the *same* id.
    let body = r#"
id = "duplicate"
display_name = "Duplicate"
kind = "openai-compat"
default_base_url = "https://api.example/v1"
requires_api_key = false
requires_model = true
featured = false
"#;
    fs::write(mod_dir.join("providers/a.toml"), body).unwrap();
    fs::write(mod_dir.join("providers/b.toml"), body).unwrap();
    let err = load_providers_from_mod(&mod_dir).expect_err("intra-mod duplicate must be rejected");
    let msg = format!("{err:?}");
    assert!(msg.contains("more than once"), "got: {msg}");
}

// ── SettingConfig language field deserialization tests ─────────────────────

#[test]
fn setting_config_with_both_languages() {
    let toml_src = r#"
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820
player_language = "en-IE"
native_language = "ga-IE"
"#;
    let cfg: SettingConfig = toml::from_str(toml_src).expect("should deserialize");
    assert_eq!(cfg.player_language, "en-IE");
    assert_eq!(cfg.native_language.as_deref(), Some("ga-IE"));
}

#[test]
fn setting_config_defaults_player_language_to_en_when_omitted() {
    let toml_src = r#"
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820
"#;
    let cfg: SettingConfig = toml::from_str(toml_src).expect("should deserialize");
    assert_eq!(
        cfg.player_language, "en",
        "player_language should default to \"en\" for backward-compat mods"
    );
    assert!(
        cfg.native_language.is_none(),
        "native_language should default to None"
    );
}

#[test]
fn setting_config_with_only_player_language() {
    let toml_src = r#"
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820
player_language = "fr-FR"
"#;
    let cfg: SettingConfig = toml::from_str(toml_src).expect("should deserialize");
    assert_eq!(cfg.player_language, "fr-FR");
    assert!(
        cfg.native_language.is_none(),
        "native_language should be None when omitted"
    );
}

#[test]
fn game_mod_accessors_expose_language_settings() {
    let tmp = create_test_mod();
    // Rewrite mod.toml to include language fields
    fs::write(
        tmp.path().join("mod.toml"),
        r#"
[mod]
name = "Lang Test Mod"
id = "lang-test"
version = "0.1.0"
description = "Language settings test."

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820
player_language = "en-IE"
native_language = "ga-IE"

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
    let gm = GameMod::load(tmp.path()).expect("should load mod with language settings");
    assert_eq!(gm.player_language(), "en-IE");
    assert_eq!(gm.native_language(), Some("ga-IE"));
}
