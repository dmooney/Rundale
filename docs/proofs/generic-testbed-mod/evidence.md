Evidence type: live gameplay transcript

## Run

```
cargo run --manifest-path /home/user/Rundale/parish/Cargo.toml --quiet --bin parish \
  -- --script /home/user/Rundale/parish/testing/fixtures/play_generic-testbed-mod.txt
```

Transcript saved to `transcript.txt` (21 lines of JSON output). The server backend
was also started (`bash parish/scripts/parish-mcp-backend.sh start`) and queried for
`/api/ui-config` to verify the blueprint palette payload.

---

## Criterion mapping

### 1. Mod loads

transcript.txt line 1:
```json
{"command":"/status","result":"system_command","response":"Location: Origin | Morning | Winter",
 "location":"Origin","time":"Morning","season":"Winter",...}
```
The engine started with `mod-list.toml` pointing to `"testbed"`, selected the testbed mod, and
placed the player at `"Origin"` (testbed's `start_location = 1`). No startup errors appear
anywhere in the transcript.

---

### 2. Five locations navigable

All four cardinal edges are exercised in the transcript:

| Line | Command         | Resulting location |
|------|-----------------|--------------------|
| 4    | `go north`      | `"North Station"`  |
| 7    | `go south`      | `"South Station"`  |
| 9    | `go east`       | `"East Station"`   |
| 12   | `go west`       | `"West Station"`   |
| 14   | `go south`      | `"South Station"` (perimeter edge S↔W) |
| 17   | `go north`      | `"North Station"` (perimeter edge N↔E) |

All five locations appear: `Origin` (line 1–3), `North Station` (lines 4–6, 17–20),
`South Station` (lines 7–8, 14–16), `East Station` (lines 9–11), `West Station`
(lines 12–13). All edges respond with `"result":"moved"`.

---

### 3. NPCs present at correct stations

**Alpha at Origin** — transcript line 3:
```json
"response":"NPCs here:\n  a methodical figure at a central console — Systems Tester (focused)"
```
Alpha (id 1, occupation "Systems Tester") is present at Origin.

**Gamma at East Station** — transcript line 9 (arrival narration):
```
"\"Welcome. I'm Gamma — I run this place,\" they say."
```
And line 11:
```json
"response":"NPCs here:\n  Gamma — Validator (brisk) [introduced]"
```
Gamma (id 3, occupation "Validator") is present at East Station.

**Beta at North Station** — transcript line 17 (arrival narration):
```
"\"And who might you be? I'm Beta, the Observer,\" they say."
```
Beta (id 2, occupation "Observer") is present at North Station (home/workplace = 2).

---

### 4. Pig Latin code-switch wired

`cargo test -p parish-npc -- language_directive` output (run 2026-05-21):
```
running 8 tests
test tests::language_directive_includes_pig_lat_guide ... ok
test tests::language_directive_en_us_no_native ... ok
test tests::language_directive_en_ie_with_native_ga_ie ... ok
... (8/8 passed)
```
`language_directive_includes_pig_lat_guide` confirms that when
`native_language = "x-pig-lat"`, the pig Latin phrase guide is injected into
the NPC system prompt directive.

---

### 5. Mod-list selection works

`cargo test -p parish-core -- discover_mods` output (run 2026-05-21):
```
running 6 tests
test game_mod::tests::discover_mods_selects_via_mod_list ... ok
test game_mod::tests::discover_mods_errors_when_active_setting_missing ... ok
test game_mod::tests::discover_mods_rejects_two_settings ... ok
test game_mod::tests::discover_mods_requires_a_setting ... ok
test game_mod::tests::discover_mods_finds_setting_and_auxiliary_in_lex_order ... ok
test game_mod::tests::discover_mods_treats_missing_kind_as_setting ... ok
(6/6 passed)
```
`discover_mods_selects_via_mod_list` verifies that a `mod-list.toml` with
`active_setting = "testbed"` selects testbed when two setting mods coexist.
`discover_mods_errors_when_active_setting_missing` verifies a clear error when
the named mod is not found.
`discover_mods_rejects_two_settings` verifies the old single-setting guard is
intact when no `mod-list.toml` is present.

---

### 6. Blueprint palette delivered to frontend

`GET http://127.0.0.1:3030/api/ui-config` response (server running with testbed active):
```json
{
  "hints_label": "Pig Latin Hints",
  "default_accent": "#00d4ff",
  "splash_text": "Engine Testbed\n...",
  "map_overlay": "grid",
  ...
}
```
`"map_overlay": "grid"` and `"default_accent": "#00d4ff"` (testbed cyan) are both
present. The `splash_text` confirms the testbed mod is loaded.

Note: `bg = "#0a1929"` is parsed into `AppState.theme_palette` server-side and stored
correctly (verified by `ThemePaletteConfig` → `ThemePalette` conversion path in
`game_mod.rs`). The dynamic `/api/theme` endpoint uses a time-of-day palette from
`parish-palette`; the mod's static bg color would take effect via a future
"mod overrides dynamic palette" extension. The `default_accent` (cyan) IS applied
immediately via `UiConfigSnapshot` → CSS `--color-accent`.

---

### 7. Blueprint CSS + grid overlay

Code inspection confirms three changes required to satisfy this criterion are in place:

**`parish/apps/ui/src/app.css`** — adds `.blueprint-mode` (monospace fonts) and
`.blueprint-grid-overlay` (SVG data URI graph-paper pattern, 30 px cells, cyan,
0.15 opacity, z-index 2).

**`parish/apps/ui/src/routes/+page.svelte`** — on every `ui-config` update:
```ts
document.body.classList.toggle('blueprint-mode', cfg.map_overlay === 'grid');
```
Since the server delivers `map_overlay: "grid"` (criterion 6), the body receives
the `blueprint-mode` class which applies monospace typography.

**`parish/apps/ui/src/components/MapPanel.svelte`** and
**`parish/apps/ui/src/components/FullMapOverlay.svelte`** — inside `.map-wrap`:
```svelte
{#if $uiConfig.map_overlay === 'grid'}
  <div class="blueprint-grid-overlay"></div>
{/if}
```
When the frontend receives `map_overlay: "grid"` from the server, the grid div is
rendered and the CSS overlay displays the graph-paper pattern on both the minimap
and full-screen map.
