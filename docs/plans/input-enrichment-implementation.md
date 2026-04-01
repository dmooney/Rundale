# Input Enrichment — Implementation Plan

> Parent: [Input Enrichment Ideas](docs/design/input-enrichment-ideas.md) | [Designs](docs/design/input-enrichment/)

## Designs Ready for Implementation

| # | Design | Effort | Backend | Frontend |
|---|--------|--------|---------|----------|
| 01 | [/slash Autocomplete](docs/design/input-enrichment/01-slash-autocomplete.md) | Low | None | InputField.svelte |
| 02 | [Emote *asterisks*](docs/design/input-enrichment/02-emote-actions.md) | Low | input/mod.rs, routes.rs, ticks.rs | ChatPanel.svelte |
| 03 | [Input History](docs/design/input-enrichment/03-input-history.md) | Low | None | InputField.svelte |
| 05 | [Whisper Syntax](docs/design/input-enrichment/05-whisper-syntax.md) | Medium | routes.rs, memory.rs, ticks.rs, types.rs | InputField, ChatPanel, ipc.ts, types.ts |
| 06 | [Emoji Reactions](docs/design/input-enrichment/06-emoji-reactions.md) | Med-High | npc/reactions.rs (new), npc/mod.rs, ticks.rs, types.rs, routes.rs | ChatPanel, game.ts, types.ts, ipc.ts |
| 07 | [Quick-Travel Buttons](docs/design/input-enrichment/07-quick-travel-buttons.md) | Low | None | QuickTravel.svelte (new), +page.svelte |
| 09 | [Smart Replies](docs/design/input-enrichment/09-smart-replies.md) | High | npc/suggestions.rs (new), routes.rs, types.rs | Suggestions.svelte (new), game.ts, +page.svelte |
| 15 | [Tab-Complete Nouns](docs/design/input-enrichment/15-tab-complete-nouns.md) | Medium | None | InputField.svelte, nouns.ts (new) |

## File Conflict Matrix

```
                    01   02   03   05   06   07   09   15
InputField.svelte   ██        ██   ░░                  ██   ← critical bottleneck
ChatPanel.svelte         ██        ░░   ██
routes.rs                ██        ██   ██        ██
types.ts                       ░░   ██   ██        ██
ipc.ts                              ██   ██        ██
game.ts                                  ██        ██
ticks.rs                 ██        ██   ██
+page.svelte                                  ██   ██
npc/mod.rs                              ██
memory.rs                          ██
input/mod.rs             ██

██ = primary changes   ░░ = minor/additive changes
```

## Execution Waves

### Wave 1 — No Conflicts (3 agents in parallel, worktree isolation)

```
Agent A: Idea 07 — Quick-Travel Buttons
  Files: ui/src/components/QuickTravel.svelte (new)
         ui/src/routes/+page.svelte (add component to layout)
  Test:  cd ui && npx vitest run
  Commit: "feat: add location quick-travel chip buttons above input"

Agent B: Idea 09 — Smart Replies (Phase 1: rule-based only)
  Files: crates/parish-core/src/npc/suggestions.rs (new)
         crates/parish-core/src/npc/mod.rs (re-export)
         crates/parish-core/src/ipc/types.rs (SuggestionsPayload)
         crates/parish-server/src/routes.rs (emit after look + conversation)
         ui/src/components/Suggestions.svelte (new)
         ui/src/stores/game.ts (suggestions store)
         ui/src/lib/types.ts (SuggestionsPayload)
         ui/src/routes/+page.svelte (event listener + layout)
  Test:  cargo test && cd ui && npx vitest run
  Commit: "feat: add rule-based contextual action suggestions"

Agent C: Idea 06 — Emoji Reactions (Phase 1: player→NPC only)
  Files: crates/parish-core/src/npc/reactions.rs (new)
         crates/parish-core/src/npc/mod.rs (re-export, ReactionLog on Npc)
         crates/parish-core/src/npc/ticks.rs (inject reaction context)
         crates/parish-core/src/ipc/types.rs (NpcReactionPayload, id on TextLogPayload)
         crates/parish-server/src/routes.rs (react_to_message handler)
         ui/src/lib/reactions.ts (new)
         ui/src/lib/types.ts (Reaction, id, reactions on TextLogEntry)
         ui/src/lib/ipc.ts (reactToMessage)
         ui/src/components/ChatPanel.svelte (hover picker, reaction bar)
         ui/src/stores/game.ts (handle npc-reaction events)
  Test:  cargo test && cd ui && npx vitest run
  Commit: "feat: add player-to-NPC emoji reactions with context injection"
```

**After Wave 1:** Merge all three to branch. Resolve any minor conflicts in shared files (`+page.svelte`, `types.ts`, `game.ts`, `routes.rs`, `npc/mod.rs`, `ipc/types.rs`). These are all additive changes (new fields, new handlers, new imports) so conflicts should be trivial.

### Wave 2 — InputField Cluster (1 agent, sequential)

```
Agent D: Ideas 01 → 03 → 15 (slash autocomplete, input history, tab-complete)
  Depends on: Wave 1 merged (so +page.svelte is stable)

  Step 1 — Slash Autocomplete:
    Files: ui/src/lib/commands.ts (new)
           ui/src/components/InputField.svelte (slash trigger, dropdown, keyboard nav)
           ui/src/components/InputField.test.ts
    Commit: "feat: add /slash command autocomplete dropdown"

  Step 2 — Input History:
    Files: ui/src/stores/history.ts (new)
           ui/src/components/InputField.svelte (Up/Down handling, pushHistory in submit)
           ui/src/components/InputField.test.ts
    Commit: "feat: add input history with Up/Down arrow navigation"

  Step 3 — Tab-Complete Nouns:
    Files: ui/src/stores/nouns.ts (new)
           ui/src/components/InputField.svelte (Tab handling, completion state, priority logic)
           ui/src/components/InputField.test.ts
    Commit: "feat: add Tab-complete for known location and NPC names"

  Test after each step: cd ui && npx vitest run
```

**Why one agent:** All three modify `handleKeydown()` in InputField.svelte. The priority chain (mention dropdown > slash dropdown > history > tab-complete) must be built incrementally. Three separate agents would produce merge conflicts in the same function.

### Wave 2b — Backend-Heavy (2 agents in parallel, alongside Agent D)

```
Agent E: Idea 02 — Emote/Action Prefix
  Depends on: Wave 1 merged (routes.rs stable)
  Files: crates/parish-core/src/input/mod.rs (EnrichedInput, extract_actions)
         crates/parish-server/src/routes.rs (use EnrichedInput in handle_game_input)
         crates/parish-core/src/npc/ticks.rs (format_player_input)
         ui/src/components/ChatPanel.svelte (parseSegments, italic rendering)
  Test:  cargo test && cd ui && npx vitest run
  Commit: "feat: support *action* emote prefix with italic rendering"

Agent F: Idea 05 — Whisper Syntax
  Depends on: Wave 1 merged (routes.rs, types.ts stable)
  Files: ui/src/components/InputField.svelte (detectWhisper — minor, no handleKeydown changes)
         ui/src/components/ChatPanel.svelte (whisper styling)
         ui/src/lib/types.ts (whisper fields on TextLogEntry, SubmitInputRequest)
         ui/src/lib/ipc.ts (extend submitInput)
         crates/parish-server/src/routes.rs (handle_whisper_conversation)
         crates/parish-core/src/ipc/types.rs (whisper fields on TextLogPayload)
         crates/parish-core/src/npc/memory.rs (private flag, context_string_public)
         crates/parish-core/src/npc/ticks.rs (use context_string_public for Tier 2)
  Test:  cargo test && cd ui && npx vitest run
  Commit: "feat: add whisper/private message syntax with context exclusion"
```

**Conflict between E and F:** Both touch `routes.rs`, `ticks.rs`, and `ChatPanel.svelte` — but in different functions/sections. Run in worktrees; merge conflicts will be additive.

**Conflict between D and F:** Both touch `InputField.svelte` — but D modifies `handleKeydown()` while F adds `detectWhisper()` to `handleSubmit()`. Low-risk overlap.

### Final Merge

After all agents complete:
1. Merge Agent D's commits (slash + history + tab-complete)
2. Merge Agent E (emote actions) — resolve `ChatPanel.svelte` conflicts with Agent C
3. Merge Agent F (whisper) — resolve `InputField.svelte`, `ChatPanel.svelte`, `types.ts` conflicts

## Timeline Diagram

```
Time ──────────────────────────────────────────────────────►

Wave 1:  [A: QuickTravel]  [B: SmartReplies]  [C: Reactions]
              │                   │                  │
              └──────── merge ────┴──────────────────┘
                          │
Wave 2:  [D: Slash → History → TabComplete ──────────────────]
         [E: Emote Actions ───────]  [F: Whisper ────────────]
              │                           │                │
              └────────── final merge ────┴────────────────┘
                          │
                     run full suite
                    cargo test && cd ui && npx vitest run
                          │
                     push to branch
```

## Agent Prompt Template

Each agent should receive:

1. **Design doc path** — e.g., "Follow `docs/design/input-enrichment/07-quick-travel-buttons.md` exactly"
2. **Allowed files** — explicit list of files the agent may create/modify
3. **Forbidden files** — files other agents own (to prevent conflicts)
4. **Test command** — what to run before committing
5. **Commit message** — pre-written conventional commit message
6. **Branch** — `claude/input-enrichment-designs-Gii7F` (or worktree branch)

## Verification Checklist

After all waves merge:

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] `cd ui && npx vitest run` passes
- [ ] `cd ui && npx playwright test` passes (E2E)
- [ ] Manual smoke test: type `/`, press Up arrow, press Tab, hover NPC message, click travel chip, see suggestions, use `*action*`, whisper to NPC
