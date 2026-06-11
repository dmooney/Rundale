# Acceptance Criteria: log-backpressure

## Task

Epic #1366 §3, first checkbox (low effort / medium payoff). The
character-log and location-log subscribers in
`parish/crates/parish-server/src/session/ticks.rs` currently spawn a
`tokio::task::spawn_blocking` **per world event** to run
`CharacterLogManager::process_event` / `LocationLogManager::process_event`
under `world.blocking_lock()` + `npc_manager.blocking_lock()`. There is no
bound on this spawn site, so under burst (many `GameEvent`s in a short
window, multiplied across every live session — N sessions × 2 subscribers)
the shared Tokio blocking-thread pool is flooded with one short-lived task
per event, with fresh `Arc` clones and event clones each time.

Replace the per-event `spawn_blocking` with a **bounded mechanism**. The
chronicle writers themselves now live in `parish-chronicle` (extracted in
#1411, re-exported as `parish_core::{character_log, location_log}`); the
writer is **stateless beyond `log_dir`** and is `Clone`. **Only the
call-sites in `parish-server/src/session/ticks.rs` change** — the writer
crate and what gets written to disk must be untouched, and the
branch-switch rebind behavior (rebuild the manager + rewrite profiles when
`current_branch_id` changes, #1011/#1034) must be preserved.

## Chosen mechanism (prescribed — do not re-decide)

**One long-lived writer task per subscriber, fed by a bounded
`tokio::sync::mpsc` channel.** Each of the two subscriber blocks
(character-log, location-log) keeps its existing async event loop that
`recv()`s from `world.event_bus` and handles the branch-switch rebind, but
instead of `spawn_blocking` + immediate `await` per event it sends a small
work item `(manager_clone, event_clone)` onto a **bounded mpsc channel**
(capacity `LOG_WRITER_QUEUE_CAPACITY`, a named constant in `ticks.rs`).
A single dedicated writer task per subscriber owns the receiver and, in a
loop, takes one work item and runs `process_event` under the world/npc
blocking locks (inside one `spawn_blocking`, or directly on a dedicated
blocking-aware task) — serially, exactly one in flight at a time.

### Justification (one paragraph)

The existing loop already `await`s each `spawn_blocking` handle before the
next `recv()`, so it is _de facto_ serial **within** a session — the real
unboundedness is (a) per-event task-spawn churn on the shared blocking pool
and (b) the absence of any explicit bound that survives future refactors
that might drop the inline `await`. A **semaphore** would only cap
concurrency that today is already effectively 1; it adds a permit acquire
on the hot path without removing the per-event spawn churn, and it does not
give a natural place to absorb a burst. A **single long-lived writer task
fed by a bounded channel** is the better fit: it spawns the blocking
context **once per subscriber per session** (not once per event), makes the
bound a first-class, reviewable constant, gives a defined saturation point
(the channel send), and leaves the branch-switch rebind logic exactly where
it is (in the async recv loop, which still holds the only `manager` that is
rebound — it clones the rebound manager into each work item, so the writer
task always uses the current branch's `log_dir`). The writer task is
spawned in `spawn_session_ticks` alongside its subscriber and observes the
same `shutdown_token`, so it is dropped on session eviction with no extra
lifecycle plumbing.

### Saturation behavior (prescribed — block, do not drop)

On a full channel the subscriber loop **blocks** (awaits `tx.send(item)`,
i.e. applies backpressure to its own `world.event_bus` `recv()` loop) rather
than dropping the work item. Rationale: chronicle entries are an append-only
historical record; silently dropping a `PlayerMoved`/`DialogueOccurred`
write would leave a permanent hole in `player.md` / `loc-*.md` that no later
event repairs. Blocking the subscriber's recv loop instead lets the upstream
`world.event_bus` broadcast channel (capacity `BUS_CAPACITY = 256`) absorb
the burst; if _that_ overflows the existing `RecvError::Lagged` arm already
skips (the documented, pre-existing lossy backstop — this task does not
change it). So the new bounded channel converts "flood the blocking pool"
into "apply bounded backpressure, then fall back to the existing lag skip"
— no **new** silent loss is introduced, and normal-load writes are never
dropped. (If an implementer finds `try_send` + an explicit dropped-count
`tracing::warn!` strictly necessary for a deadlock reason discovered during
implementation, that is a deviation requiring a documented justification in
the PR — the default and expected behavior is **block**.)

## Criteria

### C1 — Per-event `spawn_blocking` is gone from the two log subscribers

In `parish/crates/parish-server/src/session/ticks.rs`, neither the
character-log subscriber block nor the location-log subscriber block calls
`tokio::task::spawn_blocking` **inside the `rx.recv()` event-handling arm**.
The blocking `process_event` work now runs inside a single long-lived writer
task per subscriber that drains a bounded `tokio::sync::mpsc` channel.
(`spawn_blocking` may still appear _once_ inside that writer task to enter a
blocking context, but not once-per-event in the recv loop.)
Observable via: reading the diff — the two `let handle = tokio::task::spawn_blocking(move || { … process_event … }); match handle.await { … }`
blocks in the recv arms are replaced by a bounded `tx.send(...)`; the
chat-transcript subscriber and the four other tick tasks are unchanged.

### C2 — Bound exists as a named, documented constant

`ticks.rs` declares a named constant (e.g.
`const LOG_WRITER_QUEUE_CAPACITY: usize = <N>;`) with a doc comment
explaining the value and the saturation behavior. The value is a small fixed
bound (justified constant — does not need to be runtime-configurable for
this task; if made configurable it must read from `config`/env at session
start per rule #9, never from a per-call `current_dir`/marker search).
Observable via: `grep -n 'LOG_WRITER_QUEUE_CAPACITY' parish/crates/parish-server/src/session/ticks.rs`
returns the `const` declaration plus its two use sites (one per subscriber),
and the constant carries a `///`/`//` doc comment.

### C3 — No event loss under normal load (logs still populate on disk)

Running the verification fixture (`play_log-backpressure.txt`) under
`PARISH_USER_DATA_DIR=<tmp>` against a **live** `parish` server (so the real
`parish-server` session path with `spawn_session_ticks` is exercised, not
the in-process `GameTestHarness`) produces populated character and location
markdown logs at `<tmp>/logs/branch-1/`:

- at least one `player.md` and at least one `npc-*.md` file exist,
- `player.md` contains `<!-- PROFILE_START -->` (profiles written) AND at
  least one journal heading line beginning with `### ` (a `PlayerMoved`
  journal entry written via `CharacterLogManager::process_event` after the
  refactor),
- at least one `loc-*.md` file exists containing `<!-- PROFILE_START -->`
  (confirming `LocationLogManager` still runs through the new writer task).

This is the chosen observable for "no loss under normal load": **on-disk
markdown under `$PARISH_USER_DATA_DIR/logs/branch-1/`** (no app-name suffix
— `PARISH_USER_DATA_DIR` is the full root; see LEARNINGS.md). It mirrors the
extract-chronicle proof exactly, so a regression in routing or a dropped
write surfaces as a missing/empty file.
Observable via: post-run `ls` + `grep` over `<tmp>/logs/branch-1/` in the
live proof transcript (see evidence.md).

### C4 — Saturation behavior is the prescribed one and is unit-tested

A new unit test in `parish-server/src/session/ticks.rs` (`#[cfg(test)] mod tests`)
exercises the bound directly: it floods the bounded writer channel beyond
`LOG_WRITER_QUEUE_CAPACITY` and asserts the **defined** behavior — the
sender applies backpressure (blocks / pends) rather than dropping, and once
the writer task drains, **every** enqueued event is processed (no item
lost). The test must construct the bounded channel + writer-task pairing in
isolation (it need not stand up a full `AppState` if the writer task is
factored into a testable helper; if it does use `test_app_state()`, it must
assert all N flooded events were written/counted). The test name should make
the contract explicit (e.g. `log_writer_channel_blocks_when_full_and_loses_no_events`).
Observable via: `cargo test -p parish-server log_writer` runs the new test
and it passes; the test asserts a count equal to the number of items sent
(no drops) under an over-capacity flood.

### C5 — Branch-switch rebind still works

The per-event branch-switch rebind (compare `current_branch_id`; on
mismatch rebuild `CharacterLogManager`/`LocationLogManager::new(&app_name, bid, true)`
and call `write_all_profiles`) is preserved in **both** subscriber loops and
still runs in the async recv loop (where the single `manager` lives). The
work item sent to the writer task carries a **clone of the post-rebind
manager**, so events that arrive after a `/load`/`/fork` are written under
the new branch's `logs/branch-<new>/` directory, never the old one
(#1011/#1034). No `last_arrival`/`scan_existing_*` dedup state is
reintroduced (the writer is stateless beyond `log_dir`; LEARNINGS.md
"Character logs").
Observable via: reading the diff — both recv loops retain the
`let bid = …current_branch_id…; if bid != current_branch { … manager = …::new(…); write_all_profiles(…) }`
block ahead of the channel send, and the cloned manager (not a stale one) is
what is enqueued. An existing branch-switch rebind test continues to pass
(see C6); if a server-side rebind integration test exists it is run, else
this is covered by reading the diff + C6.

### C6 — All existing tests pass unmodified

No existing test in the workspace is edited to accommodate this change. In
particular `parish-server`'s existing `ticks.rs` tests
(`autosave_interval_is_60_seconds`, `game_events_subscriber_captures_published_events`,
`autosave_reuses_async_database_across_ticks`) and any chronicle/rebind
tests pass without modification.
Observable via: `cargo test -p parish-server` and `cargo test --workspace`
are green; `git diff` shows no deletions/edits to existing `#[test]`/
`#[tokio::test]` bodies (only the new C4 test is added).

### C7 — `just check` green

`cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and
`cargo test --workspace` all pass with no new warnings or `#[allow]`
attributable to this change (rule #5 — no unexplained `#[allow]`).
Observable via: `just check` exits 0.

### C8 — Scaling guardrails (rule #11) consulted and respected

This change touches **real-time push / per-session background tasks**, so it
is reviewed against `docs/agent/scaling-rules.md`. The relevant seams and how
they are respected:

- **Rule 1 (no global mutable game state):** the bounded channel, its
  `tx`/`rx`, and the writer-task `JoinHandle` are all per-session, created
  inside `spawn_session_ticks(state, shutdown_token)` and owned by that
  session's task set — nothing is stored in a `static`/module-level
  `Mutex`. The writer-task handle is collected into the returned
  `Vec<JoinHandle<()>>` (or otherwise tied to the session) so it is dropped
  on session eviction.
- **Rule 3 (real-time push only via `EventBus` + `Topic`):** the subscriber
  still consumes `world.event_bus.subscribe()` (the `GameEvent` bus) and the
  writer performs only disk I/O — no new direct `broadcast::Sender::send`
  and no new server-push `Topic` is introduced; the change is confined to
  how an existing subscriber schedules its blocking work.
- The writer task observes the existing `shutdown_token` via
  `tokio::select!` (or the channel closing when the subscriber loop exits on
  cancel), so session eviction (#228) still tears it down cleanly.

Observable via: the diff shows the channel + writer-task created inside
`spawn_session_ticks` (per-session), the handle threaded into the returned
handles vec, and no new `static`/`broadcast::channel`/`Topic`; evidence.md
records the rule-11 checklist consultation.

### C9 — Mode-parity note for Tauri (documented, scoped out)

`parish-tauri/src/setup.rs` has the **identical** unbounded per-event
`spawn_blocking` pattern in `spawn_character_log_subscriber` /
`spawn_location_log_subscriber` (lines ~600 and ~678). The epic names only
`parish-server`, so the Tauri (single-session desktop) path is **out of
scope for this task** — but the divergence must be acknowledged so it is not
lost. The PR description (and evidence.md) records that Tauri still uses the
unbounded pattern and notes it is lower-risk there (one session per process,
no N-session multiplier) with a recommendation to apply the same
bounded-channel mechanism in a follow-up. `parish-engine` (headless/CLI:
`app.rs`, `headless.rs`, `testing.rs`) uses a different (synchronous,
per-turn drain) path and is not affected.
Observable via: a "Parity" subsection in the PR body / evidence.md naming
the two Tauri functions and stating the scope decision. (No code change to
Tauri in this PR.)

### C10 — Live proof: transcript maps each criterion to output

`evidence.md` carries the live header `Evidence type: live gameplay transcript`
and a section mapping:

- C1/C2/C5 to the relevant diff hunks,
- C3 to the live `ls`+`grep` output over `<tmp>/logs/branch-1/` after the
  fixture run against a **live `parish` server** (booted via
  `bash parish/scripts/parish-mcp-backend.sh start` or `just web`, driven
  with `parish --script parish/testing/fixtures/play_log-backpressure.txt`,
  with `PARISH_USER_DATA_DIR` exported for that server process),
- C4 to the new unit test's passing output,
- C6/C7 to `cargo test -p parish-server` + `just check` exit-0 output.

`judge.md` is written independently, verifies every criterion, and includes
the line `Acceptance criteria: met`.

## Verification script

The fixture `parish/testing/fixtures/play_log-backpressure.txt` is harness
syntax (one command per line, `#` comments), modeled on
`play_extract-chronicle.txt`. It must be driven against a **live
`parish-server`** so the real `spawn_session_ticks` per-session subscriber
path is exercised (the in-process `GameTestHarness` does not run that path).

Boot + drive (note: `PARISH_USER_DATA_DIR` must be set on the **server**
process, and share one cookie/session for the whole script):

```sh
# 1. Boot a live server with logs pointed at an inspectable temp dir.
#    (active mod must be rundale — see LEARNINGS.md about mod-list.toml.)
PARISH_USER_DATA_DIR=/tmp/parish-log-backpressure \
  bash parish/scripts/parish-mcp-backend.sh start   # or: just web 3001

# 2. Drive the fixture through the synchronous client (one session).
parish --script parish/testing/fixtures/play_log-backpressure.txt

# 3. Post-run on-disk checks (the C3 observable):
ls /tmp/parish-log-backpressure/logs/branch-1/
grep '<!-- PROFILE_START -->' /tmp/parish-log-backpressure/logs/branch-1/player.md
grep '^### ' /tmp/parish-log-backpressure/logs/branch-1/player.md
ls /tmp/parish-log-backpressure/logs/branch-1/loc-*.md

# 4. The bound's unit test (the C4 observable):
cargo test -p parish-server log_writer

# 5. Full gate.
just check
```

Expected signals:

- The fixture runs many movement + `/stub` dialogue commands in a tight
  burst (more journal-producing events than a small queue capacity) and the
  process completes without error — proving the bounded channel applies
  backpressure rather than dropping or deadlocking.
- `player.md` exists with a `<!-- PROFILE_START -->` marker AND ≥1 `### `
  journal heading; ≥1 `npc-*.md`; ≥1 `loc-*.md` with a profile marker.
- `cargo test -p parish-server log_writer` passes (saturation/no-loss).
- `just check` exits 0.

## Coupling surprises

- **The loop is already serial within a session.** The current code
  `await`s each `spawn_blocking` handle before the next `recv()`, so today
  there is at most one in-flight blocking task **per subscriber per
  session**. The unboundedness the epic targets is the **per-event spawn
  churn multiplied across N concurrent sessions** (each session has its own
  char + loc subscriber), not unbounded concurrency within one session. The
  fix's value is removing per-event task creation and making the bound
  explicit — not capping a concurrency that was already ~1. Don't "fix" this
  by adding parallelism.
- **Two distinct buses share the name "event bus".** `world.event_bus`
  (`parish_types::events::EventBus`, `broadcast`, `BUS_CAPACITY = 256`,
  carries `GameEvent`) is what these subscribers consume. The separate
  `state.event_bus` (`parish_core::event_bus::BroadcastEventBus`, capacity
  256, carries server-push `ServerEvent`/`Topic`) is the rule-3 push seam
  and is **not** touched here. Don't conflate them.
- **`PARISH_USER_DATA_DIR` is the FULL log root** — logs land at
  `$DIR/logs/branch-1/`, NOT `$DIR/<app>/logs/branch-1/` (no app-name
  suffix). The old `play_extract-chronicle.txt` `*/logs/` glob is wrong; use
  the un-globbed path (LEARNINGS.md).
- **Live server, not the harness.** `--script` against `parish-engine`
  (`run_script_mode` → `GameTestHarness`) does **not** run
  `spawn_session_ticks` — that's a different code path. The proof must hit a
  live `parish-server` (`parish --script` against a running server) for the
  subscriber path under test to execute. The `GameTestHarness` also reports
  provider `ollama` and does not enable the simulator; drive deterministic
  dialogue with `/stub <name>: <text>`.
- **Writer is stateless beyond `log_dir` and is `Clone`.** Cloning the
  manager into each work item is cheap and correct; do not add per-writer
  dedup/scan state (#1032 removed it on purpose). The branch-rebind must
  clone the **post-rebind** manager into the queue, or post-`/load` events
  land under the old branch dir.
- **Tauri has the same bug, out of scope.** `spawn_character_log_subscriber`
  / `spawn_location_log_subscriber` in `parish-tauri/src/setup.rs` are
  identical and unbounded; the epic scopes only `parish-server`. Note the
  divergence in the PR rather than silently leaving parity asymmetric
  (rule #2 mode parity is partially enforced — wiring parity is convention,
  so this is a documented, deliberate scope decision, not a violation).
