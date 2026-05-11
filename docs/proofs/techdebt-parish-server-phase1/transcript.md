# Tech Debt Phase 1.1 — parish-server stale docs + dead code (TD-017..TD-020)

## Changes Made

### TD-017 (P3, Stale Docs) — `src/routes.rs`
Removed stale comment `// Semaphore is used by parish_core::game_loop::emit_npc_reactions (shared).` on the line previously at line 12. `Semaphore` is not imported or used anywhere in this file — the comment was a leftover from a previous implementation.

### TD-018 (P3, Stale Docs) — `src/session_store_impl.rs`
Updated the module-level doc comment to:
- Change title from "Default `SessionStore` + `IdentityStore` + `SessionRegistry` implementations" to "Default `IdentityStore` + `SessionStore` implementations"
- Clarify that `SqliteIdentityStore` is the only struct remaining here
- Document the relationship between this module and the canonical `session::SessionRegistry` in `session.rs`
- Note that `SqliteSessionRegistry` was a previous trait-based attempt that has been removed

### TD-019 (P3, Stale Docs) — `src/lib.rs` + `tests/security_headers.rs`
- Replaced the `# script-src 'unsafe-inline' (TODO: replace with hash)` section heading with `(deferred — see #543)` 
- Restructured the comment to be a design-decision record rather than an open TODO: it explains why `'unsafe-inline'` is retained, what the proper fix would be (SHA-256 hash or SvelteKit `kit.csp`), and links to issue #543
- Updated the matching comment in `tests/security_headers.rs` to say "Deferred (#543)" instead of "TODO(#543)"

### TD-020 (P2, Dead Code) — `src/session_store_impl.rs`
Removed the entirely unused `SqliteSessionRegistry` struct and all associated code:
- Struct definition `SqliteSessionRegistry` (~8 lines)
- `impl SqliteSessionRegistry` block (~8 lines)
- `impl SessionRegistryTrait for SqliteSessionRegistry` block (~180 lines including `lookup`, `register`, `touch`, `cleanup_stale`, `evict_idle`)
- Two unit tests: `session_registry_register_and_exists`, `session_registry_touch_updates_timestamp`
- `use dashmap::DashMap` import (only used by the removed struct)
- `use parish_core::identity::SessionRegistry as SessionRegistryTrait` import (only needed for the trait impl)
- `now_unix()` helper function (only used by SqliteSessionRegistry methods)
- `Duration`, `SystemTime`, `UNIX_EPOCH` from imports (only used by now_unix or SqliteSessionRegistry)
- Updated `SharedConn` and `open_sessions_db` doc comments to remove references to SqliteSessionRegistry

The canonical `session::SessionRegistry` in `session.rs` is the only production registry and was not touched.

## Files Changed
1. `parish/crates/parish-server/src/routes.rs` — removed stale Semaphore comment
2. `parish/crates/parish-server/src/session_store_impl.rs` — removed SqliteSessionRegistry, updated docs
3. `parish/crates/parish-server/src/lib.rs` — updated CSP docs from TODO to deferred-design note
4. `parish/crates/parish-server/tests/security_headers.rs` — updated matching TODO comment
5. `parish/crates/parish-server/TODO.md` — moved TD-017..TD-020 to Done with progress log

## Commands Run
```sh
cargo check -p parish-server     # verify compilation
cargo test -p parish-server      # 168 unit + 15 integration suites: all pass
cargo clippy -p parish-server --all-targets -- -D warnings   # clean
cargo fmt -p parish-server       # no changes needed
```

## Reasoning
`SqliteSessionRegistry` implemented `SessionRegistryTrait` but was never wired into production — `GlobalState.sessions` is typed as the concrete `session::SessionRegistry`, not `dyn SessionRegistryTrait`. Only its own unit tests constructed this type, making it pure dead code. Removal was straightforward because no other module referenced it.
