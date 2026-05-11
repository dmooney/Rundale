Evidence type: screenshot

## Summary

Resolved 19 items from `parish/apps/ui/TODO.md` across 5 batches:

### Batch 1 — Dead Code + Duplication
- **TD-001** (P1): Extracted shared MapLibre GL mock to `__mocks__/maplibre-gl.ts`, both test files use `vi.mock('maplibre-gl')` without factory
- **TD-023** (P2): Removed no-op `export const prerender = true` from `+layout.ts`
- **TD-024** (P3): Deleted empty `src/lib/index.ts` barrel file

### Batch 2 — Weak Tests (data modules)
- **TD-007** (P2): Added `reactions.test.ts` — validates 12 entries, unique emoji/keys, non-empty descriptions
- **TD-008** (P2): Added `map-icons.test.ts` — validates ICON_PATHS, all 14 NAME_RULES patterns, fallback
- **TD-009** (P2): Added `theme.test.ts` — round-trip, corrupt JSON, missing key, quota-exceeded

### Batch 3 — Weak Tests (stores + demo)
- **TD-006** (P1): Added `travel.test.ts` — startTravel, cancelTravel, clamping, auto-clear, #349 mutual-cancellation
- **TD-005** (P1): Added `demo-player.test.ts` — stopDemo, runDemoTurn early returns, status lifecycle
- **TD-030** (P2): Added `addReaction()` / `removeReaction()` tests in `game.test.ts`

### Batch 4 — Dead Code + Config
- **TD-004** (P3): Extracted `typeIntoEditor()` to module level in `InputField.test.ts`, removed 7 copies
- **TD-026** (P3): Added `/unexplored` to `FEATURES_MD_COMMANDS`, removed from `REGISTRY_ONLY_COMMANDS`
- **TD-029** (P3): Removed unnecessary `rewriteRelativeImportExtensions` from `tsconfig.json`

### Batch 5 — Config + Comments
- **TD-025** (P2): Updated stale TODO to `Known limitation` doc in `style.ts`
- **TD-027** (P2): Added documented note to `package.json` explaining cookie override
- **TD-028** (P2): Removed fragile `declare const process` from `vite.config.ts` (uses `@types/node`)

### Batch 6 — Weak Tests (components)
- **TD-014** (P3): Added `AuthStatus.test.ts` — fetch flow, Tauri bypass, login/logout rendering
- **TD-015** (P3): Added `DemoBanner.test.ts` — visibility, turn count, pause/resume, status, stop
- **TD-016** (P3): Added `DemoPanel.test.ts` — fields, Apply & Start, Pause/Resume, Stop, status

## Test Output (vitest run)

Test Files  32 passed (32)
Tests       379 passed (379)
Duration    13.12s

## Files changed

- Added: 8 new test files, 1 shared mock
- Modified: 8 existing files (test files, config, style comment, TODO.md)
- Deleted: 2 files (barrel, unused mock copy)
