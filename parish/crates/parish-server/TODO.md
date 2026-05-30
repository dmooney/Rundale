# parish-server — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-033 | Complexity | P1 | `src/routes.rs:1-3044` | Largest file in the crate. It combines snapshot/map/setup routes, input/game-loop adapters, save/branch lifecycle, admin checks, demo endpoints, mod switching, reaction handling, and ~1,350 lines of tests. Split by route family (`game_routes`, `save_routes`, `admin_routes`, `mod_routes`, `demo_routes`, tests) before adding more HTTP surface. |
| TD-034 | Complexity | P2 | `src/lib.rs:369-681` | `run_server()` still builds all route registrations, middleware layering, Tower-session setup, legal/static routes, rate limiting, and security headers inline after the earlier constructor-step extraction. Extract router/layer builders so startup config, route registration, and serving are separately reviewable. |
| TD-035 | Duplication | P2 | `src/editor_routes.rs:69-87`, `src/routes.rs:1578-1588` | `mods_root()` and `mods_root_path()` duplicate the same active-mod-parent lookup and fallback to `game_mod::find_default_mod()` / relative `mods`. Consolidate into one helper on `AppState` or a shared server utility so editor and public mod-listing routes cannot drift. |
| TD-036 | Security Debt | P2 | `src/lib.rs:96-126`, `tests/security_headers.rs:278-280` | CSP still requires `script-src 'unsafe-inline'` for the SvelteKit bootstrap and carries a `TODO` to replace it with build-time hashes (#543). Track this as open debt rather than leaving it hidden in comments, because it is a deliberate security relaxation. |
| TD-037 | API Shape | P2 | `src/state.rs:299-333` | `build_app_state()` takes seventeen parameters and needs `#[allow(clippy::too_many_arguments)]`. The comment documents why the flat state exists, but call sites still have a brittle positional constructor. Introduce a typed `AppStateParts`/builder object before adding more server-wide state. |
| TD-038 | Rule 9 / Packaging | P2 | `src/main.rs:86-120`, `src/lib.rs:683-710` | Startup helpers still parent-walk from `current_dir()` to find `mods/rundale`, UI dist, and `.env` fallbacks. They are documented as legacy/dev behavior, but packaged and daemonized launches should resolve from explicit CLI/env/config inputs only. Gate cwd discovery to debug/dev or replace it with configured paths. |
| TD-039 | Complexity | P2 | `src/session.rs:178-1448` | `session.rs` mixes registry persistence, session creation/restoration, inference queue initialization, autosave/tick scheduling, gossip budgeting, cloud-client construction, and tests. Split lifecycle, persistence, tick scheduling, and inference setup into narrower modules before more session orchestration lands. |
| TD-040 | Mode Parity (Rule #2) | P1 | `src/session.rs:1252-1260` | The web server's tick only runs `propagate_gossip_at_location` (and only when `!gossip_network.is_empty()`); it never runs the Tier-2 group-dialogue loop and never calls `create_gossip_from_tier2_event`. So on the Axum/web backend the gossip network stays permanently empty, no `GossipSpread` event ever fires, and the gossip branches in `parish-core`'s `location_log.rs`/`character_log.rs` are dead code for web players. CLI and Tauri both mint Tier-2 gossip; the server does not — a rule #2 parity gap (pre-existing, widened by the #1113 `GossipSpread` feature). Either wire Tier-2 minting into the server tick or document the gap explicitly. Pairs with `parish-core` TD-030 (the shared helper this should call once extracted). |

## In Progress

*(none)*

## Done

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Duplication | P2 | `src/auth.rs`, `src/middleware.rs` | `cookie_value()` now delegates to `extract_cookie_value()` from middleware, eliminating the duplicated `split(';')` → `strip_prefix` loop. |
| TD-002 | Duplication | P2 | `src/auth.rs` | Extracted `build_google_oauth_url()` helper used by both `login_google` and `login_google_tower`. |
| TD-003 | Duplication | P2 | `src/auth.rs` | Extracted `resolve_oauth_session()` helper used by both `callback_google` and `callback_google_tower`. |
| TD-004 | Duplication | P2 | `src/session.rs`, `src/session_store_impl.rs` | Extracted `apply_session_schema()` helper in session_store_impl.rs, called by both `SessionRegistry::open()` and `open_sessions_db()`. |
| TD-005 | Duplication | P3 | `src/lib.rs` | Extracted `make_auth_context()` helper used by all three `cf_access_guard` auth paths (loopback bypass, JWT success, debug fallback). |
| TD-006 | Complexity | P2 | `src/lib.rs` | `run_server` broken into separable constructor steps: `handle_dotenv`, `resolve_world_path`, `run_llm_bootstrap`, `resolve_splash_and_theme`, `resolve_engine_and_ui_config`, `open_session_components`, `check_ws_signing_key_warning`, `init_tile_cache`, `resolve_admission_control`, `spawn_session_cleanup_background_task`, `build_ip_rate_limiter_state`, `should_use_tower_sessions`. Router build remains inline. |
| TD-007 | Complexity | P2 | `src/session.rs` | `spawn_session_ticks` decomposed into `spawn_world_tick`, `spawn_inactivity_tick`, and `spawn_autosave_tick`. |
| TD-008 | Complexity | P2 | `src/session.rs` | `purge_expired_disk_sessions` extracted into `collect_and_delete_expired_ids` (nested method) and `cleanup_expired_session_dirs` (free function). |
| TD-009 | Complexity | P3 | `src/routes.rs` | `load_branch` extracted into `validate_and_acquire_lock`, `load_branch_snapshot`, and `restore_snapshot_and_emit`. |
| TD-010 | Complexity | P2 | `src/middleware.rs` | `idempotency_middleware` cache branches extracted into `try_replay_from_cache` and `cache_successful_response`. |
| TD-011 | Weak Tests | P1 | `src/ws.rs`, `tests/ws_integration.rs` | Extracted `validate_ws_upgrade()` from `ws_handler`; 7 unit-style tests plus 1 real TCP message-forwarding test via tokio-tungstenite. |
| TD-012 | Weak Tests | P1 | `tests/oauth_integration.rs` | 12 router-level integration tests for all 6 OAuth route handlers (legacy + tower-sessions variants). |
| TD-013 | Weak Tests | P2 | `tests/session_init.rs` | Added 2 integration tests for `POST /api/session-init`. |
| TD-014 | Weak Tests | P2 | `src/lib.rs` | Added unit tests for `GET /metrics`. |
| TD-015 | Weak Tests | P2 | `tests/auth_status.rs` | Added 3 integration tests for `GET /api/auth/status`. |
| TD-016 | Weak Tests | P3 | `src/lib.rs` | Added 3 functional tests for `ip_rate_limit_middleware`. |
| TD-017 | Stale Docs | P3 | `src/routes.rs` | Removed stale `Semaphore` comment. |
| TD-018 | Stale Docs | P3 | `src/session_store_impl.rs` | Updated module doc to clarify `SqliteSessionRegistry` is removed and `session::SessionRegistry` is canonical. |
| TD-019 | Stale Docs | P3 | `src/lib.rs` | Converted `TODO:` to descriptive comment referencing issue #543. |
| TD-020 | Dead Code | P2 | `src/session_store_impl.rs` | Deleted `SqliteSessionRegistry` struct, trait impl, and associated tests. |
| TD-021 | Manifest Hygiene | P3 | `Cargo.toml` | Dropped unused `tower-http` `cors` feature. |
| TD-022 | Manifest Hygiene | P3 | `Cargo.toml` | Removed unused `tracing-opentelemetry` and `opentelemetry` deps. |
| TD-023 | Manifest Hygiene | P3 | `Cargo.toml` | Moved `tower` to dev-dependencies (only used in tests). |
| TD-024 | Stale TODO | P2 | `src/lib.rs` | Removed no-op `MemoryStore` cleanup task body; replaced with a comment explaining the 365-day expiry bound. |
| TD-025 | Duplication | P2 | `src/session.rs` `src/auth.rs` `src/routes.rs` | Removed `google_account_for_session` from `SessionRegistry`; callers now use `identity_store.get_account`. Updated tests. |
| TD-026 | Duplication | P2 | `src/session.rs` | Extracted `finalize_session_entry` helper shared by `create_session` and `restore_session`. |
| TD-027 | Naming | P3 | `src/auth.rs` | Renamed `urlenccode` to `urlencode` across definition, call sites, and tests. |
| TD-028 | Rule 9 Violation | P2 | `src/lib.rs` | Switched `ensure_saves_dir()` to `resolve_project_saves_dir(&data_dir)`. |
| TD-029 | Weak Tests | P2 | `src/tile_routes.rs` | Added `parse_tile_path` unit tests (valid, missing suffix, too few/many segments, invalid coords, empty source, negative). |
| TD-030 | Weak Tests | P2 | `src/routes.rs` | Added `react_to_message` tests for valid emoji, invalid emoji, and injection snippet. |
| TD-031 | Weak Tests | P2 | `src/routes.rs` | Added `get_npcs_here_returns_json_array` test. |
| TD-032 | Complexity/Hidden Bug | P2 | `src/session.rs` | Fixed `restore_session` to select the most recently modified `.db` file instead of alphabetically first. |

## Progress Log

- **2026-05-07**: Phase 1.1 — resolved TD-017 through TD-020 (stale docs + dead code).
- **2026-05-07**: Phase 1.2 — resolved TD-013 through TD-016 (weak tests).
- **2026-05-07**: Phase 1.3 — resolved TD-001 through TD-005 (duplication extractions).
- **2026-05-07**: Phase 3.1 — resolved TD-006 through TD-010 (complexity).
- **2026-05-07**: Phase 4.1 — resolved TD-011 and TD-012 (WebSocket + OAuth weak tests).
- **2026-05-12**: Resolved TD-021 through TD-032. All changes verified with `cargo fmt`, `cargo clippy -p parish-server`, `cargo test -p parish-server`, and `cargo check --workspace`.
- **2026-05-25**: Refreshed the debt scan against current source. Reopened the ledger with TD-033 through TD-039 after checking LOC hotspots, inline TODOs, duplicated helpers, and clippy allows. Verified with `cargo check -p parish-server --all-targets`, `cargo clippy -p parish-server --all-targets -- -D warnings`, and `cargo test -p parish-server` (full test needed non-sandbox execution because wiremock/websocket tests bind local ports).
- **2026-05-28**: Weekly review of `c59562a..HEAD` added TD-040 — the server never mints Tier-2 gossip / emits `GossipSpread`, a rule #2 parity gap vs CLI + Tauri (pairs with parish-core TD-030).

## Follow-up

- **TD-033 first**: `routes.rs` is the highest-impact hotspot. Keep the first split mechanical and route-family based; avoid changing handler behavior while moving code.
- **TD-035 + TD-038**: Mod-root and cwd-fallback cleanup are related. Resolve path sources once at startup and keep request handlers on `AppState` data.
- **TD-036**: CSP hashing needs a build/frontend handoff. Do it as a focused security-hardening change with browser verification, not as incidental server cleanup.
