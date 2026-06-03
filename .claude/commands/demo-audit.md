---
description: Run a demo-audit session — cycle `just demo` with live MCP/HTTP inspection, surface gameplay bugs, file them via the parish_file_bug MCP (screenshot + logs + state) and log in TODO.md
allowed-tools: Bash, Read, Edit, Write, Grep, Glob, TaskCreate, TaskUpdate, TaskList, mcp__parish__parish_file_bug, mcp__parish__parish_take_screenshot, mcp__parish__parish_latest_screenshot, mcp__parish__parish_world_snapshot, mcp__parish__parish_npcs_here, mcp__parish__parish_submit_input
---

Run a demo-audit session on the Parish/Rundale engine. Goal: snapshot current gameplay quality + surface bugs via repeated `just demo` cycles combined with live MCP/HTTP inspection.

# Workflow

## 1. Pre-flight

- Confirm `--mcp-port` is wired into `just demo` (`parish/justfile`). If recipe lacks it, add `--mcp-port ${PARISH_MCP_PORT:-3030}` and ensure `parish/.demo-run.log` is gitignored before running.
- Kill any stray `parish-tauri` / `parish-server` before each cycle.

## 2. Per-cycle protocol (repeat until new-issue rate ≈ 0)

- Launch: `just demo 2 N > parish/.demo-run.log 2>&1` in background (start with N=8, raise to 12-20 once builds are cached).
- Wait for MCP: poll `curl -sf http://127.0.0.1:3030/api/health` in a background loop with `until ... do sleep 3; done`. Do not use chained `sleep` commands.
- Mid-run snapshots via `/usr/bin/curl -s` (bypass any rtk shim):
  - `/api/world-snapshot` — loc, hour:minute, time_label, weather, paused
  - `/api/npcs-here` — present NPCs, mood, mood_emoji
  - `/api/map` — adjacent locations, transport, player idx
  - `/api/save-state` — branch metadata
  - Pipe through `python3 -c 'import sys,json; ...'` for parsing. `/api/debug-snapshot` may 404 from the MCP bridge (only the standalone parish-server exposes it).
- Wait for demo PID to exit (background poll, no fixed sleep).
- Post-mortem grep on `parish/.demo-run.log`:

```sh
grep -nE "demo turn: LLM chose"      # player actions
grep -nE "chat \[npc\]"              # NPC replies
grep -nE "chat \[player\]"           # player text
grep -nE "chat source=system"        # system events (movement, ambient, errors)
grep -nE "WARN|ERROR|panic"
grep -nE "^NPCs here:|^Date and time"
grep -nE "Time stirs|clocks of the parish stand still"
grep -nE "raw_len="                  # check for empty actions
```

- For each cycle ask: distinct locations visited? movement attempts vs successes? NPC reply rate? loop patterns in replies? empty-location stranding? cross-NPC name leak? mood→emoji drift? pause-toggle bursts (correlate with the user's input cadence)?

## 3. Bug taxonomy checklist

- **Movement**: parser rejects valid natural-language intents; silent input drop at empty locations.
- **Dialogue quality**: repetition loops (trailing questions, "'Tis not X but Y", anaphoric chains); over-frequent farewells; NPC mid-reply self-introduction; wrong-time-of-day greetings.
- **Hallucination & leakage**: NPC invents not-present characters; names from prior location leak into current NPC's address of the player; NPCs mis-identify their own village.
- **Prompt**: redundant fields (weather + time appear twice); coarse time label only (no HH:MM); recent-events truncation cuts NPC replies mid-sentence; player-name not pinned each turn.
- **Inference**: tier-2 JSON parse fails; tier-3 cancellation surfaces as WARN; empty-action emit burns a turn.
- **Streaming**: `stream-manager.ts` `pendingNpcTurns` is a Map keyed by `turnId` — two NPC replies can pump in parallel. Check `parish/apps/ui/src/lib/setup/stream-manager.ts` + `stream-pacing.ts`.
- **Validators**: `poitín` flagged as `hallucinated-gaelic` (allow-list gap); `taking in the sights` flagged `modern-register` (player input leaking into NPC echo).
- **Time/pause spam**: frontend `auto-pause.ts` dispatches `/pause` + `/resume` on idle/activity — bursts correlate with user's computer use, NOT session age.
- **Mood→emoji map**: same mood label returns different emojis across cycles. Two code paths suspected.
- **Schedule**: NPCs arrive/depart at "wrong" times — `ScheduleEntry::start_hour` means "depart at"; NPC is in transit during first `travel_minutes`.

## 4. Verification of suspected-hallucinated names

Before logging a name as hallucination, grep the catalog:

```sh
grep -c '<name>' mods/rundale/npcs.json
grep -c '<name>' mods/rundale/world.json
```

Many "wrong" names are in-canon (Concannon, Niamh Darcy, Curraghboy, sídhe).

## 5. File confirmed bugs via the `parish_file_bug` MCP

File every **confirmed, reproducible** bug as a GitHub issue with
`mcp__parish__parish_file_bug` — it auto-bundles a live screenshot, recent
logs, and current game state, so the issue is self-contained. As of #1160 the
screenshot is a native window capture, so the **MapLibre minimap renders in the
image** (the old html-to-image path captured it blank).

Protocol per bug:

1. **Capture context at the moment of the bug.** The bug is filed against
   whatever state is live, so freeze it first: stop advancing the demo, then
   `mcp__parish__parish_take_screenshot` (confirm it returns a path — the live
   desktop window must be present and foregrounded; under heavy local-MLX load
   it can take up to the 45 s deadline). `parish_file_bug` attaches the latest
   screenshot automatically; an explicit capture just guarantees it shows the
   bug.
2. **Dedup before filing — do NOT spam.** Search open issues first:
   `gh issue list --repo dmooney/rundale --state open --search "<keywords>"`.
   If a matching issue exists, add a comment instead of a new issue. Many
   "wrong" names/behaviours are in-canon (see §4) — verify before filing.
3. **File it.** `mcp__parish__parish_file_bug` with:
   - `title` — one line, specific (e.g. `/wait 1 narration reads "1 minutes"`).
   - `description` — **Symptom** (observed line/behaviour), **Repro** (exact
     inputs), **Root cause** (cite `file.rs:line` — read the code first, §Constraints),
     **Expected**. Reference related issues by number.
   - optional `context` — a debug-panel record (`{kind,label,detail}`) when the
     bug is about a specific inference call / event / conversation.
   It returns `{created, issue_number, issue_url, screenshot_url}`. Verify the
   issue body has the inline image: `gh issue view <n> --json body | grep '!\['`.
4. **Dry-run when probing the loop, not real bugs.** Set
   `PARISH_BUG_REPORT_DRY_RUN=1` in the demo/backend env to write the report to
   disk (`created:false`, `bundle_path` set) instead of filing — use while
   testing the audit flow so you don't create throwaway issues. Real audit runs
   file for real.

## 6. Audit trail in TODO.md

- Maintain a `TODO.md` at repo root as the cross-cycle audit trail. One numbered
  entry per category, with **Symptom** / **Root cause** / **Fix** and the filed
  **issue #** / URL. Group by cycle of discovery. Revise (not delete) earlier
  entries when later cycles refute or refine them.
- At the end list top-10 by impact, each linked to its issue.

# Constraints

- Do NOT write code fixes. Document only, unless the user says "fix X now".
- Read code before claiming root cause. Avoid asserting based on symptom alone — grep `parish/crates/parish-core`, `parish-tauri`, `parish-input`, `parish-npc`, `parish/apps/ui/src/lib`.
- Track every cycle's distinct-location-count, movement count, NPC reply rate, error/warn count. Spot trends across cycles.
- Stop when 2 consecutive cycles add zero truly new categories, OR when the user says stop. Acceptance-criteria gate doesn't apply — this is documentation work, not a code change.

# Optional enhancements

- Use `mcp__parish__parish_submit_input` to nudge the player toward a specific location and probe uncovered NPCs (Aoife Brennan, Sean Ruadh Kelly, Brigid Ni Fhatharta, etc).
- Compare reply quality across NPCs (Padraig Darcy >> Duffy family observed); audit `npcs.json` for what makes Padraig good.
- Read `parish/crates/parish-input/src/parser.rs` to enumerate the movement-verb grammar — fixes the silent-rejection bug list.

# Arguments

`$ARGUMENTS` — optional. Examples:
- (empty) — default 5-12 cycle audit with `N=8..15` turns each.
- `quick` — single cycle, `N=8`, only flag P0/P1.
- `deep` — up to 20 cycles, vary `N` (8/12/18/20), include MCP nudges.
- `verify <issue-number>` — re-run targeting a specific TODO entry's preconditions.
