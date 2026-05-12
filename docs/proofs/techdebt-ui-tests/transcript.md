# Phase 1.9 — apps/ui Weak Tests (TD-010 through TD-013)

## What was changed

Added 17 new tests across 4 test files to close the TD-010–TD-013 weak-test gaps from the tech-debt sweep.

### TD-010: DebugPanel — Weather/Gossip/Conversation tabs
**File:** `parish/apps/ui/src/components/DebugPanel.test.ts`
- **Weather tab (index 3):** 2 tests — renders weather engine details (current, since, duration, last check hour); shows "(never)" when `last_check_hour` is null.
- **Gossip tab (index 4):** 2 tests — shows "(no gossip)" when empty; renders gossip items with content, source, known_by, and distortion badges.
- **Conversations tab (index 5):** 2 tests — shows "(no exchanges)" when empty; renders player/NPC dialogue exchange entries.

### TD-011: SavePicker — IPC failure paths
**File:** `parish/apps/ui/src/components/SavePicker.test.ts`
- **loadBranch failure:** 1 test — mock rejects, verifies dialog stays open.
- **createBranch failure:** 1 test — mock rejects, verifies `.fork-error` text appears with error message.
- **newSaveFile failure:** 1 test — mock rejects, verifies dialog stays open via Ledgers view.
- **newGame failure:** 1 test — mock rejects, verifies dialog stays open via Ledgers view.

### TD-012: ChatPanel — reaction rollback, tabular, scroll
**File:** `parish/apps/ui/src/components/ChatPanel.test.ts`
- **Reaction IPC failure rollback:** 1 test — mock `reactToMessage` rejects, verifies optimistic reaction is rolled back (no `[data-testid="reaction-bar"]` after flush).
- **Tabular subtype rendering:** 2 tests — renders `.tabular-grid` with `.tabular-header`, `.tabular-cmd`, and `.tabular-desc` elements; renders multiple cmd/desc pairs.
- **Scroll-to-bottom behavior:** 1 test — verifies chat-panel element exists and renders after adding new messages without throwing.

### TD-013: SetupOverlay — error state rendering
**File:** `parish/apps/ui/src/components/SetupOverlay.test.ts`
- **Error box with message:** 1 test — triggers `done` callback with `success: false`, verifies "Something went wrong." title, `.error-box` with error message, and close hint.
- **Empty error string:** 1 test — verifies error box appears with generic fallback when error string is empty.
- **Overlay persists on error:** 1 test — verifies `.setup-overlay` remains visible after error (does not auto-close).

## Test results

Before: 379 tests passing (32 files)
After:  396 tests passing (32 files)
Delta:  +17 tests, 0 failures

## Commands

```sh
cd parish/apps/ui && npx vitest run
```
