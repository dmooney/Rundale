# Event-Driven Improvement Drain

This repository manages improvement work by queue state and evidence gates, not
calendar estimates. GitHub issue [#1684](https://github.com/dmooney/Rundale/issues/1684)
is the bootstrap record for the current portfolio reset.

## Sources of truth

1. GitHub Issues are executable work records.
2. An active epic owns each funded initiative's outcome and child work.
3. The [roadmap](../requirements/roadmap.md) records product strategy and
   capability status; it is not an implementation queue.
4. `TODO.md` files are discovery ledgers. A validated item must link to an open
   issue before it can be scheduled.
5. Audit reports are dated evidence. A newer reconciliation must mark the old
   report superseded rather than silently rewriting its historical counts.

P0-P3 always means severity. Sequence belongs in the portfolio Horizon
(`Now`, `Next`, `Later`) and queue order. A strategically important enhancement
does not become a P1 unless it satisfies the P1 correctness/security rubric.

## Worker and review buffers

The coordinator retains one agent slot. The other three slots each own one
bounded work unit. A completed or blocked slot is refilled immediately.

- At most three atomic work-unit issues carry `in-progress`.
- At most three additional non-draft PRs wait in CI/review.
- When the review buffer is full, stop starting work and land, repair, defer,
  or close those PRs first.
- Serialize changes to persistence, identity, inference, session lifecycle,
  and `AppState` seams.
- P0 work takes every available slot. P1 bugs, security, and correctness work
  precede enhancements.

An `in-progress` epic records initiative state but is not an implementation
work unit and does not consume a worker slot. Its atomic children do. The
explicit mappings in `.github/portfolio-roadmap.json` let the audit compare
those epic states to roadmap rows without guessing from labels.

The weekly `triage-audit` workflow reports buffer excess, missing readiness
metadata, missing unblock triggers, and missing authoritative PR linkage. It
reports debt during the reset instead of making the existing backlog
permanently block unrelated PRs.

## Work-item contract

Use the Portfolio work item issue form. A Ready issue contains:

- an observable Outcome;
- exactly one P0-P3 severity and at least one canonical theme;
- Initiative, Horizon, Type, Risk, Size, and DRI;
- acceptance criteria and literal proof commands/artifacts;
- affected runtime modes and scaling seams;
- dependencies and a parent epic or roadmap entry.

Large work is split before Ready. `Blocked` and `Deferred` work names the exact
event that will unblock it. Apply `in-progress` only when a worker slot is
actually committed; `codex-automation` records Codex execution but is not a
substitute for the issue contract.

## State transitions

1. **Intake → Ready:** scope, dependencies, proof, and material product choices
   are resolved.
2. **Ready → Implementing:** a worker slot is assigned; one issue maps to one
   logical PR.
3. **Implementing → CI/Review:** focused checks pass and the PR carries
   authoritative `Fixes`/`Closes` linkage plus any required proof bundle.
4. **CI/Review → Validation:** review findings are resolved and required CI
   passes against current `main`.
5. **Validation → Done:** the PR is merged, the observable outcome is checked,
   documentation agrees, and residual work is linked with an unblock trigger.

Refactors remain behavior-preserving and separate from features or bug fixes.
Runtime and UI work follows the proof requirements in
[agent-check.md](agent-check.md); scaling-sensitive changes also use
[scaling-rules.md](scaling-rules.md).

## Authoritative linkage

Only GitHub closing references count as an issue-to-PR relationship. Put
`Fixes #N`, `Closes #N`, or `Resolves #N` in the PR body. An incidental issue
number in release notes, dependency changelogs, or prose is not coverage.
Automated Dependabot updates are exempt from creating a work-item issue; every
human- or agent-authored implementation PR still closes one atomic issue.

The coordinator merges a green PR immediately after its required checks and
review threads clear, confirms the issue closed, and refills the freed slot.
If `main` is red, all new starts stop until it is green.

## External project and merge-queue limits

The approved GitHub Project fields are Status, Initiative, Horizon, Type, DRI,
Risk, Size, Outcome, and Unblock Trigger. Project creation is an external
permission step: until the authenticated token has Project read/write scope,
the issue-form fields and repository audit are the durable fallback.

GitHub does not offer merge queues to a user-owned repository. The unblock
trigger is transfer to an eligible organization account or future GitHub
support. Until then, strict required checks plus required conversation
resolution provide the closest available pre-merge gate; Full CI must run on
the PR head when a change requires it.
