//! `backfill-issues` — recover finding → GitHub issue links for stored runs.
//!
//! A quality-harness skill run files issues out-of-band (via the MCP
//! `parish_file_bug` path). If the ingest payload omits `issue_url`, the
//! dashboard shows the finding with no `[issue]` link and the user can't tell
//! whether it was ever addressed. This subcommand recovers the link from the
//! signature both filers embed: the harness `IssueFiler` writes
//! ``**Signature:** `<sig>` `` and the skill's `parish_file_bug` path writes
//! `**Signature:** <sig>`. We list the repo's issues once, parse that line into a
//! `signature → issue_url` map, and write it onto any finding still missing a real
//! `issue_url`.
//!
//! Pairs with the inline path: future runs set `issue_url` in the ingest payload
//! directly (see the quality-harness skill §5/§6). This subcommand is the
//! safety net for past runs and for dedup-against-prior-run findings.

use std::collections::HashMap;

use parish_core::ipc::GitHubBugConfig;
use serde_json::Value;

use crate::error::{HarnessError, Result};
use crate::persist::Db;

/// Outcome of a backfill pass (drives the CLI summary and the unit tests).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BackfillReport {
    /// Findings that had no real `issue_url` before the pass.
    pub unlinked: usize,
    /// Unlinked findings matched to an issue by signature.
    pub matched: usize,
    /// Links actually written (0 in `--dry-run`).
    pub written: usize,
}

/// Extract the finding signature embedded in a harness-filed issue body.
///
/// Real issue bodies carry the marker in three shapes: ``**Signature:** `<sig>` ``
/// (filer), `**Signature:** \`<sig>\`` (escaped backticks, as `parish_file_bug`
/// writes them), and a bare `Signature: <sig>` line. Signatures are kebab-case
/// slugs containing none of whitespace / `` ` `` / `*` / `\`, so after the marker
/// we split on exactly those and take the first non-empty token — which strips any
/// leading emphasis, backticks, or escaping in every shape.
pub fn signature_from_body(body: &str) -> Option<String> {
    let idx = body.find("Signature:")?;
    let rest = &body[idx + "Signature:".len()..];
    let token = rest
        .split(|c: char| c.is_whitespace() || matches!(c, '`' | '*' | '\\'))
        .find(|s| !s.is_empty())?;
    Some(token.to_string())
}

/// Page through the repo's issues and build `signature → issue_url`.
///
/// Reuses the bug-report GitHub config (token precedence + API base). Any issue
/// whose body carries a parseable `Signature:` line is mapped (findings are filed
/// without an `[harness]` title prefix, so we don't filter by title); pull
/// requests (returned by the same endpoint) are skipped. When two issues share a
/// signature, the lowest issue number (the original) wins.
async fn fetch_signature_map(
    http: &reqwest::Client,
    cfg: &GitHubBugConfig,
) -> Result<HashMap<String, String>> {
    let token = cfg.token.as_deref().ok_or_else(|| {
        HarnessError::Config(
            "no GitHub token (set GITHUB_TOKEN / GH_TOKEN / PARISH_BUG_REPORT_TOKEN) — \
             backfill needs API access to read issue bodies"
                .into(),
        )
    })?;

    // sig → (issue_number, url); keep the lowest number per signature.
    let mut best: HashMap<String, (u64, String)> = HashMap::new();
    let api_base = cfg.api_base.trim_end_matches('/');

    for page in 1..=50u32 {
        let url = format!(
            "{api_base}/repos/{}/issues?state=all&per_page=100&page={page}",
            cfg.repo
        );
        let resp = http
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "parish-harness-backfill")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|source| HarnessError::Transport {
                url: url.clone(),
                source,
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HarnessError::HttpStatus {
                method: "GET".into(),
                url,
                status: status.as_u16(),
                body,
            });
        }

        let text = resp
            .text()
            .await
            .map_err(|source| HarnessError::Transport {
                url: url.clone(),
                source,
            })?;
        let issues: Vec<Value> =
            serde_json::from_str(&text).map_err(|source| HarnessError::Json {
                context: format!("GET {url}"),
                source,
            })?;

        if issues.is_empty() {
            break;
        }
        let page_len = issues.len();

        for issue in &issues {
            // The issues endpoint returns PRs too — skip them.
            if issue.get("pull_request").is_some() {
                continue;
            }
            // Harness findings are filed (via `parish_file_bug`) with a title like
            // "<description> (<signature>)" — NOT an "[harness]" prefix — and a
            // `**Signature:** <sig>` line in the body. The signature is the
            // discriminator, so we match on the body and don't filter by title:
            // a non-finding issue simply has no `Signature:` line to parse.
            let body = issue.get("body").and_then(Value::as_str).unwrap_or("");
            let Some(html_url) = issue.get("html_url").and_then(Value::as_str) else {
                continue;
            };
            let number = issue
                .get("number")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX);
            if let Some(sig) = signature_from_body(body) {
                best.entry(sig)
                    .and_modify(|cur| {
                        if number < cur.0 {
                            *cur = (number, html_url.to_string());
                        }
                    })
                    .or_insert_with(|| (number, html_url.to_string()));
            }
        }

        // Last (partial) page reached.
        if page_len < 100 {
            break;
        }
    }

    Ok(best.into_iter().map(|(sig, (_, url))| (sig, url)).collect())
}

/// Apply a `signature → url` map to the unlinked findings, writing links unless
/// `dry_run`. Pure DB + map logic (no network) so it is unit-testable offline.
pub fn apply_signature_map(
    db: &Db,
    unlinked: &[(i64, String)],
    sig_map: &HashMap<String, String>,
    dry_run: bool,
) -> Result<BackfillReport> {
    let mut report = BackfillReport {
        unlinked: unlinked.len(),
        ..Default::default()
    };

    for (row_id, signature) in unlinked {
        match sig_map.get(signature) {
            Some(url) => {
                report.matched += 1;
                if dry_run {
                    println!("  [dry-run] finding {row_id} ({signature}) -> {url}");
                } else {
                    db.set_finding_issue_url(*row_id, url)?;
                    report.written += 1;
                    println!("  linked finding {row_id} ({signature}) -> {url}");
                }
            }
            None => println!("  no matching issue for finding {row_id} ({signature})"),
        }
    }

    Ok(report)
}

/// Run a full backfill pass: read unlinked findings, fetch the signature map
/// from GitHub, and apply it.
pub async fn backfill_issues(
    db: &Db,
    cfg: &GitHubBugConfig,
    dry_run: bool,
) -> Result<BackfillReport> {
    let unlinked = db.findings_needing_issue_url()?;
    if unlinked.is_empty() {
        println!("no findings need an issue link — nothing to do.");
        return Ok(BackfillReport::default());
    }

    let http = reqwest::Client::new();
    let sig_map = fetch_signature_map(&http, cfg).await?;
    println!(
        "scanned issues on {}: {} harness signatures mapped; {} findings unlinked",
        cfg.repo,
        sig_map.len(),
        unlinked.len()
    );

    let report = apply_signature_map(db, &unlinked, &sig_map, dry_run)?;
    println!(
        "backfill complete: {} matched, {} written{}",
        report.matched,
        report.written,
        if dry_run { " (dry-run, no writes)" } else { "" }
    );
    Ok(report)
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

    #[test]
    fn parses_signature_from_description_form() {
        let body = "Harness finding filed.\n\n**Evidence:** foo\n\n\
                    **Signature:** `character_fidelity:abc123`";
        assert_eq!(
            signature_from_body(body).as_deref(),
            Some("character_fidelity:abc123")
        );
    }

    #[test]
    fn parses_signature_from_log_form() {
        let body = "Run id: 5\nCategory: common_sense\nSignature: common_sense:xyz\nEvidence: q";
        assert_eq!(
            signature_from_body(body).as_deref(),
            Some("common_sense:xyz")
        );
    }

    #[test]
    fn parses_signature_from_production_filebug_form() {
        // The exact shape `parish_file_bug` writes (bold marker, no backticks).
        let body = "## Finding\n\n\
                    **Signature:** guard-override-incoherent-nonsequitur-known-npc\n\
                    **Turn:** 20\n**Severity:** medium";
        assert_eq!(
            signature_from_body(body).as_deref(),
            Some("guard-override-incoherent-nonsequitur-known-npc")
        );
    }

    #[test]
    fn parses_signature_with_escaped_backticks() {
        // As seen on real issues (#1487): the backticks are markdown-escaped, so
        // the body literally contains `\` before/after the slug.
        let body = "## Finding\n\n**Signature:** \\`degenerate-repetition-loop\\`\n**Turn:** 7";
        assert_eq!(
            signature_from_body(body).as_deref(),
            Some("degenerate-repetition-loop")
        );
    }

    #[test]
    fn no_signature_returns_none() {
        assert_eq!(signature_from_body("a normal issue body, no marker"), None);
    }

    #[test]
    fn apply_links_only_unlinked_matching_findings() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "sha", "/tmp/r").unwrap();

        // Unlinked → will match.
        let _a = db.insert_finding(rid, &finding("sig-a")).unwrap();
        // Already linked → must be left untouched (not in the unlinked set).
        let b = db.insert_finding(rid, &finding("sig-b")).unwrap();
        db.set_finding_issue_url(b, "https://github.com/x/y/issues/2")
            .unwrap();
        // Prior filing error → treated as unlinked, will match.
        let c = db.insert_finding(rid, &finding("sig-c")).unwrap();
        db.set_finding_issue_url(c, "filing-error: boom").unwrap();
        // Unlinked, no matching issue → stays unlinked.
        let _d = db.insert_finding(rid, &finding("sig-d")).unwrap();

        let unlinked = db.findings_needing_issue_url().unwrap();
        assert_eq!(
            unlinked.len(),
            3,
            "a, c, d need linking; b is already linked"
        );

        let mut map = HashMap::new();
        map.insert(
            "sig-a".into(),
            "https://github.com/x/y/issues/10".to_string(),
        );
        map.insert(
            "sig-c".into(),
            "https://github.com/x/y/issues/11".to_string(),
        );

        let report = apply_signature_map(&db, &unlinked, &map, false).unwrap();
        assert_eq!(report.unlinked, 3);
        assert_eq!(report.matched, 2);
        assert_eq!(report.written, 2);

        // Only sig-d remains unlinked; sig-b's link was never disturbed.
        let after: Vec<String> = db
            .findings_needing_issue_url()
            .unwrap()
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        assert_eq!(after, vec!["sig-d".to_string()]);
    }

    #[test]
    fn dry_run_matches_but_writes_nothing() {
        let db = Db::open_in_memory().unwrap();
        let cid = db.upsert_config(&cfg()).unwrap();
        let rid = db.start_run(cid, &git(), "sha", "/tmp/r").unwrap();
        let _ = db.insert_finding(rid, &finding("sig-a")).unwrap();

        let unlinked = db.findings_needing_issue_url().unwrap();
        let mut map = HashMap::new();
        map.insert(
            "sig-a".into(),
            "https://github.com/x/y/issues/10".to_string(),
        );

        let report = apply_signature_map(&db, &unlinked, &map, true).unwrap();
        assert_eq!(report.matched, 1);
        assert_eq!(report.written, 0);
        // Still unlinked — dry-run wrote nothing.
        assert_eq!(db.findings_needing_issue_url().unwrap().len(), 1);
    }
}
