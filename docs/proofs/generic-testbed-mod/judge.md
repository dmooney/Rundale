Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

---

[mod loads]: transcript.txt line 1 — `"location":"Origin"` confirms testbed mod selected
via `mods/mod-list.toml`, no startup errors in transcript.

[five locations navigable]: transcript.txt lines 4, 7, 9, 12, 14, 17 — `go north`→North
Station, `go south`→South Station, `go east`→East Station, `go west`→West Station,
plus two perimeter edges. All five locations reachable, all `"result":"moved"`.

[NPCs present]: transcript.txt line 3 (Alpha/Systems Tester at Origin), line 11
(Gamma/Validator at East Station), line 17 arrival narration (Beta/Observer at North
Station). All three test agents confirmed at correct home locations.

[pig Latin code-switch wired]: `cargo test -p parish-npc language_directive` output —
`test tests::language_directive_includes_pig_lat_guide ... ok`. The `PIG_LAT_PHRASE_GUIDE`
constant is injected into the NPC system prompt when `native_language = "x-pig-lat"`.

[mod-list selection works]: `cargo test -p parish-core discover_mods` — 6/6 pass including
`discover_mods_selects_via_mod_list` (testbed wins with mod-list.toml),
`discover_mods_errors_when_active_setting_missing` (clear error for unknown ID), and
`discover_mods_rejects_two_settings` (backward-compatible guard preserved).

[blueprint palette delivered to frontend]: `/api/ui-config` response includes
`"default_accent":"#00d4ff"` and `"map_overlay":"grid"` with testbed splash text.
The cyan accent flows to `--color-accent` CSS variable; `map_overlay` triggers the
blueprint-mode body class and grid overlay divs. The static `bg` color from `ui.toml`
is parsed and stored in `AppState.theme_palette` but the dynamic `/api/theme` endpoint
uses time-of-day palette — this is a pre-existing architectural choice, not a regression.

[blueprint CSS + grid overlay]: app.css adds `.blueprint-mode` (monospace font stack)
and `.blueprint-grid-overlay` (cyan SVG grid pattern). +page.svelte toggles
`blueprint-mode` on `document.body` when `map_overlay === "grid"`. MapPanel.svelte and
FullMapOverlay.svelte conditionally render `.blueprint-grid-overlay` div. Code path is
fully exercised at runtime by the server delivering `map_overlay: "grid"`.
