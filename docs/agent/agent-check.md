# Agent Check

`agent-check` is the PR proof gate. It turns "I tested it" into a recorded artifact that CI can verify before the expensive Rust and UI jobs run.

The script has two source modes:

- `bash limerick/scripts/agent-check.sh --source=local` (default) — validates the bundle that lives at `.proofs/<task-id>/` on disk. This is what `just agent-check` runs, and what the Stop hook expects before a session can end. Bundles in `.proofs/` are gitignored.
- `bash limerick/scripts/agent-check.sh --source=pr <number>` — validates the bundle embedded in the PR body via `just attach-proof`; a structured comment remains a legacy fallback. CI uses this mode on `pull_request` events and reads the `<!-- limerick-proof-bundle:<task-id> v=1 -->` fenced block.

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
  | bash limerick/scripts/compose-proof-body.sh <id>)
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

The gate requires proof for engine, UI, gameplay content, runtime scripts, CI, agent instructions, and harness changes. PRs that touch no source/runtime paths are exempt: pure documentation (any `*.md` / `*.txt`, e.g. AGENTS.md, README.md, `docs/**`), CI-only (`.github/**`), agent-instruction-only (`.agents/**`, `.claude/**`), check-tooling-only (`limerick/scripts/**`), and build-config-only (`justfile`) edits all skip the gate when no code change accompanies them. Dependabot PRs are also exempt at the CI layer — automated dependency bumps have no useful signal to prove.

Historical bench receipts live in the ignored local archive at `docs/proofs/`
(symlinked to iCloud Drive on the primary macOS workstation). They are not
per-task proof bundles and are not validated by this gate. Concise leaderboard
summaries and content hashes remain committed; raw paid receipts do not.

## Live-proof Tier

When the diff touches a runtime-shipping path — `limerick-tauri/**`, `limerick-server/**`, `limerick-engine/**`, `limerick-core/src/{game_loop,game_session,ipc}/**`, `limerick-inference/src/{setup,client}.rs`, `limerick-npc/src/{ticks,manager,reactions,autonomous}/**`, `limerick-world/**`, `limerick-input/**`, `limerick/apps/ui/src/**`, `mods/**` — unit tests alone are not sufficient. The change must be exercised in a real process (Tauri, server, CLI, or browser) and the bundle's `evidence.md` header must declare `Evidence type: live gameplay transcript`, **or** the bundle must include a screenshot (`.png` / `.jpg` / `.jpeg`) or gif (`.gif`). The word "live" is the author affirmation that the run actually happened; analysis-only writeups failing this header are rejected by `just agent-check`.

**Real-loop integration tier.** Some runtime behaviours cannot be reproduced in a live process on demand — a deterministic post-generation guard whose _only_ trigger is intermittent large-model output (e.g. the 14B spontaneously impersonating another roster NPC, or looping a phrase to the token cap). The honest, strongest proof for these is a Rust integration test that drives the **real** `game_loop` (`handle_game_input` → `run_npc_turn`) via `GameTestHarness::execute_via_real_loop`, mocking only the LLM boundary — this exercises the exact production wiring (the gate's actual concern), unlike `--script`, which uses the legacy `execute()` path and bypasses `game_loop/npc_turn`. For such a change, declare `Evidence type: game-loop integration test` and **reference `execute_via_real_loop` in the same evidence file**; the gate accepts it as runtime proof. The `execute_via_real_loop` requirement ties the claim to the real mechanism so the tier cannot be stamped over plain unit tests. Use this tier only when a live trigger is genuinely non-deterministic — prefer a live transcript or screenshot whenever the behaviour can be exercised in a real process.

Accepted live signals: `mcp__limerick__*`, `mcp__claude-in-chrome__*`, the `/limerick-engine` skill (its `prove` / `play` / `demo` / `browser` modes), or a Bash invocation of `just demo` / `just play` / `just run` / `just run-headless` / `just web` / `cargo tauri dev` / `cargo run -p limerick-{engine,tauri,server,client}`.

The Stop hook (`.claude/hooks/Stop--proof-required.sh`) blocks session-end with the same matrix.

## Differential Proof

`just prove-diff [SCENARIO] [--intended FILE]` runs the same scenarios on
`main` and on your change and reports every difference. Use it for any change
that could alter runtime behaviour, and paste its report into `evidence.md`.
It proves two things a transcript alone does not: the change did what you
declared, and nothing else changed.

What it does (`limerick/scripts/proof/prove_diff.py`):

1. Builds `limerick-server` and `limerick-engine` from the merge-base with
   `origin/main` (a detached worktree under
   `~/.cache/limerick/prove-diff/<repo>/base-tree`) and from your working tree,
   uncommitted changes included. Changed files are touched before each build so
   the shared cargo target cannot reuse the other tree's fingerprint, and each
   side's binaries are copied out before the next build.
2. On each side, several times (`--runs`, default 3):
   - drives the live scenario (`limerick/scripts/proof/scenarios/<name>.txt`,
     one player line per line) through `limerick-server` over
     `POST /api/submit-input`, against the scripted model server. The server
     runs with `LIMERICK_PROVIDER=lmstudio`, `LIMERICK_MODEL=scripted`, the
     tree's own `mods/rundale`, isolated user data and config, no cloud keys,
     and a working directory without `.env`;
   - runs every `limerick/testing/fixtures/test_*.txt` through
     `limerick-engine --script ... --game-mod <tree>/mods/rundale`
     (`--fixtures ''` skips them).
3. Compares four surfaces: `responses` (each turn's submit-input reply),
   `state` (`/api/engine-state` after the last turn), `requests` (every provider
   request body, grouped by turn), and `script` (each fixture command's fields
   and log lines). Each difference is one line, for example
   `requests/talk-and-task turn 4 request (dialogue) + system| WORLD FACTS ...`.
4. Checks the differences against your intended-differences file and writes
   `report.md` in the output directory. Any undeclared difference fails; so does
   a declaration that matches nothing.

Intended-differences file (TOML; keep it in the bundle, e.g.
`.proofs/<id>/intended-diffs.toml`):

```toml
[[intended]]
surface = "requests"          # optional: responses | state | requests | script
name = "talk-and-task"        # optional: fnmatch on the scenario or fixture name
match = 'WORLD FACTS .* County Roscommon'  # regex searched in the difference line
reason = "tier-1 prompt names the county"
```

With no file, the run passes only if nothing differs, which is the proof for a
refactor.

Nondeterminism: runs on `main` still vary (unseeded dice, HashMap-ordered NPC
lists, wall-clock clocks and save times). `limerick/scripts/proof/noise.py`
masks each known source, and where a side's runs still disagree the report
lists it under "Nondeterminism" and compares that unit as a range: a line
differs only if every run of one side has it and no run of the other does.
Item 2 of #2033 removes these sources; delete a normaliser when its source is
fixed.

Writing a scenario: start with `/pause`, since the live clock otherwise runs in
wall-clock time. Lines starting with a movement verb go to the local parser;
`Let us be off, walking on toward <Place>` reaches the intent model, which the
scripted server answers with a move. The scripted NPC offers a task when the
player mentions work. `scripted_openai.py` lists the canned reply per workload.

Pieces usable alone: `scripted_openai.py --port P --log F` (point a Tauri app
at it with `LIMERICK_BASE_URL=http://127.0.0.1:P/v1`), `drive_session.py`
(launch `limerick-server`, or `--attach URL` to drive a running server or the
Tauri bridge), `body_diff.py` (request logs), and `script_compare.py`
(`--script` output directories).

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
# or: bash limerick/scripts/attach-proof.sh <id> --via-mcp
```

It runs the same local validation, then prints **only** the fenced bundle block to stdout (progress goes to stderr). Post it through the GitHub MCP — preferably as the PR **body** (`create_pull_request` / update body) so the gate is green on the first run, or as a comment via `add_issue_comment` (the gate reads both). Binary artifacts (screenshots / transcript) are uploaded separately in the GitHub UI; the CI gate only needs the text block.
