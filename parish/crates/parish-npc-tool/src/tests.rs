//! Integration-style unit tests for the parish-npc-tool CLI.

use super::*;
use crate::db::{DB_FILENAME, NPC_TOOL_DB_ENV};
use crate::generate::{OCCUPATIONS, weighted_occupation};
use crate::import_export::{import_npcs_inner, parse_import_blob};
use crate::query::escape_like;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rusqlite::{Connection, params};

fn generated_conn(parish: &str, pop: u32, seed: u64) -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    generate_world(&conn, &["roscommon".to_string()]).expect("world generation should work");
    generate_parish(&conn, parish, pop, Some(seed)).expect("parish generation should work");
    conn
}

fn assert_validation_failed(result: Result<()>) {
    assert!(result.is_err(), "validation should fail");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("validation failed"),
        "validation error should use the aggregate failure message"
    );
}

// ── notebook person-art inputs ──────────────────────────────────────────

fn minimal_art_direction(npcs: &str) -> String {
    format!(
        r#"{{
    "schema_version": 1,
    "global_style": {{
        "style_reference": "illustrated notebook concept art only",
        "source_assets": {{
            "portrait_source": ["1024x1024 transparent-background PNG source"],
            "marker_source": ["1024x1024 flat #ff00ff chroma-key PNG source"],
            "runtime_derivatives": ["downsample transparent portrait and marker PNGs from approved source art"],
            "sheet_policy": ["pack approved runtime assets into a shared atlas only after review"]
        }},
        "medium": ["ink-and-wash", "paper-native"],
        "setting": ["County Roscommon", "Ireland", "1820"],
        "palette": ["sepia ink", "muted watercolor"],
        "common_constraints": ["no modern clothing", "no text labels"]
    }},
    "fallback": {{
        "portrait_identity": {{
            "apparent_age": "ambiguous adult",
            "face_and_hair": "plain face, hair hidden under cap or shawl",
            "clothing": "plain homespun outer layer",
            "pose_expression": "neutral parish-neighbour expression",
            "props": ["none"],
            "palette_notes": ["muted browns"]
        }},
        "marker_identity": {{
            "silhouette": "ordinary villager",
            "pose": "standing",
            "readable_props": ["cap or shawl"],
            "tiny_readability_notes": ["do not imply a named NPC"]
        }},
        "avoid": ["distinctive props"],
        "authoring_notes": ["fallback only"]
    }},
    "npcs": [{}]
}}"#,
        npcs
    )
}

fn minimal_npc_art_direction(id: u32) -> String {
    format!(
        r#"{{
        "npc_id": {},
        "portrait_identity": {{
            "apparent_age": "middle-aged",
            "face_and_hair": "weathered face, dark hair under a kerchief",
            "clothing": "plain wool coat and linen shirt",
            "pose_expression": "steady, direct look",
            "props": ["work tool"],
            "palette_notes": ["earth browns"]
        }},
        "marker_identity": {{
            "silhouette": "compact working villager",
            "pose": "standing with work tool",
            "readable_props": ["work tool"],
            "tiny_readability_notes": ["large readable prop"]
        }},
        "avoid": ["modern clothing"],
        "authoring_notes": ["test fixture"]
    }}"#,
        id
    )
}

fn write_test_file(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("write fixture");
    path
}

#[test]
fn art_inputs_export_writes_one_input_per_npc() {
    let tmp = tempfile::tempdir().unwrap();
    let npcs = write_test_file(
        tmp.path(),
        "npcs.json",
        r#"{"npcs":[
            {"id":1,"name":"Bridget","brief_description":"a farmer with muddy boots","age":40,"occupation":"Farmer","personality":"A practical farmer.","home":10,"workplace":10,"mood":"busy","relationships":[],"knowledge":[]}
        ]}"#,
    );
    let world = write_test_file(
        tmp.path(),
        "world.json",
        r#"{"locations":[{"id":10,"name":"Murphy's Farm","description_template":"A working farm."}]}"#,
    );
    let art = write_test_file(
        tmp.path(),
        "art.json",
        &minimal_art_direction(&minimal_npc_art_direction(1)),
    );
    let out = tmp.path().join("out.json");

    let count = export_art_inputs(&npcs, &world, &art, &out).expect("export art inputs");
    assert_eq!(count, 1);

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap();
    assert_eq!(value["npcs"].as_array().unwrap().len(), 1);
    assert_eq!(value["npcs"][0]["name"], "Bridget");
    assert!(
        value["npcs"][0]["portrait_prompt"]
            .as_str()
            .unwrap()
            .contains("Murphy's Farm"),
        "portrait prompt should include merged world context"
    );
    assert!(
        value["npcs"][0]["portrait_prompt"]
            .as_str()
            .unwrap()
            .contains("transparent-background PNG source"),
        "portrait prompt should include source canvas constraints"
    );
}

#[test]
fn art_inputs_export_requires_art_direction_for_every_npc() {
    let tmp = tempfile::tempdir().unwrap();
    let npcs = write_test_file(
        tmp.path(),
        "npcs.json",
        r#"{"npcs":[
            {"id":1,"name":"Bridget","age":40,"occupation":"Farmer","personality":"A practical farmer.","home":10,"mood":"busy","relationships":[],"knowledge":[]},
            {"id":2,"name":"Cormac","age":50,"occupation":"Miller","personality":"A calculating miller.","home":10,"mood":"guarded","relationships":[],"knowledge":[]}
        ]}"#,
    );
    let world = write_test_file(
        tmp.path(),
        "world.json",
        r#"{"locations":[{"id":10,"name":"The Mill","description_template":"A sturdy mill."}]}"#,
    );
    let art = write_test_file(
        tmp.path(),
        "art.json",
        &minimal_art_direction(&minimal_npc_art_direction(1)),
    );

    let err = export_art_inputs(&npcs, &world, &art, &tmp.path().join("out.json"))
        .expect_err("missing NPC art direction must fail");
    assert!(
        err.to_string()
            .contains("missing art direction for NPC id(s): 2"),
        "{err}"
    );
}

#[test]
fn art_inputs_export_rejects_unknown_art_direction_id() {
    let tmp = tempfile::tempdir().unwrap();
    let npcs = write_test_file(
        tmp.path(),
        "npcs.json",
        r#"{"npcs":[
            {"id":1,"name":"Bridget","age":40,"occupation":"Farmer","personality":"A practical farmer.","home":10,"mood":"busy","relationships":[],"knowledge":[]}
        ]}"#,
    );
    let world = write_test_file(
        tmp.path(),
        "world.json",
        r#"{"locations":[{"id":10,"name":"Murphy's Farm","description_template":"A working farm."}]}"#,
    );
    let art = write_test_file(
        tmp.path(),
        "art.json",
        &minimal_art_direction(&minimal_npc_art_direction(99)),
    );

    let err = export_art_inputs(&npcs, &world, &art, &tmp.path().join("out.json"))
        .expect_err("unknown NPC art direction must fail");
    assert!(
        err.to_string()
            .contains("art direction references unknown NPC id 99"),
        "{err}"
    );
}

// ── weighted_occupation (TD-019) ─────────────────────────────────────────

#[test]
fn weighted_occupation_only_returns_known_occupations() {
    // The weights sum to 100 so the `"Other"` fallback at the end of the
    // loop is unreachable in practice; this RNG-driven sweep locks the
    // distribution down — every draw must belong to OCCUPATIONS.
    use std::collections::HashSet;
    let known: HashSet<&str> = OCCUPATIONS.iter().map(|(occ, _)| *occ).collect();
    let mut rng = StdRng::seed_from_u64(0xC0FFEE);
    let mut seen: HashSet<&str> = HashSet::new();
    for _ in 0..10_000 {
        let occ = weighted_occupation(&mut rng);
        assert!(
            known.contains(occ),
            "weighted_occupation returned an occupation outside the table: {occ}"
        );
        seen.insert(occ);
    }
    // Over 10k draws the high-weight occupations should all appear, proving
    // the helper actually samples the table rather than always returning one.
    assert!(
        seen.contains("Tenant Farmer") && seen.contains("Laborer"),
        "common occupations should be observed across 10k draws: {seen:?}"
    );
}

// ── escape_like (TD-020) ─────────────────────────────────────────────────

#[test]
fn escape_like_escapes_backslash_percent_and_underscore() {
    // Pure string-only coverage with no DB round-trip. The backslash branch
    // (`\` -> `\\`) was previously only exercised indirectly via SQLite.
    assert_eq!(escape_like("\\"), "\\\\");
    assert_eq!(escape_like("%"), "\\%");
    assert_eq!(escape_like("_"), "\\_");
    // Backslash must be escaped first so the escapes of % and _ are not
    // themselves re-escaped: `a\%_b` -> `a\\\%\_b`.
    assert_eq!(escape_like("a\\%_b"), "a\\\\\\%\\_b");
    // Plain text is left untouched.
    assert_eq!(escape_like("Bridget"), "Bridget");
}

// ── DataTier round-trip (TD-021) ─────────────────────────────────────────

#[test]
fn data_tier_as_i64_from_i64_round_trip() {
    for (tier, label) in [
        (DataTier::Sketched, "Sketched"),
        (DataTier::Elaborated, "Elaborated"),
        (DataTier::Authored, "Authored"),
    ] {
        assert_eq!(DataTier::from_i64(tier.as_i64()), label);
    }
    // Explicit numeric contract.
    assert_eq!(DataTier::Sketched.as_i64(), 0);
    assert_eq!(DataTier::Elaborated.as_i64(), 1);
    assert_eq!(DataTier::Authored.as_i64(), 2);
}

#[test]
fn data_tier_from_i64_out_of_range_is_unknown() {
    // The "Unknown" fallback defends against schema drift / corrupt rows.
    assert_eq!(DataTier::from_i64(-1), "Unknown");
    assert_eq!(DataTier::from_i64(3), "Unknown");
    assert_eq!(DataTier::from_i64(i64::MAX), "Unknown");
}

// ── import JSON-error path (TD-025) ──────────────────────────────────────

#[test]
fn parse_import_blob_rejects_malformed_json() {
    use std::io::Cursor;
    let err = parse_import_blob(Cursor::new(b"{ this is not json".to_vec()))
        .expect_err("malformed JSON must error");
    assert!(
        err.to_string().contains("invalid JSON input"),
        "JSON-validation context must bubble: {err}"
    );
}

#[test]
fn parse_import_blob_accepts_valid_blob() {
    use std::io::Cursor;
    let json = br#"{"npcs":[{"id":1,"name":"Bridget","age":40,"parish":"kilteevan","occupation":"Servant","data_tier":0,"mood":null,"personality":null}]}"#;
    let blob = parse_import_blob(Cursor::new(json.to_vec())).expect("valid blob parses");
    assert_eq!(blob.npcs.len(), 1);
    assert_eq!(blob.npcs[0].name, "Bridget");
    // The `sex` field defaults when absent (legacy blob compatibility).
    assert_eq!(blob.npcs[0].sex, "unknown");
}

#[test]
fn test_generate_world_rejects_empty_counties() {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    let result = generate_world(&conn, &[]);
    assert!(
        result.is_err(),
        "generate_world with empty counties must fail"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("--counties is required"),
        "error must mention --counties"
    );
}

/// TD-023: `generate_parish` against a county-less DB must return a clear
/// `Err` (no panic, no silently auto-created `'roscommon'` county). A
/// read-only or constrained DB previously hit `.expect()` and aborted the
/// whole process; now it surfaces a recoverable error pointing at
/// `generate-world`.
#[test]
fn test_generate_parish_without_county_errors_not_panics() {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    // No generate_world call → counties table is empty.
    let result = generate_parish(&conn, "Kiltoom", 10, Some(1));
    assert!(
        result.is_err(),
        "generate_parish with no county must return Err, not panic or auto-create one"
    );
    assert!(
        result.unwrap_err().to_string().contains("generate-world"),
        "error must direct the user to run generate-world first"
    );
    // And no county was silently invented on the user's behalf.
    let county_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM counties", [], |r| r.get(0))
        .expect("count query should succeed");
    assert_eq!(
        county_count, 0,
        "generate_parish must not auto-create a county (TD-023)"
    );
}

#[test]
fn test_schema_bootstrap_and_generation() {
    let conn = generated_conn("Kiltoom", 30, 1);

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM npcs", [], |r| r.get(0))
        .expect("count query should succeed");
    assert!(count > 0);
}

#[test]
fn test_promote_sets_personality() {
    let conn = generated_conn("Kiltoom", 20, 2);

    let npc_id: i64 = conn
        .query_row("SELECT id FROM npcs ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("must have one NPC");
    promote_npc(&conn, npc_id).expect("promotion should succeed");

    let (tier, personality): (i64, Option<String>) = conn
        .query_row(
            "SELECT data_tier, personality FROM npcs WHERE id = ?",
            params![npc_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("should read promoted NPC");
    assert_eq!(tier, 1);
    assert!(personality.is_some());
}

#[test]
fn test_promote_rejects_missing_target() {
    let conn = generated_conn("Kiltoom", 20, 2);

    let result = promote_npc(&conn, 9_999_999);
    assert!(result.is_err(), "promoting a missing NPC should fail");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("NPC 9999999 not found"),
        "error should name the missing NPC"
    );
}

#[test]
fn test_validate_detects_missing_personality() {
    let conn = generated_conn("Kiltoom", 20, 3);

    conn.execute(
        "UPDATE npcs SET data_tier = 1, personality = '' WHERE id = (SELECT id FROM npcs LIMIT 1)",
        [],
    )
    .expect("update should succeed");

    assert_validation_failed(validate_db(&conn, None, true));
}

#[test]
fn test_validate_detects_missing_household() {
    let conn = generated_conn("Kiltoom", 20, 4);

    conn.execute(
        "UPDATE npcs SET household_id = NULL WHERE id = (SELECT id FROM npcs LIMIT 1)",
        [],
    )
    .expect("update should succeed");

    assert_validation_failed(validate_db(&conn, None, true));
}

#[test]
fn test_validate_detects_invalid_age() {
    let conn = generated_conn("Kiltoom", 20, 5);

    conn.execute(
        "UPDATE npcs SET age = 111 WHERE id = (SELECT id FROM npcs LIMIT 1)",
        [],
    )
    .expect("update should succeed");

    assert_validation_failed(validate_db(&conn, None, true));
}

#[test]
fn test_validate_detects_broken_relationship() {
    let conn = generated_conn("Kiltoom", 20, 6);
    let npc_id: i64 = conn
        .query_row("SELECT id FROM npcs ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("must have an NPC");

    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("foreign keys should be configurable");
    conn.execute(
            "INSERT INTO npc_relationships(from_npc_id, to_npc_id, kind, strength) VALUES (?, 9999999, 'Acquaintance', 0.5)",
            params![npc_id],
        )
        .expect("insert broken relationship");
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .expect("foreign keys should be configurable");

    assert_validation_failed(validate_db(&conn, None, true));
}

#[test]
fn test_validate_rejects_parish_and_all_together() {
    let conn = generated_conn("Kiltoom", 20, 7);

    let result = validate_db(&conn, Some("Kiltoom".to_string()), true);
    assert!(result.is_err(), "validate should reject ambiguous scope");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("choose either --parish or --all"),
        "error should explain the mutually exclusive scope flags"
    );
}

#[test]
fn test_generate_parish_same_seed_is_deterministic() {
    fn npc_signature(conn: &Connection) -> Vec<(String, String, i64, i64, String, String)> {
        let mut stmt = conn
            .prepare(
                "
                    SELECT n.name, n.sex, n.birth_year, n.age, h.name, n.occupation
                    FROM npcs n
                    JOIN households h ON h.id = n.household_id
                    ORDER BY n.id
                    ",
            )
            .expect("prepare NPC signature query");
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .expect("query NPC signature")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect NPC signature")
    }

    fn relationship_signature(conn: &Connection) -> Vec<(i64, i64, String, String)> {
        let mut stmt = conn
            .prepare(
                "
                    SELECT from_npc_id, to_npc_id, kind, printf('%.6f', strength)
                    FROM npc_relationships
                    ORDER BY from_npc_id, to_npc_id, kind
                    ",
            )
            .expect("prepare relationship signature query");
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .expect("query relationship signature")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect relationship signature")
    }

    let left = generated_conn("Kiltoom", 30, 8);
    let right = generated_conn("Kiltoom", 30, 8);

    assert_eq!(npc_signature(&left), npc_signature(&right));
    assert_eq!(
        relationship_signature(&left),
        relationship_signature(&right)
    );
}

// ── #436 import preserves non-export columns + sex round-trips ──────────

/// Seeds one NPC with a known sex and household_id, then simulates
/// the import path on a blob that represents re-importing that NPC
/// with updated personality. household_id must survive untouched and
/// sex must come from the blob (not hard-coded 'unknown').
#[test]
fn test_import_preserves_household_and_restores_sex() {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    generate_world(&conn, &["roscommon".to_string()]).expect("world generation should work");

    let parish_id: i64 = conn
            .query_row("SELECT id FROM parishes LIMIT 1", [], |r| r.get(0))
            .ok()
            .unwrap_or_else(|| {
                conn.execute(
                    "INSERT INTO parishes(county_id, name) VALUES ((SELECT id FROM counties LIMIT 1), 'Testshire')",
                    [],
                )
                .unwrap();
                conn.last_insert_rowid()
            });

    // Insert a household so we have a non-NULL household_id to preserve.
    conn.execute(
        "INSERT INTO households(parish_id, name) VALUES (?, 'Darcy')",
        params![parish_id],
    )
    .unwrap();
    let household_id = conn.last_insert_rowid();

    conn.execute(
            "INSERT INTO npcs(id, name, sex, birth_year, age, parish_id, household_id, occupation, data_tier, mood)\n             VALUES (42, 'Pádraig Darcy', 'male', 1762, 58, ?, ?, 'Publican', 1, 'content')",
            params![parish_id, household_id],
        )
        .unwrap();

    // Build an import blob that updates personality but carries the
    // NPC's existing id. `sex` is present (no longer hard-coded).
    // parish is reused so we don't hit unrelated lookup paths.
    let parish_name: String = conn
        .query_row(
            "SELECT name FROM parishes WHERE id = ?",
            params![parish_id],
            |r| r.get(0),
        )
        .unwrap();
    let blob = ExportBlob {
        npcs: vec![ExportNpc {
            id: 42,
            name: "Pádraig Darcy".to_string(),
            sex: "male".to_string(),
            age: 58,
            parish: parish_name,
            occupation: "Publican".to_string(),
            data_tier: 1,
            mood: Some("content".to_string()),
            personality: Some("Warm-hearted publican.".to_string()),
        }],
    };

    // Use the shared import helper instead of duplicating the UPSERT SQL.
    import_npcs_inner(&conn, blob.npcs).unwrap();

    // household_id must still be set (would be NULL if INSERT OR
    // REPLACE were used — that was the #436 regression).
    let (hh, sex, personality): (Option<i64>, String, Option<String>) = conn
        .query_row(
            "SELECT household_id, sex, personality FROM npcs WHERE id = 42",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(hh, Some(household_id), "household_id must survive import");
    assert_eq!(sex, "male", "sex must come from the blob, not 'unknown'");
    assert_eq!(
        personality.as_deref(),
        Some("Warm-hearted publican."),
        "personality must update on import",
    );
}

/// A blob serialized *before* #436 (no `sex` field) must still
/// deserialize cleanly, defaulting to "unknown" — so we don't
/// break users with saved export files from earlier versions.
#[test]
fn test_export_blob_deserializes_legacy_missing_sex() {
    let legacy = r#"{
            "npcs": [{
                "id": 1,
                "name": "Legacy Mary",
                "age": 40,
                "parish": "Kiltoom",
                "occupation": "Servant",
                "data_tier": 0,
                "mood": null,
                "personality": null
            }]
        }"#;
    let blob: ExportBlob = serde_json::from_str(legacy).expect("legacy blob should parse");
    assert_eq!(blob.npcs.len(), 1);
    assert_eq!(blob.npcs[0].sex, "unknown");
}

// ── #435 family_tree gracefully handles NULL household_id ───────────────

/// An NPC with NULL household_id must not blow up family_tree
/// with a misleading "NPC not found" error. The NPC exists — we
/// just have no household to walk.
#[test]
fn test_family_tree_handles_null_household() {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    generate_world(&conn, &["roscommon".to_string()]).expect("world generation should work");

    // Insert an NPC directly with NULL household_id — import or
    // manual editing can produce this in the wild.
    let parish_id: i64 = conn
            .query_row("SELECT id FROM parishes LIMIT 1", [], |r| r.get(0))
            .ok()
            .unwrap_or_else(|| {
                conn.execute(
                    "INSERT INTO parishes(county_id, name) VALUES ((SELECT id FROM counties LIMIT 1), 'Testshire')",
                    [],
                )
                .unwrap();
                conn.last_insert_rowid()
            });
    conn.execute(
            "INSERT INTO npcs(name, sex, birth_year, age, parish_id, household_id, occupation, data_tier, mood)\n             VALUES ('Orphan', 'female', 1790, 30, ?, NULL, 'Other', 0, 'neutral')",
            params![parish_id],
        )
        .expect("insert orphan NPC");
    let orphan_id = conn.last_insert_rowid();

    // The call must succeed (return Ok) rather than surfacing a
    // confusing "NPC not found" error.
    let result = family_tree(&conn, orphan_id);
    assert!(
        result.is_ok(),
        "family_tree on NULL-household NPC should succeed, got: {:?}",
        result.err()
    );

    // And a non-existent NPC id still reports "NPC not found"
    // (regression-guard: we didn't break that path).
    let missing = family_tree(&conn, 9_999_999);
    assert!(missing.is_err());
    assert!(
        missing.unwrap_err().to_string().contains("NPC not found"),
        "missing NPC should still surface 'NPC not found'"
    );
}

// ── #607 search_npcs escapes LIKE wildcard metacharacters ──────────────

fn setup_search_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");

    let county_id: i64 = conn
        .query_row(
            "INSERT INTO counties(name) VALUES ('Roscommon') RETURNING id",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| {
            conn.execute("INSERT INTO counties(name) VALUES ('Roscommon')", [])
                .unwrap();
            conn.last_insert_rowid()
        });

    conn.execute(
        "INSERT INTO parishes(county_id, name) VALUES (?, 'Kiltoom')",
        params![county_id],
    )
    .unwrap();
    let parish_id = conn.last_insert_rowid();

    for (name, occ) in &[
        ("100%_off", "Merchant"),
        ("Alice the Smith", "Blacksmith"),
        ("O_Brien", "Farmer"),
    ] {
        conn.execute(
            "INSERT INTO npcs(name, sex, birth_year, age, parish_id, occupation, data_tier, mood) \
                 VALUES (?, 'unknown', 1800, 20, ?, ?, 0, 'neutral')",
            params![name, parish_id, occ],
        )
        .unwrap();
    }

    conn
}

fn search_names(conn: &Connection, query: &str) -> Vec<String> {
    let like = format!("%{}%", escape_like(query));
    let mut stmt = conn
        .prepare("SELECT n.name FROM npcs n WHERE n.name LIKE ? ESCAPE '\\' ORDER BY n.name")
        .unwrap();
    stmt.query_map(params![like], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

#[test]
fn test_search_wildcard_chars_match_literal_only() {
    let conn = setup_search_db();
    let results = search_names(&conn, "100%_off");
    assert_eq!(results, vec!["100%_off".to_string()]);
}

#[test]
fn test_search_normal_query_still_matches() {
    let conn = setup_search_db();
    let results = search_names(&conn, "alice");
    assert_eq!(results, vec!["Alice the Smith".to_string()]);
}

// ── #592 SQL injection guard ─────────────────────────────────────────────

/// A parish name containing SQL meta-characters must not cause query
/// errors or allow arbitrary SQL execution.  The parameterized query
/// should treat the entire string as a literal value; because no parish
/// with that name exists, all counts come back as zero and validation
/// passes (empty DB for the injected "parish").
#[test]
fn validate_db_parish_filter_rejects_sqli_payload() {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");

    // SQL injection payloads: these would break string-interpolated queries.
    let payloads = [
        "Kiltoom'); DROP TABLE npcs; --",
        "' OR '1'='1",
        "x' UNION SELECT 0,0,0,0,0 --",
        "Kil'toom",
    ];

    for payload in payloads {
        // Must not return a rusqlite error — the value is bound, not
        // interpolated, so the query is syntactically valid regardless.
        let result = validate_db(&conn, Some(payload.to_string()), false);
        assert!(
            result.is_ok(),
            "validate_db should not error on payload {payload:?}, got: {:?}",
            result.err()
        );
    }

    // Sanity check: the npcs table still exists (no DROP TABLE succeeded).
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM npcs", [], |r| r.get(0))
        .expect("npcs table must still exist after injection attempts");
    assert_eq!(count, 0, "no NPCs were inserted so count must be 0");
}

// ── TD-002 list_npcs tests ────────────────────────────────────────────────

fn setup_list_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    generate_world(&conn, &["roscommon".to_string()]).expect("world gen");
    generate_parish(&conn, "Kiltoom", 20, Some(1)).expect("parish gen");
    conn
}

#[test]
fn test_list_npcs_unfiltered() {
    let conn = setup_list_db();
    let result = list_npcs(&conn, None, None, None, 100);
    assert!(result.is_ok(), "unfiltered list should succeed");
}

#[test]
fn test_list_npcs_parish_filter() {
    let conn = setup_list_db();
    let result = list_npcs(&conn, Some("Kiltoom"), None, None, 100);
    assert!(result.is_ok(), "parish-filtered list should succeed");
}

#[test]
fn test_list_npcs_occupation_filter() {
    let conn = setup_list_db();
    let result = list_npcs(&conn, None, Some("Laborer"), None, 100);
    assert!(result.is_ok(), "occupation-filtered list should succeed");
}

#[test]
fn test_list_npcs_tier_filter() {
    let conn = setup_list_db();
    let result = list_npcs(&conn, None, None, Some(DataTier::Sketched), 100);
    assert!(result.is_ok(), "tier-filtered list should succeed");
}

#[test]
fn test_list_npcs_all_filters() {
    let conn = setup_list_db();
    let result = list_npcs(
        &conn,
        Some("Kiltoom"),
        Some("Tenant Farmer"),
        Some(DataTier::Sketched),
        50,
    );
    assert!(result.is_ok(), "all-filters list should succeed");
}

#[test]
fn test_list_npcs_empty_result() {
    let conn = setup_list_db();
    let result = list_npcs(&conn, Some("Nonexistent"), None, None, 10);
    assert!(result.is_ok(), "list with no matches should succeed");
}

// ── TD-003 show_npc tests ────────────────────────────────────────────────

#[test]
fn test_show_npc_found() {
    let conn = setup_list_db();
    let npc_id: i64 = conn
        .query_row("SELECT id FROM npcs ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("must have an NPC");
    let result = show_npc(&conn, npc_id);
    assert!(result.is_ok(), "show_npc for existing NPC should succeed");
}

#[test]
fn test_show_npc_not_found() {
    let conn = setup_list_db();
    let result = show_npc(&conn, 9_999_999);
    assert!(result.is_err(), "show_npc for missing NPC should fail");
    assert!(
        result.unwrap_err().to_string().contains("not found"),
        "error must mention 'not found'"
    );
}

// ── TD-004 edit_npc tests ────────────────────────────────────────────────

fn setup_edit_db() -> (Connection, i64) {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    generate_world(&conn, &["roscommon".to_string()]).expect("world gen");
    generate_parish(&conn, "Kiltoom", 10, Some(2)).expect("parish gen");
    let npc_id: i64 = conn
        .query_row("SELECT id FROM npcs ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("must have an NPC");
    (conn, npc_id)
}

#[test]
fn test_edit_npc_mood_only() {
    let (conn, npc_id) = setup_edit_db();
    let result = edit_npc(&conn, npc_id, Some("happy".to_string()), None);
    assert!(result.is_ok(), "edit mood should succeed");
    let mood: Option<String> = conn
        .query_row("SELECT mood FROM npcs WHERE id = ?", params![npc_id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(mood.as_deref(), Some("happy"));
}

#[test]
fn test_edit_npc_occupation_only() {
    let (conn, npc_id) = setup_edit_db();
    let result = edit_npc(&conn, npc_id, None, Some("Publican".to_string()));
    assert!(result.is_ok(), "edit occupation should succeed");
    let occ: String = conn
        .query_row(
            "SELECT occupation FROM npcs WHERE id = ?",
            params![npc_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(occ, "Publican");
}

#[test]
fn test_edit_npc_both() {
    let (conn, npc_id) = setup_edit_db();
    let result = edit_npc(
        &conn,
        npc_id,
        Some("sad".to_string()),
        Some("Laborer".to_string()),
    );
    assert!(result.is_ok(), "edit both should succeed");
    let (mood, occ): (Option<String>, String) = conn
        .query_row(
            "SELECT mood, occupation FROM npcs WHERE id = ?",
            params![npc_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(mood.as_deref(), Some("sad"));
    assert_eq!(occ, "Laborer");
}

#[test]
fn test_edit_npc_no_changes() {
    let (conn, npc_id) = setup_edit_db();
    let result = edit_npc(&conn, npc_id, None, None);
    assert!(result.is_err(), "edit with no changes should fail");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("provide at least one change"),
        "error must mention providing at least one change"
    );
}

// ── TD-005 elaborate_parish tests ────────────────────────────────────────

#[test]
fn test_elaborate_parish_basic() {
    let conn = setup_list_db();
    let result = elaborate_parish(&conn, "Kiltoom", 5);
    assert!(result.is_ok(), "elaborate_parish should succeed");
    let elaborated: i64 = conn
        .query_row("SELECT COUNT(*) FROM npcs WHERE data_tier >= 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(elaborated, 5, "should have elaborated 5 NPCs");
}

#[test]
fn test_elaborate_parish_empty_result() {
    let conn = setup_list_db();
    let result = elaborate_parish(&conn, "Nonexistent", 10);
    assert!(
        result.is_ok(),
        "elaborate_parish with no matches should succeed"
    );
}

#[test]
fn test_elaborate_parish_limit_zero() {
    let conn = setup_list_db();
    let result = elaborate_parish(&conn, "Kiltoom", 0);
    assert!(
        result.is_ok(),
        "elaborate_parish with limit 0 should succeed"
    );
}

// ── TD-008 relationships tests ───────────────────────────────────────────

#[test]
fn test_relationships_npc_found() {
    let conn = setup_list_db();
    let npc_id: i64 = conn
        .query_row("SELECT id FROM npcs ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("must have an NPC");
    let result = relationships(&conn, npc_id);
    assert!(
        result.is_ok(),
        "relationships for existing NPC should succeed"
    );
}

#[test]
fn test_relationships_npc_not_found() {
    let conn = setup_list_db();
    let result = relationships(&conn, 9_999_999);
    assert!(result.is_err(), "relationships for missing NPC should fail");
    assert!(
        result.unwrap_err().to_string().contains("NPC not found"),
        "error must mention 'NPC not found'"
    );
}

// ── TD-006 stats tests ───────────────────────────────────────────────────

#[test]
fn test_stats_with_data() {
    let conn = setup_list_db();
    let result = stats(&conn);
    assert!(result.is_ok(), "stats with data should succeed");
}

#[test]
fn test_stats_empty_db() {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    let result = stats(&conn);
    assert!(result.is_ok(), "stats on empty DB should succeed");
}

// ── TD-007 export_npcs tests ─────────────────────────────────────────────

#[test]
fn test_export_npcs_unfiltered() {
    let conn = setup_list_db();
    let result = export_npcs(&conn, None);
    assert!(result.is_ok(), "unfiltered export should succeed");
}

#[test]
fn test_export_npcs_parish_filtered() {
    let conn = setup_list_db();
    let result = export_npcs(&conn, Some("Kiltoom"));
    assert!(result.is_ok(), "parish-filtered export should succeed");
}

#[test]
fn test_export_npcs_empty_result() {
    let conn = setup_list_db();
    let result = export_npcs(&conn, Some("Nonexistent"));
    assert!(result.is_ok(), "export with no matches should succeed");
}

// ── TD-009 search_npcs tests ─────────────────────────────────────────────

#[test]
fn test_search_npcs_matches() {
    let conn = setup_search_db();
    let result = search_npcs(&conn, "alice", 10);
    assert!(result.is_ok(), "search for existing name should succeed");
}

#[test]
fn test_search_npcs_no_matches() {
    let conn = setup_search_db();
    let result = search_npcs(&conn, "zzzznotfound", 10);
    assert!(result.is_ok(), "search for missing name should succeed");
}

#[test]
fn test_search_npcs_limit_zero() {
    let conn = setup_search_db();
    let result = search_npcs(&conn, "alice", 0);
    assert!(result.is_ok(), "search with limit 0 should succeed");
}

/// A legitimate parish name must still filter correctly — the fix must not
/// break the happy path.
#[test]
fn validate_db_parish_filter_works_for_valid_name() {
    let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
    ensure_schema(&conn).expect("schema should initialize");
    generate_world(&conn, &["roscommon".to_string()]).expect("world generation should work");
    generate_parish(&conn, "Kiltoom", 5, Some(4)).expect("parish generation should work");

    // All generated NPCs should have a household and valid age, so
    // validate_db with the real parish name should pass.
    let result = validate_db(&conn, Some("Kiltoom".to_string()), false);
    assert!(
        result.is_ok(),
        "validate_db with a real parish name must pass, got: {:?}",
        result.err()
    );
}

// ── resolve_default_db (TD-024 / Rule 9) ────────────────────────────────

/// Holds the previous value of an env var and restores it on drop.
/// Env mutations must be done inside a single-threaded guard because
/// Cargo runs tests in the same process concurrently.
struct EnvGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}
impl EnvGuard {
    fn capture(key: &'static str) -> Self {
        EnvGuard {
            key,
            prev: std::env::var_os(key),
        }
    }
}
impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: gated by DB_RESOLVE_LOCK.
        match self.prev.take() {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

fn db_resolve_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn resolve_default_db_npc_tool_env_wins() {
    let _gate = db_resolve_lock();
    let _g1 = EnvGuard::capture(NPC_TOOL_DB_ENV);
    let _g2 = EnvGuard::capture("PARISH_DATA_DIR");

    // SAFETY: gated.
    unsafe {
        std::env::set_var(NPC_TOOL_DB_ENV, "/tmp/override.db");
        std::env::remove_var("PARISH_DATA_DIR");
    }

    let resolved = resolve_default_db();
    assert_eq!(
        resolved,
        PathBuf::from("/tmp/override.db"),
        "PARISH_NPC_TOOL_DB must be used verbatim"
    );
}

#[test]
fn resolve_default_db_parish_data_dir_second_priority() {
    let _gate = db_resolve_lock();
    let _g1 = EnvGuard::capture(NPC_TOOL_DB_ENV);
    let _g2 = EnvGuard::capture("PARISH_DATA_DIR");

    // SAFETY: gated.
    unsafe {
        std::env::remove_var(NPC_TOOL_DB_ENV);
        std::env::set_var("PARISH_DATA_DIR", "/tmp/mydata");
    }

    let resolved = resolve_default_db();
    assert_eq!(
        resolved,
        PathBuf::from("/tmp/mydata").join(DB_FILENAME),
        "PARISH_DATA_DIR must yield <dir>/parish-world.db"
    );
}

#[test]
fn resolve_default_db_empty_env_var_is_skipped() {
    let _gate = db_resolve_lock();
    let _g1 = EnvGuard::capture(NPC_TOOL_DB_ENV);
    let _g2 = EnvGuard::capture("PARISH_DATA_DIR");

    // SAFETY: gated.
    unsafe {
        std::env::set_var(NPC_TOOL_DB_ENV, "   ");
        std::env::set_var("PARISH_DATA_DIR", "   ");
    }

    // Both trimmed-empty — should fall through to ancestor walk or cwd fallback.
    // The result will be some path ending in `data/parish-world.db`; the key
    // property is that neither blank env var was used.
    let resolved = resolve_default_db();
    assert!(
        resolved.ends_with(PathBuf::from("data").join(DB_FILENAME)),
        "blank env vars must be ignored; got: {}",
        resolved.display()
    );
}

#[test]
fn resolve_default_db_falls_back_to_data_subdir() {
    let _gate = db_resolve_lock();
    let _g1 = EnvGuard::capture(NPC_TOOL_DB_ENV);
    let _g2 = EnvGuard::capture("PARISH_DATA_DIR");

    // SAFETY: gated.
    unsafe {
        std::env::remove_var(NPC_TOOL_DB_ENV);
        std::env::remove_var("PARISH_DATA_DIR");
    }

    // Whether we find Cargo.toml or fall all the way to the cwd fallback,
    // the result must end with data/parish-world.db.
    let resolved = resolve_default_db();
    assert!(
        resolved.ends_with(PathBuf::from("data").join(DB_FILENAME)),
        "fallback must end with data/parish-world.db; got: {}",
        resolved.display()
    );
    assert!(
        resolved.is_absolute(),
        "resolved path must be absolute (anchored); got: {}",
        resolved.display()
    );
}
