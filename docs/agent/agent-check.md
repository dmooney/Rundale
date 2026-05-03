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
```

That judge file is where the independent reviewer records whether the evidence actually proves the stated requirements and whether the change leaves obvious debt behind. CI cannot know whether the reviewer was wise, but it can refuse PRs that omit the evidence or the recorded verdict.

## What Counts As Proof-Relevant

The gate requires proof for engine, UI, gameplay content, runtime scripts, CI, agent instructions, and harness changes. Pure docs outside the agent harness do not require proof.

## What It Scans For

`agent-check` also scans changed files for common partial-completion markers such as placeholder panics, empty implementation macros, and copied "unchanged" comments. This overlaps with `witness-scan`, but it runs before toolchain setup and includes unstaged local files so agents get faster feedback.
