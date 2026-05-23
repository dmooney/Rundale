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
- **Dedup was removed in PR #1032 (89ab669d).** The bus now only publishes real physical movements (`schedule::tick_schedules`, `ticks::apply_tier3_updates`, `game_session::apply_movement`), so the writer is stateless beyond `log_dir`. Do not reintroduce `last_arrival` / `scan_existing_*` — they were obsoleted on purpose. (Issues #1013, #1014 closed because of this.)
- **Subscriber must rebind on branch switch.** Server (`parish-server/src/session.rs`) + Tauri (`parish-tauri/src/setup.rs`) + CLI (`App::rebind_log_managers_if_branch_changed`) each compare `current_branch_id` per event and rebuild the manager on mismatch. Otherwise post-`/load`/`/fork` events land under the old branch's `logs/branch-<old>/`. Same applies to `LocationLogManager` (#1011 #1034).

## rundale-bench local MLX sweep

- **mlx-lm 4-bit RAM footprint is roughly `params_b × 0.55 GB` peak under inference.** 70B+ 4-bit and 70B 8-bit OOM a 48 GB M5 Pro mid-generation (system reboot required). `magnum-v4-72b-4bit` and `Midnight-Miqu-70B-v1.5-MLX-8Bit` are marked `peak_ram_gb_est >= 52` in `rundale-bench/candidates_local_mlx.toml` so `local_runner.py`'s headroom check skips them. Don't lower without measuring on a bigger box.
- **`local_runner.py`'s "ready" signal is `/v1/models` returning 200, not actual model load.** Big-Tiger-Gemma-27B passed the readiness probe ~10 s before weights finished mmapping, so the first POSTs got HTTP 404 and the sweep recorded `overall=0.00` despite the model being viable later. On re-run it produced only `<pad>` tokens anyway — Gemma 4-bit quant is broken for in-character prose (same failure mode as `gemma-4-e4b-it-4bit`).
- **Qwen3 thinking-mode leaks unless `chat_template_kwargs={"enable_thinking": False}` is injected on mlx_lm.server requests.** `parish/scripts/local-eval/eval_lib.py::THINKING_MLX_PREFIXES` lists the affected repos. Without the flag, reasoning fills `max_tokens` and the assistant content is empty → near-zero rubric scores. Cloud reasoning models (kimi-k2.5/6, deepseek-r1, claude, openai-o*, glm-4.6/7, gemini-2.5+) are already handled centrally by `_is_reasoning_model` → `_default_reasoning_for` in `eval_lib.py::call_chat`.
- **opencode.ai is fronted by Cloudflare which 403s the default Python-urllib UA** (firewall rule 1010). `call_chat` sets `User-Agent: rundale-bench/1.0 (+...)` so the request gets through.

## Agent + tooling gotchas

- **`gh pr view --json baseRepository` was removed.** `parish/scripts/attach-proof.sh:67` still uses it and fails with "Unknown JSON field: baseRepository". Workaround: parse `gh pr view --json url --jq .url` and `sed` out `<owner>/<repo>`. Fallback for posting the proof bundle: `bash parish/scripts/render-proof-comment.sh <task-id> | gh pr comment <pr> --body-file -`.
- **`awk -v var=value` interprets backslash escapes in `value`.** `\n` becomes a real newline mid-string. To insert a multi-character literal containing `\n` (e.g. a `printf` format), write the trap to a temp file and use `getline < tf` inside `BEGIN`, not `-v`.
- **`gh pr edit` cannot change a PR's head branch.** If a sub-agent pushes a redo to `branch-v2` instead of the original branch, the PR keeps tracking the old SHA. Recovery: `git push origin +refs/remotes/origin/branch-v2:refs/heads/original-branch` to force-rewrite the PR's branch, then delete `branch-v2`.
- **Stop hooks under `set -euo pipefail` need `|| true` at the END of every command-substitution pipeline.** `grep` no-match exits 1 → pipefail propagates → silent exit. Visible only as "Stop hook error: Failed with non-blocking status code: No stderr output". All Stop hooks now have an ERR trap that surfaces future silent failures.
- **`PARISH_USER_DATA_DIR` only honors the FULL path** (no app_name appended). Tests setting `PARISH_USER_DATA_DIR=$tmp` should expect `$tmp/logs/branch-N/`, not `$tmp/<app>/logs/branch-N/`. See line 13 above; restated here because the rebind test (`headless.rs::rebind_log_managers_follows_branch_switch`) tripped on it.
- **CLI flag is `--game-mod <DIR>`, not `--mod`.** `parish --script ... --mod mods/rundale` errors with "unexpected argument". Use `--game-mod` (env: `PARISH_MOD`).
