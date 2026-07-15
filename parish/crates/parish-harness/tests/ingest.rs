//! End-to-end coverage for the `ingest` path: a skill-run payload written to
//! disk is ingested and then read back through the same query DTOs the
//! dashboard serves, proving an ingested run is indistinguishable from a binary
//! run to `/api/runs` and `/api/runs/{id}`.

use std::fs;
use std::path::{Path, PathBuf};

use parish_harness::ingest::load_and_ingest;
use parish_harness::persist::Db;

/// 1x1 PNG — a real, non-empty frame so the rule #14 content check passes.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60, 0x60, 0x60, 0x00,
    0x00, 0x00, 0x05, 0x00, 0x01, 0x5e, 0xf3, 0x2a, 0x3a, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// Write a frame for turn `idx` under `<artifacts>/runs/<uuid>/turns/NNN/`.
fn write_frame(artifacts: &Path, uuid: &str, idx: u32) {
    let dir = artifacts
        .join("runs")
        .join(uuid)
        .join("turns")
        .join(format!("{idx:03}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("frame.png"), PNG_1X1).unwrap();
    fs::write(dir.join("lines.json"), b"[]").unwrap();
}

fn write_payload(dir: &Path, name: &str, json: &str) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, json).unwrap();
    p
}

fn scored_payload(uuid: &str) -> String {
    format!(
        r#"{{
          "config": {{ "label": "ignored", "player": {{ "mode": "subagent" }}, "judge": {{ "mode": "subagent" }} }},
          "git": {{ "sha": "abc123", "branch": "main", "dirty": true, "pr_number": null }},
          "rubric_sha256": "rubricpin",
          "uuid": "{uuid}",
          "status": "completed",
          "quality_score": 74.0,
          "cost": {{ "cost_usd": 0.42, "player_tokens": 1000, "judge_tokens": 500 }},
          "turns": [
            {{ "turn_index": 0, "player_input": "look", "kind": "look", "frame_path": "turns/000/frame.png", "lines_path": "turns/000/lines.json", "location_name": "Kilteevan Village", "game_clock": "08:01", "npcs_here_count": 1 }}
          ],
          "axes": [
            {{ "axis": "narrative_coherence", "score": 85, "rationale": "a" }},
            {{ "axis": "character_fidelity", "score": 55, "rationale": "b" }},
            {{ "axis": "world_responsiveness", "score": 85, "rationale": "c" }},
            {{ "axis": "intent_fidelity", "score": 82, "rationale": "d" }},
            {{ "axis": "immersion", "score": 63, "rationale": "e" }},
            {{ "axis": "progression", "score": 84, "rationale": "f" }},
            {{ "axis": "common_sense", "score": 70, "rationale": "g" }}
          ],
          "findings": [
            {{ "category": "character_fidelity", "turn_index": 0, "severity": "high", "description": "mood-blind", "evidence_quote": "warm", "signature": "mood-blind" }},
            {{ "category": "immersion", "turn_index": 0, "severity": "high", "description": "verbose", "evidence_quote": "ramble", "signature": "verbosity", "issue_url": "https://github.com/dmooney/Rundale/issues/1387" }}
          ]
        }}"#
    )
}

#[test]
fn ingest_scored_run_is_readable_via_dashboard_dtos() {
    let tmp = tempfile::tempdir().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let uuid = "00000000-0000-0000-0000-0000000000aa";
    write_frame(&artifacts, uuid, 0);
    let payload = write_payload(tmp.path(), "scored.json", &scored_payload(uuid));

    let db = Db::open(&tmp.path().join("harness.db")).unwrap();
    let run_id = load_and_ingest(&db, &payload, &artifacts).unwrap();

    // /api/runs
    let runs = db.list_runs(50).unwrap();
    assert_eq!(runs.len(), 1);
    let r = &runs[0];
    assert_eq!(r.id, run_id);
    assert_eq!(r.status, "completed");
    assert_eq!(r.quality_score, Some(74.0));
    assert_eq!(r.finding_count, 2);
    assert_eq!(r.git_sha, "abc123");
    assert!(r.git_dirty);

    // /api/runs/{id}
    let detail = db.run_detail(run_id).unwrap().unwrap();
    assert_eq!(detail.axes.len(), 7, "all 7 axes present");
    let keys: std::collections::BTreeSet<_> = detail.axes.iter().map(|a| a.axis.clone()).collect();
    for k in [
        "narrative_coherence",
        "character_fidelity",
        "world_responsiveness",
        "intent_fidelity",
        "immersion",
        "progression",
        "common_sense",
    ] {
        assert!(keys.contains(k), "missing axis {k}");
    }
    assert_eq!(detail.findings.len(), 2);
    assert_eq!(detail.turns.len(), 1);
    assert_eq!(
        detail.turns[0].location_name.as_deref(),
        Some("Kilteevan Village")
    );
    // The stored artifact_dir resolves the frame the dashboard serves.
    assert!(
        Path::new(&detail.summary.artifact_dir)
            .join(&detail.turns[0].frame_path)
            .exists()
    );

    // /api/cost reflects the payload's accounting (not the hardcoded 0).
    let cost = db.cost_summary().unwrap();
    assert_eq!(cost.total_runs, 1);
    assert!((cost.total_cost_usd - 0.42).abs() < 1e-9);
    assert_eq!(cost.total_player_tokens, 1000);
    assert_eq!(cost.total_judge_tokens, 500);
}

#[test]
fn ingest_persists_finding_issue_url() {
    let tmp = tempfile::tempdir().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let uuid = "00000000-0000-0000-0000-0000000000ee";
    write_frame(&artifacts, uuid, 0);
    let payload = write_payload(tmp.path(), "linked.json", &scored_payload(uuid));

    let db = Db::open(&tmp.path().join("harness.db")).unwrap();
    let run_id = load_and_ingest(&db, &payload, &artifacts).unwrap();

    // The finding that carried an `issue_url` in the payload round-trips to the
    // DTO with the link set; the one without stays None (C1, C2, C4).
    let detail = db.run_detail(run_id).unwrap().unwrap();
    let linked = detail
        .findings
        .iter()
        .find(|f| f.signature == "verbosity")
        .expect("verbosity finding present");
    assert_eq!(
        linked.issue_url.as_deref(),
        Some("https://github.com/dmooney/Rundale/issues/1387"),
        "ingested issue_url must persist to the finding row"
    );
    let unlinked = detail
        .findings
        .iter()
        .find(|f| f.signature == "mood-blind")
        .expect("mood-blind finding present");
    assert_eq!(
        unlinked.issue_url, None,
        "a finding with no issue_url stays unlinked"
    );
}

#[test]
fn ingest_turn_llm_log_drives_has_transcript() {
    let tmp = tempfile::tempdir().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let uuid = "00000000-0000-0000-0000-0000000000f1";
    write_frame(&artifacts, uuid, 0);
    write_frame(&artifacts, uuid, 1);
    // Turn 0 gets an inference log; turn 1 does not.
    let log_dir = artifacts.join("runs").join(uuid).join("turns").join("000");
    fs::write(
        log_dir.join("llm.json"),
        br#"{"turn_index":0,"player_input":"hi","exchanges":[],"inferences":[]}"#,
    )
    .unwrap();
    let json = format!(
        r#"{{
          "config": {{ "player": {{ "mode": "subagent" }}, "judge": {{ "mode": "subagent" }} }},
          "git": {{ "sha": "abc", "branch": "main", "dirty": false, "pr_number": null }},
          "rubric_sha256": "r", "uuid": "{uuid}", "status": "completed", "quality_score": 70.0,
          "cost": {{ "cost_usd": 0.0, "player_tokens": 0, "judge_tokens": 0 }},
          "turns": [
            {{ "turn_index": 0, "player_input": "hi", "frame_path": "turns/000/frame.png", "lines_path": "turns/000/lines.json", "llm_transcript_path": "turns/000/llm.json" }},
            {{ "turn_index": 1, "player_input": "bye", "kind": "look", "frame_path": "turns/001/frame.png", "lines_path": "turns/001/lines.json" }}
          ],
          "axes": [], "findings": []
        }}"#
    );
    let payload = write_payload(tmp.path(), "logged.json", &json);
    let db = Db::open(&tmp.path().join("harness.db")).unwrap();
    let run_id = load_and_ingest(&db, &payload, &artifacts).unwrap();

    let detail = db.run_detail(run_id).unwrap().unwrap();
    let t0 = detail.turns.iter().find(|t| t.turn_index == 0).unwrap();
    let t1 = detail.turns.iter().find(|t| t.turn_index == 1).unwrap();
    assert!(t0.has_transcript, "turn 0 has an llm log");
    assert!(!t1.has_transcript, "turn 1 has no llm log");
}

#[test]
fn ingest_rejects_dangling_llm_log_path() {
    let tmp = tempfile::tempdir().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let uuid = "00000000-0000-0000-0000-0000000000f2";
    write_frame(&artifacts, uuid, 0);
    // Reference an llm log that was never written.
    let json = format!(
        r#"{{
          "config": {{ "player": {{ "mode": "subagent" }}, "judge": {{ "mode": "subagent" }} }},
          "git": {{ "sha": "abc", "branch": "main", "dirty": false, "pr_number": null }},
          "rubric_sha256": "r", "uuid": "{uuid}", "status": "completed", "quality_score": 70.0,
          "cost": {{ "cost_usd": 0.0, "player_tokens": 0, "judge_tokens": 0 }},
          "turns": [
            {{ "turn_index": 0, "player_input": "hi", "frame_path": "turns/000/frame.png", "lines_path": "turns/000/lines.json", "llm_transcript_path": "turns/000/llm.json" }}
          ],
          "axes": [], "findings": []
        }}"#
    );
    let payload = write_payload(tmp.path(), "dangling.json", &json);
    let db = Db::open(&tmp.path().join("harness.db")).unwrap();
    let err = load_and_ingest(&db, &payload, &artifacts).unwrap_err();
    assert!(
        format!("{err}").contains("llm_transcript_path"),
        "missing llm log must error, got: {err}"
    );
    assert_eq!(db.list_runs(50).unwrap().len(), 0, "nothing persisted");
}

#[test]
fn ingest_gated_run_has_null_quality_and_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let uuid = "00000000-0000-0000-0000-0000000000bb";
    write_frame(&artifacts, uuid, 0);
    let json = format!(
        r#"{{
          "config": {{ "player": {{ "mode": "subagent" }}, "judge": {{ "mode": "subagent" }} }},
          "git": {{ "sha": "def456", "branch": "main", "dirty": false, "pr_number": null }},
          "rubric_sha256": "rubricpin",
          "uuid": "{uuid}",
          "status": "gated",
          "gate": {{ "reason": "crash", "turn": 3, "detail": "transport error mid-run" }},
          "cost": {{ "cost_usd": 0.0, "player_tokens": 0, "judge_tokens": 0 }},
          "turns": [
            {{ "turn_index": 0, "player_input": "look", "kind": "look", "frame_path": "turns/000/frame.png", "lines_path": "turns/000/lines.json" }}
          ],
          "axes": [],
          "findings": []
        }}"#
    );
    let payload = write_payload(tmp.path(), "gated.json", &json);

    let db = Db::open(&tmp.path().join("harness.db")).unwrap();
    let run_id = load_and_ingest(&db, &payload, &artifacts).unwrap();

    let detail = db.run_detail(run_id).unwrap().unwrap();
    assert_eq!(detail.summary.status, "gated");
    assert_eq!(detail.summary.quality_score, None);
    assert_eq!(detail.summary.gate_reason.as_deref(), Some("crash"));
    assert_eq!(detail.summary.gate_turn, Some(3));
    assert!(detail.axes.is_empty());
}

#[test]
fn two_skill_runs_share_one_config_row() {
    let tmp = tempfile::tempdir().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let db = Db::open(&tmp.path().join("harness.db")).unwrap();

    for uuid in [
        "00000000-0000-0000-0000-0000000000c1",
        "00000000-0000-0000-0000-0000000000c2",
    ] {
        write_frame(&artifacts, uuid, 0);
        let payload = write_payload(tmp.path(), &format!("{uuid}.json"), &scored_payload(uuid));
        load_and_ingest(&db, &payload, &artifacts).unwrap();
    }

    let runs = db.list_runs(50).unwrap();
    assert_eq!(runs.len(), 2, "two distinct runs");
    let config_count: i64 = db.config_count().expect("config count helper");
    assert_eq!(
        config_count, 1,
        "label forced to skill:quality-harness dedups configs"
    );
}

#[test]
fn ingest_rejects_missing_frame() {
    let tmp = tempfile::tempdir().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let uuid = "00000000-0000-0000-0000-0000000000dd";
    // Deliberately do NOT write the frame.
    let payload = write_payload(tmp.path(), "noframe.json", &scored_payload(uuid));
    let db = Db::open(&tmp.path().join("harness.db")).unwrap();
    let err = load_and_ingest(&db, &payload, &artifacts).unwrap_err();
    assert!(
        format!("{err}").contains("frame"),
        "missing frame must error, got: {err}"
    );
    // And nothing was persisted.
    assert_eq!(db.list_runs(50).unwrap().len(), 0);
}
