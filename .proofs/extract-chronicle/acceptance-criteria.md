# Acceptance Criteria: extract-chronicle

## Task

Extract `parish-core/src/character_log.rs` (~1,468 lines), `location_log.rs`
(~1,254 lines), and `chat_transcript.rs` (~702 lines) into a new workspace
crate `parish-chronicle`. `parish-core` re-exports all three at their
existing `parish_core::` paths so no consumer import changes hands.
`location_log` reuses helpers (`rewrite_profile_section`,
`append_journal_entry`, `append_journal_entry_batch`, `player_diary_label_for`)
that currently live in `character_log`; they must move with the cluster.
This is a behavior-preserving refactor: compile, tests, and live game output
are unchanged.

## Criteria

### C1 — New crate in workspace members

`parish/Cargo.toml` `[workspace] members` contains `"crates/parish-chronicle"`.
`parish/crates/parish-chronicle/Cargo.toml` lists
`parish-npc`, `parish-persistence`, `parish-world`, `parish-types`,
`parish-inference` as its only internal dependencies (no `parish-core`);
external deps are `tokio`, `serde_json`, `chrono`, `anyhow`, `tracing`.
Observable via: `grep 'parish-chronicle' parish/Cargo.toml` returns a hit;
`cargo metadata --no-deps --manifest-path parish/Cargo.toml --format-version 1 | jq '.packages[].name'` includes `"parish-chronicle"`.

### C2 — Re-export shims keep old `parish_core::` paths compiling

`parish-core/src/lib.rs` publishes:

```rust
pub use parish_chronicle::character_log;
pub use parish_chronicle::chat_transcript;
pub use parish_chronicle::location_log;
```

and `parish-core/Cargo.toml` adds `parish-chronicle = { workspace = true }`.
Observable via: `cargo build -p parish-core` and `cargo build -p parish-engine`
both succeed without errors; no file in `parish/crates/` with a `parish_core::character_log` / `parish_core::location_log` / `parish_core::chat_transcript` import is changed.

### C3 — Three source files gone from parish-core; no duplication

`parish-core/src/character_log.rs`, `location_log.rs`, and
`chat_transcript.rs` do not exist on disk after the refactor. The helper
functions (`rewrite_profile_section`, `append_journal_entry`,
`append_journal_entry_batch`, `player_diary_label_for`, `slugify`) exist in
exactly one place: `parish-chronicle/src/character_log.rs`.
Observable via: `cargo test -p parish-core --test architecture_fitness` passes
(`no_orphaned_source_files` and `parish_engine_does_not_duplicate_parish_core_modules`
do not flag anything); `find parish/crates/parish-core/src -name 'character_log.rs' -o -name 'location_log.rs' -o -name 'chat_transcript.rs'` returns empty.

### C4 — Fitness tests updated, not silenced

`parish-core/tests/architecture_fitness.rs` `BACKEND_AGNOSTIC` list adds
`"parish-chronicle"` so the new crate is covered by the
`backend_agnostic_crates_do_not_pull_runtime_deps` sensor. No
`#[allow]` or `cfg(test)` gate hides violations.
Observable via: `cargo test -p parish-core --test architecture_fitness` is green.

### C5 — `just check` green

`cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and
`cargo test --workspace` all pass without new errors or warnings attributable
to this change.
Observable via: `just check` exits 0.

### C6 — Behavior parity: fixture output identical before and after

Running `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_extract-chronicle.txt`
with `PARISH_USER_DATA_DIR` set to a temp directory produces:

- a `/status` JSON line whose `location` field is non-empty,
- a `PlayerMoved` event line (confirmed by `/debug here` showing a new location),
- at least one `DialogueOccurred`-derived `/stub` dialogue exchange,
  confirming that GameEvent processing still works end-to-end through
  `CharacterLogManager::process_event` and `LocationLogManager::process_event`.
  Observable via: the transcript contains the string `"Arrived at"` (produced by
  `CharacterLogManager::process_event` for `GameEvent::PlayerMoved`) somewhere
  in the debug or log output, AND the fixture completes with exit code 0.

### C7 — Character and location markdown logs populate on disk during a live run

After running the fixture with `PARISH_USER_DATA_DIR=/tmp/parish-chronicle-test`:

- `ls /tmp/parish-chronicle-test/*/logs/branch-1/` lists at least one
  `player.md` and at least one `npc-*.md` file.
- `player.md` contains the string `<!-- PROFILE_START -->` (profile section was
  written) AND at least one journal entry line beginning with `### ` (a
  timestamp heading from `append_journal_entry`), confirming `PlayerMoved`
  events were written via the `CharacterLogManager`.
- At least one `loc-*.md` file exists containing `<!-- PROFILE_START -->`, confirming
  `LocationLogManager::write_all_profiles` ran and at least one movement
  event triggered `LocationLogManager::process_event`.
  Observable via: the post-run file inspection in the live proof transcript
  (see evidence.md) shows the file listing and `head` output of `player.md`
  and a `loc-*.md`. This is the chosen observable signal for "logs still
  populate": **on-disk markdown files under `$PARISH_USER_DATA_DIR/*/logs/branch-1/`**
  because the harness writes to an explicit temp dir and the files can be
  inspected directly after the script exits. The `chat_transcript` JSONL is
  at `$PARISH_USER_DATA_DIR/*/saves/inference_logs/*.transcript.jsonl` — populated
  only when inference logging is enabled, which requires a live LLM call; the
  script-mode harness runs with the Markov simulator, so the transcript file
  is not a reliable signal here. On-disk character/location markdown is the
  correct observable signal because it is written synchronously in script mode
  (the REPL drain loop flushes every event before the next command executes).

### C8 — Docs updated

- `docs/agent/architecture.md` workspace crate count and table updated to show
  `parish-chronicle` (18 members; remove the old 17-crate count line).
- `docs/agent/codebase-map.md` `## Parish Crates` table row for
  `parish-chronicle` added; count updated to 18.
- `README.md` repository structure section updated (currently says "16 workspace members").
- `AGENTS.md` (root) / `CLAUDE.md` symlink "17 crates" reference updated to 18.
- If `parish-chronicle` introduces any new external crate dependencies not
  already in the workspace, `just notices` has been run and the resulting
  third-party notices diff is committed in the same PR.
  Observable via: `grep -r 'parish-chronicle' docs/agent/architecture.md docs/agent/codebase-map.md README.md AGENTS.md` returns hits in all four files.

### C9 — Consumer call-sites unchanged

The following files are NOT modified (except to remove now-redundant
`use parish_core::character_log` re-imports if the re-export shim handles
the path transparently):

- `parish/crates/parish-engine/src/app.rs`
- `parish/crates/parish-engine/src/headless.rs`
- `parish/crates/parish-engine/src/testing.rs`
- `parish/crates/parish-server/src/session/ticks.rs`
- `parish/crates/parish-server/src/session/lifecycle.rs`
- `parish/crates/parish-server/src/state.rs`
- `parish/crates/parish-tauri/src/setup.rs`
- `parish/crates/parish-tauri/src/lib.rs`
  All call sites continue to reference `parish_core::character_log::CharacterLogManager`,
  `parish_core::location_log::LocationLogManager`, and
  `parish_core::chat_transcript::ChatTranscriptLog` — no import changes.
  Observable via: `git diff --name-only` (before/after) shows none of the above
  files modified, OR their diffs contain only the deletion of an explicit `use`
  that the re-export already satisfies.

### C10 — Live proof: log mapping each criterion to output lines

The `evidence.md` bundle includes a live gameplay transcript header
(`Evidence type: live gameplay transcript`) and a section that maps:

- C1 to `cargo metadata` or `grep` output confirming workspace membership.
- C3 to `find` output confirming deleted files.
- C5 to `just check` exit-0 output.
- C7 to `ls` + `head` output showing populated `player.md` and `loc-*.md`.
  The `judge.md` explicitly verifies every criterion and includes the line
  `Acceptance criteria: met`.

## Verification script

Run:

```sh
PARISH_USER_DATA_DIR=/tmp/parish-chronicle-test \
  cargo run --manifest-path parish/Cargo.toml -p parish-engine -- \
  --script parish/testing/fixtures/play_extract-chronicle.txt
```

Expected signals in output:

- `/status` command produces JSON with a non-empty `location` field (world is up).
- Movement commands produce `Arrived at <Location>` lines in the text log
  (confirms `CharacterLogManager::process_event` still routes `PlayerMoved`).
- Stub dialogue exchange produces `Good morning` echo in output without error
  (confirms `LocationLogManager::process_event` handles `DialogueOccurred`).
- Process exits 0.

Post-run file checks:

- `ls /tmp/parish-chronicle-test/*/logs/branch-1/` lists `player.md` + `npc-*.md` + `loc-*.md`.
- `grep '<!-- PROFILE_START -->' /tmp/parish-chronicle-test/*/logs/branch-1/player.md` hits.
- `grep '^### ' /tmp/parish-chronicle-test/*/logs/branch-1/player.md` hits (journal entries).
