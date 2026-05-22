# Acceptance Criteria: mod-selector-ui

## Task

Add a mod-selection overlay to the main game UI. Opening it shows all available
setting mods found on disk, with the currently active one highlighted. Selecting a
different mod and clicking Confirm writes `mods/mod-list.toml` with the new
`active_setting`, resets the running game session, and reloads the client so the
new mod takes effect immediately.

## Criteria

- **Mod list endpoint returns active flag** — `GET /api/mods` (new) returns a JSON
  array of setting mods, each with `id`, `name`, `title`, `version`, `description`,
  and `active: true` on exactly one entry (the currently active mod) — observable
  via: `curl http://127.0.0.1:3030/api/mods` in the transcript.

- **Switch endpoint updates mod-list.toml** — `POST /api/mods/switch` with body
  `{"mod_id":"rundale"}` writes `active_setting = "rundale"` to `mods/mod-list.toml`
  and returns `{"ok":true}` — observable via: curl POST in transcript + file contents
  after the request.

- **Overlay opens from the UI** — a "Switch Mod" button (or equivalent trigger) in
  the main game page opens a `ModSelectorOverlay` component — observable via:
  browser screenshot showing the overlay rendered over the game.

- **Active mod is visually indicated** — the currently active mod card is styled
  distinctly (e.g., highlighted border or checkmark) and pre-selected — observable
  via: browser screenshot of the overlay.

- **Confirm triggers switch and reload** — clicking Confirm calls `POST /api/mods/switch`,
  and the client performs a full page reload so the new mod's world, palette, and
  NPC roster load — observable via: after confirming a switch from testbed → rundale,
  a fresh CLI run shows `"location":"Kilteevan Village"` instead of `"Origin"`.

## Verification script

Run:
```
cargo run --manifest-path parish/Cargo.toml --bin parish \
  -- --script parish/testing/fixtures/play_mod-selector-ui.txt
```

Expected signals in output:
- Line 1: `"location":"Origin"` — confirms testbed is the active mod at start
- Line 2: `"location":"Origin"` — `look` still at Origin
- No error lines in the stream

Backend API criteria are verified via curl commands captured in transcript.txt
(not the CLI harness). UI criteria (overlay render, active highlight, confirm
reload) are verified via browser screenshot in evidence.md.
