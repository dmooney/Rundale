# Idempotency-Key support (#619)

The Parish web server supports the `Idempotency-Key` request header on mutating
routes so clients can safely retry network failures without side-effects being
applied twice.

## How it works

1. The client sends a `POST` (or `PUT`/`PATCH`/`DELETE`) request with an
   `Idempotency-Key: <uuid>` header.
2. `middleware::idempotency_middleware` checks the process-wide LRU cache keyed
   by `(session_id, idempotency_key)`.
3. **Cache miss** — the request is forwarded to the handler. On a successful
   (2xx) response the body is buffered (capped at 1 MiB) and stored. The
   `Idempotency-Key` header is echoed back in the response.
4. **Cache hit (within TTL)** — the stored status code, headers, and body are
   returned immediately. The handler body does **not** execute. The
   `Idempotency-Key` header is echoed back.
5. **Cache hit (expired)** — the stale entry is evicted and the request is
   treated as a cache miss.

## Supported routes

| Method | Path | Handler |
|---|---|---|
| POST | `/api/save-game` | `routes::save_game` |
| POST | `/api/create-branch` | `routes::create_branch` |
| POST | `/api/new-save-file` | `routes::new_save_file` |
| POST | `/api/new-game` | `routes::new_game` |
| POST | `/api/editor-save` | `editor_routes::editor_save` |

All mutating routes benefit automatically; only `POST`, `PUT`, `PATCH`, and
`DELETE` requests are intercepted. Safe methods (`GET`, `HEAD`) pass through
unchanged.

## Cache parameters

| Parameter | Value | Constant |
|---|---|---|
| Capacity | 1 000 entries | `session::IDEMPOTENCY_CACHE_CAPACITY` |
| TTL | 24 hours | `session::IDEMPOTENCY_TTL` |
| Body cap | 1 MiB | hardcoded in middleware |

The LRU evicts the least-recently-used entry when capacity is reached. Both
eviction and TTL expiry cause re-execution on the next request with the same key.

## Cache key

`(session_id, idempotency_key)` — the `session_id` is the `parish_sid` UUID
injected by the session middleware. This scopes each key to one browser session
so two users cannot share or collide on each other's keys.

## Feature flag

The middleware is controlled by the `idempotency-key` feature flag. The flag is
**default-on** (per CLAUDE.md rule #6): the middleware is active unless the flag
is explicitly disabled.

To disable, add `"idempotency-key": false` to `parish-flags.json`:

```json
{
  "idempotency-key": false
}
```

## Mode parity note

Idempotency keys are an HTTP concept and apply **only to the web server**
(`parish-server`). Tauri IPC and the headless CLI use direct in-process function
calls and do not benefit from or require this mechanism.

## Implementation files

| File | Purpose |
|---|---|
| `parish/crates/parish-server/src/middleware.rs` | `idempotency_middleware`, `IDEMPOTENCY_KEY_HEADER`, `IdempotencyKeyExt` |
| `parish/crates/parish-server/src/session.rs` | `CachedResponse`, `IdempotencyCache`, `IDEMPOTENCY_CACHE_CAPACITY`, `IDEMPOTENCY_TTL` |
| `parish/crates/parish-server/src/lib.rs` | Wires the middleware into the router stack |
