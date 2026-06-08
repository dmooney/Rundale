//! Git/GitHub provenance for a run.
//!
//! Captured once at run start from read-only subprocesses so a run's quality
//! number can be attributed to a commit. `dirty` is load-bearing: a dirty tree
//! means the score can't be cleanly attributed, so the dashboard excludes dirty
//! runs from regression deltas (but still shows them).

use std::process::Command;

/// Git + PR identity at the moment a run started.
#[derive(Debug, Clone)]
pub struct GitProvenance {
    pub sha: String,
    pub branch: String,
    pub dirty: bool,
    pub pr_number: Option<i64>,
}

impl GitProvenance {
    /// Capture provenance by shelling out to `git` (and best-effort `gh`).
    /// Never fails: missing git / detached states degrade to placeholder
    /// values so a run is always recordable.
    pub fn capture() -> GitProvenance {
        GitProvenance {
            sha: run_git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string()),
            branch: run_git(&["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_else(|| "unknown".to_string()),
            dirty: run_git(&["status", "--porcelain"])
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            pr_number: run_gh_pr_number(),
        }
    }
}

/// Run a `git` subcommand and return trimmed stdout on success.
fn run_git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Best-effort PR number via `gh pr view`. Returns `None` when not on a PR
/// branch or `gh` is unavailable — exactly the graceful degradation the issue
/// filer relies on.
fn run_gh_pr_number() -> Option<i64> {
    let out = Command::new("gh")
        .args(["pr", "view", "--json", "number", "-q", ".number"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_never_panics_and_yields_nonempty_fields() {
        // Runs inside the repo under test; sha/branch should resolve, but even
        // outside a repo the placeholders keep the run recordable.
        let p = GitProvenance::capture();
        assert!(!p.sha.is_empty());
        assert!(!p.branch.is_empty());
    }
}
