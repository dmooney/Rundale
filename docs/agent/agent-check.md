# Agent Check

`agent-check` is the PR proof gate. It turns "I tested it" into a recorded artifact that CI can verify before the expensive Rust and UI jobs run.

The script has two source modes:

- `bash parish/scripts/agent-check.sh --source=local` (default) — validates the bundle that lives at `.proofs/<task-id>/` on disk. This is what `just agent-check` runs, and what the Stop hook expects before a session can end. Bundles in `.proofs/` are gitignored.
- `bash parish/scripts/agent-check.sh --source=pr <number>` — validates the bundle that was posted to a PR as a structured comment via `just attach-proof`. CI uses this mode on `pull_request` events; the comment must contain a `<!-- parish-proof-bundle:<task-id> v=1 -->` fenced block.

Run it locally with `just agent-check`. It is also part of `just check` and `just verify`.

## Lifecycle of a bundle

```text
/task-start <id>          → write .proofs/<id>/acceptance-criteria.md +
                            parish/testing/fixtures/play_<id>.txt
                          → stop, get human approval
implement
run game                  → capture .proofs/<id>/transcript.txt
write .proofs/<id>/evidence.md   → 'Evidence type: live gameplay transcript'
                                   + criterion-to-line mapping
write .proofs/<id>/judge.md      → 'Verdict: sufficient'
                                   'Technical debt: clear'
                                   'Acceptance criteria: met'
just agent-check          → local mode validates the disk bundle
just attach-proof <id>    → posts the bundle to the PR as a structured
                            comment; idempotent (re-running edits the
                            existing comment instead of appending)
```

CI then re-runs the gate against the PR comment.

## What It Enforces

When proof-relevant files change, the PR must carry a proof bundle.

Accepted evidence forms:

- Gameplay transcript: a `.md` or `.txt` artifact that declares `Evidence type: gameplay transcript` (or `Evidence type: live gameplay transcript` when the diff touches a runtime-shipping path).
- Screenshot: a `.png`, `.jpg`, or `.jpeg` artifact.
- Gif: a `.gif` artifact.

The judge must include these three lines:

```text
Verdict: sufficient
Technical debt: clear
Acceptance criteria: met
```

`Acceptance criteria: met` is required when the bundle has an `acceptance-criteria.md` (see rule 13 in AGENTS.md).

## What Counts As Proof-Relevant

The gate requires proof for engine, UI, gameplay content, runtime scripts, CI, agent instructions, and harness changes. Pure docs outside the agent harness do not require proof.

The two long-lived archives under `docs/proofs/local-perf/` and `docs/proofs/rundale-bench/` are bench artifacts (written by the eval-dialogue skill and ELO benchmarks). They are not per-task proof bundles and are not validated by this gate.

## Belt-and-suspenders Lints

- Any `.proofs/<...>` path appearing in the git diff is rejected — bundles are gitignored and must be posted as PR comments.
- Changed files are scanned for placeholder debt markers (`todo!()`, `unimplemented!()`, `pass # TODO`, etc.) that often indicate partial completion.

## Acceptance Criteria Requirement

Every new proof bundle must include `.proofs/<task-id>/acceptance-criteria.md`. This file is written **before any code**, using `/task-start <task-id>`, and lists observable criteria with the game commands that prove each one. The judge then verifies each criterion individually against the transcript.
