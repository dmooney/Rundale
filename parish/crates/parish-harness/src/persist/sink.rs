//! Persistence sink — all reads/writes against `harness.db`.
//!
//! Synchronous `rusqlite` calls invoked inline from the (async) run loop. This
//! is a single-process tool; WAL mode covers the dashboard reading while a run
//! writes, and a future multi-worker setup is isolated behind the queue.

use rusqlite::{Connection, OptionalExtension, params};

use crate::config::RunConfig;
use crate::cost::CostSummary;
use crate::error::{HarnessError, Result};
use crate::git::GitProvenance;
use crate::score::{AxisScore, Finding, GateTrip};

use super::schema;

/// Default DB path under the platform user-data root (honours
/// `PARISH_USER_DATA_DIR`). App name `parish-harness`.
pub fn default_db_path() -> std::path::PathBuf {
    parish_core::persistence::paths::resolve_user_data_dir("parish-harness").join("harness.db")
}

/// One persisted turn.
pub struct TurnRecord {
    pub turn_index: u32,
    pub player_input: String,
    pub outcome: String,
    pub kind: String,
    pub elapsed_ms: u64,
    pub engine_state_json: String,
    pub location_id: Option<u32>,
    pub location_name: Option<String>,
    pub game_clock: Option<String>,
    pub npcs_here_count: Option<u32>,
    pub screenshot_path: Option<String>,
    pub frame_path: String,
    pub lines_path: String,
    pub llm_transcript_path: Option<String>,
}

/// A complete, externally-produced run ready to persist in one shot. Mirrors
/// what the live `run` pipeline writes incrementally, but assembled up front by
/// an external producer (the quality-harness skill) and replayed through the
/// same sink writers via [`Db::ingest_complete_run`].
pub struct IngestRecord {
    pub config: RunConfig,
    pub git: GitProvenance,
    pub rubric_sha256: String,
    /// Absolute path to the run's artifact directory (`.../runs/<uuid>`).
    pub artifact_dir: String,
    pub turns: Vec<TurnRecord>,
    pub axes: Vec<AxisScore>,
    pub findings: Vec<Finding>,
    /// `Some` => gated (quality is forced NULL); `None` => scored.
    pub gate: Option<GateTrip>,
    pub quality_score: Option<f64>,
    pub cost_usd: f64,
    pub player_tokens: u64,
    pub judge_tokens: u64,
}

/// A compact run summary for CLI output and the dashboard list.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub id: i64,
    pub status: String,
    pub turn_count: u32,
    pub gate_reason: Option<String>,
    pub gate_turn: Option<u32>,
    pub quality_score: Option<f64>,
    pub rubric_sha256: String,
    pub git_sha: String,
    pub git_branch: String,
    pub finding_count: u32,
}

/// The harness database handle.
pub struct Db {
    pub(crate) conn: Connection,
}

impl Db {
    /// Open (creating parent dirs) and migrate the database at `path`.
    pub fn open(path: &std::path::Path) -> Result<Db> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| HarnessError::io(parent.display().to_string(), e))?;
        }
        let conn = Connection::open(path)?;
        schema::migrate(&conn)?;
        Ok(Db { conn })
    }

    /// Open an in-memory database (tests).
    pub fn open_in_memory() -> Result<Db> {
        let conn = Connection::open_in_memory()?;
        schema::migrate(&conn)?;
        Ok(Db { conn })
    }

    /// Insert the config if new (by content hash) and return its row id.
    pub fn upsert_config(&self, cfg: &RunConfig) -> Result<i64> {
        let sha = cfg.content_hash();
        if let Some(id) = self
            .conn
            .query_row(
                "SELECT id FROM configs WHERE config_sha256 = ?1",
                params![sha],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        let json = cfg.canonical_json();
        self.conn.execute(
            "INSERT INTO configs (config_sha256, config_json, label, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![sha, json, cfg.label, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Start a run row in `in_progress` status; returns its id.
    pub fn start_run(
        &self,
        config_id: i64,
        git: &GitProvenance,
        rubric_sha256: &str,
        artifact_dir: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO runs
               (config_id, started_at, status, turn_count, git_sha, git_branch,
                git_dirty, pr_number, rubric_sha256, artifact_dir)
             VALUES (?1, ?2, 'in_progress', 0, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                config_id,
                now(),
                git.sha,
                git.branch,
                git.dirty as i64,
                git.pr_number,
                rubric_sha256,
                artifact_dir,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Persist one turn and bump the run's turn count.
    pub fn record_turn(&self, run_id: i64, t: &TurnRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO turns
               (run_id, turn_index, player_input, outcome, kind, elapsed_ms,
                engine_state_json, location_id, location_name, game_clock,
                npcs_here_count, screenshot_path, frame_path, lines_path,
                llm_transcript_path, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                run_id,
                t.turn_index,
                t.player_input,
                t.outcome,
                t.kind,
                t.elapsed_ms as i64,
                t.engine_state_json,
                t.location_id,
                t.location_name,
                t.game_clock,
                t.npcs_here_count,
                t.screenshot_path,
                t.frame_path,
                t.lines_path,
                t.llm_transcript_path,
                now(),
            ],
        )?;
        self.conn.execute(
            "UPDATE runs SET turn_count = ?1 WHERE id = ?2",
            params![t.turn_index + 1, run_id],
        )?;
        Ok(())
    }

    /// Finalize a gated run (no quality score).
    pub fn finish_run_gated(&self, run_id: i64, trip: &GateTrip) -> Result<()> {
        self.conn.execute(
            "UPDATE runs
                SET status = 'gated', ended_at = ?1, gate_passed = 0,
                    gate_reason = ?2, gate_turn = ?3, quality_score = NULL
              WHERE id = ?4",
            params![now(), trip.reason.label(), trip.turn, run_id],
        )?;
        Ok(())
    }

    /// Finalize a passing run with axis scores + quality.
    pub fn finish_run_scored(
        &self,
        run_id: i64,
        quality: Option<f64>,
        axes: &[AxisScore],
    ) -> Result<()> {
        for a in axes {
            self.conn.execute(
                "INSERT OR REPLACE INTO axis_scores (run_id, axis, score, rationale)
                 VALUES (?1, ?2, ?3, ?4)",
                params![run_id, a.axis.key(), a.score, a.rationale],
            )?;
        }
        self.conn.execute(
            "UPDATE runs
                SET status = 'completed', ended_at = ?1, gate_passed = 1,
                    quality_score = ?2
              WHERE id = ?3",
            params![now(), quality, run_id],
        )?;
        Ok(())
    }

    /// Insert a finding. Returns the new row id. Dedup against an existing
    /// already-filed finding by signature is handled by the issue filer
    /// (Phase 2); here we always record the observation.
    pub fn insert_finding(&self, run_id: i64, f: &Finding) -> Result<i64> {
        let evidence = serde_json::json!({ "quote": f.evidence_quote }).to_string();
        self.conn.execute(
            "INSERT INTO findings
               (run_id, turn_index, category, severity, signature, description,
                evidence_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                run_id,
                f.turn_index,
                f.category,
                f.severity.label(),
                f.signature,
                f.description,
                evidence,
                now(),
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Count rows in `configs` (distinct run configurations seen). Used to
    /// assert content-hash dedup behaviour.
    pub fn config_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM configs", [], |r| r.get(0))?)
    }

    /// Return the on-disk artifact directory for a run, or `None` if unknown.
    pub fn run_artifact_dir(&self, run_id: i64) -> Result<Option<String>> {
        let result = self
            .conn
            .query_row(
                "SELECT artifact_dir FROM runs WHERE id = ?1",
                params![run_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        Ok(result)
    }

    /// Check whether a finding with the given signature has already been filed
    /// as an issue. Returns `Some((issue_url, finding_row_id))` when a prior
    /// row exists with a non-null `issue_url`.
    pub fn already_filed(&self, signature: &str) -> Result<Option<(String, i64)>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, issue_url FROM findings
                  WHERE signature = ?1
                    AND issue_url IS NOT NULL
                    AND issue_url NOT LIKE 'filing-error:%'
                  ORDER BY id ASC
                  LIMIT 1",
                params![signature],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(result.map(|(id, url)| (url, id)))
    }

    /// Mark a finding row as a duplicate of a prior finding.
    pub fn mark_finding_dedup(&self, finding_row_id: i64, prior_row_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE findings SET issue_dedup_of = ?1 WHERE id = ?2",
            params![prior_row_id, finding_row_id],
        )?;
        Ok(())
    }

    /// Record the issue URL (or dry-run bundle path) against a finding row.
    pub fn set_finding_issue_url(&self, finding_row_id: i64, url: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE findings SET issue_url = ?1 WHERE id = ?2",
            params![url, finding_row_id],
        )?;
        Ok(())
    }

    /// Aggregate cost summary across all runs (sums `cost_usd`, `player_tokens`,
    /// `judge_tokens`).
    ///
    /// Token counts are 0 until `AnyClient` usage exposure is wired through.
    pub fn cost_summary(&self) -> Result<CostSummary> {
        let (total_runs, total_cost_usd, total_player_tokens, total_judge_tokens) =
            self.conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(cost_usd), 0.0),
                        COALESCE(SUM(player_tokens), 0),
                        COALESCE(SUM(judge_tokens), 0)
                   FROM runs",
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, f64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )?;
        Ok(CostSummary {
            total_runs: total_runs as u64,
            total_cost_usd,
            total_player_tokens: total_player_tokens as u64,
            total_judge_tokens: total_judge_tokens as u64,
        })
    }

    /// Write the cost/token totals for a run. The `run` pipeline leaves these
    /// at their 0 defaults until `AnyClient` usage exposure is wired through;
    /// the ingest path supplies them from the skill's own accounting.
    pub fn update_run_cost(
        &self,
        run_id: i64,
        cost_usd: f64,
        player_tokens: u64,
        judge_tokens: u64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE runs SET cost_usd = ?1, player_tokens = ?2, judge_tokens = ?3
              WHERE id = ?4",
            params![cost_usd, player_tokens as i64, judge_tokens as i64, run_id],
        )?;
        Ok(())
    }

    /// Persist a complete, externally-produced run in one transaction, reusing
    /// the exact same sink writers the live `run` pipeline calls — there is no
    /// second write path, so scored/gated semantics cannot drift. Returns the
    /// new run id. Used by the `ingest` subcommand to land quality-harness
    /// **skill** runs in the same DB the dashboard reads.
    pub fn ingest_complete_run(&self, rec: IngestRecord) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let config_id = self.upsert_config(&rec.config)?;
        let run_id = self.start_run(config_id, &rec.git, &rec.rubric_sha256, &rec.artifact_dir)?;
        for t in &rec.turns {
            self.record_turn(run_id, t)?;
        }
        for f in &rec.findings {
            self.insert_finding(run_id, f)?;
        }
        self.update_run_cost(run_id, rec.cost_usd, rec.player_tokens, rec.judge_tokens)?;
        match &rec.gate {
            Some(trip) => self.finish_run_gated(run_id, trip)?,
            None => self.finish_run_scored(run_id, rec.quality_score, &rec.axes)?,
        }
        tx.commit()?;
        Ok(run_id)
    }

    /// Read a run summary for output.
    pub fn run_summary(&self, run_id: i64) -> Result<RunSummary> {
        let summary = self.conn.query_row(
            "SELECT id, status, turn_count, gate_reason, gate_turn, quality_score,
                    rubric_sha256, git_sha, git_branch
               FROM runs WHERE id = ?1",
            params![run_id],
            |r| {
                Ok(RunSummary {
                    id: r.get(0)?,
                    status: r.get(1)?,
                    turn_count: r.get(2)?,
                    gate_reason: r.get(3)?,
                    gate_turn: r.get(4)?,
                    quality_score: r.get(5)?,
                    rubric_sha256: r.get(6)?,
                    git_sha: r.get(7)?,
                    git_branch: r.get(8)?,
                    finding_count: 0,
                })
            },
        )?;
        let finding_count: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM findings WHERE run_id = ?1",
            params![run_id],
            |r| r.get(0),
        )?;
        Ok(RunSummary {
            finding_count,
            ..summary
        })
    }
}

/// Current UTC timestamp, RFC3339.
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActorMode, GateCfg, JudgeCfg, PlayerCfg};
    use crate::score::{Axis, GateReason, Severity};
    use std::collections::BTreeMap;

    #[test]
    fn cost_summary_empty_db_returns_zeros() {
        let db = Db::open_in_memory().unwrap();
        let s = db.cost_summary().unwrap();
        assert_eq!(s.total_runs, 0);
        assert_eq!(s.total_cost_usd, 0.0);
        assert_eq!(s.total_player_tokens, 0);
        assert_eq!(s.total_judge_tokens, 0);
    }

    #[test]
    fn cost_summary_sums_across_runs() {
        let db = Db::open_in_memory().unwrap();
        // Insert two runs; the columns default to 0 (no cost wired yet).
        let cfg = RunConfig {
            label: Some("c".into()),
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
        };
        let git = GitProvenance {
            sha: "x".into(),
            branch: "main".into(),
            dirty: false,
            pr_number: None,
        };
        let cid = db.upsert_config(&cfg).unwrap();
        db.start_run(cid, &git, "sha", "/tmp/a").unwrap();
        db.start_run(cid, &git, "sha", "/tmp/b").unwrap();

        let s = db.cost_summary().unwrap();
        assert_eq!(s.total_runs, 2);
        // Columns default to 0; totals are 0.
        assert_eq!(s.total_cost_usd, 0.0);
        assert_eq!(s.total_player_tokens, 0);
        assert_eq!(s.total_judge_tokens, 0);
    }

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
            sha: "abc123".into(),
            branch: "main".into(),
            dirty: false,
            pr_number: None,
        }
    }

    #[test]
    fn upsert_config_is_idempotent_by_hash() {
        let db = Db::open_in_memory().unwrap();
        let a = db.upsert_config(&cfg()).unwrap();
        let b = db.upsert_config(&cfg()).unwrap();
        assert_eq!(a, b, "same config hash returns the same row id");
    }

    #[test]
    fn gated_run_has_null_quality_and_reason() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "rubricsha", "/tmp/run").unwrap();
        db.finish_run_gated(
            rid,
            &GateTrip {
                reason: GateReason::Crash,
                turn: 0,
                detail: "boom".into(),
            },
        )
        .unwrap();
        let s = db.run_summary(rid).unwrap();
        assert_eq!(s.status, "gated");
        assert_eq!(s.gate_reason.as_deref(), Some("crash"));
        assert_eq!(s.quality_score, None);
    }

    #[test]
    fn scored_run_records_axes_and_quality() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "rubricsha", "/tmp/run").unwrap();
        let axes: Vec<AxisScore> = Axis::ALL
            .into_iter()
            .map(|a| AxisScore {
                axis: a,
                score: 75,
                rationale: "ok".into(),
            })
            .collect();
        db.finish_run_scored(rid, Some(75.0), &axes).unwrap();
        let s = db.run_summary(rid).unwrap();
        assert_eq!(s.status, "completed");
        assert_eq!(s.quality_score, Some(75.0));
    }

    #[test]
    fn turns_and_findings_persist() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "rubricsha", "/tmp/run").unwrap();
        db.record_turn(
            rid,
            &TurnRecord {
                turn_index: 0,
                player_input: "look".into(),
                outcome: "ok".into(),
                kind: "looked".into(),
                elapsed_ms: 5,
                engine_state_json: "{}".into(),
                location_id: Some(1),
                location_name: Some("Lane".into()),
                game_clock: Some("08:00".into()),
                npcs_here_count: Some(0),
                screenshot_path: None,
                frame_path: "turns/000/frame.png".into(),
                lines_path: "turns/000/lines.json".into(),
                llm_transcript_path: None,
            },
        )
        .unwrap();
        db.insert_finding(
            rid,
            &Finding {
                category: "common_sense".into(),
                turn_index: Some(0),
                severity: Severity::Low,
                description: "minor".into(),
                evidence_quote: "x".into(),
                signature: "sig".into(),
            },
        )
        .unwrap();
        let s = db.run_summary(rid).unwrap();
        assert_eq!(s.turn_count, 1);
        assert_eq!(s.finding_count, 1);
    }
}
