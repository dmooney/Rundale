//! Issue filer — dedup-aware finding → GitHub issue bridge.
//!
//! Calls `parish_core::ipc::create_bug_report` for each new finding; dedups
//! across all runs by canonical signature so the same class of defect never
//! spawns more than one tracked issue.

use std::path::{Path, PathBuf};

use parish_core::ipc::{BugReportRequest, BugReportState, GitHubBugConfig, create_bug_report};

use crate::error::Result;
use crate::persist::Db;
use crate::score::Finding;

/// Dedup-aware issue filer.
///
/// One instance per run (or shared across runs — stateless beyond the resolved
/// config). Construct via [`IssueFiler::from_env`] or
/// [`IssueFiler::with_config`] for testing.
pub struct IssueFiler {
    http: reqwest::Client,
    cfg: GitHubBugConfig,
    bundle_root: PathBuf,
}

impl IssueFiler {
    /// Resolve configuration from the environment. Uses
    /// `GitHubBugConfig::from_env()` (token precedence + offline fallback) and
    /// places dry-run bundles under the user-data root.
    pub fn from_env() -> Self {
        let bundle_root = parish_core::persistence::paths::resolve_user_data_dir("parish-harness")
            .join("bug-bundles");
        Self {
            http: reqwest::Client::new(),
            cfg: GitHubBugConfig::from_env(),
            bundle_root,
        }
    }

    /// Construct with explicit config (used in tests).
    pub fn with_config(cfg: GitHubBugConfig, bundle_root: PathBuf) -> Self {
        Self {
            http: reqwest::Client::new(),
            cfg,
            bundle_root,
        }
    }

    /// File a finding as a GitHub issue (or dry-run bundle on disk), unless an
    /// issue has already been filed for this signature.
    ///
    /// Returns:
    /// - `Ok(Some(url))` — issue URL or bundle path (dry-run).
    /// - `Ok(None)` — deduped; a prior run already filed this finding.
    pub async fn file_finding(
        &self,
        db: &Db,
        run_id: i64,
        finding: &Finding,
        finding_row_id: i64,
        run_artifact_dir: &Path,
        dashboard_base: &str,
    ) -> Result<Option<String>> {
        // ── Dedup check ──────────────────────────────────────────────────────
        if let Some((prior_url, prior_row_id)) = db.already_filed(&finding.signature)? {
            // A prior finding with the same signature was already filed.
            // Mark this one as a duplicate and skip filing.
            db.mark_finding_dedup(finding_row_id, prior_row_id)?;
            tracing::debug!(
                signature = %finding.signature,
                prior_url = %prior_url,
                "finding deduped against prior filing"
            );
            return Ok(None);
        }

        // ── Read optional frame screenshot ───────────────────────────────────
        let screenshot_png: Option<Vec<u8>> = match finding.turn_index {
            Some(idx) => {
                let frame_path = run_artifact_dir.join(format!("turns/{idx:03}/frame.png"));
                std::fs::read(&frame_path).ok()
            }
            None => None,
        };

        // ── Build the request ────────────────────────────────────────────────
        let title = format!(
            "[harness][{}] {}: {}",
            finding.severity.label(),
            finding.category,
            truncate(&finding.description, 80),
        );

        let run_link = format!("{dashboard_base}/api/runs/{run_id}");
        let turn_note = match finding.turn_index {
            Some(idx) => format!("Turn: {idx}"),
            None => "Turn: (run-level)".to_string(),
        };
        let description = format!(
            "Harness finding filed from run [{run_id}]({run_link}).\n\n\
             {turn_note}\n\n\
             **Evidence:** {}\n\n\
             **Signature:** `{}`",
            finding.evidence_quote, finding.signature,
        );

        let req = BugReportRequest {
            title,
            description,
            screenshot_data_url: None,
            context: None,
        };

        // Build a minimal state — set text_log to the evidence/description
        // lines so the body carries useful context.
        let state = BugReportState {
            text_log: vec![
                format!("Run id: {run_id}"),
                format!("Category: {}", finding.category),
                format!("Severity: {}", finding.severity.label()),
                format!("Signature: {}", finding.signature),
                format!("Evidence: {}", finding.evidence_quote),
            ],
            ..Default::default()
        };

        // ── File the issue ───────────────────────────────────────────────────
        match create_bug_report(
            &self.http,
            &self.cfg,
            &req,
            &state,
            screenshot_png.as_deref(),
            &self.bundle_root,
        )
        .await
        {
            Ok(result) => {
                let url = result
                    .issue_url
                    .or(result.bundle_path)
                    .unwrap_or_else(|| result.message.clone());
                db.set_finding_issue_url(finding_row_id, &url)?;
                tracing::info!(
                    run_id,
                    signature = %finding.signature,
                    url = %url,
                    "finding filed"
                );
                Ok(Some(url))
            }
            Err(e) => {
                tracing::warn!(
                    run_id,
                    signature = %finding.signature,
                    "issue filing failed: {e}"
                );
                // Non-fatal: record the failure as the URL so the DB has a
                // trace but the run is not aborted.
                let msg = format!("filing-error: {e}");
                db.set_finding_issue_url(finding_row_id, &msg)?;
                Ok(Some(msg))
            }
        }
    }
}

/// Truncate a string to at most `max_chars` characters, appending "…" if
/// trimmed.
fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = chars[..max_chars].iter().collect();
        format!("{truncated}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActorMode, GateCfg, JudgeCfg, PlayerCfg, RunConfig};
    use crate::git::GitProvenance;
    use crate::persist::Db;
    use crate::score::{Finding, Severity};
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

    fn finding(sig: &str) -> Finding {
        Finding {
            category: "common_sense".into(),
            turn_index: Some(0),
            severity: Severity::Low,
            description: "minor issue".into(),
            evidence_quote: "some quote".into(),
            signature: sig.into(),
        }
    }

    // ── DB method unit tests ─────────────────────────────────────────────────

    #[test]
    fn already_filed_returns_none_when_no_prior() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "sha", "/tmp/r").unwrap();
        let row_id = db.insert_finding(rid, &finding("sig-a")).unwrap();
        // No issue URL set yet.
        let _ = row_id;
        assert!(db.already_filed("sig-a").unwrap().is_none());
    }

    #[test]
    fn already_filed_returns_url_after_set() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "sha", "/tmp/r").unwrap();
        let row_id = db.insert_finding(rid, &finding("sig-b")).unwrap();
        db.set_finding_issue_url(row_id, "https://github.com/x/y/issues/1")
            .unwrap();
        let result = db.already_filed("sig-b").unwrap();
        assert!(result.is_some());
        let (url, prior_id) = result.unwrap();
        assert_eq!(url, "https://github.com/x/y/issues/1");
        assert_eq!(prior_id, row_id);
    }

    #[test]
    fn mark_finding_dedup_links_rows() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        // Run 1: file original.
        let rid1 = db.start_run(cid, &git(), "sha", "/tmp/r1").unwrap();
        let row1 = db.insert_finding(rid1, &finding("sig-c")).unwrap();
        db.set_finding_issue_url(row1, "https://github.com/issues/5")
            .unwrap();

        // Run 2: duplicate finding.
        let rid2 = db.start_run(cid, &git(), "sha2", "/tmp/r2").unwrap();
        let row2 = db.insert_finding(rid2, &finding("sig-c")).unwrap();
        db.mark_finding_dedup(row2, row1).unwrap();

        // already_filed for the second run still returns the original URL.
        let (url, _) = db.already_filed("sig-c").unwrap().unwrap();
        assert_eq!(url, "https://github.com/issues/5");
    }

    // ── Dry-run filer integration test ───────────────────────────────────────

    #[tokio::test]
    async fn filer_dry_run_files_first_and_dedups_second() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("bundles");

        let dry_run_cfg = GitHubBugConfig {
            token: None,
            repo: "test/repo".into(),
            dry_run: true,
            api_base: "https://api.github.com".into(),
        };
        let filer = IssueFiler::with_config(dry_run_cfg, bundle_root.clone());

        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "sha", "/tmp/r").unwrap();

        let f = finding("sig-dry");
        let row1 = db.insert_finding(rid, &f).unwrap();

        // First filing: should create a dry-run bundle on disk.
        let result = filer
            .file_finding(&db, rid, &f, row1, tmp.path(), "http://localhost:8787")
            .await
            .unwrap();
        assert!(result.is_some(), "first filing should return Some(url)");

        // Second finding with same signature: should be deduped.
        let rid2 = db.start_run(cid, &git(), "sha2", "/tmp/r2").unwrap();
        let row2 = db.insert_finding(rid2, &f).unwrap();
        let result2 = filer
            .file_finding(&db, rid2, &f, row2, tmp.path(), "http://localhost:8787")
            .await
            .unwrap();
        assert!(
            result2.is_none(),
            "second finding with same signature should dedup to None"
        );
    }
}
