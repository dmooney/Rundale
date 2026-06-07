---
description: Run a demo-audit session where YOU drive the game directly via the parish MCP (parish_new_game + parish_submit_input + snapshots) instead of the LLM auto-player — deterministic, targeted gameplay probing that surfaces bugs, files them via parish_file_bug and logs them in TODO.md
allowed-tools: Bash, Read, Edit, Write, Grep, Glob, TaskCreate, TaskUpdate, TaskList, mcp__parish__parish_new_game, mcp__parish__parish_submit_input, mcp__parish__parish_world_snapshot, mcp__parish__parish_map, mcp__parish__parish_npcs_here, mcp__parish__parish_save_state, mcp__parish__parish_save_game, mcp__parish__parish_load_branch, mcp__parish__parish_file_bug, mcp__parish__parish_take_screenshot, mcp__parish__parish_latest_screenshot, mcp__parish__tauri_invoke
---

Run a demo-audit session on the Parish/Rundale engine **driving the game yourself
through the parish MCP** rather than watching the LLM auto-player (`just demo`).
Goal is the same as `/demo-audit`: snapshot gameplay quality + surface bugs. The
difference is the actor:

- `/demo-audit` — `just demo` runs the engine's `--demo` auto-player (LLM picks the
  player's actions); you **observe** via MCP/HTTP and post-mortem `parish/.demo-run.log`.
- `/demo-audit-mcp` (this) — **you are the player.** You call
  `mcp__parish__parish_submit_input` each turn and read the response inline. No
  auto-player, no demo-prompt. Deterministic, repeatable, and steerable straight at
  the bug taxonomy (probe a specific NPC, location, verb, or time-of-day on demand).

Use this variant when you want to reproduce a specific suspected bug, sweep a verb
grammar / NPC set methodically, or audit without burning auto-player inference.

# Workflow

## 1. Pre-flight — launch the desktop app

`mcp__parish__*` tools are an HTTP bridge to a backend on `127.0.0.1:3030`. Drive the
**live Tauri desktop window** (NOT the headless `parish-server`) so the visible UI is
exercised and `parish_take_screenshot` / `parish_file_bug` capture a real native-window
image of the live game (the rendered narrative + dialogue column, present-NPC chips;
per #1160 the `file_bug` native capture also includes the MapLibre minimap):

```sh
# From the repo root the workspace manifest is parish/Cargo.toml, so pass it
# explicitly (a bare `cargo run -p parish-tauri` only works from inside parish/):
cargo run --manifest-path parish/Cargo.toml -p parish-tauri -- --mcp-port 3030
```

Launch it in the background (redirect to a logfile, e.g. `> parish/.tauri-run.log 2>&1`,
so step 4 can grep replies), then poll for the bridge before driving:

```sh
until curl -fsS http://127.0.0.1:3030/api/health >/dev/null 2>&1; do sleep 3; done
```

Do not use chained fixed `sleep`s. A display must be available (real desktop session,
not a headless sandbox) — this variant requires the GUI; if no display, fall back to
`/demo-audit-mcp`'s sibling `/demo-audit` or the headless `parish-mcp-backend.sh`.

If any tool returns `isError: true` with `transport error: ...`, the window/bridge is
down — (re)launch it before retrying. Kill stray `parish-tauri` from prior runs first
(`pkill -f parish-tauri`), and free port 3030 if held.

## 2. Start a clean game

- `mcp__parish__parish_new_game` — fresh save branch. Confirm with
  `mcp__parish__parish_world_snapshot` (clock, player location, weather, paused).
- `mcp__parish__parish_save_state` — note the branch id so you can
  `parish_load_branch` back to a known point when bisecting a repro.

## 3. Per-turn drive loop (repeat; this replaces `just demo` cycling)

Each turn is: **observe → decide → act → observe → judge**.

1. **Observe** the current state:
   - `mcp__parish__parish_world_snapshot` — location_name/description, hour:minute,
     time_label, weather, season, festival, paused, inference_paused, name_hints,
     **turn_in_flight**. NOTE: the snapshot carries **no chat/dialogue log** (the
     `WorldSnapshot` struct has no log field, despite the tool's "recent log entries"
     blurb). Use it for world state + clock, not for reading replies.
   - `mcp__parish__parish_npcs_here` — present NPCs: name, real_name, occupation,
     mood, mood_emoji, introduced. Un-introduced NPCs show a descriptive placeholder
     name (e.g. "a small, sharp-eyed old woman …") with `real_name` alongside.
   - `mcp__parish__parish_map` — locations (id, name, adjacent, hops, travel_minutes,
     visited), edges, player_location, transport.
2. **Decide** the next input. Steer at the taxonomy in §4 — don't wander randomly.
   Maintain coverage counters: distinct locations visited, movement attempts vs
   successes, NPCs talked to, NPC reply rate.
3. **Act** with `mcp__parish__parish_submit_input`:
   - Movement: natural-language intents (`"walk over to the forge"`, `"go to the church"`) —
     deliberately probe the parser grammar, including phrasings you expect to fail.
   - Dialogue: free text. Scope it with the optional `addressed_to` array
     (e.g. `addressed_to: ["Peig Hannigan"]`, using the NPC's `real_name`) and verify
     only that NPC answers.
   - System: `"/wait 1"`, `"/pause"`, `"/resume"`, `"look"`.

   **`parish_submit_input` returns `null`** — it is fire-and-forget, NOT the reply.
   The player echo and the NPC reply land in the UI stream + the desktop log
   asynchronously (real inference takes seconds). Do not treat the `null` as failure.

4. **Wait for the turn, then read the reply.** Poll `parish_world_snapshot` until
   `turn_in_flight` is `false` (the clock will also have advanced). Then read what was
   said — there are two ways, since the reply is NOT in any tool's return value:
   - **Screenshot (richest):** `mcp__parish__parish_take_screenshot`, then `Read` the
     PNG — the narrative + dialogue column is rendered, so you see player text, NPC
     replies, movement narration, and the present-NPC chips in one image.
   - **Desktop log grep (scriptable):** tail the `cargo run -p parish-tauri` stdout
     (redirect it at launch, e.g. `> parish/.tauri-run.log 2>&1`):

     ```sh
     grep -nE "chat \[player\] input="   parish/.tauri-run.log   # your inputs
     grep -nE "chat \[npc\] npc=.*reply=" parish/.tauri-run.log   # NPC replies
     grep -nE "chat source=system"        parish/.tauri-run.log   # movement / ambient / errors
     grep -nE "npc-reaction npc=.*emoji=" parish/.tauri-run.log   # mood emoji per reply
     ```

     Also confirm the world mutated as expected (re-`snapshot` + `npcs_here`): the bridge
     is stateful, so a `submit_input` then `world_snapshot` sees the new location/clock.

5. **Judge** against the taxonomy. Per turn ask: did movement land in a new location?
   did the addressed NPC (and only that NPC) reply? repetition loop in the reply?
   empty-location stranding? cross-NPC name leak? mood→emoji drift? wrong-time greeting?

Also grep the same desktop log for engine-side faults the tool responses hide:

```sh
grep -nE "WARN|ERROR|panic"               parish/.tauri-run.log
grep -nE "raw_len="                        parish/.tauri-run.log   # empty actions / burnt turns
grep -nE "tier-2|tier-3|JSON parse|cancellation" parish/.tauri-run.log   # inference faults
```

## 4. Bug taxonomy checklist

- **Movement**: parser rejects valid natural-language intents; silent input drop at empty locations.
- **Dialogue quality**: repetition loops (trailing questions, "'Tis not X but Y", anaphoric chains); over-frequent farewells; NPC mid-reply self-introduction; wrong-time-of-day greetings.
- **Hallucination & leakage**: NPC invents not-present characters; names from prior location leak into current NPC's address of the player; NPCs mis-identify their own village.
- **Address scoping**: with `addressed_to` set, a non-addressed co-located NPC still replies, or the addressed NPC stays silent.
- **Prompt**: redundant fields (weather + time twice); coarse time label only (no HH:MM); recent-events truncation cuts NPC replies mid-sentence; player-name not pinned each turn.
- **Inference**: tier-2 JSON parse fails; tier-3 cancellation surfaces as WARN; empty-action emit burns a turn.
- **Validators**: `poitín` flagged as `hallucinated-gaelic` (allow-list gap); `taking in the sights` flagged `modern-register` (player input leaking into NPC echo).
- **Time/pause**: `/wait N` narration grammar ("1 minutes"); `/pause` + `/resume` idempotency; clock advancing while paused.
- **Mood→emoji map**: same mood label returns different emojis across turns. Two code paths suspected.
- **Schedule**: NPCs arrive/depart at "wrong" times — `ScheduleEntry::start_hour` means "depart at"; NPC is in transit during first `travel_minutes`.

## 5. Verification of suspected-hallucinated names

Before logging a name as hallucination, grep the catalog:

```sh
grep -c '<name>' mods/rundale/npcs.json
grep -c '<name>' mods/rundale/world.json
```

Many "wrong" names are in-canon (Concannon, Niamh Darcy, Curraghboy, sídhe).

## 6. File confirmed bugs via the `parish_file_bug` MCP

File every **confirmed, reproducible** bug as a GitHub issue with
`mcp__parish__parish_file_bug` — it auto-bundles a screenshot, recent logs, and
current game state.

Protocol per bug:

1. **Freeze state at the bug.** Stop driving, then `mcp__parish__parish_take_screenshot`
   (confirm it returns `{path, taken_at, size_bytes}` — needs the live Tauri window;
   under heavy local-MLX load it can take up to the 45 s deadline). Since you are
   driving the desktop app, the capture is a real native-window image of the live UI
   (narrative + dialogue column, present-NPC chips). `parish_file_bug` attaches the
   latest screenshot automatically; an explicit capture guarantees it shows the bug.
2. **Dedup before filing — do NOT spam.**
   `gh issue list --repo dmooney/rundale --state open --search "<keywords>"`. If a
   match exists, comment instead of opening a new issue. Verify in-canon names first (§5).
3. **File it.** `mcp__parish__parish_file_bug` with:
   - `title` — one line, specific.
   - `description` — **Symptom**, **Repro** (the exact `parish_submit_input` texts +
     any `addressed_to`, so it replays through this same MCP flow), **Root cause**
     (cite `file.rs:line` — read the code first, §Constraints), **Expected**. Reference related issues.
   - optional `context` — a debug record (`{kind,label,detail}`) for a specific
     inference call / event / conversation.
     It returns `{created, issue_number, issue_url, screenshot_url}`. Verify the inline
     image: `gh issue view <n> --json body | grep '!\['`.
4. **Dry-run while probing the loop.** Start the backend with
   `PARISH_BUG_REPORT_DRY_RUN=1` to write the report to disk (`created:false`,
   `bundle_path` set) instead of filing. Real audit runs file for real.

## 7. Audit trail in TODO.md

- Maintain `TODO.md` at repo root as the cross-session audit trail. One numbered
  entry per category with **Symptom** / **Root cause** / **Fix** and the filed
  **issue #** / URL. Revise (not delete) earlier entries when later turns refine them.
- At the end list top-10 by impact, each linked to its issue.

# Constraints

- Do NOT write code fixes. Document only, unless the user says "fix X now".
- Read code before claiming root cause — grep `parish/crates/parish-core`, `parish-tauri`, `parish-input`, `parish-npc`, `parish/apps/ui/src/lib`.
- Track coverage every session: distinct-location-count, movement count, NPC reply rate, error/warn count. Spot trends.
- Stop when a sweep adds zero truly new categories, OR when the user says stop. Acceptance-criteria gate doesn't apply — this is documentation work, not a code change.

# Why MCP-driven instead of the auto-player

- **Reproducible**: same input sequence → same path, so a repro you file replays exactly.
- **Targeted**: drive straight to an uncovered NPC/location/verb instead of waiting for the auto-player to wander there.
- **Cheaper**: no auto-player inference per turn — only the NPC reply inference you trigger.
- **Branch bisection**: `parish_save_game` / `parish_load_branch` to checkpoint before a suspect action and replay it.

# Arguments

`$ARGUMENTS` — optional. Examples:

- (empty) — default sweep: new game, visit every adjacent location, talk to each co-located NPC once, probe `/wait` + pause, flag all tiers.
- `quick` — single short drive (~8 inputs), only flag P0/P1.
- `deep` — long methodical sweep: every reachable location, every NPC, `addressed_to` scoping checks, time-of-day greetings across morning/noon/evening via `/wait`, branch-bisect any repro.
- `repro <issue-number>` — drive the exact input sequence from that issue's Repro and confirm/refute.
- `npc <name>` — drive to and probe a single NPC's dialogue quality with `addressed_to`.
