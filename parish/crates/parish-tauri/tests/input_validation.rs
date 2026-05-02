//! Mode-parity tests for editor input validation (issue #750).
//!
//! Both the Tauri and Axum server paths delegate to the shared helpers
//! `validate_npc_payload` and `validate_location_payload` in
//! `parish_core::ipc::editor`.  The shared handlers
//! `handle_editor_update_npcs` / `handle_editor_update_locations` call
//! them unconditionally, so validation is structurally enforced on BOTH
//! backends — not a convention that can silently be skipped.
//!
//! These tests exercise the handlers that the Tauri command wrappers call,
//! confirming that the Tauri path rejects the same oversized payloads as the
//! server, and that the error text is identical (same `Display` impl).
//!
//! Note: the `input_validation.rs` file referenced in the task description
//! for PR #843 does not exist in the repository.  These tests follow the
//! pattern established by `parish-tauri/tests/command_registry.rs` and the
//! `parish-core` unit tests added for #750.

use std::sync::Mutex;

use parish_core::editor::types::{EditorManifest, EditorModSnapshot, ValidationReport};
use parish_core::game_mod::{AnachronismData, EncounterTable};
use parish_core::ipc::editor::{
    EditorSession, EditorValidationError, LOCATION_DESCRIPTION_MAX, LOCATIONS_PER_FILE_MAX,
    NPC_NAME_MAX, NPC_PERSONALITY_MAX, NPC_RELATIONSHIPS_MAX, NPCS_PER_FILE_MAX,
    handle_editor_update_locations, handle_editor_update_npcs, validate_location_payload,
    validate_npc_payload,
};
// Re-exports: parish_core::npc = parish_npc, parish_core::world = parish_world.
use parish_core::npc;
use parish_core::world;

// ── helpers ───────────────────────────────────────────────────────────────────

fn minimal_snapshot() -> EditorModSnapshot {
    EditorModSnapshot {
        mod_path: std::path::PathBuf::from("/tmp/test_mod"),
        manifest: EditorManifest {
            id: "test".to_string(),
            name: "Test Mod".to_string(),
            title: None,
            version: "0.1.0".to_string(),
            description: String::new(),
            start_date: "1820-01-01".to_string(),
            start_location: 0,
            period_year: 1820,
        },
        npcs: npc::NpcFile { npcs: vec![] },
        locations: vec![],
        festivals: vec![],
        encounters: EncounterTable {
            by_time: Default::default(),
        },
        anachronisms: AnachronismData {
            context_alert_prefix: String::new(),
            context_alert_suffix: String::new(),
            terms: vec![],
        },
        validation: ValidationReport::default(),
    }
}

fn seeded_session() -> Mutex<EditorSession> {
    Mutex::new(EditorSession {
        snapshot: Some(minimal_snapshot()),
        version: 0,
        generation: 0,
    })
}

fn npc_entry(name: &str, personality: &str) -> npc::NpcFileEntry {
    npc::NpcFileEntry {
        id: 1,
        name: name.to_string(),
        brief_description: None,
        age: 30,
        occupation: "Farmer".to_string(),
        personality: personality.to_string(),
        intelligence: None,
        home: 1,
        workplace: None,
        mood: "calm".to_string(),
        schedule: None,
        seasonal_schedule: None,
        relationships: vec![],
        knowledge: vec![],
    }
}

fn location_data(name: &str, description: &str) -> world::graph::LocationData {
    // LocationId is re-exported by parish_world (= parish_core::world).
    world::graph::LocationData {
        id: world::LocationId(1),
        name: name.to_string(),
        description_template: description.to_string(),
        indoor: false,
        public: true,
        connections: vec![],
        lat: 0.0,
        lon: 0.0,
        associated_npcs: vec![],
        mythological_significance: None,
        aliases: vec![],
        geo_kind: world::graph::GeoKind::default(),
        relative_to: None,
        geo_source: None,
    }
}

// ── Parity: NPC handler rejects the same payloads as the shared validator ─────

/// Confirms that `handle_editor_update_npcs` (called by the Tauri command)
/// rejects an oversized NPC payload with the same error text as calling
/// `validate_npc_payload` directly.
#[test]
fn tauri_handler_rejects_long_npc_name_with_same_error_as_validator() {
    let long_name = "a".repeat(NPC_NAME_MAX + 1);
    let file = npc::NpcFile {
        npcs: vec![npc_entry(&long_name, "fine")],
    };

    // Error from the shared validator.
    let validator_err = validate_npc_payload(&file).unwrap_err().to_string();

    // Error from the handler (Tauri call path).
    let session = seeded_session();
    let handler_err = handle_editor_update_npcs(&session, file).unwrap_err();

    assert_eq!(
        handler_err, validator_err,
        "Tauri handler must surface the same error text as the shared validator"
    );
    // Version must not be bumped on rejection.
    let s = session.lock().unwrap();
    assert_eq!(
        s.version, 0,
        "version must not be bumped on validation failure"
    );
}

/// Confirms that `handle_editor_update_npcs` rejects too-many-NPC payloads.
#[test]
fn tauri_handler_rejects_too_many_npcs() {
    let npcs: Vec<_> = (0u32..=(NPCS_PER_FILE_MAX as u32))
        .map(|i| {
            let mut e = npc_entry("X", "ok");
            e.id = i;
            e
        })
        .collect();
    let file = npc::NpcFile { npcs };

    let session = seeded_session();
    let err = handle_editor_update_npcs(&session, file).unwrap_err();
    assert!(
        err.contains("too many NPCs"),
        "expected 'too many NPCs' error, got: {err}"
    );
    assert!(
        err.contains(&NPCS_PER_FILE_MAX.to_string()),
        "error should mention the cap, got: {err}"
    );
}

/// Confirms that `handle_editor_update_npcs` rejects NPC personality with
/// control characters.
#[test]
fn tauri_handler_rejects_npc_personality_control_char() {
    let file = npc::NpcFile {
        npcs: vec![npc_entry("Alice", "bad \x01 char")],
    };
    let session = seeded_session();
    let err = handle_editor_update_npcs(&session, file).unwrap_err();
    assert!(
        err.contains("control characters"),
        "expected control-character error, got: {err}"
    );
}

/// Confirms that `handle_editor_update_npcs` rejects too-many-relationships.
#[test]
fn tauri_handler_rejects_too_many_relationships() {
    let mut npc = npc_entry("Alice", "fine");
    npc.relationships = (0..=(NPC_RELATIONSHIPS_MAX as u32))
        .map(|i| npc::RelationshipFileEntry {
            target_id: i + 100,
            kind: npc::types::RelationshipKind::Friend,
            strength: 0.5,
        })
        .collect();
    let file = npc::NpcFile { npcs: vec![npc] };
    let session = seeded_session();
    let err = handle_editor_update_npcs(&session, file).unwrap_err();
    assert!(
        err.contains("too many relationships"),
        "expected 'too many relationships' error, got: {err}"
    );
}

/// Confirms that `handle_editor_update_npcs` rejects long personality.
#[test]
fn tauri_handler_rejects_long_personality() {
    let long_p = "p".repeat(NPC_PERSONALITY_MAX + 1);
    let file = npc::NpcFile {
        npcs: vec![npc_entry("Alice", &long_p)],
    };
    let session = seeded_session();
    let err = handle_editor_update_npcs(&session, file).unwrap_err();
    assert!(
        err.contains("personality too long"),
        "expected personality-too-long error, got: {err}"
    );
}

// ── Parity: location handler rejects the same payloads as the shared validator

/// Confirms that `handle_editor_update_locations` (called by the Tauri command)
/// rejects an oversized location payload with the same error text as calling
/// `validate_location_payload` directly.
#[test]
fn tauri_handler_rejects_long_location_description_with_same_error_as_validator() {
    let long_desc = "d".repeat(LOCATION_DESCRIPTION_MAX + 1);
    let locs = vec![location_data("Village", &long_desc)];

    // Error from the shared validator.
    let validator_err = validate_location_payload(&locs).unwrap_err().to_string();

    // Error from the handler (Tauri call path).
    let session = seeded_session();
    let handler_err = handle_editor_update_locations(&session, locs).unwrap_err();

    assert_eq!(
        handler_err, validator_err,
        "Tauri handler must surface the same error text as the shared validator"
    );
    let s = session.lock().unwrap();
    assert_eq!(
        s.version, 0,
        "version must not be bumped on validation failure"
    );
}

/// Confirms that `handle_editor_update_locations` rejects too-many-locations.
#[test]
fn tauri_handler_rejects_too_many_locations() {
    let locs: Vec<_> = (0..=(LOCATIONS_PER_FILE_MAX as u32))
        .map(|i| {
            let mut l = location_data("X", "ok");
            l.id = world::LocationId(i);
            l
        })
        .collect();
    let session = seeded_session();
    let err = handle_editor_update_locations(&session, locs).unwrap_err();
    assert!(
        err.contains("too many locations"),
        "expected 'too many locations' error, got: {err}"
    );
}

/// Confirms that `handle_editor_update_locations` rejects location description
/// with control characters.
#[test]
fn tauri_handler_rejects_location_description_control_char() {
    let locs = vec![location_data("Village", "desc with \x1b escape")];
    let session = seeded_session();
    let err = handle_editor_update_locations(&session, locs).unwrap_err();
    assert!(
        err.contains("control characters"),
        "expected control-character error, got: {err}"
    );
}

// ── Validate that valid payloads still succeed ────────────────────────────────

/// A valid NPC payload must not be rejected by the Tauri handler.
#[test]
fn tauri_handler_accepts_valid_npc_payload() {
    let file = npc::NpcFile {
        npcs: vec![npc_entry("Padraig Darcy", "Kind and generous farmer")],
    };
    let session = seeded_session();
    assert!(
        handle_editor_update_npcs(&session, file).is_ok(),
        "valid NPC payload must not be rejected"
    );
    let s = session.lock().unwrap();
    assert_eq!(s.version, 1, "version must be bumped on success");
}

/// A valid location payload must not be rejected by the Tauri handler.
#[test]
fn tauri_handler_accepts_valid_location_payload() {
    let locs = vec![location_data("Kilteevan", "A quiet village crossroads.")];
    let session = seeded_session();
    assert!(
        handle_editor_update_locations(&session, locs).is_ok(),
        "valid location payload must not be rejected"
    );
    let s = session.lock().unwrap();
    assert_eq!(s.version, 1, "version must be bumped on success");
}

// ── EditorValidationError Display smoke-tests ────────────────────────────────

/// Confirm each error variant produces a non-empty message.
#[test]
fn validation_error_display_is_non_empty() {
    let variants = vec![
        EditorValidationError::TooManyNpcs { count: 9999 },
        EditorValidationError::NpcNameControlChars {
            name: "X".to_string(),
        },
        EditorValidationError::NpcNameTooLong {
            name: "X".to_string(),
            count: 999,
        },
        EditorValidationError::NpcBioControlChars {
            name: "X".to_string(),
        },
        EditorValidationError::NpcBioTooLong {
            name: "X".to_string(),
            count: 9999,
        },
        EditorValidationError::NpcPersonalityControlChars {
            name: "X".to_string(),
        },
        EditorValidationError::NpcPersonalityTooLong {
            name: "X".to_string(),
            count: 9999,
        },
        EditorValidationError::NpcTooManyRelationships {
            name: "X".to_string(),
            count: 999,
        },
        EditorValidationError::TooManyLocations { count: 9999 },
        EditorValidationError::LocationDescriptionControlChars {
            name: "X".to_string(),
        },
        EditorValidationError::LocationDescriptionTooLong {
            name: "X".to_string(),
            count: 9999,
        },
    ];
    for v in &variants {
        let msg = v.to_string();
        assert!(
            !msg.is_empty(),
            "Display impl must not produce empty string for {v:?}"
        );
    }
}
