# Agent Check

`agent-check` is the PR evidence gate. A pull request that changes
runtime-shipping code must link its evidence page, the public recording page at
`https://dmooney.github.io/rundale-pages/pr/<number>/`, in its body. The gate
also runs two cheap lints on every change.

It checks only that the link is there. Whether the recording shows the change
working is for review: the owner watches the recording, and the
[`/gatekeeper`](../../.agents/skills/gatekeeper/SKILL.md) pass judges it
against the diff.

## Modes

- `bash limerick/scripts/agent-check.sh --source=local` (default, `just agent-check`,
  part of `just check` and `just verify`) runs the lints and reports whether the
  diff touches runtime-shipping code. It cannot see a PR body, so it never fails
  for a missing link.
- `bash limerick/scripts/agent-check.sh --source=pr <number>` also reads the PR
  body with `gh` and fails a runtime-shipping diff whose body does not contain
  `https://dmooney.github.io/rundale-pages/pr/<number>/` for that same number.
  CI's `Evidence gate` job runs this mode on every non-Dependabot pull
  request. Only the body counts; comments are ignored, so a third party cannot
  satisfy the gate.

The job reads the body when it runs, and `pull_request` runs on `opened`,
`synchronize`, and `reopened`, not on body edits. Publish the page and link it
before opening the PR, or re-run the job (or push a commit) after adding it.

## Runtime-shipping paths

A link is required when the diff touches any of these, except Markdown files
and `graphify-out/` trees anywhere:

- `mobile/**`: the iPhone app, its Swift packages, endpoint fixtures, and its
  UI tests (UI tests drive the app itself). Exempt: `mobile/scripts/**`, the
  verification and release tooling, and Swift unit tests
  (`mobile/RundaleTests/**`, `mobile/*/Tests/**`).
- `mods/**`: world content and prompt templates (`.txt` counts).
- The engine's runtime crates and seams: `limerick-tauri/**`,
  `limerick-server/**`, `limerick-engine/**`,
  `limerick-core/src/{game_loop,game_session,ipc}/**`,
  `limerick-inference/src/{setup,client}.rs`,
  `limerick-npc/src/{ticks,manager}.rs`,
  `limerick-npc/src/{reactions,autonomous}/**`, `limerick-world/**`,
  `limerick-input/**`, and `limerick/apps/ui/src/**`. Rust and UI test code under
  these (`*.test.*`, `*.spec.*`, `*/tests/*`, `*/e2e/*`, `test-setup.ts`) is
  exempt.

No link is required for pure documentation, `.github/**`, `.agents/**`,
`.claude/**`, `limerick/scripts/**`, `justfile`, `limerick/justfile`,
`mobile/scripts/**`, pure-logic crates, or Rust, UI, or Swift unit-test-only
changes.
Dependabot PRs skip the job in CI.

Such changes still need tests and an honest account of the verification run;
they simply have no player-visible behavior for a recording to show.

## What goes in the PR

- **Evidence page** (runtime-shipping changes). A recording of the change
  working in the real app or engine process, on the production path, paced for
  a human viewer per the [phase demo plan](../product-specs/phase-demo-plan.md#recordings).
  Put an `index.html`, the H.264 `.mp4`, and a short `.gif` in one directory,
  run `bash limerick/scripts/publish-pr-page.sh <pr-number> <directory>`, link
  the page from the PR body, and embed the GIF. A simulator run is not
  physical-iPhone validation, and a fixture is not live gameplay; say which one
  the recording shows.
- **Acceptance criteria and verification** (every PR). Ordinary prose in the
  body: the criteria the change meets, what shows each one (the recording, a
  test, a command's output), and the checks actually run, with skipped,
  failing, and unavailable gates named as such. No format is machine-checked.
- **Intended differences** (when the differential proof applies). A
  ` ```toml intended-diffs ` block in the body; see below.

Unit tests alone are not proof of a gameplay or runtime change; see
[gameplay proof](engineering-rules.md#gameplay-proof).

### Why not a proof bundle

Until October 2026 the gate validated a per-task `.proofs/<task-id>/` bundle
embedded in the PR body: an acceptance-criteria file, an `evidence.md` with an
`Evidence type:` header, and an `Acceptance criteria: met` line. It checked form,
not substance, the author certified their own work (the reason `judge.md` was
dropped in #2119), its runtime path list predated the iPhone app, and it
duplicated the recording page the owner actually reviews. The bundle, its
helpers (`attach-proof`, `compose-proof-body.sh`, `render-proof-comment.sh`),
and its evidence tiers were removed.

Historical bench receipts live in the ignored local archive at `docs/proofs/`
(symlinked to iCloud Drive on the primary macOS workstation). The gate does not
read them.

## Differential Proof

`just prove-diff [SCENARIO] [--intended FILE]` runs the same scenarios on
`main` and on your change and reports every difference. Use it for any change
that could alter runtime behaviour, and summarise its report in the PR body.
It proves two things a transcript alone does not: the change did what you
declared, and nothing else changed.

What it does (`limerick/scripts/proof/prove_diff.py`):

1. Builds `limerick-server` and `limerick-engine` from the merge-base with
   `origin/main` (a detached worktree under
   `~/.cache/limerick/prove-diff/<repo>/base-tree`) and from your working tree,
   uncommitted changes included. Changed files are touched before each build so
   the shared cargo target cannot reuse the other tree's fingerprint, and each
   side's binaries are copied out before the next build.
2. On each side, twice (`--runs`, default 2):
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

Intended-differences declarations (TOML). Locally, pass them as a file with
`--intended FILE`, or save the PR body and pass `--intended-markdown FILE`,
which reads the same fenced blocks CI reads. In the PR, write them in the body:

```toml
[[intended]]
surface = "requests"          # optional: responses | state | requests | script
name = "talk-and-task"        # optional: fnmatch on the scenario or fixture name
match = 'WORLD FACTS .* County Roscommon'  # regex searched in the difference line
reason = "tier-1 prompt names the county"
# required = false            # optional: only when the base side is random and
                              # can match the head by chance (reported, not failed)
```

With no file, the run passes only if nothing differs, which is the proof for a
refactor.

Nondeterminism: runs are deterministic at the source (#2033): NPC lists come
out in id order, rolls are seeded from game state, the script clock and save
stamps ignore wall time, and background simulation requests are grouped per
turn. Head runs that disagree with each other fail the check, because the
change made a run nondeterministic; a base side that disagrees (an older
`main`) is compared as a range and listed under "Nondeterminism". The one
remaining normaliser (`limerick/scripts/proof/noise.py`) masks the seconds the
live server's real-time clock adds before the scenario's `/pause`.

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

### In CI

The `Differential proof` job in `.github/workflows/ci.yml` runs the same check
on every pull request that changes code compiled into `limerick-server` or
`limerick-engine`, `mods/`, the `--script` fixtures, or the proof tooling.
Docs-only and UI-only pull requests skip it. It builds `main` and the pull
request's merge commit, runs the `talk-and-task` scenario and every fixture
on both, and reads the intended differences from the pull request body: every
fenced block opened with ` ```toml intended-diffs ` and closed with
` ``` `, each on its own line. The author writes that block in the body by
hand; there is no helper. The report goes to
the job summary, a sticky pull request comment, and a `prove-diff` artifact
with every run. The job is part of the `CI gate` aggregate, so an undeclared
difference, an unobserved required declaration, or a nondeterministic head
run blocks the merge.

To change the declaration, edit the body and re-run the job; a body edit
alone does not trigger a run. Check a declaration before pushing with
`gh pr view <n> --json body --jq .body >body.md` and
`just prove-diff talk-and-task --intended-markdown body.md`.

## Belt-and-suspenders Lints

Both modes, every change:

- Any `.proofs/<...>` path in the diff fails. The directory held the retired
  bundles and stays gitignored; a tracked file there is a leftover.
- Changed files other than Markdown are scanned for language-specific
  unfinished-work macros and placeholder comments that often indicate partial
  completion.
