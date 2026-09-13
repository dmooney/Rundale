# limerick-chronicle

The on-disk **chronicle** writers for Limerick — the `GameEvent`-bus subscribers
that persist the player-visible history of a parish to disk.

This is a backend-agnostic leaf crate: it depends only on the logic crates
(`limerick-npc`, `limerick-world`, `limerick-types`, `limerick-persistence`,
`limerick-inference`) and never on a runtime backend (`tauri` / `axum` /
`tower*`). It is covered by the `backend_agnostic_crates_do_not_pull_runtime_deps`
architecture-fitness sensor.

## Module map

- `character_log` — per-NPC and player markdown logs under
  `<user-data-dir>/<app>/logs/<branch>/` (`player.md`, `npc-NNN-slug.md`). Each
  file has a profile section bounded by `<!-- PROFILE_START -->` /
  `<!-- PROFILE_END -->` (rewritten every session by
  `CharacterLogManager::write_all_profiles`) and an append-only journal that
  grows as `GameEvent`s flow through `CharacterLogManager::process_event`. Owns
  the shared profile/journal helpers (`rewrite_profile_section`,
  `append_journal_entry`, `append_journal_entry_batch`,
  `player_diary_label_for`, `slugify`) reused by `location_log`.
- `location_log` — per-location markdown logs (`loc-NNN-slug.md`), mirroring the
  character-log structure and reusing its profile/journal helpers.
  `LocationLogManager::write_all_profiles` / `process_event`.
- `chat_transcript` — a persistent JSONL transcript of the player-visible chat
  stream (`{saves_dir}/inference_logs/{session_id}.transcript.jsonl`),
  correlated to inference-log lines via `limerick.request_id`. Sibling of
  `limerick_inference::file_log::InferenceFileLog`; shares its enable flag.

## State model

The character-log writer is **stateless beyond `log_dir`**: the profile section
is rewritten every session, the journal is append-only, and movement dedup was
deliberately removed in #1032 (the bus now publishes only real physical
movements). Do not reintroduce `last_arrival` / `scan_existing_*`.

## Re-export

`limerick-core` re-exports all three modules at their historical paths:

```rust
pub use limerick_chronicle::character_log;
pub use limerick_chronicle::chat_transcript;
pub use limerick_chronicle::location_log;
```

so every consumer (`limerick_core::character_log::CharacterLogManager`,
`limerick_core::location_log::LocationLogManager`,
`limerick_core::chat_transcript::ChatTranscriptLog`) compiles with zero import
changes. The branch-switch subscriber-rebind call sites stay in their
entry-point crates (`limerick-server/src/session.rs`,
`limerick-tauri/src/setup.rs`, the CLI `App`) and reach the managers through
these re-exports (#1011, #1034).
