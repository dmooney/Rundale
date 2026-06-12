# parish-chronicle — agent scope

Backend-agnostic leaf crate that owns the three `GameEvent`-bus subscribers that write the on-disk record of a parish session: per-character markdown logs, per-location markdown logs, and a JSONL chat transcript. Extracted from `parish-core` in #1411; `parish-core` re-exports all three modules at their historical paths. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo build -p parish-chronicle
cargo test -p parish-chronicle                    # unit tests (character_log, location_log, chat_transcript)
cargo test -p parish-chronicle -- --nocapture     # with stdout for debugging
```

## Gotchas

- **Leaf-crate dependency rule (rule #1).** Depends on `parish-types`, `parish-world`, `parish-npc`, `parish-persistence`, `parish-inference`, `tokio`, `serde_json`, `chrono`, `anyhow`, `tracing`. Never depend on `parish-core` or any runtime crate (`tauri`, `axum`, `tower*`).
- **Log directory fixed at construction (rule #9).** `CharacterLogManager::new` and `LocationLogManager::new` call `resolve_user_data_dir` once and store the result. Use `new_at_dir` in tests to pass a `tempfile::tempdir` path without setting `PARISH_USER_DATA_DIR` — the env-var races in parallel test runs.
- **Profile section is rewritten every session start; journal is append-only.** `write_all_profiles` replaces the `<!-- PROFILE_START -->`/`<!-- PROFILE_END -->` block from current world state. Journal entries appended via `process_event` are never discarded. `rewrite_profile_section` has a three-stage recovery fallback if the `PROFILE_END` marker is missing.
- **Append idempotence.** `append_journal_entry` skips the write if the rendered heading+body already exists in the file, guarding against replayed fixtures.
- **World-wide events use batched fan-out (TD-031).** `WeatherChanged` and `FestivalStarted` call `append_journal_entry_batch`, which renders the heading/body once and applies it across all location paths, instead of re-rendering per location.
- **`ChatTranscriptLog` shares the `InferenceFileLog` enable flag.** Use `spawn_with_flag` when wiring so `/inference-log off` silences both writers. File opens lazily on the first admitted record; a session that stays opted-out leaves no empty file on disk.
- **Dialogue events are routed by event-time location, not the NPC's current location.** The async bus can deliver an event after a schedule tick has moved the NPC (#1035, #1077, #1079).
- **`character-logs` / `location-logs` feature flags, default on.** When the flag is off, the manager is a no-op and no directories are probed.

## Module map

`lib.rs` — crate root; pub re-exports of the three modules; module-level doc.
`character_log.rs` — `CharacterLogManager` (per-NPC + player markdown logs), `JournalEntry` / `append_journal_entry` / `append_journal_entry_batch` (shared I/O helpers reused by `location_log`), `format_npc_profile` / `format_player_profile` / `format_schedule`, `rewrite_profile_section`, `player_diary_label_for`.
`location_log.rs` — `LocationLogManager` (per-location markdown logs); `format_location_profile` + `strip_description_placeholders` (strips `{time}`/`{weather}` template tokens before writing the static profile pane).
`chat_transcript.rs` — `ChatTranscriptLog` (async JSONL writer backed by a bounded `tokio::mpsc` channel); records NPC dialogue, player moves, NPC interactions, weather and festival events; correlates dialogue records to inference logs via `parish.request_id`; redacts secrets via `parish_inference::secret_scrub::scrub`.
