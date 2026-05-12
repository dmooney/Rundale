# parish-server — Technical Debt

## Open

*(none — all items resolved)*

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
