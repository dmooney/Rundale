# Acceptance Criteria: appstate-locks

## Task

Epic #1366 §2, checkboxes 1–2 (the lock-contention-metrics checkbox is
**explicitly deferred** and out of scope here).

`parish/crates/parish-server/src/state.rs` `AppState` holds ~20 independently
`Mutex`-guarded fields whose lock-ordering invariant (`world → npc_manager →
inference_queue → … → save_db`) is enforced **only by a doc comment** (#483).
Two things must change:

1. **Group related fields behind one lock each.** Introduce sub-structs so a
   coherent cluster is acquired with a single `.lock()`:
   - an **inference group** — `client` / `cloud_client` / `inference_queue`
     (the config that controls them stays on `config`; see coupling note CS-4
     for why `config` is _not_ folded in);
   - a **save-identity group** — `save_path` / `current_branch_id` /
     `current_branch_name`.
     The exact field membership is adjusted to what the real code supports —
     see the coupling-surprises section, which records the cross-crate
     constraint that blocks a naïve merge.
2. **Encode the canonical lock order as a `const` schema** next to the struct,
   **plus an architecture-fitness-style sensor** that greps handler bodies for
   out-of-order acquisition pairs, in the existing style of
   `parish/crates/parish-core/tests/architecture_fitness.rs` (fast textual
   check, rule citation + canonical-fix hint in the assert message).

This is a **behavior-preserving refactor of state shape** plus **one new
sensor**. No gameplay behavior, no HTTP/WS contract, and no lock _ordering_
(only lock _granularity_) may change.

Legend: **[INV]** = behavior-preserving invariant (must stay green before and
after). **[NEW]** = new functionality this task adds.

---

## Criteria

### C1 [NEW] — Inference field group behind one lock

`AppState` no longer has three separate `client` / `cloud_client` /
`inference_queue` `Mutex` fields. They are replaced by a single
`Mutex<InferenceClients>` (or similarly named) sub-struct field holding the
three values as plain (un-Mutexed) members. Every former
`state.client.lock()` / `state.cloud_client.lock()` /
`state.inference_queue.lock()` site now acquires the one group lock and reads
the member.
Observable via: `grep -rn 'state\.client\.lock\|state\.cloud_client\.lock\|state\.inference_queue\.lock' parish/crates/parish-server/src` returns **zero** hits outside the sub-struct definition; `grep -n 'struct InferenceClients' parish/crates/parish-server/src/state.rs` returns a hit; `cargo build -p parish-server` succeeds.

### C2 [NEW] — Save-identity field group behind one lock

`AppState` no longer has three separate `save_path` / `current_branch_id` /
`current_branch_name` `Mutex` fields. They are replaced by a single
`Mutex<SaveIdentity>` (or similarly named) sub-struct field. The four current
multi-lock sites that take all three in sequence (`session/lifecycle.rs`
~360–363, `routes/saves.rs` ~347–349 & ~414–416 & ~447–448,
`session/inference_setup.rs` ~136–139) acquire the group once.
Observable via: `grep -rn 'state\.save_path\.lock\|state\.current_branch_id\.lock\|state\.current_branch_name\.lock' parish/crates/parish-server/src` returns **zero** hits outside the sub-struct definition; `grep -n 'struct SaveIdentity' parish/crates/parish-server/src/state.rs` returns a hit.

### C3 [NEW] — Canonical lock order encoded as a `const` schema

A module-level `const` array (e.g. `pub const LOCK_ORDER: &[&str] = &[ … ];`)
lives next to `AppState` in `state.rs`, listing the lock fields in their
canonical acquisition order. After grouping, the schema lists the **group**
field names (e.g. `world`, `npc_manager`, `inference`, `conversation`,
`config`, …, `save_identity`, …, `save_db`) — not the dissolved member names.
The existing prose ordering doc-comment is rewritten to _reference_ the const
(single source of truth) rather than duplicate the list.
Observable via: `grep -n 'const LOCK_ORDER' parish/crates/parish-server/src/state.rs` returns a hit; the array contains `"world"` at index 0 and `"save_db"` last; the doc-comment no longer contains a second hand-maintained copy of the ordering text (verified by inspection in evidence.md).

### C4 [NEW] — Out-of-order sensor exists and PASSES on the clean tree

A new architecture-fitness-style test (in
`parish/crates/parish-core/tests/architecture_fitness.rs` **or** a new
`parish/crates/parish-server/tests/lock_order_fitness.rs`) reads `LOCK_ORDER`,
greps the server handler/session/route source for adjacent lock-acquisition
pairs `(A, B)` on `AppState` group fields, and asserts every observed pair
respects `index(A) < index(B)`. The assert message names the offending
file + pair, cites rule #11 / #483, and gives the canonical fix ("acquire
`<earlier>` before `<later>`; see `LOCK_ORDER` in `state.rs`"), matching the
style of `backend_agnostic_crates_do_not_pull_runtime_deps`.
Observable via: `cargo test -p parish-server --test lock_order_fitness` (or `cargo test -p parish-core --test architecture_fitness lock_order`) is **green** on the refactored tree.

### C5 [NEW] — Sensor FAILS when an out-of-order pair is introduced (negative proof)

The sensor must actually catch a violation, not be a no-op. The proof bundle
demonstrates this: temporarily insert an inverted acquisition (e.g. acquire
`save_identity` then `world`, or `config` then `npc_manager`) into a handler
**or** a sensor unit-test fixture string, run the test, and capture the
**failing** output showing the offending pair + file + the canonical-fix hint.
Preferred form: the sensor ships with an in-test negative fixture (a hardcoded
bad snippet the grep is run against) asserting it _would_ flag, so the negative
case is permanently regression-guarded without leaving real bad code in tree.
Observable via: evidence.md contains the captured `assert` failure panic
message naming the inverted pair; if a permanent negative fixture is used,
`grep -n 'out_of_order\|rejects_inverted\|bad_pair' parish/crates/.../{architecture_fitness,lock_order_fitness}.rs` returns a hit and that sub-test is green.

### C6 [INV] — Shared `parish-core` game-loop signatures unchanged (mode parity, Rule 2 & #12)

The grouping must **not** be pushed down into the shared structs
`parish_core::game_loop::context::GameLoopContext` and
`parish_core::game_loop::save::NewGameParams`, which today take
`save_path` / `current_branch_id` / `current_branch_name` / `client` /
`cloud_client` / `inference_queue` as **separate `&Mutex<…>` references** and
are constructed by **all three** entry points (server, Tauri, engine). The
server adapter bridges its grouped `AppState` to these per-field `&Mutex`
params (e.g. by holding the group guard and passing `&guard.member`, or by
keeping the borrow shape the shared API expects).
Observable via: `git diff --stat parish/crates/parish-core/src/game_loop/` shows `context.rs` and `save.rs` **unmodified** (no field-signature change); `cargo build -p parish-tauri` and `cargo build -p parish-engine` both succeed with **no** edits to their call-sites of `GameLoopContext` / `NewGameParams`.

### C7 [INV] — All existing server tests pass unmodified in intent

`parish/crates/parish-server/src/routes/tests.rs` (~56 lock sites) and every
other existing server test continue to pass. Test _bodies_ may be mechanically
updated to acquire the new group lock instead of the dissolved per-field locks
(that is a required mechanical edit, not a behavior change), but **no test
assertion is weakened, deleted, or `#[ignore]`d**, and no `#[allow]` is added.
Observable via: `cargo test -p parish-server` is green; `git diff parish/crates/parish-server/src/routes/tests.rs` shows only lock-acquisition mechanics changing (no `assert*` lines removed, no `#[ignore]` added) — confirmed by inspection in evidence.md.

### C8 [INV] — Lock ordering preserved; no new nested-acquire inversions

The refactor changes lock _granularity_, never lock _order_. Because the
inference group collapses `client`/`cloud_client`/`inference_queue` (formerly
three adjacent steps in the chain) into one node, and the save-identity group
collapses `save_path`/`current_branch_id`/`current_branch_name` into one node,
no handler may now acquire `config` after the inference group or `save_lock`/
`save_db` before the save-identity group in a way that inverts the documented
chain. The `LOCK_ORDER` const reflects the collapsed positions.
Observable via: the C4 sensor passing is the mechanical proof; additionally `cargo test -p parish-server` runs the existing concurrency/tick tests without deadlock or hang (process exits, does not time out).

### C9 [INV] — `just check` green

`cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and
`cargo test --workspace` pass with no new warnings or errors attributable to
this change, and no unexplained `#[allow]` (Rule 5).
Observable via: `just check` exits 0.

### C10 [INV] — Live server behavior parity via the fixture

Running the verification fixture against a **live** `parish-server`
(`bash parish/scripts/parish-mcp-backend.sh start`, then
`parish --script parish/testing/fixtures/play_appstate-locks.txt`) exercises
the server-relevant flows that touch the regrouped locks — `/status`, `/time`,
movement (world+npc_manager), `/save` + `/fork` + `/load` branch ops
(save-identity group), and `/stub` dialogue (inference group via the command
host) — and
completes without error, deadlock, or 409/500. This is the live-proof tier
signal required by Rule 10 (the diff touches `parish-server/**`).
Observable via: the fixture transcript in evidence.md shows a non-empty
`location` from `/status`, a successful save line, a successful load/branch
line, and a `/stub` dialogue echo, with the process exiting 0 and no
`transport error` / panic. evidence.md header declares
`Evidence type: live gameplay transcript`.

### C11 [INV] — Tauri parity decision recorded (Rule 2)

`parish-tauri`'s `AppState` (`parish/crates/parish-tauri/src/lib.rs` ~446)
mirrors the same flat per-field `Mutex`es. The PR explicitly states the parity
decision for the Tauri side (recommendation: **out of scope for this PR** —
see coupling note CS-6) and either (a) applies the identical grouping to the
Tauri struct in the same PR, or (b) records a tracked follow-up so the two
backends do not silently diverge on lock granularity.
Observable via: the PR body contains a "Tauri parity" subsection naming the
chosen option; if (b), a follow-up issue/checkbox reference is included.

### C12 [INV/Rule 11] — Scaling-guardrail seams untouched

The PR touches `AppState`, so the seam checklist in
`docs/agent/scaling-rules.md` applies. The regrouping must preserve:
Rule 1 (state stays per-session on `AppState`; no new `static`/module-level
`Mutex`), Rule 2 (no raw `Database` handle introduced; persistence still via
`SessionStore` / existing `save_db` path), Rule 3 (no new direct
`broadcast::send`; events still via `EventBus`). The grouping is a pure
field-shape change and introduces none of these.
Observable via: `grep -rn 'static .*Mutex\|lazy_static\|OnceCell<.*Mutex' parish/crates/parish-server/src/state.rs` returns no new global-state hit; `git diff parish/crates/parish-server/src/state.rs` shows no new `Database::open` / `broadcast::Sender` field. Confirmed in evidence.md against the scaling-rules checklist.

### C13 [NEW] — Live proof bundle maps criteria to output

`evidence.md` carries the `Evidence type: live gameplay transcript` header and
maps: C1/C2/C3 to the `grep`/`build` output; C4 to the green sensor run; C5 to
the captured **failing** sensor output; C9 to `just check` exit-0; C10 to the
live fixture transcript. `judge.md` (independent) verifies every criterion and
includes the line `Acceptance criteria: met`.

---

## Verification script

Start a live server and drive it with the fixture:

```sh
bash parish/scripts/parish-mcp-backend.sh start
parish --script parish/testing/fixtures/play_appstate-locks.txt
# or, deterministic harness-level:
just run-headless --script parish/testing/fixtures/play_appstate-locks.txt
```

Expected signals from the fixture:

- `/status` returns JSON with a non-empty `location` field (world lock group up).
- `/time` returns a clock string (world group readable).
- A movement command (`go to …`) succeeds and `/debug here` shows a new
  location (world + npc_manager group acquired in order, no deadlock).
- `/save` returns a success line (save-identity group + save_lock + save_db
  acquired in canonical order).
- `/fork <name>` then `/load main` (and `/branches`) succeeds (save-identity
  group written and re-read consistently; branches are addressed by name in the
  harness, not integer id).
- `/stub` + `say` produces a dialogue echo (inference group acquired through
  the command host without inversion).
- Process exits 0; transcript contains no `transport error`, no panic, no 409
  on a single-session run.

Sensor checks (separate from the fixture):

```sh
cargo test -p parish-server --test lock_order_fitness        # C4: green
# or: cargo test -p parish-core --test architecture_fitness lock_order
cargo test -p parish-server                                  # C7/C8: green, no hang
just check                                                   # C9: exit 0
```

Negative (C5): introduce an inverted acquire (or run the in-test bad fixture),
re-run the sensor, capture the failing panic naming the offending pair, then
revert.

---

## Coupling surprises — read before implementing

Everything below was learned by reading the real code and changes the epic's
naïve "just wrap three fields in a struct" assumption.

### CS-1 — Full field map of `AppState` (`state.rs`)

20 `Mutex`-guarded fields (mix of `tokio::sync::Mutex` and `std::sync::Mutex`):

| Field                 | Mutex kind                               | In a group?                                |
| --------------------- | ---------------------------------------- | ------------------------------------------ |
| `world`               | tokio                                    | no (stays standalone, chain head)          |
| `npc_manager`         | tokio                                    | no (standalone)                            |
| `inference_queue`     | tokio                                    | **inference group**                        |
| `client`              | tokio                                    | **inference group**                        |
| `cloud_client`        | tokio                                    | **inference group**                        |
| `config`              | tokio                                    | no — see CS-4                              |
| `conversation`        | tokio                                    | no (standalone)                            |
| `debug_events`        | tokio                                    | no (debug-snapshot cluster)                |
| `game_events`         | tokio                                    | no (debug-snapshot cluster)                |
| `inference_log`       | tokio (`InferenceLog` = `Arc<Mutex<…>>`) | no                                         |
| `editor_sessions`     | tokio                                    | no                                         |
| `active_ws`           | tokio                                    | no (ws-only)                               |
| `save_path`           | tokio                                    | **save-identity group**                    |
| `current_branch_id`   | tokio                                    | **save-identity group**                    |
| `current_branch_name` | tokio                                    | **save-identity group**                    |
| `worker_handle`       | tokio                                    | no                                         |
| `save_lock`           | tokio                                    | no (distinct lifetime; advisory file lock) |
| `save_db`             | tokio                                    | no (chain tail; lazy DB handle)            |
| `setup_status`        | std                                      | no                                         |
| `command_lock`        | tokio (unit)                             | no (per-request serialiser)                |

Non-`Mutex` set-once fields (`event_bus`, `transport`, `ui_config`,
`theme_palette`, `theme_keyframes`, `static_raw_palette`,
`inference_failure_messages`, `idle_messages`, `saves_dir`, `data_dir`,
`game_mod`, `pronunciations`, `flags_path`, `session_id`, `inference_config`,
`inference_file_log`, `chat_transcript_log`, `session_store`,
`language_settings`) are not coordination points — leave them.

### CS-2 — Call-site counts the groupings touch (server `src/`, prod + tests)

Per-field `.lock()`/`.try_lock()` occurrences (from grep over
`parish-server/src`):

- **Inference group** members: `config` ~24, `client` 2, `cloud_client` 1,
  `inference_queue` ~6. NOTE the epic suggested folding `config` into the
  inference group; **don't** (CS-4). The pure inference-client group
  (`client`+`cloud_client`+`inference_queue`) is ~9 prod+test sites.
- **Save-identity group** members: `save_path` ~14, `current_branch_id` ~13,
  `current_branch_name` ~9. They are acquired **together** in the four
  hotspots listed in C2; many other sites read just one. Grouping means every
  single-field read now goes through the group guard — a mechanical but
  wide edit (~36 member references across `routes/saves.rs`,
  `session/lifecycle.rs`, `session/inference_setup.rs`, `session/ticks.rs`,
  `command_host.rs`, `routes/world.rs`).
- Total per-field lock sites across the crate: **230** occurrences in 12 files
  (`routes/tests.rs` alone has ~56). Budget for a large, boring mechanical diff.

### CS-3 — Riskiest (multi-lock) handlers, in lock order

- `session/inference_setup.rs` (`finalize_session_entry` / rebuild): config →
  inference_queue → worker_handle → world → npc_manager → save_lock →
  save_path → current_branch_id → current_branch_name. Touches **both** new
  groups + crosses the chain widely — highest risk.
- `routes/saves.rs` (`do_save_game_inner`, fork, load): save_path → save_lock
  → world → npc_manager → current_branch_id → current_branch_name; and the
  ~347/414/447 triples. Heaviest save-identity user.
- `session/ticks.rs` autosave tick (~399–412): save_path → current_branch_id →
  world → npc_manager → save_db. Crosses save-identity → world → save_db.
- `routes/world.rs` `get_debug_snapshot` (~259–293, ~354–380): inference_queue
  → config → debug_events → game_events → inference_log — the canonical
  inference-group + debug-cluster reader.
- `command_host.rs` `run_command`: world → npc_manager → config (the
  system-command host); `rebuild_cloud_client`: config → cloud_client.
- `session/lifecycle.rs` ~360–363: save_lock → save_path → current_branch_id →
  current_branch_name (pure save-identity write on resume).

### CS-4 — Why `config` is NOT folded into the inference group

The epic text says "an inference group (`client`/`cloud_client`/
`inference_queue`/config)". **Reject folding `config` in.** Reasons:
(1) `config` has ~24 acquisitions, the vast majority **without** any inference
client (`/status`, movement, reactions, world snapshot, command host) — folding
it in would force every config read to contend on the inference lock and would
_widen_ the lock's critical section, the opposite of the goal.
(2) The shared `GameLoopContext` (CS-6) takes `config` and the inference
clients as **separate** `&Mutex` refs; folding them server-side makes the
bridge to that shared API harder, not easier.
(3) The documented chain already orders `config` **between** `conversation` and
`client`; folding it into the inference group would move it past
`conversation`. Keep `config` standalone in its current chain position; group
only the three inference **clients** (`client`/`cloud_client`/
`inference_queue`). Record this deviation from the epic in the PR.

### CS-5 — Why `save_lock`/`save_db` are NOT in the save-identity group

They have distinct lifetimes and chain positions: `save_lock` (advisory file
lock) and `save_db` (cached async DB handle) sit at the **tail** of the chain,
after `save_path`/`current_branch_id`/`current_branch_name`, and are acquired
independently (e.g. the autosave tick takes `save_db` without re-taking the
identity triple). Group only the three identity scalars.

### CS-6 — Mode-parity blast radius: the shared structs pin the field shape

The decisive constraint. `parish_core::game_loop::context::GameLoopContext`
and `parish_core::game_loop::save::NewGameParams` take these fields as
**separate `&Mutex<…>` references**:

- `GameLoopContext` (context.rs): `&Mutex<Option<AnyClient>> client`,
  `&Mutex<Option<AnyClient>> cloud_client`, `&Mutex<Option<InferenceQueue>>
inference_queue`, `&Mutex<GameConfig> config`.
- `NewGameParams` (save.rs): `&Mutex<Option<PathBuf>> save_path`,
  `&Mutex<Option<i64>> current_branch_id`, `&Mutex<Option<String>>
current_branch_name`.

These are constructed by **server** (`routes/saves.rs::do_new_game_inner`,
input/movement paths), **Tauri** (`commands/{input,saves}.rs`, `lib.rs`,
`mcp_bridge.rs`), and **engine** (`real_loop.rs`). Therefore the grouping
**must stay inside `parish-server`'s `AppState`** and the server adapter must
re-derive per-field `&Mutex` borrows when calling the shared API — DO NOT
change the shared struct signatures (that would force Tauri + engine edits and
risk a mode-parity break, Rules 2 & 12). This is C6. If the implementer finds
the borrow bridging too awkward (you cannot hand out `&guard.member` as
`&Mutex<T>` because the member is no longer a `Mutex`), the fallback is to keep
the _members themselves_ as `Mutex` inside the group struct (group = a struct
of `Mutex`es accessed via one `&` to the struct) — but that does **not**
reduce lock count, so the preferred shape is plain members + a server-side
adapter that clones/ös-out what the shared API needs **before** the call (the
codebase already clones config/clients out of locks before `.await`, per the
state.rs "Don't hold these locks across .await" guidance). **Flag this to the
human reviewer at AC sign-off — it is the main design risk.**

### CS-7 — Tauri parity recommendation

`parish-tauri`'s `AppState` (lib.rs ~446) has the identical flat per-field
`Mutex`es (`world`, `npc_manager`, `inference_queue`, `client`, `cloud_client`,
`config`, `save_path`, `current_branch_id`, `current_branch_name`, …).
**Recommendation: keep the Tauri grouping OUT of this PR** to bound the diff
(this PR is already a ~230-site mechanical change in the server), BUT record an
explicit tracked follow-up (C11 option b) so the two backends do not diverge on
lock granularity — divergence is exactly the "invisible at review time" drift
Rule 12 warns about. If the reviewer prefers a single atomic parity change,
option (a) applies the same grouping to Tauri in this PR; the AC supports
either, the PR body must state which.

### CS-8 — Lock-order schema shape

`LOCK_ORDER` is a `&[&str]` of **group/field names in canonical order**, e.g.:
`["world", "npc_manager", "inference", "conversation", "config",
"debug_events", "game_events", "inference_log", "editor_sessions",
"active_ws", "save_identity", "worker_handle", "save_lock", "save_db"]`
(`active_ws`, `setup_status`, `command_lock`, `worker_handle` are WS/setup/
request-scoped and rarely co-acquired with the chain; include the ones the
sensor actually needs to order — minimally the chain members). The sensor maps
a dissolved member name to its group name before comparing indices (e.g. a
literal `client` reference, if any survives in test code, maps to `inference`).
The prose ordering comment in `state.rs` must cite this const, not duplicate it
(C3) — single source of truth.

### CS-9 — Sensor mechanics (match `architecture_fitness.rs` style)

Model it on `backend_agnostic_crates_do_not_pull_runtime_deps`: read source
files textually, collect adjacent `state.<field>.lock()` / `.try_lock()`
occurrences per file (in line order), and for each adjacent ordered pair assert
`index(earlier_in_LOCK_ORDER) < index(later)`. Skip lines inside the sub-struct
definitions and skip `routes/tests.rs` only if test scaffolding intentionally
acquires out of order (prefer NOT skipping — fix the tests). The assert message
must name file, the bad pair, and the fix (`acquire "<A>" before "<B>"; see
LOCK_ORDER in state.rs (Rule 11 / #483)`). Keep it textual + fast (no
compilation, no runtime), consistent with the other sensors.
