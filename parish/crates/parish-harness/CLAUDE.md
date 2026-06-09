# parish-harness — agent scope

Game quality-control harness. Runs automated multi-turn playtests where an LLM plays the
player and an LLM judges the transcript, then scores (gate + quality axes), records discrete
findings, and persists everything to its own SQLite DB + on-disk artifacts. Tool/entry-point
crate (binary `parish-harness`), **not** part of the game runtime.

**Usage / command cookbook:** [`README.md`](./README.md) (build → backend → `run` / `serve` /
`queue` / `worker` / `compare` / `db-path`, config knobs, player/judge modes, caveats).

See root [`AGENTS.md`](../../../AGENTS.md) and the approved design in
[`docs/design/game-quality-harness.md`](../../../docs/design/game-quality-harness.md).

## Hard rules

- **Drive the live app over HTTP only.** Never depend on `parish-tauri` or `parish-server`.
  Talk to a running backend (`127.0.0.1:3030`) via `client::HttpGameClient`. Wire types
  (`CommandResponse`) are mirrored locally in `client/wire.rs` with a parity test — keep them
  in lockstep with `parish-server::sync_types`.
- **Reuse `parish-inference` for all LLM calls.** `AnyClient` / `build_client` /
  `generate_json` already implement Anthropic + OpenAI-compat + local vllm-mlx with
  rate-limit/retry/timeout. Do not add another HTTP-LLM client.
- **Reuse `parish_core::ipc::bug_report`** for issue filing (Phase 2). Do not invent a GitHub
  client.
- **Own DB, own schema.** `harness.db` under `parish_persistence::paths::resolve_user_data_dir`.
  Mirror the persistence crate's WAL + hand-rolled `migrate()` idiom; do not reuse the game's
  save schema.
- **Validate artifact content (rule #14).** The state-frame renderer must `Err` on a
  blank/degenerate frame, never ship empty bytes.

## Scoped commands

```sh
cargo test -p parish-harness                              # deterministic core + wire parity
cargo clippy -p parish-harness --all-targets -- -D warnings
bash parish/scripts/parish-mcp-backend.sh start           # boot a backend first (port 3030)
# play one run (see README.md for run/serve/queue/worker/compare):
cargo run -p parish-harness -- run --config parish/crates/parish-harness/configs/smoke.json \
  --turns 12 --player scripted --db /tmp/harness.db --artifacts /tmp/harness
cargo run -p parish-harness -- serve --db /tmp/harness.db --artifacts /tmp/harness --port 8787
```

## Module map

`client/` HTTP game client + wire mirror, `score/` gate + axes + rubric + finding, `frame/`
state-frame renderer (png + svg), `actor/` Player/Judge seam (scripted + parish-inference
API), `persist/` SQLite schema + sink, `run/` the turn loop, `git.rs` provenance, `config.rs`
run config + content hash.
