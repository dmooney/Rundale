//! Dashboard read queries — surfaces run/turn/axis/finding data for the
//! dashboard API without coupling the sink module to serde.

use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::error::Result;
use crate::persist::Db;

/// A compact run summary returned by the `/api/runs` list endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummaryDto {
    pub id: i64,
    pub status: String,
    pub turn_count: u32,
    pub gate_reason: Option<String>,
    pub gate_turn: Option<u32>,
    pub quality_score: Option<f64>,
    pub rubric_sha256: String,
    pub git_sha: String,
    pub git_branch: String,
    pub git_dirty: bool,
    pub finding_count: u32,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub artifact_dir: String,
}

/// One axis score row for the detail view.
#[derive(Debug, Clone, Serialize)]
pub struct AxisScoreDto {
    pub axis: String,
    pub score: i64,
    pub rationale: Option<String>,
}

/// One finding row for the detail view.
#[derive(Debug, Clone, Serialize)]
pub struct FindingDto {
    pub id: i64,
    pub turn_index: Option<u32>,
    pub category: String,
    pub severity: String,
    pub signature: String,
    pub description: String,
    pub evidence_json: Option<String>,
    pub issue_url: Option<String>,
    pub issue_dedup_of: Option<i64>,
}

/// A condensed per-turn record for the detail view.
#[derive(Debug, Clone, Serialize)]
pub struct TurnSummaryDto {
    pub turn_index: u32,
    pub player_input: String,
    pub outcome: String,
    pub kind: String,
    pub elapsed_ms: i64,
    pub location_name: Option<String>,
    pub game_clock: Option<String>,
    pub npcs_here_count: Option<u32>,
    pub frame_path: String,
}

/// Full run detail for the `/api/runs/{id}` endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct RunDetail {
    pub summary: RunSummaryDto,
    pub axes: Vec<AxisScoreDto>,
    pub findings: Vec<FindingDto>,
    pub turns: Vec<TurnSummaryDto>,
}

impl Db {
    /// Return the most recent `limit` runs in reverse-chronological order.
    pub fn list_runs(&self, limit: u32) -> Result<Vec<RunSummaryDto>> {
        let conn = &self.conn;
        let mut stmt = conn.prepare(
            "SELECT r.id, r.status, r.turn_count, r.gate_reason, r.gate_turn,
                    r.quality_score, r.rubric_sha256, r.git_sha, r.git_branch,
                    r.git_dirty, r.started_at, r.ended_at, r.artifact_dir,
                    COUNT(f.id) AS finding_count
               FROM runs r
               LEFT JOIN findings f ON f.run_id = r.id
              GROUP BY r.id
              ORDER BY r.id DESC
              LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(RunSummaryDto {
                id: r.get(0)?,
                status: r.get(1)?,
                turn_count: r.get(2)?,
                gate_reason: r.get(3)?,
                gate_turn: r.get(4)?,
                quality_score: r.get(5)?,
                rubric_sha256: r.get(6)?,
                git_sha: r.get(7)?,
                git_branch: r.get(8)?,
                git_dirty: {
                    let v: i64 = r.get(9)?;
                    v != 0
                },
                started_at: r.get(10)?,
                ended_at: r.get(11)?,
                artifact_dir: r.get(12)?,
                finding_count: {
                    let v: i64 = r.get(13)?;
                    v as u32
                },
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Return the full detail for a single run (summary + axes + findings + turns).
    pub fn run_detail(&self, run_id: i64) -> Result<Option<RunDetail>> {
        let conn = &self.conn;

        // Summary row.
        let maybe_summary = conn
            .query_row(
                "SELECT r.id, r.status, r.turn_count, r.gate_reason, r.gate_turn,
                        r.quality_score, r.rubric_sha256, r.git_sha, r.git_branch,
                        r.git_dirty, r.started_at, r.ended_at, r.artifact_dir,
                        COUNT(f.id) AS finding_count
                   FROM runs r
                   LEFT JOIN findings f ON f.run_id = r.id
                  WHERE r.id = ?1
                  GROUP BY r.id",
                params![run_id],
                |r| {
                    Ok(RunSummaryDto {
                        id: r.get(0)?,
                        status: r.get(1)?,
                        turn_count: r.get(2)?,
                        gate_reason: r.get(3)?,
                        gate_turn: r.get(4)?,
                        quality_score: r.get(5)?,
                        rubric_sha256: r.get(6)?,
                        git_sha: r.get(7)?,
                        git_branch: r.get(8)?,
                        git_dirty: {
                            let v: i64 = r.get(9)?;
                            v != 0
                        },
                        started_at: r.get(10)?,
                        ended_at: r.get(11)?,
                        artifact_dir: r.get(12)?,
                        finding_count: {
                            let v: i64 = r.get(13)?;
                            v as u32
                        },
                    })
                },
            )
            .optional()?;

        let summary = match maybe_summary {
            Some(s) => s,
            None => return Ok(None),
        };

        // Axis scores.
        let mut axes_stmt = conn.prepare(
            "SELECT axis, score, rationale FROM axis_scores
              WHERE run_id = ?1
              ORDER BY id ASC",
        )?;
        let axes: Vec<AxisScoreDto> = axes_stmt
            .query_map(params![run_id], |r| {
                Ok(AxisScoreDto {
                    axis: r.get(0)?,
                    score: r.get(1)?,
                    rationale: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        // Findings.
        let mut findings_stmt = conn.prepare(
            "SELECT id, turn_index, category, severity, signature, description,
                    evidence_json, issue_url, issue_dedup_of
               FROM findings WHERE run_id = ?1 ORDER BY id ASC",
        )?;
        let findings: Vec<FindingDto> = findings_stmt
            .query_map(params![run_id], |r| {
                Ok(FindingDto {
                    id: r.get(0)?,
                    turn_index: r.get(1)?,
                    category: r.get(2)?,
                    severity: r.get(3)?,
                    signature: r.get(4)?,
                    description: r.get(5)?,
                    evidence_json: r.get(6)?,
                    issue_url: r.get(7)?,
                    issue_dedup_of: r.get(8)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        // Turn summaries.
        let mut turns_stmt = conn.prepare(
            "SELECT turn_index, player_input, outcome, kind, elapsed_ms,
                    location_name, game_clock, npcs_here_count, frame_path
               FROM turns WHERE run_id = ?1 ORDER BY turn_index ASC",
        )?;
        let turns: Vec<TurnSummaryDto> = turns_stmt
            .query_map(params![run_id], |r| {
                Ok(TurnSummaryDto {
                    turn_index: r.get(0)?,
                    player_input: r.get(1)?,
                    outcome: r.get(2)?,
                    kind: r.get(3)?,
                    elapsed_ms: r.get(4)?,
                    location_name: r.get(5)?,
                    game_clock: r.get(6)?,
                    npcs_here_count: r.get(7)?,
                    frame_path: r.get(8)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        Ok(Some(RunDetail {
            summary,
            axes,
            findings,
            turns,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{ActorMode, GateCfg, JudgeCfg, PlayerCfg, RunConfig};
    use crate::git::GitProvenance;
    use crate::persist::{Db, TurnRecord};
    use crate::score::{Axis, AxisScore, Finding, GateReason, GateTrip, Severity};
    use std::collections::BTreeMap;

    fn cfg() -> RunConfig {
        RunConfig {
            label: Some("t".into()),
            engine_models: BTreeMap::new(),
            flags: vec![],
            player: PlayerCfg {
                mode: ActorMode::Scripted,
                model: None,
                persona: String::new(),
                strategy: String::new(),
            },
            judge: JudgeCfg {
                mode: ActorMode::Scripted,
                model: None,
                rubric_version: "v1".into(),
                rubric_sha256: None,
            },
            gate: GateCfg::default(),
        }
    }

    fn git() -> GitProvenance {
        GitProvenance {
            sha: "abc".into(),
            branch: "main".into(),
            dirty: false,
            pr_number: None,
        }
    }

    fn make_full_run(db: &Db) -> (i64, i64) {
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "rubricsha", "/tmp/run").unwrap();

        // Add a turn.
        db.record_turn(
            rid,
            &TurnRecord {
                turn_index: 0,
                player_input: "look".into(),
                outcome: "ok".into(),
                kind: "looked".into(),
                elapsed_ms: 10,
                engine_state_json: "{}".into(),
                location_id: Some(1),
                location_name: Some("Lane".into()),
                game_clock: Some("08:00".into()),
                npcs_here_count: Some(2),
                screenshot_path: None,
                frame_path: "turns/000/frame.png".into(),
                lines_path: "turns/000/lines.json".into(),
                llm_transcript_path: None,
            },
        )
        .unwrap();

        // Score.
        let axes: Vec<AxisScore> = Axis::ALL
            .into_iter()
            .map(|a| AxisScore {
                axis: a,
                score: 80,
                rationale: "good".into(),
            })
            .collect();
        db.finish_run_scored(rid, Some(80.0), &axes).unwrap();

        // Add a finding.
        let fid = db
            .insert_finding(
                rid,
                &Finding {
                    category: "common_sense".into(),
                    turn_index: Some(0),
                    severity: Severity::Low,
                    description: "minor".into(),
                    evidence_quote: "quote".into(),
                    signature: "testsig".into(),
                },
            )
            .unwrap();

        (rid, fid)
    }

    #[test]
    fn list_runs_returns_inserted_run() {
        let db = Db::open_in_memory().unwrap();
        let (rid, _) = make_full_run(&db);
        let runs = db.list_runs(50).unwrap();
        assert!(!runs.is_empty());
        let r = runs.iter().find(|r| r.id == rid).unwrap();
        assert_eq!(r.status, "completed");
        assert_eq!(r.turn_count, 1);
        assert_eq!(r.quality_score, Some(80.0));
        assert_eq!(r.finding_count, 1);
    }

    #[test]
    fn run_detail_returns_axes_findings_turns() {
        let db = Db::open_in_memory().unwrap();
        let (rid, _) = make_full_run(&db);
        let detail = db.run_detail(rid).unwrap().expect("detail should be Some");
        assert_eq!(detail.summary.id, rid);
        assert_eq!(detail.axes.len(), 7);
        assert_eq!(detail.findings.len(), 1);
        assert_eq!(detail.turns.len(), 1);
        assert_eq!(detail.turns[0].player_input, "look");
        assert_eq!(detail.findings[0].category, "common_sense");
    }

    #[test]
    fn run_detail_returns_none_for_unknown_id() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.run_detail(9999).unwrap().is_none());
    }

    #[test]
    fn list_runs_respects_limit() {
        let db = Db::open_in_memory().unwrap();
        // Insert 3 runs.
        for _ in 0..3 {
            make_full_run(&db);
        }
        let runs = db.list_runs(2).unwrap();
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn list_runs_includes_gated_run() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "sha", "/tmp/r").unwrap();
        db.finish_run_gated(
            rid,
            &GateTrip {
                reason: GateReason::Crash,
                turn: 0,
                detail: "boom".into(),
            },
        )
        .unwrap();
        let runs = db.list_runs(10).unwrap();
        let r = runs.iter().find(|r| r.id == rid).unwrap();
        assert_eq!(r.status, "gated");
        assert_eq!(r.gate_reason.as_deref(), Some("crash"));
    }
}
