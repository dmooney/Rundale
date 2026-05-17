# Agent Check

`agent-check` is the PR proof gate. It turns "I tested it" into a committed artifact that CI can verify before the expensive Rust and UI jobs run.

Run it locally with `just agent-check`. It is also part of `just check` and `just verify`, and CI runs it as the `agent-check` job in `.github/workflows/ci.yml`.

## What It Enforces

When proof-relevant files change, the PR must include a changed proof bundle under `docs/proofs/`.

Accepted evidence forms:

- Gameplay transcript: a `.md` or `.txt` artifact that declares `Evidence type: gameplay transcript`.
- Screenshot: a `.png`, `.jpg`, or `.jpeg` artifact.
- Gif: a `.gif` artifact.

The same proof bundle must also include `judge.md` with these lines:

```text
Verdict: sufficient
Technical debt: clear
Acceptance criteria: met
```

`Acceptance criteria: met` is required when the same bundle contains `acceptance-criteria.md` (see rule 13 in AGENTS.md). That judge file is where the independent reviewer records whether the evidence actually proves the stated requirements, whether the change leaves obvious debt behind, and whether every acceptance criterion from `acceptance-criteria.md` is satisfied by the game log. CI cannot know whether the reviewer was wise, but it can refuse PRs that omit the evidence or the recorded verdict.

## What Counts As Proof-Relevant

The gate requires proof for engine, UI, gameplay content, runtime scripts, CI, agent instructions, and harness changes. Pure docs outside the agent harness do not require proof.

## Acceptance Criteria Requirement

Every new proof bundle must include `docs/proofs/<task-id>/acceptance-criteria.md`. This file is written **before any code**, using `/task-start <task-id>`, and lists observable criteria with the game commands that prove each one.

When a proof bundle has an `acceptance-criteria.md`, `agent-check` additionally requires that `judge.md` contains `Acceptance criteria: met`. The judge must verify each criterion individually against the game log captured in `transcript.txt`.

The sequential workflow that produces a valid bundle:

```
/task-start <id>          → write acceptance-criteria.md + play fixture
                          → stop, get human approval
implement
run game                  → capture transcript.txt
write evidence.md         → map each criterion to transcript lines
write judge.md            → Verdict: sufficient
                            Technical debt: clear
                            Acceptance criteria: met
just agent-check          → validates all three lines + AC file presence
```

## What It Scans For

`agent-check` also scans changed files for common partial-completion markers such as placeholder panics, empty implementation macros, and copied "unchanged" comments. This overlaps with `witness-scan`, but it runs before toolchain setup and includes unstaged local files so agents get faster feedback.
