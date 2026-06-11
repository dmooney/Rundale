# Acceptance Criteria: tauri-parity

## Task

The two tracked **Tauri follow-ups** from epic #1366 §2 and §3. Both fixes
already landed on the **server** side (`parish-server`) and were explicitly
scoped out of Tauri at the time (see `log-backpressure` C9 and `appstate-locks`
C11/CS-7, the direct predecessors). This task closes the parity gap so the two
backends do not silently diverge on lock granularity or blocking-pool churn.

**A. Bounded chronicle-log writer (from #1415 / §3).** Apply the
bounded-writer pattern to `parish-tauri/src/setup.rs`'s
`spawn_character_log_subscriber` (~line 540) and `spawn_location_log_subscriber`
(~line 622). Both currently call `tokio::task::spawn_blocking` **per world
event** inside the `rx.recv()` arm (setup.rs lines ~600 and ~678), flooding the
shared Tokio blocking pool with one short-lived task per `GameEvent`, with no
bound. The server reference is `parish-server/src/session/ticks.rs` on main
(bounded mpsc `LOG_WRITER_QUEUE_CAPACITY = 32`, one long-lived writer task per
subscriber drained serially, block-not-drop saturation, cancellation-aware send,
post-rebind manager clone enqueued).

**B. Lock grouping (from #1416 / §2).** Apply the `InferenceClients` +
`SaveIdentity` groupings to `parish-tauri`'s flat `AppState`
(`parish-tauri/src/lib.rs` ~446). Mirror the server's shape exactly: members
stay individually `Mutex`-wrapped inside the group struct, accessed via the
field path (`state.inference.client`, `state.save_identity.save_path`), so the
per-field `&Mutex<…>` borrows that `parish_core::game_loop::{GameLoopContext,
NewGameParams}` require keep compiling unchanged.

Legend: **[INV]** = behavior-preserving invariant (green before and after).
**[NEW]** = new functionality this task adds.

---

## Prescribed mechanism (do not re-decide)

### A — Hoist the writer-task helper into `parish-chronicle` (the Rule-12 call)

**Decision: SHARE, do not duplicate.** Hoist a single generic writer-task
helper into the backend-agnostic `parish-chronicle` crate (re-exported through
`parish-core`, where the managers already live — LEARNINGS.md line 78), and have
**both** `parish-server/src/session/ticks.rs` and `parish-tauri/src/setup.rs`
call it. Do **not** copy the server's `run_character_log_writer` /
`run_location_log_writer` bodies into `parish-tauri`.

**Justification (Rule 12, #687/#696).** The orchestration body is _not_ thin and
it is about to be quadrupled. The server already carries **two** near-identical
helpers (`run_character_log_writer`, `run_location_log_writer`) that differ only
in the concrete manager type — `CharacterLogManager` vs `LocationLogManager` —
and in nothing else: both loop `while let Some((mgr, event)) = rx.recv().await`,
`spawn_blocking` a `world.blocking_lock()` + `npc_manager.blocking_lock()` +
`mgr.process_event(&event, &world, &npc_mgr)`, and `match handle.await` with the
same three `tracing::warn!` arms. Copy-pasting them into Tauri would make **four**
copies of the identical block across **two** entry-point crates — exactly the
"invisible at review time… silently produces security/behaviour drift" failure
Rule 12 names. The two managers already share a byte-identical method signature
(`pub fn process_event(&self, &GameEvent, &WorldState, &NpcManager) -> Result<()>`
— verified in `parish-chronicle/src/{character_log,location_log}.rs`), so the
generic abstraction is cheap and natural:

- Define a tiny trait in `parish-chronicle` (e.g. `ChronicleWriter` with one
  method `fn process_event(&self, &GameEvent, &WorldState, &NpcManager) ->
anyhow::Result<()>`, plus `Clone + Send + 'static`) and `impl` it for both
  `CharacterLogManager` and `LocationLogManager` (one-line forwarding impls), OR
  use a plain generic bound `M: Clone + Send + 'static` with a `process_event`
  fn pointer / closure passed in. The trait form is preferred (self-documenting,
  reusable by the third entry point — the CLI — later).
- Hoist the writer-loop body into a single generic
  `pub async fn run_chronicle_log_writer<M, F>(…)` (or
  `run_chronicle_log_writer<M: ChronicleWriter>(…)`) in `parish-chronicle`. It
  must be **parameterized over the runtime-specific concern**, which is "how to
  acquire `&WorldState` + `&NpcManager` for the blocking write" — passed as a
  closure / trait object so the helper does **not** depend on either
  `AppState` type (`parish-chronicle` is backend-agnostic and must stay off the
  `tauri`/`axum` deps per the architecture-fitness test). The natural shape:
  the caller hands the helper a bounded `mpsc::Receiver<(M, GameEvent)>` plus a
  `Fn() -> (impl Deref<Target=WorldState>, impl Deref<Target=NpcManager>)`-style
  accessor that does the two `blocking_lock`s. Each entry-point crate supplies
  the accessor closing over its own `Arc<AppState>`.
- **Both** `parish-server` and `parish-tauri` then become thin wiring: build a
  `tokio::sync::mpsc::channel(LOG_WRITER_QUEUE_CAPACITY)`, spawn the shared
  `run_chronicle_log_writer`, and replace their per-event `spawn_blocking` +
  inline `await` with a cancellation-aware `tx.send((manager.clone(), event))`.
  `parish-server/src/session/ticks.rs` must drop its two now-redundant private
  `run_character_log_writer` / `run_location_log_writer` and call the shared
  helper (de-duplicating the existing pair is part of this task, not a separate
  one — otherwise the hoist leaves three copies, two of them dead).
- `LOG_WRITER_QUEUE_CAPACITY = 32` becomes a `pub const` in `parish-chronicle`
  (single source of truth) consumed by both entry points, replacing the
  server's local `const` in `ticks.rs`.

**If the implementer discovers the accessor-closure bridging is genuinely
infeasible** (e.g. a borrow-checker wall on handing out the two blocking guards
through a closure boundary), the fallback is a documented deviation: keep the
helper generic over the **manager type only** (`M: ChronicleWriter`) and let
each entry point pass its `Arc<AppState>` plus a `world`/`npc` accessor as two
`fn(&AppState) -> &Mutex<…>` pointers — still one shared body, still no
copy-paste. Pure body-duplication into `parish-tauri` is **rejected** and must
not be the outcome; if the implementer believes duplication is unavoidable they
must STOP and re-open AC sign-off with the borrow-checker evidence.

### A — Tauri lifecycle adaptation (differs from the server)

The server creates its subscribers + writer task inside
`spawn_session_ticks(state, shutdown_token) -> Vec<JoinHandle<()>>` and collects
every handle into the returned vec (per-session, dropped on session eviction —
Rule 11). **Tauri's lifecycle is different and the adaptation must respect it:**

1. **No per-session token; one app-lifetime token.** Tauri has a single
   `AppState::shutdown_token: CancellationToken` (lib.rs ~562), created once at
   startup (lib.rs ~1268) and cancelled on `tauri::WindowEvent::Destroyed`
   (lib.rs ~1481). There is exactly **one** session per process. The new writer
   task must observe **this** `state.shutdown_token` (clone it, same as the
   existing subscriber loops at setup.rs 570/649) so it tears down on window
   close — there is no `spawn_session_ticks`-style token parameter to thread.

2. **Fire-and-forget spawns, no handle vec.** Tauri's subscribers are each
   spawned via a bare `tokio::spawn` from
   `spawn_character_log_subscriber` / `spawn_location_log_subscriber` (called in
   the `lib.rs` setup closure ~1465–1466) and the `JoinHandle` is **dropped**
   (fire-and-forget). The new writer task follows the same pattern: spawn it
   inside the same `spawn_*_log_subscriber` fn, `tokio::spawn`, drop the handle.
   Do **not** invent a handle-collection vec for Tauri — that would be a
   gratuitous lifecycle divergence from the rest of `setup.rs`. Cancellation is
   covered by (a) the writer's own `tx` being dropped when the subscriber loop
   exits on `token.cancelled()` (closing the channel → `rx.recv()` returns
   `None` → writer loop ends), which is the primary teardown path, identical to
   the server's "dropping `tx` closes the channel so the writer task exits".

3. **`app_name` is a parameter, not derived.** Unlike the server (which calls
   `parish_core::game_mod::app_name_from_mod(&s.game_mod)` inside the task),
   Tauri receives `app_name: String` as a fn parameter (setup.rs 540/622).
   Preserve that — the writer-helper hoist must not require deriving app_name
   inside the shared helper; the manager is already constructed with the right
   `app_name` before the channel send.

4. **Send must be cancellation-aware.** Mirror the server: wrap the
   `tx.send((manager.clone(), event))` in a
   `tokio::select! { _ = token.cancelled() => break, send_res = tx.send(…) => … }`
   so a saturated channel on a closing window cannot stall teardown, and a
   closed channel (`send_res.is_err()`) breaks the loop.

5. **Branch-rebind stays in the async recv loop.** The post-rebind manager
   clone is what gets enqueued (setup.rs 580–593 / 659–671 already do the
   rebind; #1011/#1034; LEARNINGS line 27). The cloned **post-rebind** manager
   must be the one sent, so events after a `/load`/`/fork` land under the new
   branch dir.

### B — Lock grouping shape (mirror the server exactly)

Introduce two sub-structs on `parish-tauri/src/lib.rs`, **structurally identical**
to `parish-server/src/state.rs`'s `InferenceClients` and `SaveIdentity`:

- `InferenceClients { client: Mutex<Option<AnyClient>>, cloud_client:
Mutex<Option<AnyClient>>, inference_queue: Mutex<Option<InferenceQueue>> }` —
  members stay individually `Mutex`-wrapped. Replaces the three flat fields
  `client` / `cloud_client` / `inference_queue` with one `inference:
InferenceClients` field.
- `SaveIdentity { save_path: Mutex<Option<PathBuf>>, current_branch_id:
Mutex<Option<i64>>, current_branch_name: Mutex<Option<String>> }`. Replaces
  the three flat fields `save_path` / `current_branch_id` / `current_branch_name`
  with one `save_identity: SaveIdentity` field.

**`config` is NOT folded into the inference group** (server CS-4: `config` is
acquired far more often and on paths with no client; folding widens the critical
section). **`save_lock` / `save_db`-equivalents are NOT in the save-identity
group** (server CS-5). This is a change of lock **granularity**, never lock
**order**. Because members stay `Mutex`-wrapped, every `GameLoopContext` /
`NewGameParams` construction in Tauri keeps working by changing only the borrow
path (`&state.client` → `&state.inference.client`, `&state.save_path` →
`&state.save_identity.save_path`).

### B — Lock-order sensor (decision: extend coverage to `parish-tauri`)

**Decision: generalize the server's `lock_order_fitness` sensor to also scan
`parish-tauri/src`, OR add a sibling sensor.** The server already ships a
drop-aware textual sensor (`parish-server/tests/lock_order_fitness.rs`) that
reads a `LOCK_ORDER` const, maps dissolved group members to their group node,
and asserts no held guard inverts the chain — with a permanent in-test negative
fixture (`sensor_rejects_inverted_pair`). Tauri's flat `AppState` has the
**identical deadlock surface** and the same documented lock-ordering contract
(lib.rs ~425–445), so leaving it unsensored re-opens exactly the #483 hazard the
server now guards. **Prescribed:** add `parish-tauri/tests/lock_order_fitness.rs`
modeled on the server's, with:

- a `pub const LOCK_ORDER: &[&str]` next to the Tauri `AppState` (lib.rs),
  encoding Tauri's canonical chain with the grouped `inference` and
  `save_identity` nodes, rewriting the prose doc-comment to **reference** the
  const (single source of truth, mirrors server C3);
- the same drop-aware scanner mechanics and `group_of()` member→node mapping;
- the same permanent negative fixture (`sensor_rejects_inverted_pair`).

If the scanner logic can be factored to avoid duplicating ~250 lines across the
two test files, prefer that (e.g. a shared `parish-chronicle`-or-test-support
helper), but a second test file modeled on the server's is acceptable since test
sensors are not production orchestration (Rule 12 targets runtime orchestration
bodies, not fitness-test scaffolding) — note the choice in the PR.

---

## Criteria

### C1 [NEW] — Per-event `spawn_blocking` is gone from the two Tauri log subscribers

In `parish-tauri/src/setup.rs`, neither `spawn_character_log_subscriber` nor
`spawn_location_log_subscriber` calls `tokio::task::spawn_blocking` **inside the
`rx.recv()` event-handling arm**. The blocking `process_event` work now runs in a
single long-lived writer task per subscriber that drains a bounded
`tokio::sync::mpsc` channel. (`spawn_blocking` may still appear once **inside the
shared writer helper** to enter the blocking context — but not once-per-event in
the recv loop.)
Observable via: `grep -n 'spawn_blocking' parish/crates/parish-tauri/src/setup.rs`
returns no hit inside the two subscriber recv loops (lines ~600/~678 in the old
code); the recv arms now contain a cancellation-aware `tx.send(...)`. The
chat-transcript subscriber and the other Tauri tick tasks are unchanged.

### C2 [NEW] — Writer-task helper is SHARED in `parish-chronicle`, not duplicated (Rule 12)

A single generic writer-task helper (e.g. `run_chronicle_log_writer`) and the
bound constant (`LOG_WRITER_QUEUE_CAPACITY`) live in `parish-chronicle`
(re-exported via `parish-core`). **Both** `parish-server` and `parish-tauri`
call it; neither defines a private copy. The server's previous
`run_character_log_writer` / `run_location_log_writer` are **removed** (folded
into the shared helper).
Observable via:
`grep -rn 'fn run_chronicle_log_writer\|const LOG_WRITER_QUEUE_CAPACITY' parish/crates/parish-chronicle/src`
returns the definitions;
`grep -rn 'fn run_character_log_writer\|fn run_location_log_writer' parish/crates`
returns **zero** hits (the server copies are gone);
`grep -rn 'spawn_blocking' parish/crates/parish-tauri/src/setup.rs` shows no
per-event spawn; `parish-chronicle/Cargo.toml` gains no `tauri`/`axum` dep
(architecture-fitness `backend_agnostic_crates_do_not_pull_runtime_deps` stays
green).

### C3 [NEW] — Bound is a single named, documented `pub const`

`LOG_WRITER_QUEUE_CAPACITY` is a `pub const usize` in `parish-chronicle` with a
doc comment explaining the value and the **block-not-drop** saturation behavior
(copied/adapted from the server's `ticks.rs` doc comment). The server's local
`const` in `ticks.rs` is removed in favour of the shared one.
Observable via: `grep -rn 'LOG_WRITER_QUEUE_CAPACITY' parish/crates` shows one
`pub const` definition (in `parish-chronicle`) plus the use sites in both entry
points; the server's old `const LOG_WRITER_QUEUE_CAPACITY: usize = 32;` in
`ticks.rs` is gone.

### C4 [NEW] — Saturation behavior is block-not-drop and is unit-tested in the shared crate

A unit test in `parish-chronicle` exercises the bound directly: it floods the
bounded writer channel beyond `LOG_WRITER_QUEUE_CAPACITY` and asserts the
**defined** behavior — the sender applies backpressure (blocks/pends) rather than
dropping, and once the writer drains, **every** enqueued event is processed (no
loss). This mirrors the server's existing
`log_writer_channel_blocks_when_full_and_loses_no_events` test (which can be
moved into `parish-chronicle` alongside the hoisted helper rather than
duplicated). The test name makes the contract explicit.
Observable via: `cargo test -p parish-chronicle log_writer` runs the test and it
passes; it asserts a processed count equal to the number of items sent (no drops)
under an over-capacity flood.

### C5 [NEW] — Tauri `AppState` inference group behind one node

The Tauri `AppState` (`parish-tauri/src/lib.rs`) no longer has three separate
`client` / `cloud_client` / `inference_queue` `Mutex` fields. They are replaced
by a single `inference: InferenceClients` field whose three members stay
individually `Mutex`-wrapped. Every former `state.client.lock()` /
`state.cloud_client.lock()` / `state.inference_queue.lock()` site (14 inventoried
sites across `command_host.rs`, `commands/{demo,input,reactions,movement,admin}.rs`,
`setup.rs`, `mcp_bridge.rs`) now reads through `state.inference.<member>`.
Observable via:
`grep -rn 'state\.client\.lock\|state\.cloud_client\.lock\|state\.inference_queue\.lock\|self\.state\.client\.lock\|self\.state\.cloud_client\.lock' parish/crates/parish-tauri/src`
returns **zero** hits outside the sub-struct definition;
`grep -n 'struct InferenceClients' parish/crates/parish-tauri/src/lib.rs` returns
a hit; `cargo build -p parish-tauri` succeeds.

### C6 [NEW] — Tauri `AppState` save-identity group behind one node

The Tauri `AppState` no longer has three separate `save_path` /
`current_branch_id` / `current_branch_name` `Mutex` fields; they are replaced by
a single `save_identity: SaveIdentity` field with the three members individually
`Mutex`-wrapped. The 41 inventoried member sites (across `commands/saves.rs`,
`command_host.rs`, `commands/admin.rs`, `mcp_bridge.rs`, `setup.rs`) now read
through `state.save_identity.<member>`.
Observable via:
`grep -rn 'state\.save_path\.lock\|state\.current_branch_id\.lock\|state\.current_branch_name\.lock' parish/crates/parish-tauri/src`
returns **zero** hits outside the sub-struct definition;
`grep -n 'struct SaveIdentity' parish/crates/parish-tauri/src/lib.rs` returns a
hit.

### C7 [INV] — Shared `parish-core` game-loop signatures unchanged (Rule 2 & #12)

The grouping must **not** be pushed into
`parish_core::game_loop::context::GameLoopContext` or
`parish_core::game_loop::save::NewGameParams`, which take `client` /
`cloud_client` / `inference_queue` / `save_path` / `current_branch_id` /
`current_branch_name` as **separate `&Mutex<…>` references** and are constructed
by all three entry points. The Tauri construction sites
(`commands/input.rs` ~368 & ~401 for `GameLoopContext`, `commands/saves.rs` ~273
for `NewGameParams`) change only the **borrow path**
(`&state.client` → `&state.inference.client`, `&state.save_path` →
`&state.save_identity.save_path`), never the struct field signatures.
Observable via: `git diff --stat parish/crates/parish-core/src/game_loop/` shows
`context.rs` and `save.rs` **unmodified**; `cargo build -p parish-server` and
`cargo build -p parish-engine` both succeed with no edits to their
`GameLoopContext` / `NewGameParams` call-sites.

### C8 [NEW] — `LOCK_ORDER` const + lock-order sensor for `parish-tauri`

A module-level `pub const LOCK_ORDER: &[&str]` lives next to the Tauri `AppState`
(lib.rs), listing the chain in canonical order with the grouped `inference` and
`save_identity` nodes (not the dissolved members). The prose ordering
doc-comment is rewritten to reference the const. A
`parish-tauri/tests/lock_order_fitness.rs` (modeled on the server's) reads
`LOCK_ORDER`, maps dissolved members to their group node, scans the Tauri
handler/command/setup source for nested out-of-order acquisitions, and is
**green** on the refactored tree. The assert message names the file + offending
pair, cites Rule 11 / #483, and points at `LOCK_ORDER`.
Observable via: `grep -n 'const LOCK_ORDER' parish/crates/parish-tauri/src/lib.rs`
returns a hit; the doc-comment no longer hand-duplicates the ordering list;
`cargo test -p parish-tauri --test lock_order_fitness` is green.

### C9 [NEW] — Sensor FAILS on an inverted pair (negative proof)

The Tauri sensor ships with a permanent in-test negative fixture (a hardcoded bad
snippet, e.g. `save_identity` held then `world` acquired) asserting it **would**
flag — mirroring the server's `sensor_rejects_inverted_pair`. The negative case
is regression-guarded forever without leaving real bad code in tree.
Observable via:
`grep -n 'sensor_rejects_inverted_pair\|out_of_order\|bad_handler' parish/crates/parish-tauri/tests/lock_order_fitness.rs`
returns a hit and that sub-test is green; evidence.md shows the captured failing
panic message naming the inverted pair + the canonical-fix hint.

### C10 [INV] — Lock ordering preserved; no new nested-acquire inversions

The refactor changes lock granularity, never lock order. No Tauri handler may now
acquire `config` after the inference group, or a save-tail lock before the
save-identity group, in a way that inverts the documented chain. The
`LOCK_ORDER` const reflects the collapsed positions (the server placed
`inference` just after `config` because that is the only slot a _held_ inference
guard is acquired; apply the same reasoning to Tauri's chain and document it).
Observable via: the C8 sensor passing is the mechanical proof; `cargo test -p
parish-tauri` runs the existing tick/command tests without deadlock or hang
(process exits, does not time out).

### C11 [INV] — All existing Tauri tests pass; mechanical lock-path edits only

The 125 existing `parish-tauri` `#[test]`/`#[tokio::test]` cases continue to
pass. Test bodies and the three `AppState` constructors in test code
(`commands/cmd_tests.rs::test_app_state` ~76, `mcp_bridge.rs` ~926, and the
production constructor in `lib.rs` ~1320) are mechanically updated to construct /
acquire the new group fields, but **no test assertion is weakened, deleted, or
`#[ignore]`d**, and no `#[allow]` is added (Rule 5).
Observable via: `cargo test -p parish-tauri` is green; `git diff` shows only
lock-acquisition / field-construction mechanics changing in test bodies (no
`assert*` removed, no `#[ignore]` added) — confirmed by inspection in evidence.md.

### C12 [INV/Rule 11] — Scaling-guardrail seams respected

The change touches `AppState`, per-session/app background tasks, and the inference
path, so the `docs/agent/scaling-rules.md` checklist applies. Preserve:
Rule 1 (state stays on `AppState`; the channel + writer task are app-scoped,
created in `spawn_*_log_subscriber`; no new `static`/module-level `Mutex`),
Rule 3 (the subscriber still consumes `world.event_bus.subscribe()` and the
writer does only disk I/O — no new `broadcast::send`/`Topic`). Tauri is
single-session, so the N-session multiplier that motivated the server fix is
absent here — the value is parity + removing per-event blocking-pool churn, not
contention relief; record this framing.
Observable via:
`grep -rn 'static .*Mutex\|lazy_static\|OnceCell<.*Mutex' parish/crates/parish-tauri/src/lib.rs`
returns no new global-state hit; the diff shows the writer task + channel created
inside `spawn_*_log_subscriber` and no new `broadcast::channel`/`Topic`;
evidence.md records the scaling-rules consultation.

### C13 [INV] — `just check` green

`cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and
`cargo test --workspace` pass with no new warnings or unexplained `#[allow]`
attributable to this change (Rule 5).
Observable via: `just check` exits 0.

### C14 [INV] — Live-proof tier signal + honest display-limitation note (Rule 10)

The diff touches runtime-shipping paths (`parish-tauri/**`,
`parish-server/src/session/ticks.rs`, `parish-chronicle/**`), so unit tests alone
are not sufficient. **Tauri cannot run with a display in this environment**, so
the live signal is produced on the **shared code path** that the hoist now makes
common to both backends, plus the full Tauri test suite, plus an explicit
limitation note. Specifically the bundle must provide ALL of:

1. A **live server transcript** driving
   `parish/testing/fixtures/play_tauri-parity.txt` against a live `parish`
   server (`bash parish/scripts/parish-mcp-backend.sh start`, then
   `parish --script …`, with `PARISH_USER_DATA_DIR=<tmp>` exported on the server
   process), with post-run `ls`+`grep` over `<tmp>/logs/branch-1/` showing
   populated `player.md` (a `<!-- PROFILE_START -->` marker AND ≥1 `### `
   journal heading), ≥1 `npc-*.md`, and ≥1 `loc-*.md` with a profile marker.
   This exercises the **same `run_chronicle_log_writer` helper** the Tauri
   subscribers now call (post-hoist it is one shared code path), proving the
   bounded-channel writer routes real events to disk without loss.
2. `cargo test -p parish-tauri` green (all 125 tests — exercises the regrouped
   `AppState` constructors and command handlers).
3. `cargo test -p parish-tauri --test lock_order_fitness` green (C8) + the
   captured **failing** negative-fixture output (C9).
4. `cargo test -p parish-chronicle log_writer` green (C4).
5. An explicit **`Display limitation`** note in evidence.md stating that
   `parish-tauri` has no display in this environment so the live window path
   (`cargo run -p parish-tauri -- --mcp-port 3030` / `just run`) could not be
   exercised here; the bounded-writer behavior is proven through the shared
   helper on the server path (item 1) and the grouping through the full Tauri
   compile + test suite (items 2–3). The note recommends a desktop-display
   `mcp__parish__*` or `just run` smoke-check before release as the residual
   verification.

evidence.md header declares `Evidence type: live gameplay transcript`. (The
server-path transcript is a genuine live run; the affirmation is honest because
the writer helper is literally the same code Tauri executes after the hoist.)
Observable via: evidence.md carries the header, the five items above, and maps
each criterion to its output line(s); `judge.md` (independent) verifies every
criterion and includes the line `Acceptance criteria: met`.

---

## Verification script

```sh
# ── A: bounded writer, proven on the SHARED helper via a live server ──────────
# (Tauri itself can't run headless here; the hoisted helper is the same code.)
PARISH_USER_DATA_DIR=/tmp/parish-tauri-parity \
  bash parish/scripts/parish-mcp-backend.sh start          # live server, logs → tmp
parish --script parish/testing/fixtures/play_tauri-parity.txt

# Post-run on-disk checks (no-loss-under-load observable; note: FULL path, no
# app-name suffix — LEARNINGS.md):
ls   /tmp/parish-tauri-parity/logs/branch-1/
grep '<!-- PROFILE_START -->' /tmp/parish-tauri-parity/logs/branch-1/player.md
grep '^### '                  /tmp/parish-tauri-parity/logs/branch-1/player.md
ls   /tmp/parish-tauri-parity/logs/branch-1/loc-*.md

# ── Shared-crate bound test (C4) ──────────────────────────────────────────────
cargo test -p parish-chronicle log_writer

# ── B: grouping + sensor (C5/C6/C8/C9/C11) ────────────────────────────────────
cargo build -p parish-tauri
cargo test  -p parish-tauri                                  # all 125, green
cargo test  -p parish-tauri --test lock_order_fitness        # C8 green, C9 negative

# ── Rule-12 de-dup proof (C2) ─────────────────────────────────────────────────
grep -rn 'fn run_character_log_writer\|fn run_location_log_writer' parish/crates   # expect ZERO
grep -rn 'fn run_chronicle_log_writer' parish/crates/parish-chronicle/src          # expect a hit

# ── Shared-signature invariant (C7) ───────────────────────────────────────────
git diff --stat parish/crates/parish-core/src/game_loop/      # context.rs/save.rs unchanged
cargo build -p parish-server && cargo build -p parish-engine  # both compile, no call-site edits

# ── Full gate (C13) ───────────────────────────────────────────────────────────
just check
```

Expected signals:

- The fixture runs a burst of movement + `/stub` dialogue (more journal-producing
  events than the queue capacity) and completes without error — proving the
  bounded channel applies backpressure rather than dropping/deadlocking on the
  shared writer path.
- `player.md` exists with a `<!-- PROFILE_START -->` marker AND ≥1 `### ` heading;
  ≥1 `npc-*.md`; ≥1 `loc-*.md` with a profile marker.
- `cargo test -p parish-chronicle log_writer` and
  `cargo test -p parish-tauri [--test lock_order_fitness]` all pass; the negative
  fixture flags an inverted pair.
- The two server writer helpers are gone; the shared helper exists in
  `parish-chronicle`; `context.rs`/`save.rs` are untouched.
- `just check` exits 0.

---

## Coupling surprises — read before implementing

### CS-1 — The server has ALREADY landed both fixes; this is the parity close

`log-backpressure` (#1415, §3) and `appstate-locks` (#1416, §2) are merged on the
server side. Their AC files (`.proofs/log-backpressure/acceptance-criteria.md`
C9, `.proofs/appstate-locks/acceptance-criteria.md` C11 + CS-7) **explicitly
deferred Tauri** and recommended a tracked follow-up — this is that follow-up.
Read both predecessors; this AC reuses their prescribed mechanism verbatim where
it transfers.

### CS-2 — The Rule-12 hoist is the whole point of doing this now

The server currently carries **two** private writer helpers
(`run_character_log_writer`, `run_location_log_writer` at
`parish-server/src/session/ticks.rs` ~707 and ~730) differing only in manager
type. Copy-pasting them into Tauri = **four** copies across **two** entry-point
crates = the exact Rule-12 violation (#687/#696). The two managers share an
identical `process_event(&self, &GameEvent, &WorldState, &NpcManager) ->
Result<()>` signature (`parish-chronicle/src/{character_log,location_log}.rs`
~168/~147), so a generic `run_chronicle_log_writer` over a one-method
`ChronicleWriter` trait is cheap. Hoisting also **de-duplicates the existing
server pair** (net -1 copy on the server, +0 on Tauri) — strictly better than
the status quo. This is the SHARE-not-duplicate call; it is justified, not
overkill.

### CS-3 — `parish-chronicle` is backend-agnostic; the helper must stay off runtime deps

LEARNINGS line 78 + 75: `parish-chronicle` is in the `BACKEND_AGNOSTIC`
architecture-fitness list and must never depend on `tauri`/`axum`/`tower*`. The
hoisted helper therefore **cannot** reference `AppState` (server or Tauri). It
must be parameterized over "how to acquire `&WorldState` + `&NpcManager`" via a
closure / accessor the caller supplies — each entry point closes over its own
`Arc<AppState>`. If the implementer reaches for `state.world.blocking_lock()`
inside the shared crate, that's wrong: the crate doesn't know what `state` is.

### CS-4 — Tauri lifecycle map (differs from `spawn_session_ticks`)

| Concern              | Server (`session/ticks.rs`)                        | Tauri (`setup.rs` + `lib.rs`)                                 |
| -------------------- | -------------------------------------------------- | ------------------------------------------------------------- |
| Spawn site           | `spawn_session_ticks(state, token) -> Vec<Handle>` | `spawn_*_log_subscriber(&state, app_name)` from lib.rs ~1465  |
| Handle lifecycle     | collected into returned vec, dropped on eviction   | **fire-and-forget** `tokio::spawn`, handle dropped            |
| Sessions per process | **N** (per browser visitor)                        | **1** (single desktop session)                                |
| Shutdown token       | per-session token param                            | app-lifetime `state.shutdown_token`, cancelled on `Destroyed` |
| `app_name` source    | derived inside task via `app_name_from_mod`        | passed as fn **parameter**                                    |
| Teardown of writer   | token in `handles` vec                             | `tx` dropped on subscriber exit → channel close → writer ends |

Adapt accordingly: clone `state.shutdown_token`, fire-and-forget the writer
spawn, cancellation-aware send, no handle vec.

### CS-5 — Inference-group lock-site inventory (14 sites, Tauri `src/`)

`state.client.lock`: `commands/demo.rs` 462, `commands/input.rs` 281,
`commands/reactions.rs` 91, `commands/movement.rs` 62 & 142, `setup.rs` 286 &
389 & 1031 & 1167. `state.cloud_client.lock`: `command_host.rs` 80 (note:
`self.state.cloud_client` — the command host holds an `Arc<AppState>`).
`state.inference_queue.lock`: `commands/admin.rs` 32, `setup.rs` 414 & 1276,
`mcp_bridge.rs` 1029 (test). Renamed access path: `state.inference.client`,
`state.inference.cloud_client`, `state.inference.inference_queue` (and
`self.state.inference.cloud_client` in command_host).

### CS-6 — Save-identity lock-site inventory (41 sites, Tauri `src/`)

Across `commands/saves.rs` (the heaviest — ~22 sites incl. the new-game/load/fork
triples at 150–152, 256–258, 317–319, 391–393), `command_host.rs` (117),
`commands/admin.rs`, `mcp_bridge.rs`, `setup.rs` (511–513 the new-save triple).
Renamed access path: `state.save_identity.save_path`,
`state.save_identity.current_branch_id`, `state.save_identity.current_branch_name`.
Mechanical but wide — budget for a large boring diff (mirrors server CS-2's ~230
sites, scaled to Tauri).

### CS-7 — Three `AppState` constructors must be updated

The flat fields are set in **three** places: the production constructor
(`lib.rs` ~1320 `Arc::new(AppState { … })`), and **two** test constructors
(`commands/cmd_tests.rs::test_app_state` ~76, `mcp_bridge.rs` ~926). All three
must construct `inference: InferenceClients { … }` and `save_identity:
SaveIdentity { … }` instead of the six flat fields. Miss one and `cargo test -p
parish-tauri` won't compile.

### CS-8 — `GameLoopContext`/`NewGameParams` pin the per-field borrow shape (C7)

`commands/input.rs` ~368/~401 builds `GameLoopContext { … client:
&state.client, cloud_client: &state.cloud_client, inference_queue:
&state.inference_queue, … }` and `commands/saves.rs` ~273 builds `NewGameParams
{ … save_path: &state.save_path, current_branch_id: &state.current_branch_id,
current_branch_name: &state.current_branch_name, … }`. Because members stay
`Mutex`-wrapped inside the group struct, the fix is a pure borrow-path rename
(`&state.inference.client`, `&state.save_identity.save_path`) — the shared
struct signatures do NOT change (this is exactly why the server kept members
`Mutex`-wrapped; server CS-6). Do **not** flatten the group into one outer
`Mutex<…>` — that would break these borrows and force a `parish-core` signature
change (mode-parity break).

### CS-9 — Tauri `LOCK_ORDER` differs slightly from the server's chain

The Tauri `AppState` doc-comment (lib.rs ~425–445) documents a chain:
world → npc_manager → conversation → debug_events/game_events → config →
save_path/branch → client/cloud_client → inference_log → inference_queue. Note
this is **not** identical to the server's `LOCK_ORDER` (the server places
`inference` just after `config`, before `save_identity`; the Tauri prose places
save-identity _before_ the clients and splits `inference_queue` to the tail).
**Derive the Tauri `LOCK_ORDER` from Tauri's actually-attested call sites**, not
by copying the server const — then place the collapsed `inference` /
`save_identity` nodes at the single position where each is _held_ across another
lock (the server's reasoning, applied to Tauri's real paths). Document the
placement reasoning in the doc-comment as the server did. The sensor will catch
any mis-ordering.

### CS-10 — Display limitation is real and must be declared, not hidden

`parish-tauri` cannot open a window in this sandbox (no display). The honest live
signal is the **shared writer helper exercised on the server path** (post-hoist,
literally the same code) + the full Tauri compile/test suite + the lock-order
sensor. evidence.md MUST carry an explicit `Display limitation` note (C14 item 5)
— do not imply a Tauri window was driven. This is the strongest honest evidence;
the judge must accept it on the basis that the writer path is shared code and the
grouping is fully compile-and-test-covered. A desktop smoke-check via `just run`
/ `cargo run -p parish-tauri -- --mcp-port 3030` is recommended as a pre-release
residual but is out of reach here.
