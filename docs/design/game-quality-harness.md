# Design: game-quality-harness (`parish-harness` crate)

> Full architecture lives in the approved plan. This note is the crate-scoped design of
> record and names the observable signals for the proof gate.

## What it does

`parish-harness` is a new tool/entry-point crate that runs automated multi-turn playtests of
Rundale. An LLM plays the player; an LLM judges the finished transcript. Each run drives a
**live Parish backend over HTTP** (`127.0.0.1:3030`), plays N turns, captures per-turn state

- artifacts, evaluates deterministic hard-fail **gates**, scores ~7 judge **axes** into a
  weighted-mean quality score, records discrete human-like **findings**, and persists
  everything to its own SQLite DB + on-disk artifacts. Later phases file deduped GitHub issues
  and serve a live dashboard. Goal: measure whether the _game_ is good and detect regressions
  tied to git history, run unattended at scale.

It is **not** a gameplay feature — it adds no behavior to the game itself; it observes the
game through existing public IPC.

## Affected subsystems

- **New crate** `parish/crates/parish-harness/` (binary `parish-harness` + library). Added to
  `parish/Cargo.toml` members. Tool/entry-point crate like `parish-geo-tool`.
- **Reused, unchanged:**
  - `parish-inference` — `AnyClient` / `build_client` / `generate_json::<T>` for the Player +
    Judge LLM transport (native Anthropic Messages + OpenAI-compat + local vllm-mlx, with
    rate-limit/retry/timeout/secret-scrub). No new Anthropic SDK.
  - `parish-core` — `EngineState`, `DiagnosticPayload`, and `ipc::bug_report::create_bug_report`
    (issue filing, Phase 2).
  - `parish-persistence` — `paths::resolve_user_data_dir` for the DB/artifact root; the WAL +
    hand-rolled `migrate()` idiom (own DB, own schema).
  - `parish-mcp` `backend.rs` HTTP-client shape (kebab path, null=GET/else=POST) — mirrored,
    not depended on.
- **No change to runtime-shipping crates** (`parish-tauri`/`parish-server`/game logic). The
  harness depends on `parish-core` only and talks HTTP; wire types (`CommandResponse`,
  `StateBundle`) are mirrored locally like `parish-client` does, with a key-set parity test.

## Data model

Own SQLite DB `harness.db` under the user-data root. Tables: `configs` (content-addressed
`config_sha256`), `queue` (worker seam), `runs` (gate + quality + git provenance + cost),
`turns` (per-turn outcome/kind/engine-state + artifact paths), `axis_scores`, `findings`
(dedup `signature`, `issue_url`). Heavy bytes (screenshots, state-frames, transcripts, raw LLM
exchanges) live on disk under `runs/<uuid>/`, referenced by path. Full column list in the
plan file.

## Observable signals (proof gate)

The deliverable is a tool, so the harness driving a live backend is the proof, not an in-game
fixture. Signals:

- `cargo test -p parish-harness` — deterministic core: `gate_*`, `axes_*`, `rubric_*`
  (sha256 drift → `Err`), `config_hash`, `signature_*`, `wire_parity`, `frame_nonblank`.
- `parish-harness run` summary line: `run_id=… turns=N status={completed|gated} gate_reason=…
quality_score=…`.
- `harness.db`: one `runs` row with `turn_count`, gate fields, `quality_score`,
  `rubric_sha256`, `git_sha`/`git_branch`; N `turns` rows; 7 `axis_scores` rows on a passing
  run; on-disk non-blank state-frame PNGs.
- `cargo test -p parish-core architecture_fitness` green (no tauri/server dependency edge).

## Feature flag

Per `AGENTS.md` §6, any game-side hook gates behind `config.flags.is_enabled(
"quality-harness")` (default-on, documented in PR). Phase 1 adds no game-side hook — the
harness is a separate, opt-in binary — so the flag is reserved for any future runtime-path
touch (e.g. a new `/api` route the dashboard might need). Per-run feature flags the harness
_applies_ to the game under test go through the existing `/flag` slash command.
