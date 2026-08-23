# Agent Check

`agent-check` is the PR proof gate. It turns "I tested it" into a recorded artifact that CI can verify before the expensive Rust and UI jobs run.

The script has two source modes:

- `bash parish/scripts/agent-check.sh --source=local` (default) — validates the bundle that lives at `.proofs/<task-id>/` on disk. This is what `just agent-check` runs, and what the Stop hook expects before a session can end. Bundles in `.proofs/` are gitignored.
- `bash parish/scripts/agent-check.sh --source=pr <number>` — validates the bundle embedded in the PR body via `just attach-proof`; a structured comment remains a legacy fallback. CI uses this mode on `pull_request` events and reads the `<!-- parish-proof-bundle:<task-id> v=1 -->` fenced block.

Run it locally with `just agent-check`. It is also part of `just check` and `just verify`.

## Lifecycle of a bundle

```text
write .proofs/<id>/acceptance-criteria.md
implement
run game                  → capture .proofs/<id>/transcript.txt
write .proofs/<id>/evidence.md   → 'Evidence type: live gameplay transcript'
                                   + criterion-to-line mapping
write .proofs/<id>/judge.md      → 'Verdict: sufficient'
                                   'Technical debt: clear'
                                   'Acceptance criteria: met'
just agent-check          → local mode validates the disk bundle
gh pr create --body-file <(printf '%s\n' "$desc" \
  | bash parish/scripts/compose-proof-body.sh <id>)
                          → opens the PR with the bundle ALREADY in the
                            body, so the gate is green on its first run
just attach-proof <id>    → (re-)injects the bundle into the body of an
                            existing PR; idempotent (replaces the prior
                            region, never appends a duplicate). Use after
                            fixing a bundle. `--as-comment` keeps the legacy
                            comment path.
```

CI reads the bundle from the PR body (or a comment) — the body is present on
the `pull_request.opened` run, so a fresh proof-relevant PR is green on the
first run with no re-push (#1177).

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

`Acceptance criteria: met` is required when the bundle has an `acceptance-criteria.md`.

## What Counts As Proof-Relevant

The gate requires proof for engine, UI, gameplay content, runtime scripts, CI, agent instructions, and harness changes. PRs that touch no source/runtime paths are exempt: pure documentation (any `*.md` / `*.txt`, e.g. AGENTS.md, README.md, `docs/**`), CI-only (`.github/**`), agent-instruction-only (`.agents/**`, `.claude/**`), check-tooling-only (`parish/scripts/**`), and build-config-only (`justfile`) edits all skip the gate when no code change accompanies them. Dependabot PRs are also exempt at the CI layer — automated dependency bumps have no useful signal to prove.

Historical bench receipts live in the ignored local archive at `docs/proofs/`
(symlinked to iCloud Drive on the primary macOS workstation). They are not
per-task proof bundles and are not validated by this gate. Concise leaderboard
summaries and content hashes remain committed; raw paid receipts do not.

## Live-proof Tier

When the diff touches a runtime-shipping path — `parish-tauri/**`, `parish-server/**`, `parish-engine/**`, `parish-core/src/{game_loop,game_session,ipc}/**`, `parish-inference/src/{setup,client}.rs`, `parish-npc/src/{ticks,manager,reactions,autonomous}/**`, `parish-world/**`, `parish-input/**`, `parish/apps/ui/src/**`, `mods/**` — unit tests alone are not sufficient. The change must be exercised in a real process (Tauri, server, CLI, or browser) and the bundle's `evidence.md` header must declare `Evidence type: live gameplay transcript`, **or** the bundle must include a screenshot (`.png` / `.jpg` / `.jpeg`) or gif (`.gif`). The word "live" is the author affirmation that the run actually happened; analysis-only writeups failing this header are rejected by `just agent-check`.

**Real-loop integration tier.** Some runtime behaviours cannot be reproduced in a live process on demand — a deterministic post-generation guard whose _only_ trigger is intermittent large-model output (e.g. the 14B spontaneously impersonating another roster NPC, or looping a phrase to the token cap). The honest, strongest proof for these is a Rust integration test that drives the **real** `game_loop` (`handle_game_input` → `run_npc_turn`) via `GameTestHarness::execute_via_real_loop`, mocking only the LLM boundary — this exercises the exact production wiring (the gate's actual concern), unlike `--script`, which uses the legacy `execute()` path and bypasses `game_loop/npc_turn`. For such a change, declare `Evidence type: game-loop integration test` and **reference `execute_via_real_loop` in the same evidence file**; the gate accepts it as runtime proof. The `execute_via_real_loop` requirement ties the claim to the real mechanism so the tier cannot be stamped over plain unit tests. Use this tier only when a live trigger is genuinely non-deterministic — prefer a live transcript or screenshot whenever the behaviour can be exercised in a real process.

Accepted live signals: `mcp__parish__*`, `mcp__claude-in-chrome__*`, the `/parish-engine` skill (its `prove` / `play` / `demo` / `browser` modes), or a Bash invocation of `just demo` / `just play` / `just run` / `just run-headless` / `just web` / `cargo tauri dev` / `cargo run -p parish-{engine,tauri,server,client}`.

The Stop hook (`.claude/hooks/Stop--proof-required.sh`) blocks session-end with the same matrix.

## Belt-and-suspenders Lints

- Any `.proofs/<...>` path appearing in the git diff is rejected — bundles are gitignored and are carried in the PR body (or a comment), never committed.
- Changed files are scanned for language-specific unfinished-work macros and
  placeholder comments that often indicate partial completion.

## Acceptance Criteria Requirement

Every new proof bundle must include `.proofs/<task-id>/acceptance-criteria.md`. This file lists observable criteria with the game commands or screenshots that prove each one. The judge then verifies each criterion individually against the transcript or visual artifact.

## Posting from a no-gh sandbox

The web / MCP sandbox has no `gh`. Use `--via-mcp` (no network):

```sh
just attach-proof <id> --via-mcp        # validates locally, prints the block to stdout
# or: bash parish/scripts/attach-proof.sh <id> --via-mcp
```

It runs the same local validation, then prints **only** the fenced bundle block to stdout (progress goes to stderr). Post it through the GitHub MCP — preferably as the PR **body** (`create_pull_request` / update body) so the gate is green on the first run, or as a comment via `add_issue_comment` (the gate reads both). Binary artifacts (screenshots / transcript) are uploaded separately in the GitHub UI; the CI gate only needs the text block.
