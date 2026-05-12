# Techdebt UI Splits — TD-018, TD-021, TD-022

## What was changed and why

Three over-large Svelte components were split into smaller, focused modules. No behavior changes — public API and rendered output are identical.

### TD-022: SetupOverlay.svelte (1059 -> 897 lines, -162)

Extracted three modules into `$lib/setup/`:
- **`download-rate.ts`** (52 lines) — `formatBytes()`, `formatDuration()`, `formatElapsed()`, `formatDownloadStats()`, rate sampling constants
- **`setup-messages.ts`** (80 lines) — `formatSetupStatusMessage()`, `sentenceBreakParts()`, `isLongSetupWaitMessage()`, `compactSetupMessages()`, constant re-exports
- **`storage.ts`** (83 lines) — `readSetupCompleteFlag()`, `readSetupActivity()`, `markSetupComplete()`, `clearSetupComplete()`, `persistSetupActivity()`, `clearSetupActivity()`, `StoredSetupActivity` type

The component keeps reactive state and orchestration (onMount/onDestroy, IPC wiring, timer management, session flag wrappers).

### TD-021: SavePicker.svelte (786 -> 366 lines, -420)

Extracted two sub-components into `$lib/save-picker/`:
- **`LedgerList.svelte`** (131 lines) — ledger rows, fork/new-game actions. Props: `files`, `saveState`, `loading`. Callbacks: `onswitchledger`, `onforkledger`, `onnewgame`.
- **`DagTree.svelte`** (318 lines) — inverted DAG tree with SVG edges, branch nodes, phantom node for fork creation. Props: `activeFile`, `layout`, `forkingBranchId`, `forkName`, `forkError`, `loading`, `modalBodyEl`, plus callbacks.

CSS was moved alongside the extracted components.

### TD-018: setupMount() in +page.svelte (799 -> 591 lines, -208)

Extracted stream/NPC-turn management into `$lib/setup/stream-manager.ts` (268 lines):
- `appendStreamToken()` — standalone function
- `createStreamManager()` — factory encapsulating `PendingNpcTurn` map, hint state, and all pump/turn lifecycle functions
- Exposes `dispose()`, `pendingTurnCount()`, `hasPendingEndHints()`, `setPendingEndHints()` for the onLoading/onStreamEnd event handlers

The page keeps auto-pause setup, initial data fetch, event listener registration, and demo config fetching.

### Before/after line counts

| File | Before | After | Delta |
|------|--------|-------|-------|
| `SetupOverlay.svelte` | 1059 | 897 | -162 |
| `SavePicker.svelte` | 786 | 366 | -420 |
| `+page.svelte` | 799 | 591 | -208 |
| **Total removed** | | | **-790** |
| `download-rate.ts` (new) | — | 52 | +52 |
| `setup-messages.ts` (new) | — | 80 | +80 |
| `storage.ts` (new) | — | 83 | +83 |
| `LedgerList.svelte` (new) | — | 131 | +131 |
| `DagTree.svelte` (new) | — | 318 | +318 |
| `stream-manager.ts` (new) | — | 268 | +268 |
| **Total added** | | | **+932** |

## Commands run

```sh
npx vitest run          # 396 tests pass (all 32 files)
npx svelte-check        # 5 pre-existing errors (no new errors)
```

## Files changed (9 files)

1. `src/components/SetupOverlay.svelte` — removed local functions, imported from `$lib/setup/`
2. `src/lib/setup/download-rate.ts` — new: download rate tracking and formatting
3. `src/lib/setup/setup-messages.ts` — new: message formatting, wait message detection
4. `src/lib/setup/storage.ts` — new: session storage helpers
5. `src/components/SavePicker.svelte` — replaced inline templates with `<LedgerList>` and `<DagTree>`
6. `src/lib/save-picker/LedgerList.svelte` — new: ledger list view component
7. `src/lib/save-picker/DagTree.svelte` — new: DAG tree view component
8. `src/routes/+page.svelte` — replaced inline stream management with `createStreamManager()`
9. `src/lib/setup/stream-manager.ts` — new: NPC stream turn manager
10. `parish/apps/ui/TODO.md` — moved TD-018/TD-021/TD-022 to Done
