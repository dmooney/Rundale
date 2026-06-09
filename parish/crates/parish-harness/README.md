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

The harness talks HTTP to a running backend on `127.0.0.1:3030` (override with `--base-url`).
Two options, with different capabilities:

| Backend                  | Start                                             | Real LLM dialogue  | Screenshots  | Engine-model BYOK A/B          |
| ------------------------ | ------------------------------------------------- | ------------------ | ------------ | ------------------------------ |
| Headless `parish-server` | `bash parish/scripts/parish-mcp-backend.sh start` | no (simulator)     | no           | no (`/api/submit-byok` absent) |
| Desktop `parish-tauri`   | `cargo run -p parish-tauri -- --mcp-port 3030`    | yes (bundled/BYOK) | yes (F2/MCP) | yes                            |

Use the headless server for fast, deterministic, key-free runs (scripted player/judge). Use
the Tauri app when you want real model dialogue, per-turn screenshots, or to A/B engine models.

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
    // per-category BYOK; Tauri backend only
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
- **Engine-model A/B (`engine_models`) needs the Tauri backend** — the headless server has no
  `/api/submit-byok`, so per-category model overrides are skipped there.
- **Cost token columns are 0** today: the `CostTracker` seam exists but `AnyClient` does not
  yet surface per-call token usage. No numbers are faked.
- `QueueBackend` ships a SQLite implementation; a Postgres impl (multi-machine workers) is a
  documented future seam, not shipped.
