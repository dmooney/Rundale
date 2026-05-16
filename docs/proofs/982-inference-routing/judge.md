# Judge Verdict — #982 inference routing + observability

## Review scope

Reviewed three commits on `worktree-sorted-percolating-jellyfish`
against `origin/main`:

- `90b68ed1 fix(npc): promote tier 2 inference failure to error level`
- `39a49939 fix(tauri): hydrate category_overrides from parish.toml on startup`
- `85c150bd feat(observability): log resolved inference config + npc dialogue/emoji`

Net: 94 insertions, 26 deletions across 7 files in
`parish-core`, `parish-npc`, `parish-tauri`.

## Structural assessment

**Root-cause fix.** The category-overrides hydration patch addresses
the real failure mode demonstrated in `evidence.md`. The Tauri startup
path previously read only `provider`, `base_url`, and `model` from the
wizard-persisted `parish.toml`, ignoring the `[category_overrides.*]`
blocks. For the two-slot Apple Silicon vllm-mlx preset, the small-model
slot then collapsed onto the dialogue port and every Intent / Reaction
request returned 404. The fix delegates to a new
`GameConfig::apply_user_category_overrides()` so the same path is
shared between the BYOK wizard save and runtime startup.

**Mode parity.** `parish-server` does not load `parish.toml` (deployed
service, not desktop wizard target) and `parish-cli` has its own
`--category-*` flags, so no parallel changes are required there. The
new helper lives in `parish-core::ipc::config`, keeping the override
mapping logic in one place if a future entry point needs it.

**Dead-code hygiene.** The local `parse_category` helper in `byok.rs`
became unused after the refactor and is deleted in the same commit,
satisfying the `dead_code` lint.

**Observability additions are pure tracing.** No behaviour changes from
the new `log_resolved_inference_config`, `chat [npc]`, or
`npc-reaction` log lines — they read existing state and emit at INFO.
Risk of log-volume regression is bounded: the startup dump fires once
per session; `chat [npc]` fires per assembled NPC reply (already a
heavyweight operation); `npc-reaction` fires per emoji that was
already being persisted and event-emitted.

**Level promotion is intentional.** Tier 2 inference failures now log
at ERROR so they trip the monitoring/CI gates that filter on error
level. This is the level the sibling `infer_player_message_reaction`
inference failure already used in `parish-npc/src/reactions/emoji_reactions.rs`.

## Testing

- `cargo test -p parish-core --lib` — 354 passed, 1 ignored.
- `cargo test -p parish-npc --lib`  — 417 passed.
- `cargo test -p parish-tauri --lib` — 68 passed.
- `just demo 1 5` reproduced the misroute under `origin/main` and
  verified the fix under this branch (see `evidence.md`).

## Risk

- BYOK wizard save path was refactored to call the new helper. The
  behaviour is equivalent: same fields written, same order, same
  per-category clear-then-write pattern. Covered by existing
  `parish-core::ipc::byok` unit tests.
- Startup hydration applies overrides *before* `fill_missing_models_from_presets()`.
  This matches the BYOK ordering and is the order the existing tests
  in `parish-core::ipc::config` assume.

## Verdict

Verdict: sufficient

Technical debt: clear
