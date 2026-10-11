# limerick-chronicle — agent scope

Backend-agnostic leaf crate that owns the three `GameEvent`-bus subscribers that write the on-disk record of a limerick session: per-character markdown logs, per-location markdown logs, and a JSONL chat transcript. Extracted from `limerick-core` in #1411; `limerick-core` re-exports all three modules at their historical paths. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo build -p limerick-chronicle
cargo test -p limerick-chronicle                    # unit tests (character_log, location_log, chat_transcript)
cargo test -p limerick-chronicle -- --nocapture     # with stdout for debugging
```

## Gotchas

- **Leaf-crate dependency rule ([module ownership](../../../docs/agent/engineering-rules.md#module-ownership)).** Depends on `limerick-types`, `limerick-world`, `limerick-npc`, `limerick-persistence`, `limerick-inference`, `tokio`, `serde_json`, `chrono`, `anyhow`, `tracing`. Never depend on `limerick-core` or any runtime crate (`tauri`, `axum`, `tower*`).
- **Log directory fixed at construction ([runtime paths](../../../docs/agent/persistence-and-session-rules.md#runtime-paths)).** `CharacterLogManager::new` and `LocationLogManager::new` call `resolve_user_data_dir` once and store the result. Use `new_at_dir` in tests to pass a `tempfile::tempdir` path without setting `LIMERICK_USER_DATA_DIR` — the env-var races in parallel test runs.
- **Profile section is rewritten every session start; journal is append-only.** `write_all_profiles` replaces the `<!-- PROFILE_START -->`/`<!-- PROFILE_END -->` block from current world state. Journal entries appended via `process_event` are never discarded. `rewrite_profile_section` has a three-stage recovery fallback if the `PROFILE_END` marker is missing.
- **Append idempotence.** `append_journal_entry` skips the write if the rendered heading+body already exists in the file, guarding against replayed fixtures.
- **World-wide events use batched fan-out (TD-031).** `WeatherChanged` and `FestivalStarted` call `append_journal_entry_batch`, which renders the heading/body once and applies it across all location paths, instead of re-rendering per location.
- **`ChatTranscriptLog` shares the `InferenceFileLog` enable flag.** Use `spawn_with_flag` when wiring so `/inference-log off` silences both writers. File opens lazily on the first admitted record; a session that stays opted-out leaves no empty file on disk.
- **`character-logs` / `location-logs` feature flags, default on.** When the flag is off, the manager is a no-op and no directories are probed.
- **Dedup was removed in PR #1032 (89ab669d).** The bus now only publishes real physical movements (`schedule::tick_schedules`, `ticks::apply_tier3_updates`, `game_session::apply_movement`), so the writer is stateless beyond `log_dir`. Do not reintroduce `last_arrival` / `scan_existing_*` — they were obsoleted on purpose. (Issues #1013, #1014 closed because of this.)
- **Log subscribers must capture `location` at publish time, not re-resolve `npc.location` at consume time.** The `GameEvent` bus is async — a schedule tick can move the NPC between `publish` and the subscriber running — so resolving the destination from the NPC's _current_ location mis-files the entry (#1035). `DialogueOccurred`, `NpcInteraction`, and now `MoodChanged` + `LifeEvent` all carry an event-time `location`; the location-log subscriber routes by it. When adding a new event whose location matters, add the field and populate it at every publish site (the compiler enumerates them) — `MoodChanged`/`LifeEvent` had ~9 publish sites across `tier4.rs`, `banshee.rs`, `ticks.rs` (#1077/#1079).
- **The on-disk chronicle writers live in `limerick-chronicle` now, re-exported by `limerick-core`.** `character_log.rs`, `location_log.rs`, `chat_transcript.rs` moved out of `limerick-core/src/` into the leaf crate `limerick-chronicle` (deps: `limerick-npc`/`limerick-world`/`limerick-types`/`limerick-persistence`/`limerick-inference`; backend-agnostic, in the `BACKEND_AGNOSTIC` fitness list). `limerick-core/src/lib.rs` keeps the historical paths via `pub use limerick_chronicle::{character_log, chat_transcript, location_log};`, so every consumer (`limerick_core::character_log::CharacterLogManager`, server/tauri/engine) compiles unchanged. The event-pump call-sites AND the branch-switch subscriber rebinds (`limerick-server/src/session.rs`, `limerick-tauri/src/setup.rs`, CLI `App::rebind_log_managers_if_branch_changed`) STAY in their entry-point crates and reach the managers through the re-export.
- **Script-mode chronicle proofs: `LIMERICK_USER_DATA_DIR` is the FULL log root, so logs land at `$DIR/logs/branch-1/`, NOT `$DIR/*/logs/branch-1/`** (no app-name suffix). The `play_extract-chronicle.txt` fixture's `*/logs/` glob and the AC's "Arrived at" string are both slightly off: the real `PlayerMoved` journal heading is `### <date>, HH:MM — Arrived` (no "at"). Script runs replay byte-identically (#2033), so compare whole transcripts; `just prove-diff` does this against `main`.

## Module map

`lib.rs` — crate root; pub re-exports of the three modules; module-level doc.
`character_log.rs` — `CharacterLogManager` (per-NPC + player markdown logs), `JournalEntry` / `append_journal_entry` / `append_journal_entry_batch` (shared I/O helpers reused by `location_log`), `format_npc_profile` / `format_player_profile` / `format_schedule`, `rewrite_profile_section`, `player_diary_label_for`.
`location_log.rs` — `LocationLogManager` (per-location markdown logs); `format_location_profile` + `strip_description_placeholders` (strips `{time}`/`{weather}` template tokens before writing the static profile pane).
`chat_transcript.rs` — `ChatTranscriptLog` (async JSONL writer backed by a bounded `tokio::mpsc` channel); records NPC dialogue, player moves, NPC interactions, weather and festival events; correlates dialogue records to inference logs via `limerick.request_id`; redacts secrets via `limerick_inference::secret_scrub::scrub`.
