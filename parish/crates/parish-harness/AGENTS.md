# parish-harness — agent scope

Binary crate (`parish-harness`) that drives automated multi-turn playtests against a running Parish backend — an LLM plays the player, an LLM judges the transcript, each run is scored (deterministic gate + quality axes), discrete findings are recorded, and everything is persisted to a private SQLite DB plus on-disk artifacts. It also ships a read-only dashboard HTTP server (Axum, port 8787 by default) for browsing run history. This crate never links the game runtime; see root [`AGENTS.md`](../../../AGENTS.md). Command cookbook: [`README.md`](./README.md); approved design: [`docs/design/game-quality-harness.md`](../../../docs/design/game-quality-harness.md).

## Scoped commands

```sh
cargo build -p parish-harness
cargo test -p parish-harness                               # deterministic core + wire parity
cargo clippy -p parish-harness --all-targets -- -D warnings

# boot a backend first (port 3030)
bash parish/scripts/parish-mcp-backend.sh start

# play one run (scripted/no-key mode)
cargo run -p parish-harness -- run \
  --config parish/crates/parish-harness/configs/smoke.json \
  --turns 12 --player scripted --db /tmp/harness.db --artifacts /tmp/harness

# live dashboard
cargo run -p parish-harness -- serve \
  --db /tmp/harness.db --artifacts /tmp/harness --port 8787

# queue + worker
cargo run -p parish-harness -- queue --db /tmp/harness.db add --config configs/smoke.json
cargo run -p parish-harness -- worker --db /tmp/harness.db --once

# ingest a quality-harness skill run
cargo run -p parish-harness -- ingest --payload run.json --artifacts /tmp/artifacts

# A/B compare two stored runs
cargo run -p parish-harness -- compare --a 1 --b 2 --db /tmp/harness.db
```

## Gotchas

- **Drive the live app over HTTP only — never link `parish-tauri` or `parish-server` at runtime** (`parish-server` is a dev-dep solely for the wire-parity test). `HttpGameClient` in `client/backend.rs` posts to `POST /api/command` — the endpoint served by `parish-server`. The Tauri bridge serves `/api/submit-input` and does not handle `/api/command`, so the harness cannot drive the live Tauri window without a separate `parish-server` backend.
- **Wire mirror must stay in lockstep.** `client/wire.rs` hand-mirrors `parish_server::sync_types::CommandResponse`. A round-trip parity test (dev-dep on `parish-server`) fails CI if the server renames a field. Update both sides together.
- **Reuse `parish-inference` for all LLM calls.** `actor/api.rs` uses `AnyClient` / `build_client` / `generate_json`. Do not introduce a second HTTP-LLM client.
- **Own DB, own schema.** `harness.db` lives under `parish_persistence::paths::resolve_user_data_dir`. Mirror the persistence crate's WAL + hand-rolled `migrate()` idiom; do not touch the game's save schema.
- **Validate artifact content (rule #14).** `frame/renderer.rs` must `Err` on a blank/degenerate frame. Ingest (`ingest.rs`) applies the same check before replaying through the sink — never accept empty frame bytes.
- **Dirty tree excluded from regression deltas.** `git.rs` sets `dirty = true` when the working tree is unclean; the dashboard suppresses dirty runs from quality-over-time charts.
- **A gated `run` exits nonzero after persisting its summary.** Do not turn a deterministic crash/parser/timeout/empty-turn gate back into process success; scheduled automation relies on the CLI exit status.
- **`--player` sets both roles for back-compat; `--judge` overrides independently.** Passing only `--player api` also sets the judge to `api`. Passing `--judge api` separately keeps the roles independent (#1363).
- **No dependents.** No other crate in the workspace depends on `parish-harness` at runtime; `parish-server` appears only as a dev-dependency for the wire-parity test.

## Module map

`client/` — `HttpGameClient` trait + `backend.rs` Reqwest impl + `wire.rs` lenient mirror of server `CommandResponse` with round-trip parity test; `actor/` — `Player`/`Judge` trait seam (`traits.rs`), scripted (deterministic, no key), `parish-inference` API, and subagent driver; `score/` — deterministic `gate.rs`, quality `axes.rs`, `rubric.rs` with SHA-256 pinning, and `finding.rs` dedup by signature; `run/` — `execute_run` turn loop, per-turn artifact capture, and `build_actors` factory; `persist/` — `Db` SQLite sink, `schema.rs` migration, `queries.rs` read paths, `IngestRecord`; `dashboard/` — Axum read-only HTTP server (routes, SSE live-stream, static `index.html`); `queue/` — `QueueStore` enqueue/claim/complete/fail over the `queue` table, `QueueBackend` trait; `issue/` — `IssueFiler` deduped GitHub issue filing via `parish_core::ipc::create_bug_report`; `frame/` — `StateFrame` SVG + PNG renderer with non-blank content guard; `ingest.rs` — deserialize + validate + replay skill-run payloads into the DB; `config.rs` — `RunConfig` with SHA-256 content-addressing for A/B stability; `git.rs` — `GitProvenance` (sha, branch, dirty, pr_number); `cost.rs` — `CostTracker` accumulator (token wiring pending `AnyClient` usage exposure); `error.rs` — `HarnessError` / `Result`.
