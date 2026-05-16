Evidence type: gameplay transcript

# Proof — vllm-mlx two-slot routing fix + Tier 2 ERROR + console observability

Related issue: #982 (dialogue-quality findings surfaced by the new logs).

## What changed

Three commits on this branch:

1. `fix(npc): promote tier 2 inference failure to error level` —
   `parish/crates/parish-npc/src/ticks.rs:720`, `tracing::warn!` →
   `tracing::error!` so Tier 2 failures surface in normal monitoring.

2. `fix(tauri): hydrate category_overrides from parish.toml on startup` —
   Tauri previously dropped `[category_overrides.*]` blocks at startup,
   silently misrouting the two-slot Apple Silicon vllm-mlx loadout's
   small-model slot. Adds `GameConfig::apply_user_category_overrides()`
   and calls it from `parish_tauri::run()` after `load_user_config`.
   BYOK reuses the same helper.

3. `feat(observability): log resolved inference config + npc dialogue/emoji` —
   `parish-tauri::setup::log_resolved_inference_config` dumps the
   resolved per-category routing at startup; `parish-core::game_loop`
   adds `chat [npc]` and `npc-reaction` INFO lines.

## Reproduction

Local macOS, with the user's wizard-saved `parish.toml`:

```
provider = "vllm-mlx"
base_url = "http://localhost:8000"
model = "mlx-community/Qwen2.5-14B-Instruct-4bit"

[category_overrides.intent]
provider = "vllm-mlx"
base_url = "http://localhost:8001"
model = "mlx-community/Qwen2.5-1.5B-Instruct-4bit"

[category_overrides.reaction]
provider = "simulator"

[category_overrides.simulation]
provider = "simulator"
```

Command: `just demo 1 5` (5 auto-player turns, 1-second pause).

## Before (origin/main)

Console showed only the base provider line and a flood of 404s:

```
INFO  parish_inference::setup: vllm-mlx already running at http://localhost:8000
INFO  parish_inference::setup: vllm-mlx already running at http://localhost:8000
WARN  parish_npc::ticks: Tier 2 inference failed at The Forge: HTTP 404 Not Found at http://localhost:8000/v1/chat/completions
WARN  parish_npc::ticks: Tier 2 inference failed at The Mill:  HTTP 404 Not Found at http://localhost:8000/v1/chat/completions
WARN  parish_npc::ticks: Tier 2 inference failed at Darcy's Pub: HTTP 404 ...
ERROR parish_npc::reactions::emoji_reactions: inference call failed in infer_player_message_reaction error=Network("HTTP status client error (404 Not Found) for url (http://localhost:8000/v1/chat/completions)")
```

Root cause: `[category_overrides.intent]` was dropped at startup, so
`extra_vllm_mlx_slots` produced `(localhost:8000, 1.5B)` instead of
`(localhost:8001, 1.5B)`. `is_reachable(:8000)` returned true (14B
server), no second process was spawned, and runtime requests for the
1.5B model returned 404 from the dialogue server.

## After (this branch)

```
INFO  parish_inference::setup: vllm-mlx already running at http://localhost:8000
INFO  parish_inference::setup: vllm-mlx not detected, starting vllm-mlx serve...
INFO  parish_inference::setup: vllm-mlx ready after ~4000ms
INFO  parish_tauri_lib::setup: Inference ready (base) provider=vllmmlx base_url=http://localhost:8000 model=mlx-community/Qwen2.5-14B-Instruct-4bit
INFO  parish_tauri_lib::setup: Inference ready (category) category="dialogue"   provider=vllmmlx  base_url=http://localhost:8000 model=mlx-community/Qwen2.5-14B-Instruct-4bit
INFO  parish_tauri_lib::setup: Inference ready (category) category="simulation" provider=simulator
INFO  parish_tauri_lib::setup: Inference ready (category) category="intent"     provider=vllm-mlx base_url=http://localhost:8001 model=mlx-community/Qwen2.5-1.5B-Instruct-4bit
INFO  parish_tauri_lib::setup: Inference ready (category) category="reaction"   provider=simulator
```

The intent slot now spawns at `:8001` with the 1.5B model (confirmed via
`curl http://localhost:8001/v1/models` → `mlx-community/Qwen2.5-1.5B-Instruct-4bit`).

Five demo turns ran cleanly with zero 404s, zero
`infer_player_message_reaction` errors. The only ERROR was a Tier 2
cancellation on shutdown (`Tier 2 cancelled mid-stream`) — now at the
correct level.

NPC dialogue + the per-emoji event lines now appear on stderr:

```
INFO  parish_tauri_lib::commands:   chat [player] input=Good day! This village seems peaceful. What's the mood like around here?
INFO  parish_core::game_loop::npc_turn: chat [npc] npc=Brigid Ni Fhatharta reply=Good day to ye. 'Tis a peaceful place, sure enough. ...
```

(`npc-reaction` lines did not fire this session because the user's
config routes `reaction = simulator`; that is configuration, not a bug
in the logging path. The infrastructure works — see #982 for the
follow-up.)

## Tests

- `cargo test -p parish-core --lib` — 354 passed, 1 ignored.
- `cargo test -p parish-npc  --lib` — 417 passed.
- `cargo test -p parish-tauri --lib` —  68 passed.

## Mode parity note

The category-overrides hydration only changes the Tauri startup path
(`parish_tauri::run()`). `parish-server` does not load `parish.toml` at
startup (it is a deployed web service, not the desktop wizard target);
`parish-cli` has its own `--category-*` flag mechanism. The new helper
on `GameConfig` is in `parish-core` so any future entry point can reuse
it without duplicating the loop.
