# LEARNINGS — gotchas for future maintainers

Brief notes worth a future agent's time. Append new entries at the
bottom; don't lengthen items past 2-3 lines.

## Engine + runtime

- **`parish-cli` package is named `parish` in Cargo.toml.** `cargo run -p parish-cli` errors; use `cargo run -p parish`.
- **`--script` runs `run_script_mode` → `GameTestHarness`**, NOT `run_headless`. Different code path. Wire features in both.
- **`apply_movement` (in `parish-core/src/game_session.rs`) is the lowest-shared movement seam.** Tauri/server reach it via `game_loop::movement::handle_movement`; the script harness calls `apply_movement` directly. Publish movement-related events there for parity.
- **`snapshot.restore` wipes `tier_assignments`** — fixed by calling `npc_manager.seed_tier_state(world)` at the end of `restore`. The seed function is in `parish-npc/src/tier_assign.rs` and shares its BFS distance compute with `assign_tiers` but publishes no events. Tier is derivable from `(player_location, npc.location, npc.state)` so the snapshot file itself doesn't need to carry it.
- **`GameTestHarness::new()` runs in hundreds of tests.** Never default a side-effecting writer (filesystem, network) on in `new()` — `new_with_<feature>()` opt-in or env-var gate is the pattern. See `character_log` opt-in for example.
- **`PARISH_USER_DATA_DIR` env var IS the full path.** No app-name suffix is appended. `/tmp/x` → all apps use `/tmp/x`, not `/tmp/x/Rundale`. Resolution is in `parish-persistence/src/paths.rs::resolve_user_data_dir`.
- **`parish-flags.json` is not loaded by `GameTestHarness`.** Runtime `/flag enable/disable` doesn't persist across script runs in the harness. For test-time flag behaviour, set `app.flags` directly.
- **`DayType` has three variants** (`Weekday`, `Sunday`, `MarketDay`). No `Holiday`. See `parish-types/src/time.rs`.
- **`parish --script` uses the active mod from `mods/mod-list.toml`.** `GameTestHarness::new()` loads Rundale, but live script mode currently follows `active_setting`; use a Rundale-only `PARISH_MODS_DIR` for Rundale-specific proof runs when the active setting is `testbed`.
- **`mods/mod-list.toml` selects the default setting mod when both Rundale and testbed exist.** Keep the checked-in value at `active_setting = "rundale"` unless a test explicitly switches it.

## Character logs (`parish-core/src/character_log.rs`)

- **Profile section is rewritten every session**, journal is append-only. HTML comment markers `<!-- PROFILE_START -->` / `<!-- PROFILE_END -->` bound the rewritable region.
- **Dedup has three layers**: in-memory `bump_last_arrival` for in-session, disk-scan in `new()` for cross-session, and heading-level idempotence in `append_journal_entry` for replay safety.

## rundale-bench local MLX sweep

- **mlx-lm 4-bit RAM footprint is roughly `params_b × 0.55 GB` peak under inference.** 70B+ 4-bit and 70B 8-bit OOM a 48 GB M5 Pro mid-generation (system reboot required). `magnum-v4-72b-4bit` and `Midnight-Miqu-70B-v1.5-MLX-8Bit` are marked `peak_ram_gb_est >= 52` in `rundale-bench/candidates_local_mlx.toml` so `local_runner.py`'s headroom check skips them. Don't lower without measuring on a bigger box.
- **`local_runner.py`'s "ready" signal is `/v1/models` returning 200, not actual model load.** Big-Tiger-Gemma-27B passed the readiness probe ~10 s before weights finished mmapping, so the first POSTs got HTTP 404 and the sweep recorded `overall=0.00` despite the model being viable later. On re-run it produced only `<pad>` tokens anyway — Gemma 4-bit quant is broken for in-character prose (same failure mode as `gemma-4-e4b-it-4bit`).
- **Qwen3 thinking-mode leaks unless `chat_template_kwargs={"enable_thinking": False}` is injected on mlx_lm.server requests.** `parish/scripts/local-eval/eval_lib.py::THINKING_MLX_PREFIXES` lists the affected repos. Without the flag, reasoning fills `max_tokens` and the assistant content is empty → near-zero rubric scores. Cloud reasoning models (kimi-k2.5/6, deepseek-r1, claude, openai-o*, glm-4.6/7, gemini-2.5+) are already handled centrally by `_is_reasoning_model` → `_default_reasoning_for` in `eval_lib.py::call_chat`.
- **opencode.ai is fronted by Cloudflare which 403s the default Python-urllib UA** (firewall rule 1010). `call_chat` sets `User-Agent: rundale-bench/1.0 (+...)` so the request gets through.
