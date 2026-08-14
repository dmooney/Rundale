//! Dashboard read queries — surfaces run/turn/axis/finding data for the
//! dashboard API without coupling the sink module to serde.

use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;

use crate::config::RunConfig;
use crate::error::Result;
use crate::git::commit_date;
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
    pub cost_usd: f64,
    pub player_tokens: u64,
    pub judge_tokens: u64,
    pub measured_turn_count: u32,
    pub avg_response_ms: Option<f64>,
    pub rubric_sha256: String,
    pub git_sha: String,
    pub git_branch: String,
    pub git_dirty: bool,
    pub finding_count: u32,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub artifact_dir: String,
    pub config: RunConfig,
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
    /// True when this turn has a captured inference log (`turns/NNN/llm.json`),
    /// so the dashboard can make the turn clickable to view it.
    pub has_transcript: bool,
}

/// Full run detail for the `/api/runs/{id}` endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct RunDetail {
    pub summary: RunSummaryDto,
    pub timing: RunTimingDto,
    pub axes: Vec<AxisScoreDto>,
    pub findings: Vec<FindingDto>,
    pub turns: Vec<TurnSummaryDto>,
}

/// Response-time statistics over turns with a real positive elapsed time.
#[derive(Debug, Clone, Serialize)]
pub struct RunTimingDto {
    pub measured_turns: u32,
    pub avg_ms: Option<f64>,
    pub p95_ms: Option<i64>,
    pub max_ms: Option<i64>,
}

fn run_summary_from_row(r: &Row<'_>) -> rusqlite::Result<RunSummaryDto> {
    let config_json: String = r.get(18)?;
    let config = serde_json::from_str(&config_json).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(18, Type::Text, Box::new(source))
    })?;
    Ok(RunSummaryDto {
        id: r.get(0)?,
        status: r.get(1)?,
        turn_count: r.get(2)?,
        gate_reason: r.get(3)?,
        gate_turn: r.get(4)?,
        quality_score: r.get(5)?,
        cost_usd: r.get(6)?,
        player_tokens: r.get::<_, i64>(7)? as u64,
        judge_tokens: r.get::<_, i64>(8)? as u64,
        measured_turn_count: r.get::<_, i64>(9)? as u32,
        avg_response_ms: r.get(10)?,
        rubric_sha256: r.get(11)?,
        git_sha: r.get(12)?,
        git_branch: r.get(13)?,
        git_dirty: r.get::<_, i64>(14)? != 0,
        started_at: r.get(15)?,
        ended_at: r.get(16)?,
        artifact_dir: r.get(17)?,
        config,
        finding_count: r.get::<_, i64>(19)? as u32,
    })
}

/// One data point on the quality-over-commits timeline.
#[derive(Debug, Clone, Serialize)]
pub struct TimelinePoint {
    pub run_id: i64,
    pub git_sha: String,
    pub git_branch: String,
    pub quality_score: Option<f64>,
    /// ISO-8601 committer date; `None` when the SHA is unknown / placeholder.
    pub commit_date: Option<String>,
    pub axes: Vec<(String, i64)>,
}

/// Per-axis delta entry for the A/B compare view.
#[derive(Debug, Clone, Serialize)]
pub struct AxisDelta {
    pub axis: String,
    pub a_score: i64,
    pub b_score: i64,
    pub delta: i64,
}

/// Full A/B comparison between two runs.
#[derive(Debug, Clone, Serialize)]
pub struct AbCompare {
    pub a: RunSummaryDto,
    pub b: RunSummaryDto,
    pub axis_deltas: Vec<AxisDelta>,
    /// Gate status for each run.
    pub gate_diff: (String, String),
    /// Finding signatures present in run A but not in run B.
    pub findings_only_in_a: Vec<String>,
    /// Finding signatures present in run B but not in run A.
    pub findings_only_in_b: Vec<String>,
}

impl Db {
    /// Findings that still need a real issue link — `issue_url` is NULL or
    /// records a prior filing error. Returns `(finding_row_id, signature)`
    /// across all runs, ordered by id, for the `backfill-issues` subcommand.
    pub fn findings_needing_issue_url(&self) -> Result<Vec<(i64, String)>> {
        let conn = &self.conn;
        let mut stmt = conn.prepare(
            "SELECT id, signature FROM findings
              WHERE issue_url IS NULL OR issue_url LIKE 'filing-error:%'
              ORDER BY id ASC",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return the most recent `limit` runs in reverse-chronological order.
    pub fn list_runs(&self, limit: u32) -> Result<Vec<RunSummaryDto>> {
        let conn = &self.conn;
        let mut stmt = conn.prepare(
            "SELECT r.id, r.status, r.turn_count, r.gate_reason, r.gate_turn,
                    r.quality_score, r.cost_usd, r.player_tokens, r.judge_tokens,
                    (SELECT COUNT(*) FROM turns t
                      WHERE t.run_id = r.id AND t.elapsed_ms > 0),
                    (SELECT AVG(t.elapsed_ms) FROM turns t
                      WHERE t.run_id = r.id AND t.elapsed_ms > 0),
                    r.rubric_sha256, r.git_sha, r.git_branch,
                    r.git_dirty, r.started_at, r.ended_at, r.artifact_dir,
                    c.config_json, COUNT(f.id) AS finding_count
               FROM runs r
               JOIN configs c ON c.id = r.config_id
               LEFT JOIN findings f ON f.run_id = r.id
              GROUP BY r.id
              ORDER BY r.id DESC
              LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], run_summary_from_row)?;
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
                        r.quality_score, r.cost_usd, r.player_tokens, r.judge_tokens,
                        (SELECT COUNT(*) FROM turns t
                          WHERE t.run_id = r.id AND t.elapsed_ms > 0),
                        (SELECT AVG(t.elapsed_ms) FROM turns t
                          WHERE t.run_id = r.id AND t.elapsed_ms > 0),
                        r.rubric_sha256, r.git_sha, r.git_branch,
                        r.git_dirty, r.started_at, r.ended_at, r.artifact_dir,
                        c.config_json, COUNT(f.id) AS finding_count
                   FROM runs r
                   JOIN configs c ON c.id = r.config_id
                   LEFT JOIN findings f ON f.run_id = r.id
                  WHERE r.id = ?1
                  GROUP BY r.id",
                params![run_id],
                run_summary_from_row,
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
                    location_name, game_clock, npcs_here_count, frame_path,
                    llm_transcript_path IS NOT NULL
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
                    has_transcript: r.get(9)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        let mut elapsed: Vec<i64> = turns
            .iter()
            .map(|turn| turn.elapsed_ms)
            .filter(|elapsed_ms| *elapsed_ms > 0)
            .collect();
        elapsed.sort_unstable();
        let timing = if elapsed.is_empty() {
            RunTimingDto {
                measured_turns: 0,
                avg_ms: None,
                p95_ms: None,
                max_ms: None,
            }
        } else {
            let total: i64 = elapsed.iter().sum();
            let p95_index = ((elapsed.len() * 95).div_ceil(100)).saturating_sub(1);
            RunTimingDto {
                measured_turns: elapsed.len() as u32,
                avg_ms: Some(total as f64 / elapsed.len() as f64),
                p95_ms: elapsed.get(p95_index).copied(),
                max_ms: elapsed.last().copied(),
            }
        };

        Ok(Some(RunDetail {
            summary,
            timing,
            axes,
            findings,
            turns,
        }))
    }

    /// Return all completed runs as timeline points, optionally filtered by
    /// branch. Points are sorted by commit date (newest first); points with no
    /// commit date are appended at the end.
    pub fn timeline(&self, branch: Option<&str>) -> Result<Vec<TimelinePoint>> {
        let conn = &self.conn;

        // Fetch completed runs (with optional branch filter).
        let run_rows: Vec<(i64, String, String, Option<f64>)> = if let Some(b) = branch {
            let mut stmt = conn.prepare(
                "SELECT id, git_sha, git_branch, quality_score
                   FROM runs
                  WHERE status = 'completed' AND git_branch = ?1
                  ORDER BY id DESC",
            )?;
            stmt.query_map(params![b], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<std::result::Result<_, _>>()?
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, git_sha, git_branch, quality_score
                   FROM runs
                  WHERE status = 'completed'
                  ORDER BY id DESC",
            )?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<std::result::Result<_, _>>()?
        };

        // For each run, load its axis scores and resolve the commit date.
        let mut points: Vec<TimelinePoint> = Vec::with_capacity(run_rows.len());
        for (run_id, git_sha, git_branch, quality_score) in run_rows {
            let mut axes_stmt = conn
                .prepare("SELECT axis, score FROM axis_scores WHERE run_id = ?1 ORDER BY id ASC")?;
            let axes: Vec<(String, i64)> = axes_stmt
                .query_map(params![run_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?;

            let cd = commit_date(&git_sha);
            points.push(TimelinePoint {
                run_id,
                git_sha,
                git_branch,
                quality_score,
                commit_date: cd,
                axes,
            });
        }

        // Sort: known commit dates newest-first, then unknowns at the end.
        points.sort_by(|a, b| match (&a.commit_date, &b.commit_date) {
            (Some(da), Some(db)) => db.cmp(da),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        Ok(points)
    }

    /// Compare two runs by id. Returns `None` if either run does not exist.
    pub fn compare(&self, a: i64, b: i64) -> Result<Option<AbCompare>> {
        let conn = &self.conn;

        // Helper: load a summary for a run id.
        let load_summary = |run_id: i64| -> Result<Option<RunSummaryDto>> {
            conn.query_row(
                "SELECT r.id, r.status, r.turn_count, r.gate_reason, r.gate_turn,
                        r.quality_score, r.cost_usd, r.player_tokens, r.judge_tokens,
                        (SELECT COUNT(*) FROM turns t
                          WHERE t.run_id = r.id AND t.elapsed_ms > 0),
                        (SELECT AVG(t.elapsed_ms) FROM turns t
                          WHERE t.run_id = r.id AND t.elapsed_ms > 0),
                        r.rubric_sha256, r.git_sha, r.git_branch,
                        r.git_dirty, r.started_at, r.ended_at, r.artifact_dir,
                        c.config_json, COUNT(f.id) AS finding_count
                   FROM runs r
                   JOIN configs c ON c.id = r.config_id
                   LEFT JOIN findings f ON f.run_id = r.id
                  WHERE r.id = ?1
                  GROUP BY r.id",
                params![run_id],
                run_summary_from_row,
            )
            .optional()
            .map_err(Into::into)
        };

        let summary_a = match load_summary(a)? {
            Some(s) => s,
            None => return Ok(None),
        };
        let summary_b = match load_summary(b)? {
            Some(s) => s,
            None => return Ok(None),
        };

        // Load axis scores for both runs.
        let load_axes = |run_id: i64| -> Result<std::collections::HashMap<String, i64>> {
            let mut stmt = conn.prepare("SELECT axis, score FROM axis_scores WHERE run_id = ?1")?;
            let rows: Vec<(String, i64)> = stmt
                .query_map(params![run_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?;
            Ok(rows.into_iter().collect())
        };

        let axes_a = load_axes(a)?;
        let axes_b = load_axes(b)?;

        // Build axis deltas over the union of all axes.
        let mut all_axes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        all_axes.extend(axes_a.keys().cloned());
        all_axes.extend(axes_b.keys().cloned());

        let axis_deltas: Vec<AxisDelta> = all_axes
            .into_iter()
            .map(|axis| {
                let sa = *axes_a.get(&axis).unwrap_or(&0);
                let sb = *axes_b.get(&axis).unwrap_or(&0);
                AxisDelta {
                    axis,
                    a_score: sa,
                    b_score: sb,
                    delta: sb - sa,
                }
            })
            .collect();

        // Load finding signatures for both runs.
        let load_sigs = |run_id: i64| -> Result<std::collections::HashSet<String>> {
            let mut stmt = conn.prepare("SELECT signature FROM findings WHERE run_id = ?1")?;
            let sigs: Vec<String> = stmt
                .query_map(params![run_id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            Ok(sigs.into_iter().collect())
        };

        let sigs_a = load_sigs(a)?;
        let sigs_b = load_sigs(b)?;

        let mut findings_only_in_a: Vec<String> = sigs_a.difference(&sigs_b).cloned().collect();
        findings_only_in_a.sort();
        let mut findings_only_in_b: Vec<String> = sigs_b.difference(&sigs_a).cloned().collect();
        findings_only_in_b.sort();

        let gate_diff = (
            summary_a
                .gate_reason
                .clone()
                .unwrap_or_else(|| "passed".to_string()),
            summary_b
                .gate_reason
                .clone()
                .unwrap_or_else(|| "passed".to_string()),
        );

        Ok(Some(AbCompare {
            a: summary_a,
            b: summary_b,
            axis_deltas,
            gate_diff,
            findings_only_in_a,
            findings_only_in_b,
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
                persona: "tester".into(),
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
                elapsed_ms: 250,
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
        db.update_run_cost(rid, 1.25, 1_234, 567).unwrap();

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
        assert_eq!(r.cost_usd, 1.25);
        assert_eq!(r.player_tokens, 1_234);
        assert_eq!(r.judge_tokens, 567);
        assert_eq!(r.measured_turn_count, 1);
        assert_eq!(r.avg_response_ms, Some(250.0));
        assert_eq!(r.config.player.persona, "tester");
        assert_eq!(r.finding_count, 1);
    }

    #[test]
    fn run_detail_returns_axes_findings_turns() {
        let db = Db::open_in_memory().unwrap();
        let (rid, _) = make_full_run(&db);
        let detail = db.run_detail(rid).unwrap().expect("detail should be Some");
        assert_eq!(detail.summary.id, rid);
        assert_eq!(detail.summary.cost_usd, 1.25);
        assert_eq!(detail.timing.measured_turns, 1);
        assert_eq!(detail.timing.avg_ms, Some(250.0));
        assert_eq!(detail.timing.p95_ms, Some(250));
        assert_eq!(detail.timing.max_ms, Some(250));
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
    fn timeline_returns_completed_runs_with_axes() {
        let db = Db::open_in_memory().unwrap();
        let (rid, _) = make_full_run(&db);

        let points = db.timeline(None).unwrap();
        assert!(!points.is_empty());
        let p = points.iter().find(|p| p.run_id == rid).unwrap();
        assert_eq!(p.axes.len(), 7, "all 7 axes should be present");
        assert!(p.quality_score.is_some());
    }

    #[test]
    fn timeline_branch_filter_works() {
        let db = Db::open_in_memory().unwrap();
        let (rid, _) = make_full_run(&db);

        // "main" is the branch baked into git() above.
        let points_main = db.timeline(Some("main")).unwrap();
        assert!(points_main.iter().any(|p| p.run_id == rid));

        let points_other = db.timeline(Some("not-a-branch")).unwrap();
        assert!(points_other.iter().all(|p| p.run_id != rid));
    }

    #[test]
    fn compare_returns_axis_deltas_and_finding_diffs() {
        let db = Db::open_in_memory().unwrap();

        // Build run A with score=80 everywhere.
        let (rid_a, _) = make_full_run(&db);

        // Build run B with score=60 everywhere + a different finding.
        let cid_b = db.upsert_config(&cfg()).unwrap();
        let rid_b = db
            .start_run(cid_b, &git(), "rubricsha", "/tmp/run2")
            .unwrap();
        let axes_b: Vec<AxisScore> = Axis::ALL
            .into_iter()
            .map(|ax| AxisScore {
                axis: ax,
                score: 60,
                rationale: "lower".into(),
            })
            .collect();
        db.finish_run_scored(rid_b, Some(60.0), &axes_b).unwrap();
        db.insert_finding(
            rid_b,
            &Finding {
                category: "immersion".into(),
                turn_index: None,
                severity: Severity::High,
                description: "break".into(),
                evidence_quote: "q".into(),
                signature: "sig_b_only".into(),
            },
        )
        .unwrap();

        let cmp = db
            .compare(rid_a, rid_b)
            .unwrap()
            .expect("compare should return Some");
        assert_eq!(cmp.a.id, rid_a);
        assert_eq!(cmp.b.id, rid_b);
        assert_eq!(cmp.a.cost_usd, 1.25);
        assert_eq!(cmp.a.player_tokens, 1_234);
        assert_eq!(cmp.a.avg_response_ms, Some(250.0));
        assert_eq!(cmp.a.config.player.persona, "tester");

        // All axis deltas should be -20 (b - a = 60 - 80).
        for d in &cmp.axis_deltas {
            assert_eq!(d.delta, -20, "axis {} delta should be -20", d.axis);
        }

        // Run A had "testsig"; run B had "sig_b_only".
        assert!(cmp.findings_only_in_a.contains(&"testsig".to_string()));
        assert!(cmp.findings_only_in_b.contains(&"sig_b_only".to_string()));
        assert!(!cmp.findings_only_in_a.contains(&"sig_b_only".to_string()));
    }

    #[test]
    fn compare_returns_none_for_unknown_run() {
        let db = Db::open_in_memory().unwrap();
        let (rid, _) = make_full_run(&db);
        assert!(db.compare(rid, 9999).unwrap().is_none());
        assert!(db.compare(9999, rid).unwrap().is_none());
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
