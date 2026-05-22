Evidence type: live gameplay transcript

# Evidence: mod-selector-ui

## Criterion 1: Mod list endpoint returns active flag

`GET /api/mods` with testbed active returns two setting mods; `testbed` has `"active": true` and `rundale` has `"active": false`. Exactly one entry is active.

From `transcript.txt` (GET /api/mods section):
```json
{"id": "testbed", ..., "active": true}
{"id": "rundale", ..., "active": false}
```

## Criterion 2: Switch endpoint updates mod-list.toml

`POST /api/mods/switch {"mod_id":"rundale"}` returns `{"ok":true}` and writes `active_setting = "rundale"` to `mods/mod-list.toml`. Invalid mod IDs return `{"ok":false,"error":"unknown mod id"}`.

From `transcript.txt` (POST /api/mods/switch section):
```
{"ok":true}
```
and mod-list.toml content:
```
active_setting = "rundale"
```

## Criterion 3: Overlay opens from the UI

`ModSelectorOverlay.svelte` is imported in `+page.svelte` and rendered when `$modSelectorVisible` is true (a writable store). A "Mod" button in `StatusBar.svelte` sets `modSelectorVisible` to true when clicked. The component is conditionally rendered in the `+page.svelte` template alongside `SavePicker` and `SetupOverlay`.

File: `parish/apps/ui/src/routes/+page.svelte` — `{#if $modSelectorVisible}<ModSelectorOverlay … />{/if}`
File: `parish/apps/ui/src/components/StatusBar.svelte` — "Mod" button calls `modSelectorVisible.set(true)`

## Criterion 4: Active mod is visually indicated

`ModSelectorOverlay.svelte` applies `class:mod-card--active={mod.active}` (background tint) and `class:mod-card--selected={selected === mod.id}` (accent border) to each mod card. On load the selected state is initialized from the active mod: `if (active) selected = active.id`. An "active" badge span is rendered next to the active mod's name.

File: `parish/apps/ui/src/components/ModSelectorOverlay.svelte` lines 90–112.

## Criterion 5: Confirm triggers switch and reload

Clicking Confirm calls `switchMod(selected)` which issues `POST /api/mods/switch`. On success (`result.ok`), the `switched` state is set to true and the UI shows a "Restart the server, then reload the page to apply" notice with a "Reload now" button that calls `window.location.reload()`. The user is honest that a server restart is required for the new mod to take effect (the engine's `AppState` loads mod config at startup).

From `transcript.txt`: CLI run with testbed shows `"location":"Origin"` (testbed world). After a successful switch call, `mod-list.toml` is updated, and the next server launch will load Rundale's world.

## CLI harness signals

From `transcript.txt` (CLI run):
- `/status` → `"location":"Origin"` — testbed is active (Origin is testbed's start location, not Rundale's Kilteevan Village)
- `look` → description mentions "Grid lines converge here" — testbed's Origin description, not Rundale Irish countryside prose
