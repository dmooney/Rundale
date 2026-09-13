//! SQLite schema + migrations for the harness's own database.
//!
//! Mirrors the persistence crate's idiom: WAL mode, foreign keys on, and a
//! hand-rolled `migrate()` of `CREATE TABLE IF NOT EXISTS` statements (no
//! migration framework). This is the harness's *own* QA-telemetry DB — entirely
//! separate from the game's save model.

use rusqlite::Connection;

use crate::error::Result;

/// Apply pragmas and create tables if absent.
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;

         CREATE TABLE IF NOT EXISTS configs (
             id            INTEGER PRIMARY KEY,
             config_sha256 TEXT UNIQUE NOT NULL,
             config_json   TEXT NOT NULL,
             label         TEXT,
             created_at    TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS queue (
             id          INTEGER PRIMARY KEY,
             config_id   INTEGER NOT NULL REFERENCES configs(id),
             state       TEXT NOT NULL,
             priority    INTEGER NOT NULL DEFAULT 0,
             claimed_by  TEXT,
             enqueued_at TEXT NOT NULL,
             claimed_at  TEXT,
             run_id      INTEGER REFERENCES runs(id)
         );

         CREATE TABLE IF NOT EXISTS runs (
             id            INTEGER PRIMARY KEY,
             config_id     INTEGER NOT NULL REFERENCES configs(id),
             started_at    TEXT NOT NULL,
             ended_at      TEXT,
             status        TEXT NOT NULL,
             turn_count    INTEGER NOT NULL DEFAULT 0,
             gate_passed   INTEGER,
             gate_reason   TEXT,
             gate_turn     INTEGER,
             quality_score REAL,
             git_sha       TEXT NOT NULL,
             git_branch    TEXT NOT NULL,
             git_dirty     INTEGER NOT NULL,
             pr_number     INTEGER,
             rubric_sha256 TEXT NOT NULL,
             cost_usd      REAL NOT NULL DEFAULT 0,
             player_tokens INTEGER NOT NULL DEFAULT 0,
             judge_tokens  INTEGER NOT NULL DEFAULT 0,
             artifact_dir  TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS turns (
             id                  INTEGER PRIMARY KEY,
             run_id              INTEGER NOT NULL REFERENCES runs(id),
             turn_index          INTEGER NOT NULL,
             player_input        TEXT NOT NULL,
             outcome             TEXT NOT NULL,
             kind                TEXT NOT NULL,
             elapsed_ms          INTEGER NOT NULL,
             engine_state_json   TEXT NOT NULL,
             location_id         INTEGER,
             location_name       TEXT,
             game_clock          TEXT,
             npcs_here_count     INTEGER,
             screenshot_path     TEXT,
             frame_path          TEXT NOT NULL,
             lines_path          TEXT NOT NULL,
             llm_transcript_path TEXT,
             created_at          TEXT NOT NULL,
             UNIQUE(run_id, turn_index)
         );

         CREATE TABLE IF NOT EXISTS axis_scores (
             id        INTEGER PRIMARY KEY,
             run_id    INTEGER NOT NULL REFERENCES runs(id),
             axis      TEXT NOT NULL,
             score     INTEGER NOT NULL,
             rationale TEXT,
             UNIQUE(run_id, axis)
         );

         CREATE TABLE IF NOT EXISTS findings (
             id             INTEGER PRIMARY KEY,
             run_id         INTEGER NOT NULL REFERENCES runs(id),
             turn_index     INTEGER,
             category       TEXT NOT NULL,
             severity       TEXT NOT NULL,
             signature      TEXT NOT NULL,
             description    TEXT NOT NULL,
             evidence_json  TEXT,
             issue_url      TEXT,
             issue_dedup_of INTEGER REFERENCES findings(id),
             created_at     TEXT NOT NULL
         );

         CREATE INDEX IF NOT EXISTS idx_runs_git ON runs(git_sha);
         CREATE INDEX IF NOT EXISTS idx_runs_config ON runs(config_id);
         CREATE INDEX IF NOT EXISTS idx_turns_run ON turns(run_id, turn_index);
         CREATE INDEX IF NOT EXISTS idx_axis_run ON axis_scores(run_id);
         CREATE INDEX IF NOT EXISTS idx_findings_sig ON findings(signature);
         CREATE INDEX IF NOT EXISTS idx_findings_run ON findings(run_id);
         CREATE INDEX IF NOT EXISTS idx_queue_state ON queue(state, priority DESC, enqueued_at);",
    )?;
    Ok(())
}
