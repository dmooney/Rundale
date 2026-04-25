# Triage vocabulary

Canonical label set for open-issue triage. The machine-readable list lives in [`.github/triage-labels.json`](../../.github/triage-labels.json) — the `triage-audit` workflow reads that file directly, and the `triage-backlog` skill reads this doc. When introducing a new theme or priority level, update **both** the JSON and the prose below; never invent labels at the call site.

## Priority labels (exactly one per issue)

| Label | Meaning |
|---|---|
| `P0` | Exploitable security vuln, deadlock, data loss/corruption, or production outage path. Drop everything. |
| `P1` | Correctness bug in a shipping user flow; broken feature; serious leak/race; auth/permission gap. |
| `P2` | Perf regression, UX paper cut, mode-parity gap, missing-but-not-blocking feature. |
| `P3` | Cleanup, refactor, minor test debt, doc/version chore, micro-perf. |

## Theme labels (one or more per issue)

| Label | Scope |
|---|---|
| `bug` | Existing behavior is wrong. Pair with another theme when the bug is specifically a perf/security/frontend issue. |
| `enhancement` | Requested feature or improvement that is not a regression. |
| `performance` | Latency, throughput, allocation, or resource use. (Do not use the older `perf` label.) |
| `security` | Exploitable surface, auth/permission, secrets exposure, supply chain, workflow trust. |
| `refactor` | Code quality, structural change, no behavior change. |
| `frontend` | Anything in `apps/ui/` — Svelte components, MapLibre, styles. |
| `a11y` | Accessibility-specific (keyboard nav, ARIA, contrast). Pair with `frontend`. |
| `mode-parity` | Tauri / web server / headless CLI behavioral divergence (project rule #2). |
| `npc-reactions` | The LLM-driven NPC-reaction subsystem. |
| `witness-scan` | The `/witness` verification workflow tooling. |
| `infra` | CI, deploy, Docker, Cloudflare, Railway, GitHub Actions, runners. |

## Process labels (independent of theme/priority)

| Label | Meaning |
|---|---|
| `ready-for-test` | The fix has merged but the issue isn't auto-closed. Verify and close. (Do not use the older `ready for test` form.) |
| `in-progress` | A human or agent is actively working on this. |
| `codex-automation` | Issue is being driven by Codex. |

## Application rules

1. Every open issue should carry exactly one `P*` label and at least one theme label. The `triage-audit` workflow reports violations weekly.
2. `mcp__github__issue_write` *replaces* the label set, so always pass the union of (existing labels) + (new theme labels) + (priority label).
3. New labels are auto-created by GitHub on first use, but ship without descriptions or colors. After introducing a new label here, update its color/description manually in the GitHub UI or via `gh label create`.
4. Don't relabel an issue that already has a `P*` label unless explicitly asked to re-triage.

## Priority rubric — concrete examples

- **P0**: SQL injection (#592), deadlock risk (#337), GitHub Actions privilege escalation (#602), CLI proceeding past lock failure in script mode (#608).
- **P1**: TOCTOU race (#283), missing SQLite transaction around multi-statement delete (#593), broken-on-desktop UI button (#600), Anthropic client without structured-output guarantee (#416).
- **P2**: Lock contention in debug snapshot (#282), background task lacking graceful shutdown (#104, #228), mode-parity gap in NPC reactions (#402), MapLibre migration regressions (#309).
- **P3**: Double `.cloned()` (#106), magic-constant ring buffer (#611), 16-month-old pinned binary (#610), validate-lat-lon (#88).
