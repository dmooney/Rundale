# parish-harness

Game quality-control harness for Rundale. Runs automated multi-turn playtests where an LLM
plays the player and an LLM judges the finished transcript, scores each run (deterministic
**gates** + weighted quality **axes**), records discrete human-like **findings**, files
deduped GitHub issues for the `/backlog` drain, and serves a live dashboard.

It is a standalone tool (binary `parish-harness`) that drives a **running** Parish backend
over HTTP. It never links the game runtime.

> Agent-facing scope + hard rules live in [`CLAUDE.md`](./CLAUDE.md). Design of record:
> [`docs/design/game-quality-harness.md`](../../../docs/design/game-quality-harness.md);
> full architecture: [`docs/design/game-quality-harness-architecture.md`](../../../docs/design/game-quality-harness-architecture.md).

## Build

```sh
cargo build -p parish-harness --manifest-path parish/Cargo.toml
BIN=parish/target/debug/parish-harness        # or: cargo run -p parish-harness --
```

## TL;DR

```sh
# 1. start a backend the harness can drive
bash parish/scripts/parish-mcp-backend.sh start          # headless, port 3030

# 2. play + score one run
$BIN run --config parish/crates/parish-harness/configs/smoke.json \
  --turns 100 --player scripted \
  --db /tmp/harness.db --artifacts /tmp/harness

# 3. browse results
$BIN serve --db /tmp/harness.db --artifacts /tmp/harness --port 8787
#    open http://localhost:8787
```

## Backends

The harness talks to `POST /api/command` on a running `parish-server` at
`127.0.0.1:3030` (override with `--base-url`).

| Backend                  | Drive commands | Real model dialogue           | Player-visible screenshots |
| ------------------------ | -------------- | ----------------------------- | -------------------------- |
| Headless `parish-server` | yes            | yes with `--headless-models`  | no                         |
| Desktop `parish-tauri`   | no             | not reachable by this harness | no                         |

The desktop bridge intentionally does not serve `/api/command`, so this binary
cannot drive Tauri. Its `frame.png` artifacts are rendered state telemetry, not
captures of the player-visible UI. Per-category engine-model overrides work on
`parish-server` through runtime slash commands. Use the `quality-harness` skill
when a real model must drive the desktop game and inspect screenshots.

## Commands

### `run` — play and score one session

```sh
$BIN run --config <cfg.json> [options]
```

| Flag                     | Default                 | Meaning                                                        |
| ------------------------ | ----------------------- | -------------------------------------------------------------- |
| `--config <path>`        | (required)              | Run config JSON (see [Config](#config)).                       |
| `--turns <n>`            | `100`                   | Turns to play.                                                 |
| `--player <mode>`        | from config             | Override actor mode: `scripted` \| `api` \| `subagent`.        |
| `--base-url <url>`       | `http://127.0.0.1:3030` | Backend to drive.                                              |
| `--db <path>`            | user-data `harness.db`  | Telemetry DB (see [`db-path`](#db-path)).                      |
| `--artifacts <dir>`      | next to the DB          | Where per-run frames/logs are written.                         |
| `--file-issues`          | off                     | File deduped GitHub issues for findings (dry-run if no token). |
| `--dashboard-base <url>` | `http://localhost:8787` | Dashboard URL embedded in filed issue bodies.                  |
| `--command-timeout-ms`   | `60000`                 | Per-command timeout sent to the backend.                       |
| `--ready-timeout-secs`   | `30`                    | How long to wait for the backend to come up.                   |

Prints one summary line:
`run_id=… status={completed|gated} gate_reason=… quality_score=… findings=… rubric_sha256=… git_sha=… branch=…`.

### `serve` — live dashboard

```sh
$BIN serve --db <path> --artifacts <dir> [--port 8787]
```

Serves the UI at `/` and a JSON API: `GET /api/runs`, `/api/runs/{id}`,
`/api/runs/{id}/turns/{idx}/frame.png`, `/api/runs/{id}/stream` (SSE), `/api/timeline`,
`/api/compare?a=&b=`, `/api/cost`. The UI has Runs / Trends / A/B tabs (run list, 7-axis
radial chart, per-turn frame gallery, findings, score-vs-commit trend, A/B delta table).

### `queue` + `worker` — run many, unattended

```sh
$BIN queue --db <path> add --config <cfg.json> [--priority 5]
$BIN queue --db <path> list
$BIN worker --db <path> --artifacts <dir> [--base-url …] [--turns 100] \
            [--once] [--max-runs N] [--worker-id NAME] [--poll-secs 5]
```

`worker` claims pending configs (highest priority, then oldest first), runs each, marks it
done/failed, and loops. `--once` drains the queue then exits; `--max-runs N` caps total runs;
without `--once` it polls every `--poll-secs` for new work. Run several workers for throughput.

### `compare` — A/B two runs

```sh
$BIN compare --a <run_id> --b <run_id> [--db <path>]
```

Prints a per-axis delta table, the gate-status diff, and the finding-signature diff. The
dashboard `/api/compare` + `/api/timeline` give the same data plus score-vs-git-history.

### `ingest` — land an externally-produced run

```sh
$BIN ingest --payload <run.json> --artifacts <root> [--db <path>]
```

Persists a complete run that was produced **outside** the `run` loop — specifically a
quality-harness **skill** run (an agent drives the live game over the parish MCP, judges by
hand, and files bugs). The payload is replayed through the exact same sink writers `run` uses,
so the run shows on the dashboard indistinguishably from a binary run (quality score, 7-axis
bars, findings, per-turn frames, cost). Prints `ingested run <id>`.

`--artifacts <root>` must contain `runs/<uuid>/turns/NNN/frame.png` for every turn the payload
references (each frame must be non-empty — rule #14). The run's `artifact_dir` is stored as
`<root>/runs/<uuid>` so the dashboard's frame route resolves unchanged.

Payload schema (one JSON object):

| Field           | Type    | Notes                                                                                                                                                                                                                                                                        |
| --------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `config`        | object  | A `RunConfig`; its `label` is forced to `skill:quality-harness` so all skill runs share one `configs` row.                                                                                                                                                                   |
| `git`           | object  | `{sha, branch, dirty, pr_number}`.                                                                                                                                                                                                                                           |
| `rubric_sha256` | string  | Pass the binary's pinned rubric sha so timeline / A-B treat skill and binary runs as comparable.                                                                                                                                                                             |
| `uuid`          | string  | Run dir name under `runs/`.                                                                                                                                                                                                                                                  |
| `gate`          | object? | `{reason, turn, detail}` — present **iff** the run hard-failed; stores it gated with NULL quality.                                                                                                                                                                           |
| `quality_score` | number? | Weighted mean; recomputed from `axes` when omitted on a non-gated run.                                                                                                                                                                                                       |
| `cost`          | object  | `{cost_usd, player_tokens, judge_tokens}`.                                                                                                                                                                                                                                   |
| `turns[]`       | array   | Mirror of a `TurnRecord`; `frame_path` / `lines_path` are relative (`turns/NNN/...`). Optional `llm_transcript_path` (e.g. `turns/NNN/llm.json`) points to that turn's inference log; when set the file must exist, and the dashboard makes the turn clickable to view it.   |
| `axes[]`        | array   | `{axis, score, rationale}` — the 7 quality-axis keys.                                                                                                                                                                                                                        |
| `findings[]`    | array   | `{category, turn_index?, severity, description, evidence_quote, signature, issue_url?}`. `issue_url` (optional) is the GitHub issue the skill filed for this finding; it is stored on the finding row so the dashboard renders an `[issue]` link, exactly like a binary run. |

A worked sample lives at `parish/testing/fixtures/ingest_harness_skill/sample-payload.json`
(with `verify.sh` exercising the full ingest → serve → API round-trip).

### `backfill-issues` — recover finding → issue links

```sh
$BIN backfill-issues [--db <path>] [--repo owner/name] [--dry-run]
```

Links stored findings to their GitHub issues for runs that were ingested **without** an inline
`issue_url` (e.g. an older skill run, or a finding deduped against a prior run's issue). It lists
the repo's issues once, parses the `**Signature:** <sig>` line every filer embeds in the body
(plain, backtick-wrapped, or escaped-backtick forms all parse), and writes the matching
`html_url` onto any finding whose `issue_url` is NULL or a prior `filing-error:`. Exact-signature
only — never fuzzy — so it can't fabricate an "addressed" link. `--dry-run` prints the matches
without writing. Needs a GitHub token (`GITHUB_TOKEN` ‖ `GH_TOKEN` ‖ `PARISH_BUG_REPORT_TOKEN`).
The quality-harness skill runs this after each ingest as a safety net.

### `db-path`

```sh
$BIN db-path        # prints the resolved default harness.db location
```

Resolves under the platform user-data root, honoring `PARISH_USER_DATA_DIR`.

## Config

A run config is JSON. Copy [`configs/smoke.json`](./configs/smoke.json) and edit. Configs are
content-addressed (sha256 of the canonical JSON), so identical configs collapse to one row and
A/B comparisons are exact.

```jsonc
{
  "label": "smoke",
  "engine_models": {
    // applied to parish-server through /provider, /url, /model, and /key commands
    // "dialogue":  { "provider": "anthropic", "model": "claude-sonnet-4-6" },
    // "intent":    { "provider": "ollama", "model": "qwen2.5:1.5b", "base_url": "http://localhost:11434" }
  },
  "flags": [
    // { "name": "some-feature", "on": true }   // toggled via the /flag command
  ],
  "player": {
    "mode": "scripted", // scripted | api | subagent
    "model": null, // model id when mode = api
    "persona": "A curious newcomer …",
    "strategy": "Look around, greet anyone present, move between locations.",
  },
  "judge": {
    "mode": "scripted", // scripted | api | subagent
    "model": "claude-sonnet-4-6",
    "rubric_version": "v1",
    "rubric_sha256": null, // null = pin to current; set to fail on rubric drift
  },
  "gate": { "max_rejects": 3, "empty_burn_limit": 5 },
}
```

## Player / Judge modes

- **`scripted`** — deterministic, no API key. The player cycles safe commands; the judge
  derives axis scores from transcript statistics. Use for CI and the loop proof. Scores sit in
  a sensible mid-band; this is **not** a quality oracle.
- **`api`** — `parish-inference`-backed LLM (Anthropic / OpenAI-compat / local vllm-mlx). The
  real play-quality path. Reads `ANTHROPIC_API_KEY` from the environment for Anthropic.
- **`subagent`** — judge only: writes a bundle to `…/pending/<uuid>.json` and polls
  `…/done/<uuid>.json`, scored by an external Claude Code drain loop (no in-process key). The
  player falls back to `scripted` in this mode.

## Output

- **SQLite DB** (`harness.db`): `configs`, `runs`, `turns`, `axis_scores`, `findings`, `queue`.
  Small, queryable telemetry. WAL mode (dashboard reads while a run writes).
- **Artifacts** (`<artifacts>/runs/<uuid>/`): `config.json`, `transcript.json`,
  `verdict.json`, and per turn `turns/NNN/{frame.png, frame.svg, lines.json}`. The state-frame
  exposes simulation data the player never sees (NPC moods, grapevine distortion) as ground
  truth for the judge.

## Scoring

- **Gates** (deterministic hard-fails): crash, parser-reject, timeout, empty-turn-burn,
  JSON-parse, judge-critical. A tripped gate short-circuits the run and forces
  `quality_score = NULL` — a broken run is never given a quality number.
- **Axes** (0–100, weighted mean = the quality score when gates pass): `narrative_coherence`,
  `character_fidelity`, `world_responsiveness`, `intent_fidelity`, `immersion`, `progression`,
  `common_sense`.
- **Findings**: discrete bugs (NPC mid-sentence exit, third-person self-reference, name
  mix-up, intent-parse miss, …) with a canonical dedup signature, optionally filed as issues.

## Caveats

- The **scripted judge is a deterministic stand-in**, not real LLM judgment. For true
  play-quality numbers use `--player api` (needs a key).
- **This binary does not drive Tauri.** Engine-model A/B is supported against
  `parish-server`; player-visible screenshots still require the live desktop
  `quality-harness` skill.
- **Cost token columns are 0** today: the `CostTracker` seam exists but `AnyClient` does not
  yet surface per-call token usage. No numbers are faked.
- `QueueBackend` ships a SQLite implementation; a Postgres impl (multi-machine workers) is a
  documented future seam, not shipped.
