# Phase 3.1 — parish-server complexity refactors (TD-006 through TD-010)

## What was changed

Extracted private helper functions from five oversized functions in `parish-server`. Pure extraction — no behavior changes.

### TD-006 — `run_server` (547 lines -> 280 lines)

Extracted 12 construction-step helpers into `src/lib.rs`:

| Helper | Lines Extracted | Purpose |
|--------|----------------|---------|
| `handle_dotenv()` | 28 | .env loading (debug vs release) |
| `resolve_world_path(data_dir)` | 5 | parish.json / world.json selection |
| `run_llm_bootstrap(provider_cfg, config)` | 31 | Cloud env merge + provider setup |
| `resolve_splash_and_theme(game_mod)` | 11 | Game title, splash text, theme palette |
| `resolve_engine_and_ui_config(...)` | 31 | Engine config + UiConfigSnapshot construction |
| `open_session_components(...)` | 14 | sessions.db, identity store, pronunciations |
| `check_ws_signing_key_warning()` | 12 | WS key warning in debug builds |
| `init_tile_cache(...)` | 26 | Tile cache dir + TileCache init |
| `resolve_admission_control(...)` | 26 | Max concurrent sessions from env/TOML |
| `spawn_session_cleanup_background_task(...)` | 32 | Background stale-session reaper |
| `build_ip_rate_limiter_state()` | 15 | Global per-IP rate limiter |
| `should_use_tower_sessions(global)` | 14 | Session middleware selection |

Router build and middleware layers remain inline (preserving axum's type flow).

### TD-007 — `spawn_session_ticks` (230 lines -> 4 + 3 x ~75)

Decomposed into:
- `spawn_world_tick` — world snapshot, weather, NPC schedules, gossip, banshee
- `spawn_inactivity_tick` — 1-second idle check
- `spawn_autosave_tick` — periodic GameSnapshot capture + persist

### TD-008 — `purge_expired_disk_sessions` (160 lines -> 15 + 2 helpers)

Extracted:
- `collect_and_delete_expired_ids(&self, cutoff)` — DB query + two-phase atomic delete
- `cleanup_expired_session_dirs(ids, saves_root)` — standalone free fn, canonicalization + containment guard + remove_dir_all

### TD-009 — `load_branch` (100 lines -> 15 + 3 helpers)

Extracted:
- `validate_and_acquire_lock(state, body)` — path validation + containment check + advisory lock
- `load_branch_snapshot(path, branch_id)` — blocking DB open + snapshot load
- `restore_snapshot_and_emit(state, snapshot, ...)` — world restore + event emission + state update

### TD-010 — `idempotency_middleware` (135 lines -> 40 + 2 helpers)

Extracted:
- `try_replay_from_cache(cache, key, idem_key)` — LRU lookup, expiry check, response reconstruction
- `cache_successful_response(cache, response, idem_key, key)` — body buffering, cache insert, key echo

## Files changed

| File | Before (loc) | After (loc) | Delta |
|------|-------------|-------------|-------|
| `src/lib.rs` | 1517 | 1520 | +3 (net; helpers added, run_server shrunk) |
| `src/session.rs` | 1710 | 1690 | -20 |
| `src/routes.rs` | 2469 | 2490 | +21 (helpers extend file) |
| `src/middleware.rs` | 965 | 995 | +30 |
| `TODO.md` | — | — | Moved TD-006--010 to Done |

## Commands run

```
cargo check -p parish-server    # compilation clean
cargo test -p parish-server      # 168 unit + 63 integration: all pass
cargo clippy -p parish-server --all-targets -- -D warnings  # clean
```
