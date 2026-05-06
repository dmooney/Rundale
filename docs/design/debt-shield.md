# Debt Shield: Autonomous Technical Debt Prevention & Remediation System

## Overview

Debt Shield is a multi-layered, automated system that prevents the accumulation of technical debt and remediates existing debt in the Parish codebase. It combines pre-commit hooks, CI lint gates, an adversarial LLM-powered PR reviewer, quantitative debt measurement with SQALE ratios, and an autonomous detect-fix-land pipeline.

## Architecture

```
+-------------------------------------------------------------+
|                    LAYER 0: PREVENTION                       |
|  Developer types code -> pre-commit hooks catch debt locally  |
+-------------------------------------------------------------+
|                    LAYER 1: PR GATING                        |
|  PR opened -> diff-only linting + adversarial agent review   |
|  Blocker/Critical findings -> merge blocked                   |
+-------------------------------------------------------------+
|                    LAYER 2: MEASUREMENT                      |
|  Weekly cron -> full codebase scan -> SQALE debt ratio ->    |
|  DEBT_REPORT.md + tracking issue + categorized GitHub issues |
+-------------------------------------------------------------+
|                    LAYER 3: AUTONOMOUS PIPELINE              |
|  New debt issue -> Claude Code fixes it -> opens PR ->       |
|  CI passes -> land agent squash-merges                        |
+-------------------------------------------------------------+
|                    LAYER 4: ARCHITECTURAL                    |
|  Custom lint rules + expanded fitness tests prevent drift    |
+-------------------------------------------------------------+
```

## Phase Ordering (Pipeline-First)

```
Phase 3 (Autonomous Pipeline) -> Phase 2 (Adversarial Agent) -> Phase 0 (Foundation)
-> Phase 1 (Measurement) -> Phase 4 (Architectural)
```

The novel pieces (adversarial agent, autonomous pipeline) are built first because they are the highest-risk / highest-impact components. Foundation (pre-commit hooks, clippy config) and measurement (SQALE ratio, trending) are commodity and built after.

## Hyperaggressive Posture

| Dimension | Standard | Hyperaggressive |
|---|---|---|
| Auto-land scope | Simple fixes only | All non-logic changes: format, lint fixes, dead code removal, doc additions, dep patches, `#[allow]` cleanup, clippy fixes |
| Adversarial agent trigger | On-demand (`@debt-review`) | Every PR, every push, gating (blocker/critical block merge) |
| Debt ratio CI threshold | Report only | Fail CI if ratio > 10% or ratio increases by >2% since last scan |
| Clippy posture | `-W clippy::pedantic` with targeted allows | `-D clippy::pedantic` at crate root, only explicit `#[allow]` with comments permitted |
| Pre-commit | Warn | Block (reject commits that fail checks) |
| Adversarial agent skepticism | Balanced review | Hostile critic -- assume nothing acceptable until proven otherwise |

---

## Technical Debt Taxonomy (Agent Reference)

The adversarial agent and debt scanner use this exhaustive taxonomy to classify findings.

### By Intent (Fowler Quadrant)

| Quadrant | Description |
|---|---|
| **Prudent & Deliberate** | Conscious choice with a concrete repayment plan (e.g., "we'll refactor after launch") |
| **Reckless & Deliberate** | Conscious choice with negligence -- no plan to repay |
| **Prudent & Inadvertent** | Retroactive discovery -- "now we know how it should have been designed" |
| **Reckless & Inadvertent** | Accrued through ignorance or lack of experience |

### By Nature (Concrete Manifestations)

| Category | Sub-Type | Manifestation (Principal) | Interest Payment |
|---|---|---|---|
| **Code Debt** | Duplication | Cloned logic with slight variations | Bug fix must be applied in multiple places, inevitably missing one |
| | Complexity / God Objects | Cyclomatic complexity > 15, Single Responsibility Principle violations, files > 600 lines | Extreme difficulty understanding, testing, and modifying; high cognitive load |
| | Poor Naming | Non-semantic variable/class names (`tmp`, `data`, `obj1`) | Code is unreadable; mental mapping between domain and code is impossible |
| | Comment Debt | Outdated/misleading comments or commented-out code blocks | False narrative leads developers to misunderstand actual behavior |
| **Design/Architecture Debt** | Structural Erosion | Violations of defined architectural patterns | Architecture invariants (scalability, modularity) silently break down |
| | Tight Coupling | Change in module A always requires cascading changes in B and C | Cost of change becomes non-linear |
| | Technical Silos | Critical component in legacy language no one on team knows | Changes are high-risk, slow, dependent on external expertise |
| **Testing Debt** | Lack of Tests | No unit, integration, or end-to-end tests | Refactoring is gambling; releases are manual and terrifying |
| | Slow/Brittle Tests | Test suite takes hours or tests fail randomly (flaky) | Loss of trust in test suite; developers ignore failures |
| | Inadequate Coverage | Happy-path testing only; no edge cases or error handling | Production failures for predictable scenarios |
| **Documentation Debt** | Missing/Outdated Docs | API specs don't match implementation; no ADRs | Onboarding time explodes; decisions re-litigated |
| **Infrastructure Debt** | Manual Processes | Manual build, deployment, environment provisioning | Deployments are high-ceremony, high-risk events |
| | Outdated Dependencies | End-of-life framework or unmaintained third-party library | Accumulating security vulnerabilities; upgrade cost grows exponentially |
| **Knowledge Debt** | Tribal Knowledge | Only one person understands a critical subsystem | Key-person risk; work grinds to a halt if they leave |

### Measurement & Quantification

**Debt Principal (Proxy Metrics)**
- **Cyclomatic Complexity**: Values > 10-15 indicate testability/understandability debt
- **Coupling Between Objects (CBO)**: High counts signal architectural rigidity
- **Duplication Percentage**: % of duplicated lines (from tools like SonarQube)
- **Rule Violations**: Count of violations of coding standards, weighted by severity
- **Test Coverage**: % of lines/branches/paths covered, paired with mutation testing scores

**Debt Interest (Effort & Risk Impact)**
- **Issue Resolution Rate Variance**: How much slower developers resolve bugs in high-debt vs. low-debt modules
- **Feature Lead Time**: Total time from idea to production -- growing lead time = interest payment
- **Defect Escape Rate**: Defects found in production vs. pre-production

**SQALE Debt Ratio Formula**
```
debt_ratio = (cost_to_fix_all_issues / estimated_development_cost) * 100
```
- **Cost to Fix All Issues**: Sum of estimated fix minutes for all lint/smell findings
- **Estimated Development Cost**: 30 minutes per line of code (configurable)
- **Thresholds**: A < 5%, B < 10%, C < 20%, D < 50%, E >= 50%

---

## Phase 3: Autonomous Pipeline

**Goal**: Issues created by the debt scanner are automatically fixed by Claude Code agents, PR'd, verified, and merged -- with zero human intervention for allowed change categories.

### Flow

```
Weekly Debt Scanner
       |
       v
Creates GitHub issues (label: debt, auto-fixable)
       |
       |  issues: [opened] event, filtered to label:debt + auto-fixable
       v
Debt Fix Dispatcher (.github/workflows/debt-fix.yml)
  - Checks out repo
  - Invokes Claude Code in CI (anthropics/claude-code-action@v1):
    "Diagnose and fix issue #N. Implement the minimal fix. Add tests. Open a PR."
  - Claude Code reads the issue, implements fix, runs tests,
    creates a PR via gh CLI
  - Labels PR with auto-debt-fix, references the issue
  - Branch: debt-fix/issue-<N>
       |
       |  pull_request: [opened] event, filtered to label:auto-debt-fix
       v
Debt Land (.github/workflows/debt-land.yml)
  - Waits for all CI checks to pass (poll check_runs API)
  - Classifies PR changes into "allowed" or "risky" categories
  - If all green AND all changes in "allowed" categories:
    -> squash-merge via gh pr merge --squash --auto
  - If any changes in "risky" categories:
    -> post comment requesting human review, add label needs-review
```

### Change Classification for Auto-Land

| Auto-Land Allowed | Requires Human Review |
|---|---|
| Clippy warning fixes | Logic/behavior changes |
| `#[allow]` cleanup | Architecture/API changes |
| Formatting fixes (`cargo fmt`) | New abstractions or traits |
| Dependency patch bumps (0.x.Y, x.y.Z) | Major version upgrades |
| Dead code removal | Module relocations |
| Doc comment additions/ fixes | New feature gating |
| Conventional `#[allow(clippy::*)]` additions with comments | Config changes |
| Witness marker removal (`todo!` / `unimplemented!` calls) | New dependencies added |
| Simple renames (no logic change) | Test fixture changes |

### Safety Mechanisms

1. **Category gating**: Only issues labeled `auto-fixable` by the debt scanner enter the pipeline. The scanner applies conservative heuristics to decide what's safe.
2. **Turn limit**: Fix workflow capped at `--max-turns 15` (`--max-turns 25` for complex fixes) to prevent runaway loops.
3. **CI verification**: Land workflow polls `gh api /repos/{owner}/{repo}/commits/{sha}/check-runs` until all complete and all pass. Does not proceed on any failure.
4. **Change classification**: Land workflow parses the PR diff and classifies each hunk. Any hunk falling outside "allowed" categories flags the entire PR for human review.
5. **Branch naming convention**: `debt-fix/issue-<N>` provides traceability back to the originating issue.
6. **Budget cap**: `--max-budget-usd 2.00` per fix to prevent cost overruns.
7. **Rollback ready**: Auto-merged PRs are squashed to a single commit. A revert is one click.

### File: `.github/workflows/debt-fix.yml`

```yaml
name: Debt Fix Dispatcher

on:
  issues:
    types: [opened, labeled]

jobs:
  dispatch:
    if: |
      github.event.action == 'labeled' &&
      contains(github.event.issue.labels.*.name, 'debt') &&
      contains(github.event.issue.labels.*.name, 'auto-fixable')
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
      issues: write
    steps:
      - uses: actions/checkout@v4
      - name: Generate app token
        id: app-token
        uses: actions/create-github-app-token@v3
        with:
          app-id: ${{ vars.CLAUDE_APP_ID }}
          private-key: ${{ secrets.CLAUDE_APP_PRIVATE_KEY }}
      - uses: anthropics/claude-code-action@v1
        with:
          anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
          github_token: ${{ steps.app-token.outputs.token }}
          prompt: |
            This is an automated debt fix dispatch for issue #${{ github.event.issue.number }}.

            1. Read the issue at ${{ github.event.issue.html_url }} to understand the debt.
            2. Diagnose the root cause using the /five-whys method.
            3. Implement the minimal fix. Follow Parish conventions in docs/agent/code-style.md and docs/agent/gotchas.md.
            4. Run `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` to verify.
            5. Create a PR:
               - Branch: debt-fix/issue-${{ github.event.issue.number }}
               - Title: "fix: [debt] <summary>"
               - Body: "Closes #${{ github.event.issue.number }}"
               - Label: auto-debt-fix
            6. Do NOT change any behavior. Only fix the specific debt described in the issue.
          claude_args: "--max-turns 15 --max-budget-usd 2.00 --output-format json --permission-mode bypassPermissions"
```

### File: `.github/workflows/debt-land.yml`

```yaml
name: Debt Land

on:
  pull_request:
    types: [opened, synchronize, labeled]

jobs:
  land:
    if: |
      contains(github.event.pull_request.labels.*.name, 'auto-debt-fix') &&
      github.event.action != 'labeled'
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
      checks: read
    steps:
      - uses: actions/checkout@v4
      - name: Generate app token
        id: app-token
        uses: actions/create-github-app-token@v3
        with:
          app-id: ${{ vars.CLAUDE_APP_ID }}
          private-key: ${{ secrets.CLAUDE_APP_PRIVATE_KEY }}

      - name: Wait for CI
        run: |
          # Poll check runs for this commit until all complete
          for i in $(seq 1 30); do
            STATUS=$(gh api "repos/${{ github.repository }}/commits/${{ github.event.pull_request.head.sha }}/check-runs" \
              --jq '.check_runs | map(select(.status != "completed")) | length')
            if [ "$STATUS" = "0" ]; then
              CONCLUSION=$(gh api "repos/${{ github.repository }}/commits/${{ github.event.pull_request.head.sha }}/check-runs" \
                --jq '[.check_runs[].conclusion] | unique')
              if echo "$CONCLUSION" | grep -q "failure\|cancelled\|timed_out"; then
                echo "CI failed -- will not auto-land"
                exit 1
              fi
              echo "All CI checks passed"
              exit 0
            fi
            echo "Waiting for CI... ($i/30)"
            sleep 30
          done
          echo "CI timed out"
          exit 1
        env:
          GH_TOKEN: ${{ steps.app-token.outputs.token }}

      - name: Classify PR changes
        id: classify
        run: |
          # Fetch the PR diff
          gh pr diff ${{ github.event.pull_request.number }} --repo ${{ github.repository }} > /tmp/pr.diff

          # Check for risky patterns
          RISKY=0
          # Logic changes (function body changes beyond formatting)
          if grep -P '^[-+]\s*(?!\s*//|\s*\*|use\s|#\[|pub\s|fn\s|let\s|if\s|match\s|for\s|while\s|return\s|Ok\(|Err\()' /tmp/pr.diff | head -1; then
            RISKY=1
          fi
          # New dependencies
          if grep -P '^\+.*Cargo\.toml.*\n\+.*=' /tmp/pr.diff; then
            RISKY=1
          fi
          # Module changes
          if grep -P '^[-+].*mod\s|^[-+].*pub\smod' /tmp/pr.diff; then
            RISKY=1
          fi

          if [ "$RISKY" = "1" ]; then
            echo "classification=risky" >> $GITHUB_OUTPUT
          else
            echo "classification=allowed" >> $GITHUB_OUTPUT
          fi
        env:
          GH_TOKEN: ${{ steps.app-token.outputs.token }}

      - name: Auto-land or flag
        run: |
          if [ "${{ steps.classify.outputs.classification }}" = "allowed" ]; then
            gh pr merge ${{ github.event.pull_request.number }} \
              --repo ${{ github.repository }} \
              --squash \
              --auto \
              --delete-branch
            echo "Auto-landed debt-fix PR"
          else
            gh pr edit ${{ github.event.pull_request.number }} \
              --repo ${{ github.repository }} \
              --add-label "needs-review"
            gh pr comment ${{ github.event.pull_request.number }} \
              --repo ${{ github.repository }} \
              --body "## Debt Land: Human Review Required

            This PR contains changes classified as **risky** (logic changes, new dependencies,
            or module restructuring). Please review manually before merging."
            echo "Flagged for human review"
          fi
        env:
          GH_TOKEN: ${{ steps.app-token.outputs.token }}
```

---

## Phase 2: Adversarial Agent

**Goal**: An LLM agent loaded with the exhaustive debt taxonomy and Parish conventions reviews every PR as a hostile critic. Blocker/Critical findings prevent merge.

### Skill: `.agents/skills/debt-review/SKILL.md`

```
---
name: debt-review
description: >
  Adversarial technical debt auditor. Reviews PRs with a hostile, skeptical posture
  against the full technical debt taxonomy. Blocker/Critical findings gate the PR.
  Trigger: runs automatically on every PR, or on-demand via @debt-review comment.
---

# Adversarial Technical Debt Review

## Role

You are an **adversarial technical debt auditor**. Your sole purpose is to find every
instance of technical debt in the code under review. You are hostile, skeptical, and
assume nothing is acceptable until proven otherwise.

Your review is **gating**: Blocker and Critical severity findings will prevent the PR
from being merged until resolved.

## Pre-Flight

Before reviewing the PR diff, load these reference documents:

1. `docs/agent/code-style.md` -- Rust and Svelte conventions
2. `docs/agent/gotchas.md` -- Tokio, SQLite, Ollama, mode parity pitfalls
3. `docs/agent/architecture.md` -- Workspace layout and module ownership
4. `docs/design/debt-shield.md` -- This document (the exhaustive debt taxonomy)

## Review Categories

For each of the following categories, examine the PR diff and report ALL findings:

### 1. Code Debt
- **Duplication**: Does the added code duplicate logic already present elsewhere in the codebase?
  Search the repo for similar patterns before concluding.
- **Complexity**: New functions with apparent cognitive complexity > 15. Files approaching or
  exceeding 600 lines. Functions approaching or exceeding 50 lines.
- **Poor Naming**: Non-semantic names (`data`, `tmp`, `obj`, `handle`, `info`, `result`).
  Names that don't match the domain language (Irish historical world, 1820).
- **Comment Debt**: Outdated comments in modified areas (comments that describe behavior
  the code no longer implements). Commented-out code blocks. TODO comments without issue references.

### 2. Design / Architecture Debt
- **Structural Erosion**: New code that violates Parish's module ownership rules.
  Leaf crate logic placed in `parish-cli/src/`. Backend-agnostic crates depending on
  tauri/axum/tower/wry/tao.
- **Tight Coupling**: Changes that create new dependencies between crates that should be
  independent. Changes in one module that force coordinated changes in unrelated modules.
- **Technical Silos**: New subsystems that only one author could understand. Overly
  clever or obscure patterns.

### 3. Testing Debt
- **Missing Tests**: New functions, methods, or behavior without corresponding tests.
  New error paths without test coverage. New public API without integration tests.
- **Brittle Patterns**: Tests that sleep, tests that depend on ordering, tests without
  assertions, tests that only cover happy paths.
- **Inadequate Coverage**: Error handling paths without tests. Edge cases not covered.

### 4. Documentation Debt
- **Missing Docs**: New public items without `///` doc comments. New features without
  updates to relevant `docs/` files. Changed behavior without corresponding doc updates.
- **Outdated Docs**: The PR changes behavior documented elsewhere but doesn't update that doc.

### 5. Infrastructure Debt
- **Dependency Changes**: New dependencies added without `just notices`. Dependency
  version pins without justification.
- **Config Drift**: Hardcoded values that should be configurable. Feature flags missing
  `config.flags.is_enabled()` gating.

### 6. Knowledge Debt
- **Tribal Knowledge**: Patterns suggesting only the PR author understands the change.
  Unusual patterns without explanatory comments. Magic numbers or obscure constants.

### 7. Parish-Specific Anti-Patterns
- `unwrap()` / `expect()` in library crates (should propagate errors)
- `println!` / `eprintln!` in library crates (use `tracing` macros)
- `std::thread::sleep` anywhere (use `tokio::time::sleep`)
- `current_dir()` or parent-walk patterns (use `AppState` runtime paths)
- `#[allow(clippy::*)]` without a justifying comment
- `reqwest::get()` or `reqwest::Client::new()` without an explicit timeout
- Module ownership violation: leaf crate logic in `parish-cli/src/`

## Severity Classification

| Severity | Criteria | Action |
|---|---|---|
| **Blocker** | Security vulnerability, data loss risk, architecture violation that will cause cascading failures, `unwrap()` in library code, missing timeout on HTTP. | **Block merge.** Comment with `severity: blocker` label. |
| **Critical** | Missing tests for new behavior, missing docs for new public API, `#[allow]` without comment, module ownership violation. | **Block merge.** Comment with `severity: critical` label. |
| **Major** | High cognitive complexity, tight coupling, brittle test patterns, poor naming in public API. | **Warn prominently.** Comment with `severity: major` label. |
| **Minor** | Style deviation, missing doc on private items, comment debt. | **Informational.** Comment with `severity: minor` label. |

## Output Format

After reviewing, output a structured summary:

```json
{
  "findings": [
    {
      "category": "code_debt.complexity",
      "severity": "major",
      "file": "parish/crates/parish-core/src/game_loop/reactions.rs",
      "lines": "88-145",
      "description": "The `process_reactions` function has cyclomatic complexity of approximately 18 due to nested match arms. Consider extracting reaction-type-specific handlers.",
      "interest_payment": "Each new reaction type requires modifying this function, increasing the risk of bugs in existing reactions. The function is already 60 lines and growing.",
      "remediation": "Extract each reaction handler into a separate function and dispatch via a HashMap or match on an enum."
    }
  ],
  "summary": {
    "blocker_count": 0,
    "critical_count": 1,
    "major_count": 2,
    "minor_count": 4,
    "verdict": "blocked"
  }
}
```

## Verdict

- **`blocked`**: One or more blocker or critical findings. PR must be revised.
- **`approved_with_warnings`**: Only major/minor findings. PR can proceed but warnings should be addressed.
- **`approved`**: No findings. (Rare -- you are an adversarial auditor.)

---

## Integration Notes

- This skill is loaded via the `anthropics/claude-code-action@v1` GitHub Action in `.github/workflows/debt-review.yml`.
- The action is triggered automatically on `pull_request: [opened, synchronize]`.
- It can also be triggered manually via PR comment `@debt-review`.
- The review check is added as a **required status check** in branch protection rules.
- Blocker/Critical findings cause the check to fail (exit code 1), which blocks merge.
- Major/Minor findings post as PR review comments but do not fail the check.
```

### Workflow: `.github/workflows/debt-review.yml`

```yaml
name: Adversarial Debt Review

on:
  pull_request:
    types: [opened, synchronize]
  issue_comment:
    types: [created]

jobs:
  review:
    if: |
      (github.event_name == 'pull_request') ||
      (github.event_name == 'issue_comment' &&
       contains(github.event.comment.body, '@debt-review') &&
       github.event.issue.pull_request != null)
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write
      issues: write
    concurrency:
      group: debt-review-${{ github.event.pull_request.number || github.event.issue.number }}
      cancel-in-progress: true
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Need full history for searching existing patterns

      - name: Get PR number
        id: pr
        run: |
          if [ "${{ github.event_name }}" = "pull_request" ]; then
            echo "number=${{ github.event.pull_request.number }}" >> $GITHUB_OUTPUT
          else
            echo "number=${{ github.event.issue.number }}" >> $GITHUB_OUTPUT
          fi

      - uses: anthropics/claude-code-action@v1
        with:
          anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
          github_token: ${{ secrets.GITHUB_TOKEN }}
          prompt: |
            /debt-review Evaluate PR #${{ steps.pr.outputs.number }} for all forms of
            technical debt. Load the full debt taxonomy from docs/design/debt-shield.md.
            Load Parish conventions from docs/agent/code-style.md, docs/agent/gotchas.md,
            and docs/agent/architecture.md.

            Be hostile. Assume nothing is acceptable until proven otherwise.

            Return structured findings with severity (blocker/critical/major/minor).
            Blocker and critical findings must gate the PR.
          claude_args: "--max-turns 10 --output-format json"

      - name: Post findings as PR review
        if: always()
        run: |
          # If the agent found blocker or critical issues, fail the check
          FINDINGS=$(cat /tmp/debt-review-output.json 2>/dev/null || echo '{}')
          BLOCKER_COUNT=$(echo "$FINDINGS" | jq -r '.summary.blocker_count // 0')
          CRITICAL_COUNT=$(echo "$FINDINGS" | jq -r '.summary.critical_count // 0')

          if [ "$BLOCKER_COUNT" -gt 0 ] || [ "$CRITICAL_COUNT" -gt 0 ]; then
            echo "Debt review found $BLOCKER_COUNT blocker(s) and $CRITICAL_COUNT critical issue(s)"
            exit 1
          fi
          echo "Debt review: no blocker or critical findings"
```

---

## Phase 0: Foundation (Prevention)

**Goal**: Close the human-bypass gap (humans committing directly skip all Claude Code hooks) and set project-specific clippy thresholds.

### File: `.pre-commit-config.yaml`

```yaml
repos:
  - repo: local
    hooks:
      - id: fmt
        name: cargo fmt
        entry: cargo fmt --check
        language: system
        files: \.rs$
        fail_fast: true

      - id: clippy
        name: cargo clippy
        entry: cargo clippy --workspace --all-targets -- -D warnings
        language: system
        files: \.rs$
        pass_filenames: false

      - id: witness-scan
        name: witness scan
        entry: bash parish/scripts/witness-scan.sh
        language: system
        files: \.rs$

      - id: parish-lint
        name: parish custom lint
        entry: bash parish/scripts/parish-lint.sh
        language: system
        files: \.rs$

      - id: check-doc-paths
        name: check doc paths
        entry: bash parish/scripts/check-doc-paths.sh
        language: system
        pass_filenames: false

      - id: secrets
        name: no secrets
        entry: bash parish/scripts/check-secrets.sh
        language: system
        stages: [commit]

      - id: large-files
        name: no large files
        entry: bash parish/scripts/check-large-files.sh
        language: system
        stages: [commit]

  - repo: https://github.com/compilerla/conventional-pre-commit
    rev: v3.2.0
    hooks:
      - id: conventional-pre-commit
        stages: [commit-msg]
        args: [feat, fix, refactor, docs, test, chore, style, perf, ci, build, revert]
```

### File: `parish/clippy.toml`

```toml
# Clippy configuration for the Parish engine
# Hyperaggressive posture: tighten all thresholds

# Cognitive complexity -- functions exceeding this are considered too complex
cognitive-complexity-threshold = 25

# Too many arguments -- functions exceeding this need refactoring
too-many-arguments-threshold = 7

# Too many lines -- files exceeding this should be split
too-many-lines-threshold = 600

# Type complexity -- types exceeding this need simplification
type-complexity-threshold = 250

# Large enum variants -- variants exceeding this cause stack allocation issues
# (already part of clippy::large_enum_variant)
enum-variant-size-threshold = 200

# Large stack arrays
array-size-threshold = 512000

# Allow some known-heavy types
# (add as needed if specific types legitimately need more complexity)
```

### Crate-Root Lint Attributes

Add to every library crate's `src/lib.rs`:

```rust
// Clippy hyperaggressive posture
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
// Explicit allows with justification only
// #[allow(clippy::module_name_repetitions)] // justified: ...
```

### Diff-Only Linting (add to `ci.yml`)

The existing `ci.yml` runs `cargo clippy --workspace --all-targets -- -D warnings` which flags pre-existing debt in files that weren't changed. Add a diff-only mode:

```yaml
# In rust-quality-gate job, add:
- name: Diff-only clippy (only flag new issues)
  if: github.event_name == 'pull_request'
  uses: reviewdog/action-clippy@v1
  with:
    github_token: ${{ secrets.GITHUB_TOKEN }}
    reporter: github-pr-review
    clippy_flags: '--workspace --all-targets -- -D warnings'
    filter_mode: diff_context
    fail_on_error: true
```

### Helper Scripts

**`parish/scripts/check-commit-msg.sh`** -- validates conventional commit format in the commit message file.

**`parish/scripts/check-secrets.sh`** -- scans staged files for common secret patterns (API keys, tokens, passwords).

**`parish/scripts/check-large-files.sh`** -- rejects commits adding files over 500KB.

---

## Phase 1: Measurement (Debt Scanning)

**Goal**: Quantitative debt measurement with SQALE ratios, trending, and automated issue creation. Fail CI if debt ratio exceeds 10% or increases by more than 2% since last baseline.

### File: `.github/workflows/debt-scanner.yml`

```yaml
name: Debt Scanner

on:
  schedule:
    - cron: '0 3 * * 0'  # Sunday 3 AM UTC
  workflow_dispatch:

jobs:
  scan:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      issues: write
      pull-requests: write
    env:
      DEBT_THRESHOLD_PCT: 10
      DEBT_INCREASE_THRESHOLD_PCT: 2
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.95.0

      - name: Run debt scanner
        id: scan
        run: bash parish/scripts/debt-scanner.sh
        env:
          DEBT_REPORT_PATH: DEBT_REPORT.md
          BASELINE_PATH: .github/debt-baseline.json

      - name: Check thresholds
        run: |
          RATIO=$(jq -r '.debt_ratio' /tmp/debt-scan-result.json)
          PREV_RATIO=$(jq -r '.debt_ratio // 0' .github/debt-baseline.json 2>/dev/null || echo "0")

          echo "Current debt ratio: ${RATIO}%"
          echo "Previous debt ratio: ${PREV_RATIO}%"

          if (( $(echo "$RATIO > $DEBT_THRESHOLD_PCT" | bc -l) )); then
            echo "::error::Debt ratio ${RATIO}% exceeds threshold of ${DEBT_THRESHOLD_PCT}%"
            exit 1
          fi

          INCREASE=$(echo "$RATIO - $PREV_RATIO" | bc -l)
          if (( $(echo "$INCREASE > $DEBT_INCREASE_THRESHOLD_PCT" | bc -l) )); then
            echo "::error::Debt ratio increased by ${INCREASE}% (threshold: ${DEBT_INCREASE_THRESHOLD_PCT}%)"
            exit 1
          fi

          echo "Debt ratio within acceptable bounds"

      - name: Update baseline
        run: cp /tmp/debt-scan-result.json .github/debt-baseline.json

      - name: Commit DEBT_REPORT.md and baseline
        run: |
          git config user.name "debt-shield[bot]"
          git config user.email "debt-shield[bot]@users.noreply.github.com"
          git add DEBT_REPORT.md .github/debt-baseline.json
          git diff --staged --quiet || git commit -m "chore: update debt report and baseline"
          git push

      - name: Create/update tracking issue
        run: |
          ISSUE_NUMBER=$(gh issue list --label "debt-tracking" --limit 1 --json number -q '.[0].number' 2>/dev/null || echo "")
          RATIO=$(jq -r '.debt_ratio' /tmp/debt-scan-result.json)
          RATING=$(jq -r '.rating' /tmp/debt-scan-result.json)
          CATEGORY_COUNTS=$(jq -r '.categories | to_entries | map("\(.key): \(.value)") | join(", ")' /tmp/debt-scan-result.json)

          BODY="## Debt Scan $(date +%Y-%m-%d)

          **Debt Ratio:** ${RATIO}% (Rating: ${RATING})
          **Categories:** ${CATEGORY_COUNTS}

          Full report: [DEBT_REPORT.md](DEBT_REPORT.md)"

          if [ -z "$ISSUE_NUMBER" ]; then
            gh issue create --title "Debt Tracker" --body "$BODY" --label "debt-tracking"
          else
            gh issue comment "$ISSUE_NUMBER" --body "$BODY"
          fi
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}

      - name: Create debt issues
        run: bash parish/scripts/debt-issue-creator.sh
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### Script: `parish/scripts/debt-scanner.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# Debt Scanner: runs clippy with pedantic/restriction, parses JSON output,
# calculates SQALE debt ratio, and outputs structured results.

OUTPUT_JSON="/tmp/debt-scan-result.json"
REPORT_PATH="${DEBT_REPORT_PATH:-DEBT_REPORT.md}"
LOC=$(tokei --output json parish/crates/ | jq '.Rust.code // 0')
PREV_RATIO=$(jq -r '.debt_ratio // 0' .github/debt-baseline.json 2>/dev/null || echo "0")

echo "=== Debt Scanner ==="
echo "Lines of Rust code: $LOC"
echo ""

# Run clippy with pedantic + restriction, output JSON
# Restriction lints are allow-by-default and very aggressive -- we enable
# a curated subset rather than the full group
echo "Running clippy..."
cargo clippy --workspace --all-targets --message-format=json \
  -W clippy::pedantic \
  -W clippy::nursery \
  -W clippy::cargo \
  -W clippy::restriction 2>/dev/null | \
  jq -s '[.[] | select(.reason == "compiler-message") | .message]' > /tmp/clippy-output.json

# Categorize findings by severity
# clippy::style -> minor (1 min each)
# clippy::complexity -> major (10 min each)
# clippy::perf -> major (5 min each)
# clippy::correctness -> blocker (15 min each)
# clippy::suspicious -> critical (15 min each)
# clippy::pedantic -> minor (2 min each)
# clippy::cargo -> minor (2 min each)
# clippy::nursery -> major (5 min each)
# clippy::restriction -> minor (1 min each)
declare -A FIX_MINUTES
FIX_MINUTES[correctness]=15
FIX_MINUTES[suspicious]=15
FIX_MINUTES[complexity]=10
FIX_MINUTES[perf]=5
FIX_MINUTES[nursery]=5
FIX_MINUTES[style]=1
FIX_MINUTES[pedantic]=2
FIX_MINUTES[cargo]=2
FIX_MINUTES[restriction]=1
FIX_MINUTES[default]=2

# Count by category
declare -A CATEGORY_COUNTS
TOTAL_FIX_MINUTES=0

while IFS= read -r lint; do
  CODE=$(echo "$lint" | jq -r '.code.code // "unknown"')
  CATEGORY=$(echo "$CODE" | cut -d':' -f2)
  FIX_MINS=${FIX_MINUTES[$CATEGORY]:-${FIX_MINUTES[default]}}
  TOTAL_FIX_MINUTES=$((TOTAL_FIX_MINUTES + FIX_MINS))
  CATEGORY_COUNTS[$CATEGORY]=$((CATEGORY_COUNTS[$CATEGORY] + 1))
done < <(jq -c '.[]' /tmp/clippy-output.json)

# Calculate SQALE debt ratio
# debt_ratio = (fix_minutes / (30 min/LOC * LOC)) * 100
if [ "$LOC" -gt 0 ]; then
  DEBT_RATIO=$(echo "scale=2; ($TOTAL_FIX_MINUTES / (0.5 * $LOC)) * 100" | bc -l)
else
  DEBT_RATIO=0
fi

# Calculate rating
if (( $(echo "$DEBT_RATIO < 5" | bc -l) )); then
  RATING="A"
elif (( $(echo "$DEBT_RATIO < 10" | bc -l) )); then
  RATING="B"
elif (( $(echo "$DEBT_RATIO < 20" | bc -l) )); then
  RATING="C"
elif (( $(echo "$DEBT_RATIO < 50" | bc -l) )); then
  RATING="D"
else
  RATING="E"
fi

TREND=$(echo "$DEBT_RATIO - $PREV_RATIO" | bc -l)
if (( $(echo "$TREND > 0.5" | bc -l) )); then
  TREND_ICON=":arrow_up:"
elif (( $(echo "$TREND < -0.5" | bc -l) )); then
  TREND_ICON=":arrow_down:"
else
  TREND_ICON=":arrow_right:"
fi

echo "Debt ratio: ${DEBT_RATIO}% (Rating: ${RATING}) ${TREND_ICON}"
echo "Total fix minutes: ${TOTAL_FIX_MINUTES}"
echo "Total issues: $(jq 'length' /tmp/clippy-output.json)"

# Output structured JSON for downstream consumers
jq -n \
  --argjson ratio "$DEBT_RATIO" \
  --arg rating "$RATING" \
  --argjson fix_minutes "$TOTAL_FIX_MINUTES" \
  --argjson loc "$LOC" \
  --argjson prev_ratio "$PREV_RATIO" \
  --arg trend "$(printf '%.2f' "$TREND")" \
  --argjson categories "$(for k in "${!CATEGORY_COUNTS[@]}"; do echo "$k: ${CATEGORY_COUNTS[$k]}"; done | jq -R 'split(": ") | {(.[0]): (.[1] | tonumber)}' | jq -s add)" \
  '{debt_ratio: $ratio, rating: $rating, fix_minutes: $fix_minutes, loc: $loc, prev_ratio: $prev_ratio, trend: $trend, categories: $categories}' \
  > "$OUTPUT_JSON"

# Generate DEBT_REPORT.md
{
  echo "# Technical Debt Report"
  echo ""
  echo "**Generated:** $(date '+%Y-%m-%d %H:%M UTC')"
  echo ""
  echo "## Summary"
  echo ""
  echo "| Metric | Value |"
  echo "|---|---|"
  echo "| Debt Ratio | ${DEBT_RATIO}% |"
  echo "| Rating | ${RATING} |"
  echo "| Trend | ${TREND_ICON} ${TREND}% |"
  echo "| Estimated Fix Minutes | ${TOTAL_FIX_MINUTES} |"
  echo "| Lines of Code | ${LOC} |"
  echo ""
  echo "## By Category"
  echo ""
  echo "| Category | Issues | Est. Fix Minutes |"
  echo "|---|---|---|"
  for k in "${!CATEGORY_COUNTS[@]}"; do
    count=${CATEGORY_COUNTS[$k]}
    mins=$((count * ${FIX_MINUTES[$k]:-${FIX_MINUTES[default]}}))
    echo "| $k | $count | $mins |"
  done
  echo ""
  echo "## Top Debt Hotspots"
  echo ""
  # Find files with the most issues
  jq -r '.[].spans[0].file_name' /tmp/clippy-output.json | sort | uniq -c | sort -rn | head -10 | while read -r count file; do
    echo "- **$file**: $count issues"
  done
  echo ""
  echo "## Trend"
  echo ""
  echo "Previous ratio: ${PREV_RATIO}%"
  echo "Current ratio: ${DEBT_RATIO}%"
  echo "Change: ${TREND}%"
  echo ""
  if (( $(echo "$TREND > 0" | bc -l) )); then
    echo ":warning: Debt is **increasing**. Review recent PRs for debt introduction."
  elif (( $(echo "$TREND < 0" | bc -l) )); then
    echo ":white_check_mark: Debt is **decreasing**."
  else
    echo "Debt is stable."
  fi
} > "$REPORT_PATH"

echo "Report written to $REPORT_PATH"
echo "JSON output written to $OUTPUT_JSON"
```

### Script: `parish/scripts/debt-issue-creator.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# Creates GitHub issues for debt findings above threshold.
# Reads /tmp/clippy-output.json from debt scanner.

MIN_SEVERITY="major"  # Only create issues for major and above

# Map clippy lint groups to severity
declare -A SEVERITY_MAP
SEVERITY_MAP[correctness]="blocker"
SEVERITY_MAP[suspicious]="critical"
SEVERITY_MAP[complexity]="major"
SEVERITY_MAP[perf]="major"
SEVERITY_MAP[nursery]="major"
SEVERITY_MAP[style]="minor"
SEVERITY_MAP[pedantic]="minor"
SEVERITY_MAP[cargo]="minor"
SEVERITY_MAP[restriction]="minor"

# Determine if issue should be auto-fixable
is_auto_fixable() {
  local category="$1"
  case "$category" in
    style|pedantic|restriction|cargo|nursery)
      echo "true" ;;
    *)
      echo "false" ;;
  esac
}

# Group findings by file+category to avoid issue spam
jq -c '.[]' /tmp/clippy-output.json | while IFS= read -r finding; do
  FILE=$(echo "$finding" | jq -r '.spans[0].file_name // "unknown"')
  LINE=$(echo "$finding" | jq -r '.spans[0].line_start // 0')
  MESSAGE=$(echo "$finding" | jq -r '.message // ""')
  CODE=$(echo "$finding" | jq -r '.code.code // "unknown"')
  CATEGORY=$(echo "$CODE" | cut -d':' -f2)
  SEVERITY=${SEVERITY_MAP[$CATEGORY]:-"minor"}

  # Skip below threshold
  if [ "$MIN_SEVERITY" = "major" ] && [ "$SEVERITY" != "blocker" ] && [ "$SEVERITY" != "critical" ] && [ "$SEVERITY" != "major" ]; then
    continue
  fi

  AUTO_FIXABLE=$(is_auto_fixable "$CATEGORY")
  LABELS="debt,$SEVERITY"
  if [ "$AUTO_FIXABLE" = "true" ]; then
    LABELS="$LABELS,auto-fixable"
  fi

  # Determine priority from severity
  case "$SEVERITY" in
    blocker) PRIORITY="P0" ;;
    critical) PRIORITY="P1" ;;
    major) PRIORITY="P2" ;;
    minor) PRIORITY="P3" ;;
    *) PRIORITY="P3" ;;
  esac
  LABELS="$LABELS,$PRIORITY"

  TITLE="[debt][$CATEGORY] $MESSAGE"
  BODY="## Technical Debt Finding

**Category:** \`$CATEGORY\`
**Severity:** $SEVERITY
**Priority:** $PRIORITY
**Location:** \`$FILE:$LINE\`
**Lint Code:** \`$CODE\`

### Description
$MESSAGE

### Estimated Fix Effort
Based on category \`$CATEGORY\`, estimated fix time is derived from SQALE model.

### Remediation
Run the following to see details:
\`\`\`bash
cargo clippy -- -W clippy::$CODE
\`\`\`

---
*Auto-generated by debt-shield scanner*"

  echo "Creating issue: $TITLE"
  gh issue create \
    --title "$TITLE" \
    --body "$BODY" \
    --label "$LABELS" \
    --repo "$GITHUB_REPOSITORY"
done
```

---

## Phase 4: Architectural Hardening

**Goal**: Encode Parish-specific anti-patterns as enforceable rules. Expand architecture fitness tests to cover mode parity, documentation coverage, and dependency graph constraints.

### File: `parish/scripts/parish-lint.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# Parish-specific lint rules that clippy cannot express directly.
# Run on staged .rs files (or all files if no arguments).
# Exits non-zero if any violations found.

FILES="${@:-$(git diff --cached --name-only --diff-filter=ACM | grep '\.rs$' || true)}"
if [ -z "$FILES" ]; then
  echo "No Rust files to check"
  exit 0
fi

VIOLATIONS=0

for FILE in $FILES; do
  LIBRARY_CRATE=false
  case "$FILE" in
    parish/crates/parish-*/src/*)
      # Leaf crates are libraries
      LIBRARY_CRATE=true ;;
    parish/crates/parish-core/src/*)
      LIBRARY_CRATE=true ;;
    parish/crates/parish-cli/src/*)
      # CLI is a binary, different rules apply
      ;;
    parish/crates/parish-server/src/*)
      # Server is a binary
      ;;
    parish/crates/parish-tauri/src/*)
      # Tauri is a binary
      ;;
    *)
      # Skip non-crate files
      continue ;;
  esac

  # Rule 1: No unwrap() or expect() in library crates
  if $LIBRARY_CRATE && grep -n '\.unwrap()\|\.expect(' "$FILE" | grep -v '//.*unwrap\|//.*expect\|#\[allow' > /tmp/parish-lint-unwrap.txt 2>/dev/null; then
    while IFS= read -r match; do
      LINE=$(echo "$match" | cut -d: -f1)
      echo "::error file=$FILE,line=$LINE::[parish-lint] No .unwrap() or .expect() in library crates. Use proper error propagation."
      VIOLATIONS=$((VIOLATIONS + 1))
    done < /tmp/parish-lint-unwrap.txt
  fi

  # Rule 2: No println! or eprintln! in library crates
  if $LIBRARY_CRATE && grep -n 'println!\|eprintln!' "$FILE" | grep -v '//.*println\|//.*eprintln' > /tmp/parish-lint-println.txt 2>/dev/null; then
    while IFS= read -r match; do
      LINE=$(echo "$match" | cut -d: -f1)
      echo "::error file=$FILE,line=$LINE::[parish-lint] No println!/eprintln! in library crates. Use tracing macros."
      VIOLATIONS=$((VIOLATIONS + 1))
    done < /tmp/parish-lint-println.txt
  fi

  # Rule 3: No std::thread::sleep
  if grep -n 'std::thread::sleep\|thread::sleep' "$FILE" | grep -v '//.*sleep\|spawn_blocking' > /tmp/parish-lint-sleep.txt 2>/dev/null; then
    while IFS= read -r match; do
      LINE=$(echo "$match" | cut -d: -f1)
      echo "::error file=$FILE,line=$LINE::[parish-lint] No std::thread::sleep. Use tokio::time::sleep or spawn_blocking."
      VIOLATIONS=$((VIOLATIONS + 1))
    done < /tmp/parish-lint-sleep.txt
  fi

  # Rule 4: No current_dir() or parent-walk patterns
  if grep -n 'current_dir()\|\.parent()\|ancestors()' "$FILE" | grep -v '//.*path\|AppState\|picker::resolve' > /tmp/parish-lint-cwd.txt 2>/dev/null; then
    while IFS= read -r match; do
      LINE=$(echo "$match" | cut -d: -f1)
      echo "::error file=$FILE,line=$LINE::[parish-lint] No current_dir() or parent-walk. Use AppState runtime paths."
      VIOLATIONS=$((VIOLATIONS + 1))
    done < /tmp/parish-lint-cwd.txt
  fi

  # Rule 5: #[allow(clippy::*)] must have a justifying comment
  if grep -n '#\[allow(clippy::' "$FILE" > /tmp/parish-lint-allow.txt 2>/dev/null; then
    while IFS= read -r match; do
      LINE=$(echo "$match" | cut -d: -f1)
      # Check if next line has a comment
      NEXT_LINE=$((LINE + 1))
      if ! sed -n "${NEXT_LINE}p" "$FILE" | grep -q '//'; then
        echo "::error file=$FILE,line=$LINE::[parish-lint] #[allow(clippy::*)] must have a justifying comment on the following line."
        VIOLATIONS=$((VIOLATIONS + 1))
      fi
    done < /tmp/parish-lint-allow.txt
  fi

  # Rule 6: No reqwest::get() or Client::new() without timeout
  if grep -n 'reqwest::get\|Client::new()' "$FILE" | grep -v 'timeout\|//.*timeout' > /tmp/parish-lint-timeout.txt 2>/dev/null; then
    while IFS= read -r match; do
      LINE=$(echo "$match" | cut -d: -f1)
      echo "::error file=$FILE,line=$LINE::[parish-lint] reqwest HTTP calls must have explicit timeouts."
      VIOLATIONS=$((VIOLATIONS + 1))
    done < /tmp/parish-lint-timeout.txt
  fi
done

echo "Parish lint: $VIOLATIONS violation(s) found"
exit $VIOLATIONS
```

### Expanded Architecture Fitness Tests

Add to `parish/crates/parish-core/tests/architecture_fitness.rs`:

```rust
// Existing tests... plus:

/// Mode parity: CLI, server, and Tauri must call the same IPC handlers.
/// Each entry point should call the same core game loop functions.
#[test]
fn mode_parity_check() {
    // Verify that parish-cli, parish-server, and parish-tauri
    // all invoke the same set of game loop / session functions.
    // This is a structural check -- exact implementation details TBD
    // based on the current handler registration pattern.
}

/// All public items in library crates must have /// documentation.
#[test]
fn all_public_items_documented() {
    // Walk each library crate's public API and verify every
    // pub fn, pub struct, pub enum, pub trait, pub mod has a doc comment.
    // Use rustdoc JSON output or source-level scanning.
}

/// Crate dependency graph must follow allowed edges.
/// For example: parish-input should not depend on parish-npc.
#[test]
fn crate_dependency_graph_valid() {
    // Parse Cargo.toml dependency declarations and verify
    // no disallowed dependency edges exist.
    // Allowed edges are defined in a dependency matrix.
}

/// Feature flags must be documented in AGENTS.md.
#[test]
fn feature_flags_documented() {
    // Scan config.flags for known features, verify each
    // appears in AGENTS.md or docs/features.md.
}
```

---

## File Inventory

### New Files (14)

| File | Phase | Purpose |
|---|---|---|
| `.github/workflows/debt-fix.yml` | 3 | Issue -> Claude Code fix -> PR |
| `.github/workflows/debt-land.yml` | 3 | PR -> verify CI -> classify -> auto-merge |
| `.github/workflows/debt-scanner.yml` | 1 | Weekly scan + SQALE + issue creation |
| `.github/workflows/debt-review.yml` | 2 | Adversarial PR review (gating) |
| `.agents/skills/debt-review/SKILL.md` | 2 | Hostile debt auditor skill definition |
| `.pre-commit-config.yaml` | 0 | Human-facing git hooks |
| `parish/clippy.toml` | 0 | Clippy thresholds |
| `parish/scripts/debt-scanner.sh` | 1 | Clippy JSON parser + SQALE calculator |
| `parish/scripts/debt-issue-creator.sh` | 1 | Converts scan findings to GitHub issues |
| `parish/scripts/parish-lint.sh` | 4 | Custom Parish-specific lint rules |
| `parish/scripts/check-commit-msg.sh` | 0 | Conventional commit validator |
| `parish/scripts/check-secrets.sh` | 0 | Secret pattern scanner |
| `parish/scripts/check-large-files.sh` | 0 | Large file blocker |
| `docs/design/debt-shield.md` | -- | This document |

### Modified Files (5)

| File | Phase | Change |
|---|---|---|
| `.github/workflows/ci.yml` | 0 | Add reviewdog diff-only clippy linting step |
| `parish/crates/parish-core/tests/architecture_fitness.rs` | 4 | Add mode parity, docs coverage, dep graph tests |
| `.agents/skills/techdebt/SKILL.md` | 3 | Integrate with autonomous pipeline |
| `AGENTS.md` | 4 | Document new workflows, skills, and debt-shield system |
| All library crate `src/lib.rs` | 0 | Add `#![deny(clippy::pedantic)]` and `#![deny(clippy::nursery)]` crate attributes |

### Supporting Files

| File | Purpose |
|---|---|
| `.github/debt-baseline.json` | Stored scan baseline for trend comparison (auto-generated, checked in) |
| `DEBT_REPORT.md` | Human-readable debt report at repo root (auto-generated, checked in) |
| `.github/triage-labels.json` | Update with `debt`, `auto-fixable`, `auto-debt-fix`, `debt-tracking`, `needs-review` labels |

---

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Auto-merged PR introduces a regression | Squash-merge enables one-click revert. CI verification is mandatory before auto-merge. Change classification prevents logic changes from auto-merging. |
| Adversarial agent produces false positives | Agents are instructed to cite specific lines and patterns. Major/Minor findings are advisory only. The gating posture is limited to Blocker/Critical (security, data loss, architecture violations). |
| Debt scanner creates too many issues (noise) | Issues are only created for major severity and above (configurable). Issues are grouped by file+category. The `MIN_SEVERITY` threshold can be adjusted. |
| Claude Code in CI is expensive | `--max-budget-usd 2.00` per fix dispatch. `--max-turns 10` for review, `--max-turns 15` for fix. Adversarial review costs ~$0.50-1.00 per PR. Weekly scan issues are batched. |
| Pre-commit hooks slow down development | `fail_fast: true` on fmt (fastest check). Clippy runs `--workspace --all-targets` but only on `.rs` file changes. Can be bypassed with `--no-verify` in emergencies. |
| Debt ratio CI failure blocks unrelated work | Thresholds are configurable (10% ratio, 2% increase). Workflow can be re-run. Baseline updates automatically on successful scans. |
| Claude Code not available in CI (auth/network) | Uses `ANTHROPIC_API_KEY` from secrets. Fallback: the existing Gemini dispatch pattern is already proven in this repo. |

---

## Rollout Plan

1. **Week 1**: Build Phase 3 (autonomous pipeline) -- `debt-fix.yml`, `debt-land.yml`, `debt-scanner.yml`, `debt-scanner.sh`, `debt-issue-creator.sh`. Run first scan to establish baseline.
2. **Week 2**: Build Phase 2 (adversarial agent) -- `debt-review/SKILL.md`, `debt-review.yml`. Run in shadow mode (comment-only, no gating) for one week to tune false positive rate, then enable gating.
3. **Week 3**: Build Phase 0 (foundation) -- `clippy.toml`, `.pre-commit-config.yaml`, helper scripts. Add `#![deny(clippy::pedantic)]` to crate roots. Fix initial wave of pedantic warnings.
4. **Week 4**: Build Phase 1 (measurement) -- complete SQALE ratio, trending, hotspot detection. Wire up the CI threshold check.
5. **Week 5**: Build Phase 4 (architectural) -- `parish-lint.sh`, expanded architecture fitness tests. Final integration testing of all layers.
