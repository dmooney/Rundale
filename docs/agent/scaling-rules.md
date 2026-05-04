# Scaling Guardrails — Review Checklist

This checklist captures the architectural seams introduced in the wave-1
scaling work (issues #614–#622). Each rule names the seam it protects so
reviewers and agents know exactly which file to inspect.

Use it as a diff-review gate: if a PR touches any of the listed seams,
verify the corresponding rule is respected before merging.

---

### Rule 1: No global mutable game state

Per-session state must live on `AppState` (Axum / Tauri) or `GlobalState`
(headless CLI). Nothing that varies per user or per game session may be
stored in a `static` or module-level `Mutex` / `RwLock`. This keeps each
session isolated and makes horizontal scaling possible without shared-memory
coordination between workers.

**Seam:** [`parish/crates/parish-core/src/ipc/state.rs`](../../parish/crates/parish-core/src/ipc/state.rs) — state is owned by
`AppState`, injected via Axum `Extension` or Tauri `State`.
**What this prevents:** session bleed-through and race conditions when
multiple users or game instances share a process.

---

### Rule 2: Persistence only through `SessionStore`

Route handlers must not construct or borrow a raw `Database` handle.
All game-session reads and writes go through the `SessionStore` trait
([`parish/crates/parish-core/src/session_store.rs`](../../parish/crates/parish-core/src/session_store.rs)).
The concrete implementation is injected at startup; callers see only the
trait object. Direct `Database` calls in route code are a merge blocker.

**Seam:** [`parish/crates/parish-core/src/session_store.rs`](../../parish/crates/parish-core/src/session_store.rs) — see issue #614.
**What this prevents:** unmediated database access that bypasses session
lifecycle management and makes future backend swaps impossible.

---

### Rule 3: Real-time pushes only through `EventBus` with an explicit `Topic`

Server-sent events and real-time notifications must go through the `EventBus`
trait ([`parish/crates/parish-core/src/event_bus.rs`](../../parish/crates/parish-core/src/event_bus.rs)).
Callers supply an explicit `Topic` variant so events are addressable and
filterable. Direct calls to `broadcast::Sender::send` from route handlers or
game-logic crates are forbidden — they bypass the subscriber model and make
it impossible to fan out across processes.

**Seam:** [`parish/crates/parish-core/src/event_bus.rs`](../../parish/crates/parish-core/src/event_bus.rs) — see issue #616.
**What this prevents:** hard-wired in-process broadcast calls that cannot
be replaced with a pub/sub broker when the service scales horizontally.

---

### Rule 4: Inference only through `InferenceClient`

LLM calls must go through the `InferenceClient` trait defined in
`parish/crates/parish-inference/`. Callers must not construct provider-specific
clients (`OllamaClient`, `AnthropicClient`, `OpenAiClient`) directly in route
or game-logic code. The client is injected from `AppState`; swap the
implementation in tests with the `SimulatorClient`.

**Seam:** `parish/crates/parish-inference/src/client.rs` — see issue #617
(in-flight; the trait boundary is being formalised in that issue).
**What this prevents:** provider-specific imports leaking into shared logic
and tightly coupling the game engine to a single LLM vendor.

---

### Rule 5: Identity always via stable `account_id`

Authentication and session code must key game data on the stable `account_id`
returned by `IdentityStore`, never on raw OAuth email, provider user ID, or
cookie value. The `IdentityStore` and `SessionRegistry` traits
([`parish/crates/parish-core/src/identity.rs`](../../parish/crates/parish-core/src/identity.rs)) are the sole
source of truth for the mapping between external credentials and internal
account keys.

**Seam:** [`parish/crates/parish-core/src/identity.rs`](../../parish/crates/parish-core/src/identity.rs) — see issues #615
(IdentityStore) and #618 (account_id keying, in-flight).
**What this prevents:** account fragmentation when a user reconnects with
a refreshed token, and security bugs from keying on mutable external values.

---

### Rule 6: Mutating routes accept `Idempotency-Key` and are idempotent

Any HTTP handler that creates or modifies persistent state must:

1. Accept an `Idempotency-Key` request header.
2. Return the cached response for repeated requests with the same key within
   the deduplication window.
3. Be written so that executing it twice with identical inputs produces the
   same outcome (no duplicate records, no double-charges).

**Seam:** idempotency middleware — see issue #619 (in-flight; the middleware
layer will live in `parish/crates/parish-server/`).
**What this prevents:** duplicate game actions caused by client retries or
network hiccups, which corrupt save state and leaderboard data.

---

### Rule 7: Every HTTP request carries `request_id`; spans include `account_id` and `session_id`

The request-ID middleware ([`parish/crates/parish-server/src/middleware.rs`](../../parish/crates/parish-server/src/middleware.rs))
assigns a UUID to every inbound request and injects it as the `X-Request-Id`
response header and as a `RequestId` Axum extension. Tracing spans opened by
route handlers must attach `account_id` and `session_id` fields where those
values are available on `AppState`. Do not open spans without a request-ID in
server code.

**Seam:** [`parish/crates/parish-server/src/middleware.rs`](../../parish/crates/parish-server/src/middleware.rs) — see issue #621
(merged).
**What this prevents:** untraceble production failures where a cascade of
errors cannot be tied back to the originating request or account.

---

### Rule 8: Sticky-session routing required at the web tier

The web tier (Railway, nginx, or any future reverse proxy) must be configured
to route requests from the same browser session to the same server instance,
keyed on the `parish_sid` cookie. In-memory game state on `AppState` is not
replicated across instances; load-balancing without stickiness sends requests
to cold instances with no session state.

**Seam:** `parish_sid` cookie set by `parish/crates/parish-server/src/middleware.rs` —
the infrastructure configuration that enforces stickiness lives outside the
Rust codebase (load-balancer / `railway.toml`).
**What this prevents:** session state loss mid-game when a request lands on
the wrong server instance, and the cascade of 500 errors that follows.

---

### Rule 9: Mod content via `ModSource` trait

Runtime code must not read mod files directly from the filesystem via paths
like `mods/<id>/world.json`. All mod-content access goes through `ModSource`
([`parish/crates/parish-core/src/mod_source.rs`](../../parish/crates/parish-core/src/mod_source.rs)).
The concrete implementation (`LocalDiskModSource`) is injected at startup;
tests use an in-memory stub. Direct `fs::read` / `std::fs::File::open` calls
that target mod directories in route handlers or game-logic crates are a
merge blocker.

**Seam:** [`parish/crates/parish-core/src/mod_source.rs`](../../parish/crates/parish-core/src/mod_source.rs) — see issue #622
(merged).
**What this prevents:** filesystem coupling that makes remote mod hosting,
in-memory test fixtures, and future CDN-backed mod delivery impossible.

---

## Quick-reference table

| Rule | Keyword | Seam file | Issue |
|------|---------|-----------|-------|
| 1 | No global state | `ipc/state.rs` | — |
| 2 | `SessionStore` only | `session_store.rs` | #614 |
| 3 | `EventBus` + `Topic` | `event_bus.rs` | #616 |
| 4 | `InferenceClient` | `parish-inference/src/client.rs` | #617 (in-flight) |
| 5 | `account_id` keying | `identity.rs` | #615 / #618 (in-flight) |
| 6 | Idempotency-Key | (middleware, in-flight) | #619 (in-flight) |
| 7 | `request_id` + tracing | `middleware.rs` | #621 |
| 8 | Sticky-session routing | `parish_sid` cookie | — |
| 9 | `ModSource` trait | `mod_source.rs` | #622 |
