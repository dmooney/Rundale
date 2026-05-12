Evidence type: test-output + svelte-check

## Summary

Resolved TD-002 and TD-003 from `parish/apps/ui/TODO.md` — two P2 duplication items in the map-related components.

### TD-002: Extract `<MapTooltip>` component

The tooltip HTML template (conditional rendering of name, explored/unexplored, indoor/outdoor, travel minutes) was duplicated verbatim in both `MapPanel.svelte` and `FullMapOverlay.svelte`. Created `src/components/MapTooltip.svelte` with a `variant` prop (`"minimap"` / `"full"`) that carries the slightly different positional/sizing CSS each parent was using. Both parents now render `<MapTooltip info={tooltip} variant="minimap|full" />` instead of the inline block.

### TD-003: Extract shared tile-source block

Both components had an identical `$effect(() => { if (!mounted || !controller) return; controller.setTileSource(currentTileSource($tiles)); })` block. Created `src/lib/map/tileSync.ts` with a `subscribeTileSource(getController)` function that uses `tiles.subscribe()` under the hood — eliminating the duplicate effect and the need to manage a `mounted` flag for tile-sync purposes. Each component now calls `subscribeTileSource(() => controller)` inside `onMount` and unsubscribes in the cleanup closure.

## Files changed

- **Added:** `src/components/MapTooltip.svelte` (new extracted component)
- **Added:** `src/lib/map/tileSync.ts` (new shared tile-source subscriber)
- **Modified:** `src/components/MapPanel.svelte` (imports, tooltip markup replaced, tile-sync effect replaced)
- **Modified:** `src/components/FullMapOverlay.svelte` (imports, tooltip markup replaced, tile-sync effect replaced)
- **Modified:** `parish/apps/ui/TODO.md` (moved TD-002/TD-003 to Done, added progress log)

## Test Output (vitest run)

```
Test Files  32 passed (32)
Tests       379 passed (379)
```

## svelte-check

```
svelte-check found 3 errors and 1 warning in 2 files
```

3 errors are pre-existing (`process.env` in `vite.config.ts` without `@types/node`). 1 warning is a pre-existing CSS vendor-prefix compatibility. No errors/warnings in modified files.
