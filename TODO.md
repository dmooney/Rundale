# Demo Run TODO — Gameplay Quality Snapshot

Generated from `just demo 2 8` plus MCP/`/api/*` audit on 2026-05-25.
Branch: `fix/demo-run-tauri-panic-and-travel-detail`.

## P0 — Blocks gameplay

### 1. Auto-player never moves (8/8 turns at one location)

- **Symptom**: 8-turn demo stayed at The Mill. LLM produced literal text "I must be on my way now" twice, but no movement action emitted. Adjacent `Kilteevan Village — 1 min away` never used.
- **Root cause**: The `{"action": "<free text>"}` JSON schema in `parish-tauri/src/commands.rs` demo turn only accepts dialogue. Demo prompt (`mods/rundale/demo-prompt.txt`) says "travel widely" but the action grammar has no movement verb.
- **Fix**: Extend demo-turn action schema with `{"action": "...", "go_to": "<location-name>"}` variant, OR add an explicit "movement" tool the auto-player can call. Update demo prompt to document the movement syntax.

### 2. MCP port not opened by `just demo` — RESOLVED 2026-06-04 (commit 5d7a935c)

- **Symptom**: Recipe `parish/justfile:122` builds `DEMO_ARGS` without `--mcp-port`. `parish-tauri/src/lib.rs:1003` only opens the MCP bridge when the flag is present. So `mcp__parish__*` tools always fail during a demo.
- **Fix**: Add `--mcp-port 3030` (or accept env override) to the demo recipe so MCP works by default.

## P1 — Visible gameplay bugs

### 3. Mood→emoji map miscategorises negative moods — RESOLVED 2026-06-04 (commit 5cafc389)

- **Symptom**: `/api/npcs-here` returned `bitter` → 🙂 and `sharp` → 🙂 (Sean Ruadh Kelly, Peig Hannigan). Bitter ≠ smile.
- **Fix**: Audit `MOOD_EMOJI` table in `parish-npc` (or wherever `mood_emoji` is derived). Bitter should be 😠/😒; sharp should be 😤 or similar.

### 4. NPCs sign off mid-conversation

- **Symptom**: Turn 2 Cormac Duffy reply ends `"...Slán abhaile"` (Irish "safe journey home") yet conversation continued five more turns. Premature farewell suggests dialogue prompt allows closure tokens.
- **Fix**: Add anti-farewell guidance to `mods/rundale/prompts/dialogue.*` template, or post-filter `Slán abhaile` / `Goodbye` / `Farewell` from mid-conversation replies.

### 5. Time label hides progression from LLM — RESOLVED 2026-06-04 (commit 2a1f133e)

- **Symptom**: Demo prompt feeds only `Date and time: Monday, 20 March 1820, Afternoon | Spring` — no hour:minute. 36× speed factor advances clock but model can't see it.
- **Fix**: Include `HH:MM` in the `user_prompt` time line (`parish_tauri_lib::commands::demo_turn`).

## P2 — Noise / housekeeping

### 6. Pause/unpause emission spam

- **Symptom**: 38 `clocks stand still` ↔ `Time stirs` toggles across 8 turns (~5 per turn).
- **Fix**: Dedupe per turn — emit at most one pause edge and one resume edge per demo step.

### 7. NPC reply truncated in recent-events buffer — RESOLVED 2026-06-04 (commit b8629534)

- **Symptom**: Line 151 of `.demo-run.log` shows Cormac reply mid-sentence: `"...what brings ye to this side"`. The full reply was longer (line 119); display buffer cap, not model output.
- **Fix**: Either raise the recent-events char cap or append `…` so the LLM sees the truncation.

### 8. `parish/.demo-run.log` not gitignored — RESOLVED 2026-06-04 (commit 5d7a935c)

- **Symptom**: Demo output file showed up in `git status`.
- **Fix**: Add `parish/.demo-run.log` to `.gitignore`.

### 9. Server save-state disconnect

- **Symptom**: Standalone `parish-server` started after a demo returns `{"filename":null,"branch_id":null,"branch_name":null}` for `/api/save-state` — demo session not visible.
- **Fix**: Either auto-load most-recent branch on server start (no-arg case) or document that demo + server are separate sessions.

## Cycle 2 — additional findings (`just demo 2 8` with `--mcp-port 3030`)

### 10. NPC dialogue degenerates into repetition loop (P0) — RESOLVED 2026-06-04 (commit 6736eb6a)

- **Symptom**: Brendan Duffy reply ≈1900 chars; Nora Duffy replies ≈1900 chars. Both repeat phrases verbatim 3-4×: `"Sometimes it turns slow, sometimes it turns fast. But it always turns, so it does"`, `"'Tis not just a matter of X, but Y, so it is"`. Cut off only by token cap.
- **Root cause**: Likely (a) no `repetition_penalty` on dialogue sampling, (b) `max_tokens` set too high so cap is the only brake, (c) prompt encourages length over closure. Qwen2.5-14B-4bit is prone to this without rep penalty.
- **Fix**: Set `repetition_penalty` ≥ 1.1 in vllm-mlx call for dialogue category; cap `max_tokens` to 200-300 in `parish-inference` dialogue request; consider stop tokens at `'\n\n'` or 3rd `'Tis`.

### 11. NPC hallucinates absent characters (P1)

- **Symptom**: Brendan addressed `"Nora"` mid-reply while Nora not yet present (`NPCs here: none` after Brendan + Cormac departed). Nora later replied referencing `"Father Declan"` (not in NPC catalog, never appears). Player LLM then mirrored Nora's bait → `"Good mornin' to ye. Might I speak with ye a moment, Nora?"` before Nora arrived.
- **Fix**: Dialogue prompt should list **only** present NPCs and explicitly forbid invoking names not in that list. Add post-validation: if NPC reply mentions a name not in `{present NPCs ∪ recently-mentioned NPCs}`, regenerate or strip.

### 12. Player stranded with empty location for 4 turns (P0)

- **Symptom**: After Brendan + Cormac departed, `NPCs here: none` for 4 consecutive turns. LLM-as-player continued speaking aloud: `"I'll wait here by the mill, maybe see more of this stream as ye mentioned"`, `"Nay, I'll not be goin' just yet"`, etc. No movement, no system hint that nobody hears.
- **Fix**: When `NPCs here: none`, demo-turn prompt should explicitly tell the auto-player to consider moving. Tie to fix #1 (movement schema). Also surface "no one is here to listen" hint.

### 13. NPC greets with wrong time-of-day (P1)

- **Symptom**: Nora at Dusk replied `"Ah, good mornin' to ye"`. Mirrored player's earlier `"Good mornin'"` (which was already wrong — game was at Dusk hour 17).
- **Root cause**: Time label visible to LLM is only `Dusk`; that doesn't strongly cue "evening" register. LLMs default to "good morning" greetings without context push.
- **Fix**: Strengthen time signal in dialogue prompt with explicit greeting cue (`Greet appropriate for: {time_label}, hour {hour}`). Same root as TODO #5.

### 14. NPC mid-reply farewells to multiple parties (P1)

- **Symptom**: Brendan's reply mixed greetings + monologue + multiple goodbyes within one message: `"Slán abhaile, Father, I'll be back soon..."` + `"Slán leat, stranger..."` + back to chatting about the wheel. NPC stage-direction leakage.
- **Fix**: Constrain dialogue to single addressee per turn. Use `addressed_to` field on NPC replies. Reject replies with multiple `Slán*` tokens.

### 15. Redundant Weather field in prompt (P3) — RESOLVED 2026-06-04 (see demo.rs:181)

- **Symptom**: Prompt repeats weather twice — embedded in `location_description` (`"The weather is partly cloudy"`) and again on `Weather: Partly Cloudy` line. Same for time-of-day (`"It is afternoon"` + `Date and time: ... Afternoon`).
- **Fix**: Drop weather sentence from location description, OR drop the explicit `Weather:` line. Keep one source of truth.

### 16. `/api/debug-snapshot` returns 404 from MCP bridge (P3)

- **Symptom**: Bridge on :3030 (when launched via `parish-tauri --mcp-port`) does not expose `/api/debug-snapshot` (`parish-server` does). Returns `HTTP/1.1 404 Not Found, content-length: 0`.
- **Fix**: Either document the route subset the bridge exposes (in `parish-mcp/README.md`), or proxy `debug-snapshot` through the bridge.

### 17. Server `/api/save-state` is NOT broken (revoke TODO #9)

- **Update**: When MCP bridge runs inside the demo process, `/api/save-state` correctly returns `{"filename":"sessions.db","branch_id":1,"branch_name":"main"}`. Earlier "all-null" was because I'd spawned a standalone `parish-server` that started a brand-new session with no save loaded — server-only init path appears not to auto-load the most-recent branch. Lower severity but still worth fixing for the headless server case.
- **Restated fix**: `parish-server` boot should auto-load the most-recent main branch from `sessions.db` (matching tauri behavior). Saves debugging confusion.

## Cycle 3 — additional findings

### 18. Auto-player emits empty action and burns a turn (P1) — RESOLVED 2026-06-04 (commands.rs:2848)

- **Symptom**: Last 2 turns of cycle 3 logged `raw_len=137 action=` and `raw_len=139 action=` (137/139 chars came back from LLM, but parsed `action` was empty). Player input recorded as nothing, no NPC turn fired.
- **Root cause**: Action parser in `parish_tauri_lib::commands::demo_turn` likely strips/loses content when LLM completion doesn't match expected `<text>"}` shape, or model returned `{"action": ""}` and the code accepts empty without retry.
- **Fix**: On empty action, log the raw LLM completion at WARN level and either retry (with `temperature` bump) or skip turn explicitly. Add a unit test feeding common malformed completions.

### 19. Pause/unpause spam is fresh-session-only (revise TODO #6)

- **Update**: Cycle 1 logged 38 `clocks stand still` / `Time stirs` toggles. Cycles 2 + 3 (loaded-save sessions) logged 0. Spam happens only on fresh game boot; loaded saves don't trigger it.
- **Restated fix**: Trace which subsystem emits the pause edges on initial bootstrap (likely setup status / initial NPC schedule / first weather tick), and dedupe at the emission source.

### 20. Mood→emoji map inconsistent across cycles (revise TODO #3) — RESOLVED 2026-06-04 (commit 5cafc389)

- **Update**: Same NPC + same mood label returned different emojis across cycles. `friendly` → 😊 (cycle 1) vs 🤗 (cycle 3) for Brendan / Nora. Either reaction inference overrides the static mood→emoji map per turn, or two code paths return different values.
- **Restated fix**: Confirm single source-of-truth for `mood_emoji`. If reaction inference is doing the override, document it; otherwise pin to a stable table.

## Cycle 4 — additional findings (12-turn run)

### 21. NPCs mis-identify their location (`Curraghboy` for `Kilteevan`) (P1) — RESOLVED 2026-06-04 (commit d89ae98a)

- **Symptom**: Nora and Cormac repeatedly refer to "their" village as `Curraghboy`: `"...for bein' here in Curraghboy on this fine evening?"`, `"...plans in mind for Curraghboy"`. Eight separate Cormac/Nora replies use it.
- **Verified data**: `Curraghboy Village` IS a real location in `mods/rundale/world.json:649` (separate from `Kilteevan Village`). NPC backstories in `mods/rundale/npcs.json` reference Curraghboy:
  - Cormac/Brendan Duffy: `"Overheard his father talking about buying a second mill near Curraghboy"` (line 2715)
  - Kathleen Mahoney grew up in Curraghboy (line 2921)
    So Curraghboy is canon — but the Duffy mill is at _The Mill near Kilteevan_, not Curraghboy. NPC pulled the nearby-townland name from backstory and applied it to their current location.
- **Fix**: Dialogue prompt for an NPC reply must inject `"You are currently at {location_name}, in {parent_settlement}"` as a hard anchor. Post-filter: if NPC names a location that exists in world.json but isn't the current location, warn.

### 22. Gaelic validator over-flags real Irish words (`poitín`) (P2) — RESOLVED 2026-06-04 (commit 803e7e63)

- **Symptom**: `WARN parish_core::game_loop::npc_turn: quality issue in NPC reply ... kind="hallucinated-gaelic" detail=unrecognized Gaelic word: 'poitín'` (line 127). Poitín (a.k.a. "poteen") is a real Irish word for home-distilled spirits — well documented, period-appropriate for 1820.
- **Root cause**: Gaelic word list in the validator (likely `parish-npc::anachronism` or `mods/rundale/anachronisms.json`) doesn't include `poitín`.
- **Fix**: Add `poitín` to the allow-list. Audit other false positives by sampling a session's `quality issue` WARNs.

### 23. NPC reply repetition loops worsen with conversation length (revise TODO #10) — RESOLVED 2026-06-04 (commit 6736eb6a)

- **Symptom**: Cormac's late-cycle replies (lines 472, 544, 614) escalate to ≈1500-2000 chars with `"what brings ye here this eve? Is it just the mill, or is there somethin' else ye're after?"` repeated 4-5 times verbatim. Pattern present in earlier cycles but worse as context grows.
- **Likely amplifier**: As the recent-events buffer accumulates, the model echoes its own prior question structure more. Repetition penalty in sampling would help.
- **Fix**: As TODO #10 (`repetition_penalty` + tighter `max_tokens`). Add eval: replies > 600 chars OR containing 3+ identical sentences should be auto-truncated and re-sampled.

### 24. NPC self-introduces questions before the asked-to NPC arrives (confirms TODO #11)

- **Symptom**: Turn 7 player input: `"I thank ye, Nora. Might I ask, Cormac, if there be any work to be done here..."` — but `NPCs here:` at that prompt was Nora only. Cormac arrived NEXT turn (per schedule). Player addressing absent NPC, then NPC magically shows up to answer.
- **Confirms**: TODO #11 root cause — auto-player names not-present NPCs. Need to constrain `addressed_to` to current NPCs list, OR teach the player to use only-present names.

### 25. NPC time-of-day descriptor matches but greeting doesn't (LOW-MED)

- **Symptom**: Cormac line 351: `"this eve, m'lad"` — correct (Night). But Nora line 161 still says `"Now, Aiden, do ye have any other questions on yer mind?"` with no eve/dusk cue. Mixed. Not as severe as cycle 2/3 "good mornin'" at dusk.
- **Note**: Improvement vs cycles 2/3, suggesting time cue lands when reinforced repeatedly across turns.

### 26. Player invokes catch-phrases ("Just sayin', mind ye") from NPCs (LOW)

- **Symptom**: Cormac uses `"Just askin', mind ye"` 4-5 times across replies. Player has not yet, but this style mimicry from prior session-state risk exists when the LLM-as-player consumes the recent-events buffer.

## Cycle 5 — additional findings (12-turn run, family arrived)

### 27. Off-screen NPC Tier 2 inference dies on JSON parse (P1) — RESOLVED 2026-06-04 (commit f3f13d1f)

- **Symptom**: `ERROR parish_npc::ticks: Tier 2 inference failed at Murphy's Farm: inference error: Tier 2 JSON parse failed: key must be a string at line 2 column 3` (line 221).
- **Impact**: Off-screen NPC simulation at Murphy's Farm aborted silently — anyone there gets no Tier 2 update this tick. Visible only in logs, not surfaced in UI.
- **Likely cause**: 1.5B Qwen2.5 (intent/simulation tier) emitted unquoted keys or malformed JSON (e.g. `{thought: "..."}` instead of `{"thought": "..."}`). 1.5B model is weaker at strict JSON.
- **Fix**: Either (a) tighten Tier 2 prompt schema with JSON example + grammar-constrained sampling (jsonformer / outlines / vllm `guided_json`), (b) catch parse error and retry with explicit `Return strict JSON only` reminder, (c) upgrade Tier 2 to a stronger model. Add per-NPC tick error counter so failures don't stay invisible.

### 28. Time-of-day descriptor in location_description disagrees with time_label (P2)

- **Symptom**: At 20:30+ game time, `location_description` says `"It is evening. The weather is partly cloudy."` while `Date and time: ... Night | Spring`. Both shown in the same prompt block. Evening ≠ Night for the LLM.
- **Root cause**: `location_description`'s time-of-day phrase is recomputed but uses a different bucket boundary than `time_label`.
- **Fix**: Drive both from same source-of-truth. Pick one bucketing table.

### 29. Tier 2 errors not visible to user but visible to ERROR-grep (P3, observability) — RESOLVED 2026-06-04 (commit e07042b6)

- **Symptom**: Only error across 5 demo cycles was line 221's Tier 2 fail. No surface metric.
- **Fix**: Add a `parish_metrics` counter for `tier2_parse_failures_total{location=...}`. Surface in `/api/debug-snapshot`.

## Cycle 6 — additional findings

### 30. Auto-player CAN move — issue is prompt, not schema (revise TODO #1) — RESOLVED 2026-06-04 (commit f12c7c11)

- **Symptom**: Cycle 6 turn 8, LLM produced `action=go to the stream` (raw_len=30). System replied `"You set off along the track east along the stream back to the village toward Connolly's Shop. (15 minutes on foot)"` and player relocated to `Connolly's Shop`.
- **Revision**: Movement IS supported by natural-language `action` text. TODO #1's root-cause hypothesis was wrong — the action grammar accepts movement verbs and the input parser resolves them. The real bug is that the auto-player almost never produces movement-style actions across cycles 1-5. Out of 38+ turns, exactly 1 produced movement (and only after the LLM was implicitly nudged by Brendan saying "take a walk by the stream").
- **Fix**: Strengthen `mods/rundale/demo-prompt.txt` with explicit movement directive: `"After 3-5 turns at one location, choose to move. Use simple commands: 'go to X', 'walk to X', 'head to X'."`. Optionally inject a system hint `> [system] You have been here {n} turns. Consider moving.` when stuck.

### 31a. ROOT CAUSE: pause toggles are user-activity-driven (revise TODO #6, #19, #31) — RESOLVED 2026-06-04 (commit 19aeca82)

- **Confirmed source**: `parish/apps/ui/src/lib/auto-pause.ts`. Frontend timer dispatches `/pause` after `idleMs` of no keyboard/mouse/touch activity, then `/resume` on next event. Each dispatch lands as `Command::Pause`/`Command::Resume` in `parish-core/src/ipc/commands.rs:282-289` which print the system messages.
- **Why the spam is bursty during demo**: User interacting with other apps (not the demo window) is interpreted by `auto-pause.ts` as idle/active flips. Each blur of attention → idle timer fires → `/pause`. Each return → `/resume`. Demo runs in a Tauri window receiving global mouse/keyboard via DOM, so window-out-of-focus doesn't shield it.
- **Why TODO #19 was wrong**: Cycle-1 fresh-session burst correlated with user activity at boot. Cycles 2/3 had near-zero toggles because user was relatively still. Cycle 6 (user actively using computer) brought the spam back. Behaviour is correlated with **user input cadence**, not session age.
- **Why duplicate `Time stirs again`**: `recordActivity` may fire twice within the timer-clear window before `pausedByAutoIdle` resets, OR `isWorldPaused()` returns stale value between dispatch + server-apply. Either way, the frontend lacks reentrancy guard.
- **Fix options**:
  1. Detect window focus state (`visibilitychange` / `document.hasFocus()`) and only auto-pause when the Tauri window is foregrounded. Demo runs autonomously — should suppress auto-pause when the demo loop is in flight.
  2. Suppress auto-pause entirely while `--demo` mode is active (the LLM is the player; user activity is irrelevant).
  3. Add reentrancy guard: ignore activity events when `submitInput('/pause')` is in-flight.
- **Lower-priority cleanup** (was TODO #31): also reference-count `Command::Pause` server-side so duplicate dispatches don't emit duplicate text messages.

### 31. Pause/unpause emits duplicates and back-to-back toggles (revise TODO #6, #19) — RESOLVED 2026-06-04 (commit 19aeca82)

- **Symptom** (`parish/.demo-run.log` lines 235-243):

  ```text
  > [system] The clocks of the parish stand still.
  > [system] Time stirs again in the parish.
  > [system] Time stirs again in the parish.      ← duplicate
  > [system] The clocks of the parish stand still.
  > [player] ...
  > [Brendan Duffy] ...
  > [system] Time stirs again in the parish.
  > [system] The clocks of the parish stand still.
  > [system] Time stirs again in the parish.
  ```

- **Confirms**: Spam happens on loaded saves too (TODO #19 was wrong). And two `stirs` fire back-to-back without an intervening `stand still` — the state transition isn't idempotent. Multiple subsystems must be hitting `pause()`/`resume()` independently.
- **Fix**: Reference-count the pause source (intent inference, dialogue inference, reaction inference) and only emit the system message on edges where the count crosses 0. Add a unit test feeding 3 nested pause/resume pairs.

### 32. Movement time accounting looks off (P2)

- **Symptom**: Pre-move snapshot was `Night 20:30+`. Post-move snapshot is `Midnight 01:29`. System message said `(15 minutes on foot)`. With `speed_factor=36×` that should be 9 game-hours; observed advance is ~5 hours. Or — if `speed_factor` doesn't apply during travel — should be 15 game-min; observed is ~300 min.
- **Possibilities**: (a) travel uses different time-advance than free turns, (b) intermediate transit through Kilteevan adds time, (c) bug.
- **Fix**: Confirm intended travel-time model; log per-turn `game_minutes_advanced` so the discrepancy is visible.

### 33. Map endpoint hides reachable-via-transit locations (P2)

- **Symptom**: Earlier `/api/map` listed 11 locations. `Connolly's Shop` was reachable via "go to the stream" but did not appear in the map response. Either the endpoint filters to `hops <= N` or to a specific subgraph.
- **Fix**: Either document the filter (`/api/map` returns only adjacent + 2-hop locations), or expand it. Auto-player can't choose to move to a location it can't see in its prompt — combined with TODO #30 this magnifies the no-movement problem.

### 34. Extreme repetition pattern: anaphora loop (refines TODO #10/#23) — RESOLVED 2026-06-04 (commit 6736eb6a)

- **Symptom**: Brendan's c6 reply (line 230) packs `"'Tis a place of steady X, but not without its Y"` 12+ times verbatim (steady hands / hearts / souls / feet, cycled twice). Same pattern line 234. This is a different failure mode than the question-loop seen earlier — it's pure anaphoric chain.
- **Note**: At least three distinct loop patterns observed across cycles: (i) trailing-question loop, (ii) "'Tis not just X, but Y", (iii) anaphora chain "'Tis a place of steady X, but not without its Y". All from same underlying lack of repetition penalty.

## Cycle 7 — additional findings (15-turn run, player roamed)

### 35. NPC cross-conversation name leak: addresses player by previous NPC's name (P0) — RESOLVED 2026-06-04 (commit d89ae98a)

- **Symptom**: Roisin Connolly (Shopkeeper at Connolly's Shop) addressed the player as `"Nora"` twice (lines 189, 274). Player's name is Aiden Carney. `Nora Duffy` is the Miller's Wife from the prior location's conversation, present in the recent-events buffer.
- **Root cause**: NPC dialogue prompt feeds the recent-events transcript including prior turns at other locations. The LLM-NPC at the new location picks up an earlier proper noun and uses it as the player's name. Names in dialogue history aren't grounded against "who is the player" anchor.
- **Fix**: Dialogue prompt must inject `"The player is {player_name}. Refer to them only by this name."` as a hard anchor at the top. Optionally trim recent-events to the current-location turns + last N global turns, OR redact other-NPC names when feeding cross-location history.

### 36. Adjacent-locations list grows / shrinks; Hedge School wasn't on earlier maps (refines TODO #33)

- **Observation**: `/api/map` earlier returned 11 locations. Player typed `go to The Hedge School` from The Crossroads and it worked, and `The Hedge School` appeared in subsequent prompts as an adjacent option. So adjacent list is location-relative (computed per player position) — not a global static.
- **Implication**: TODO #33's "map hides reachable locations" is not strictly a bug — it's by design. But the LLM's prompt does show `Adjacent locations:` and was historically capped to _visited_ neighbours; new locations only appear once reached. Demo-prompt should encourage discovery: list unvisited adjacent too.

### 37. Empty-location stranding repeats at Crossroads + Hedge School (confirms TODO #12)

- **Symptom**: Player moved to The Crossroads (line 322) → `NPCs here: none`. Stayed 1 turn, moved to The Hedge School (line 566) → `NPCs here: none`. Player talked to nobody for 4 consecutive turns: `"who be the schoolmaster hereabouts?"`, `"if anyone be livin' here, or if the place be abandoned?"`, `"if the place be abandoned?"`, `"are there any tales..."`. Self-conversation drift.
- **Confirms**: TODO #12 root cause. Same pattern as cycle 2's empty Mill window. No "nobody is here to listen" system hint, no auto-move.

### 38. Concannon reference VERIFIED real (resolved — not a bug)

- **Symptom**: Roisin mentioned `"the landlord's man, Concannon"` (line 152).
- **Verified**: `Martin Concannon` exists in `mods/rundale/npcs.json:4002` (landlord's clerk). Backstory references in other NPCs' entries (lines 2104, 3054, 3656). Roisin's mention is in-canon, not a hallucination.

### 39. NPC dialogue → speaker self-introduction redundancy (P3) — RESOLVED 2026-06-04 (commit 3773669a)

- **Symptom**: Roisin reply line 274 includes `"...ye share yer plans with me, Roisin Connolly, of Connolly's Shop, and a keen eye for opportunity?"`. Mid-reply self-introduction ("Roisin Connolly, of Connolly's Shop") is breaking immersion; NPC already introduced.
- **Fix**: Post-filter NPC replies to remove `<own_name>, of <own_location>` patterns when `introduced=true`.

### 40. NPC reply rate at single-NPC location only 27% (4/15 turns) (P2)

- **Symptom**: 15 player turns at Connolly's Shop → only 4 Roisin replies. 11 player turns went unanswered (most of them when player had moved to empty locations).
- **At Connolly's Shop specifically (turns 1-7)**: 4 replies for 7 turns. Then player moved at turn 8.
- **Likely**: dialogue inference queue or cooldown skips NPC replies when player monologues without addressing. Investigate `npc_turn` decision criteria for "should the NPC reply this turn".

## Cycle 8 — additional findings (18-turn run, mostly empty locations)

### 41. Movement parser silently rejects valid intent phrasings (P1) — RESOLVED 2026-06-04 (commit 0b247306)

- **Symptom**: At The Hurling Green, player produced 9 consecutive turns of `"I'll make for the Crossroads then."`, `"I'll be making for the Hedge School then."`, `"I best be on me way then."` — **none triggered a move**. No system response, no error. Player stayed stuck. Turn 12 finally moved with `"Seems a quiet spot for a wander. I'll make for the Hedge School then."` — the only successful one had a leading descriptive clause before the move verb.
- **Root cause**: Movement parser in `parish-input` likely matches strict prefixes (`go to X`, `walk to X`) and rejects `"I'll make for X then"` or `"I best be on me way"`. Parser doesn't even surface "I don't understand" — input is treated as dialogue at a location with no NPCs, so it silently no-ops.
- **Fix**: (a) Extend movement parser with patterns: `(I'll|I will|I best|I'll be) (make|making|be makin') for ...`, `...on me way to ...`, `head to ...`, `make my way to ...`. (b) When no NPC is present AND input is unparseable as dialogue, surface a system hint instead of silently dropping.

### 42. Movement parser handles "on my way" with graceful error (good) (verified working)

- **Symptom**: Player line 126 `"Seems a lonely place at this hour. Might I be on me way now?"` produced system reply: `"You haven't the faintest notion how to reach 'on my way'. You can go to: The Hedge School (4 min on foot), The Crossroads (4 min on foot)"`.
- **Verdict**: Good. Parser tried to resolve "on my way" as a location, failed, surfaced reachable options. Player ignored the hint and kept malforming — that's the auto-player's fault.

### 43. System ambient event during travel (informational)

- **Symptom**: During Hurling Green → Hedge School transit, system line 695: `"A lone figure trudges along the road in the early morning grey, bundle on their back."`. Atmospheric event, fired alongside the arrival description. Working as intended.

### 44. Time of day transition Night → Dawn worked visibly (good)

- **Symptom**: First half of c8 was `"It is late night"`. Post-move at line 696: `"It is dawn"`. So the `location_description`'s embedded time-of-day phrase IS recomputed on each prompt (refutes earlier hypothesis that it might be cached). The buggy disagreement in TODO #28 is specifically at the dusk/night boundary, not all transitions.

### 45. NPC streaming reveal runs in parallel — turns not serialized (P0, user-reported in c6) — RESOLVED 2026-06-04 (commit 296c783d)

- **User observation**: During cycle 6, two NPC replies were visibly being revealed (streamed token-by-token) at the same time. A previous long reply was still pumping out characters when the next demo-player command + NPC reply landed.
- **Term**: "streaming reveal" / "token streaming" / "stream pump". UI implementation at `parish/apps/ui/src/lib/setup/stream-manager.ts`; pacing at `parish/apps/ui/src/lib/stream-pacing.ts` (120/90/190 ms base/clause/sentence).
- **Why it happens**:
  - `pendingNpcTurns: Map<number, PendingNpcTurn>` (line 74) holds multiple concurrent streams keyed by `turnId`. The data structure intentionally supports >1 in flight.
  - Backend `parish-core/src/game_loop/npc_turn.rs:183` spawns NPC replies with `tokio::spawn`. Multi-NPC turns (e.g. cycle 5 line 350-351: Nora replied, then Cormac replied right after the same player input) start as parallel inference calls and stream as parallel reveals.
  - Pacing math: a 600-char reply at ~120 ms/word ≈ 12 s reveal time. Inference also takes ~10-15 s per NPC. So NPC #2's first tokens land before NPC #1's pump drains.
  - Each `PendingNpcTurn` owns its own `pumpHandle` setTimeout chain — no cross-turn serialization on the UI.
- **Fix options** (pick one):
  1. **UI serialization**: `pumpTurn` should only run for the oldest `PendingNpcTurn`. New entries queue but defer their `pumpHandle` until the prior `finalizePendingTurn` fires. Smallest change.
  2. **Backend serialization**: in `handle_npc_conversation`, await stream completion (`stream_complete` signal back from UI) before spawning the next NPC's inference. Cleaner but adds round-trips.
  3. **Batched emit**: collect all NPC replies for a single player turn server-side, then emit them ordered with explicit `prior_turn_complete` gating fields. Hybrid.
- **Related**: comment at stream-manager.ts:79 references issue #991 (`handle_npc_conversation cancels and re-spawns the loading animation per addressed NPC turn`) — that fix addressed loading animation flicker but didn't add reveal serialization.

## Cycle 9 — additional findings (18 sterile turns at empty Hedge School)

### 46. Silent input drop: parser swallows ALL turns at empty location (P0, sharpens #12/#41) — RESOLVED 2026-06-04 (commit 0b247306 + e1d31c14)

- **Symptom**: 18/18 c9 turns at The Hedge School. Zero NPC presence. Zero system responses. Zero state changes. Player produced reasonable text every turn including:
  - Movement attempts: `"I'll venture to the nearby Crossroads"`, `"I'll make my way back to the Crossroads, seein' if there's aught happenin' there"` (5 turns)
  - Roleplay actions: `"Walking up to the cabin, I knock gently on the door"`, `"Sittin' here, I notice a book half-open on the table. Might I take a glance at it?"`, `"I'll take a seat on the bench"` (8 turns)
  - Dialogue at nobody: `"Might I inquire if this be the place where young ones learn their lessons?"` (3 turns)
- **Result**: All 18 inputs vanish into a void. No "you are alone" hint, no "I don't understand", no movement trigger, no idle banter. Player loop is unrecoverable without external intervention.
- **Fix combines #12 + #41**: (a) When `NPCs here: none`, surface system response for unparseable input — at minimum echo back `"You speak, but no one is here to hear."`. (b) Expand movement parser to catch `I'll venture to X`, `I'll make my way to X`, `head/walk/wander/stroll/go to X`. (c) Roleplay-action recognition would be a larger gameplay feature — at minimum, treat unparsed input at an empty location as a movement-intent check.

### 47. Roleplay narration treated as dialogue, not action (P1, design gap) — RESOLVED 2026-06-04 (commit 206854f1)

- **Symptom**: Player narratives in past-tense or third-person ("Walking up to the cabin, I knock gently on the door", "Sittin' here, I notice a book half-open on the table") have no game-state effect. The auto-player produces this style ~40% of the time because the demo prompt allows it ("Use first person speech or direct commands"). The engine has no action-verb parser to extract `knock`, `sit`, `look`, `pick up`.
- **Fix**: Either (a) tighten demo-prompt to forbid narrative-action style, restrict to greetings + movement + dialogue, OR (b) add an action-verb parser layer (`knock`, `sit`, `wait`, `look around`) that fires synthetic system descriptions.

## Cycle 10 — additional findings (20-turn, 2 locations, Padraig met)

### 48. NPC reply quality varies dramatically across personas (P2, observation)

- **Symptom**: Padraig Darcy (Storyteller, Crossroads) produced 6 focused replies in c10, each 250-400 chars, no extreme repetition, with usable game info (`"the old mill to the west"`, `"Brendan"`, `"the path to the mill"`). Compare to Cormac/Nora/Brendan Duffy in c2-c6 producing 1500-2000-char looping replies.
- **Hypothesis**: Either (a) Padraig's persona description in `npcs.json` is shorter/cleaner, (b) Padraig has fewer "memories" loaded into prompt, (c) Storyteller occupation prompt template differs. Investigating could reveal what makes good NPC dialogue.
- **Fix**: Audit Padraig's `npcs.json` entry vs the Duffys'. Identify what's different. Backport pattern to other NPCs.

### 49. Padraig dialogue provides navigation hints (good) (feature working)

- **Symptom**: Padraig spontaneously gave directions: `"Keep to the road west, and take care to mind yer step where the brambles be thick"`, `"follow the road east, 'til ye come to the old stone cross"`. These hints could help the auto-player discover unvisited locations.
- **Verdict**: Working as intended; possibly enabled by Padraig's Storyteller occupation. Other NPCs should be prompted to do this too.

### 50. NPC references in-canon mythology (sídhe) — verified working (resolved)

- **Symptom**: Padraig mentioned `"the old sídhe, where the fae folk be said to dance"`.
- **Verified**: `mods/rundale/world.json:326` has a `mythological_significance` field describing a rath that is "home to the sídhe". In-canon. Not a hallucination.

## Cycle 11 — additional findings (15-turn, Padraig + Concannon)

### 51. LLM-as-player roleplays as NPC instead of player (P1)

- **Symptom**: Line 130 player input: `"Good mornin', stranger. Any particular reason for askin' about that road?"`. The LLM-as-player has flipped roles — replying to itself as if it were Padraig answering Aiden. Player is supposed to BE Aiden Carney, the wandering stranger; here it is speaking AS the local responding to a stranger.
- **Likely cause**: When the previous player turn ends without an NPC reply yet visible (inference still in flight), the auto-player's prompt context shows only "[player] Good mornin'. Might I ask where the road..." with no NPC line. The LLM continues the dialogue naturally — and picks the NPC role.
- **Fix**: (a) Demo-turn prompt should always pin `"You are Aiden Carney. Speak ONLY in Aiden's voice — never as an NPC."`. (b) When a previous player turn is awaiting NPC response, demo loop should wait for the reply before issuing the next prompt (relates to #45 serialization).

### 52. Padraig + Concannon both present in-canon (resolved — not bugs)

- Padraig Darcy at The Crossroads (Storyteller).
- Martin Concannon at The Letter Office (landlord's clerk). First in-game appearance after Roisin mentioned him in c7.
- Concannon reply mentioned `"the calf being born at the Murphys' farm"` — Murphy's Farm is real (in c2 map).

### 53. Movement parse: even "Off I go to X then" worked once but failed on identical phrasings later (refines #41) — RESOLVED 2026-06-04 (commit 01abc444)

- **Symptom**: At line 580: `"Off I go to the Letter Office then..."` → movement succeeded. Same player produced 5+ similar variants in earlier turns (`"Might I venture to the Letter Office next"`, `"I shall go"`, `"Mayhaps the Letter Office doth hold tales..."`) — none triggered movement.
- **Parser pattern guess**: "Off I go" + "to" + LOCATION works. "Might I venture" + "to" doesn't. Parser keys on assertive verb forms ("go", "walk", "head"), rejects modals + questions. Worth verifying in `parish-input/src/parser.rs`.

## Cycle 12 — additional findings (15 turns, 3 locations, 2 new WARN categories)

### 54. Tier 3 batch inference cancelled mid-stream (P2) — RESOLVED 2026-06-04 (commit 7cb52090)

- **Symptom**: `WARN parish_npc::ticks: Tier 3 batch inference failed: inference error: Tier 3 cancelled mid-stream` (line 123).
- **Likely cause**: Tier 3 (low-fidelity off-screen NPC simulation) was running when player input arrived. Cancellation handling in `parish-core/src/inference_guard.rs` / sim_cancel preempts in-flight sim batches per cycle 7 commit note (`#9` comment in commands.rs:703). But the cancellation surfaces as a WARN — could be downgraded to INFO since it's intentional.
- **Fix**: Distinguish "preempted by player input" (expected, INFO) from "failed for other reason" (WARN). Or suppress when `sim_cancel` was triggered intentionally.

### 55. NEW validator: modern-register anachronism flag (P2) — RESOLVED 2026-06-04 (commit 0a8e15b2)

- **Symptom**: `WARN parish_core::game_loop::npc_turn: quality issue in NPC reply ... kind="modern-register" detail=modern-register phrase: 'taking in the sights'` (line 275). Concannon NPC reply used "taking in the sights" — flagged as anachronistic modern English.
- **Note**: Echoed phrase — player used `"taking in the sights"` in c11 line 777 (`"I've come from over by the Shannon, just takin' in the sights and sounds"`). NPC mirrored player wording. So the NPC isn't generating modern register independently; it's echoing the LLM-as-player's modern register. The validator is doing its job but flags downstream from the real source (player prompt).
- **Fix**: Two angles. (a) Validate LLM-as-player output too, not just NPC replies — surface anachronisms earlier. (b) Add the player's last N inputs to the dialogue prompt's "do not echo this style" guidance.

### 56. NPC reply rate at populated locations remains low (refines #40)

- **Symptom c12**: 15 player turns, only 6 NPC replies (40%). Padraig answered 1/10 turns at Crossroads (10% — very poor). Concannon answered 5/? turns at Letter Office.
- **Note**: NPC reply rate seems sensitive to NPC persona. Padraig in c10 replied 6× → here 1×. Same NPC, very different behaviour. Possibly because c12 had a different conversational opening.

## Convergence note

12 cycles, **56 distinct issue categories** (some refined/revoked across cycles). Last 3 cycles added 1, 0, 3 truly new categories. New-find rate is not yet zero — the engine has enough surface area (input parsing, NPC inference, schedules, validators, streaming, time, mood) that each new run lights up edge cases. A reasonable cutoff: stop active cycling but treat the TODO as a living doc.

Highest-impact items by cycle (top 10 to fix first):

1. **#1/#30/#41/#46** — auto-player movement (silent input drops at empty locations)
2. **#10/#23/#34** — NPC repetition loops (no `repetition_penalty`)
3. **#11/#24/#35** — NPC name hallucinations + cross-conversation name leak
4. **#45** — streaming reveal parallelism (user-reported)
5. **#12** — empty-location stranding
6. **#21** — NPC mis-identifies its location (Curraghboy vs Kilteevan)
7. **#5/#13/#28** — time-of-day cue weak in prompt
8. **#27/#54** — tier 2/3 inference parse/cancel surfacing as ERROR/WARN
9. **#3/#20** — mood→emoji map inconsistent
10. **#31a** — frontend auto-pause spam driven by user computer activity

After cycle 4 (12 turns, fresh-ish save) the only truly new categories surfaced were #21 (wrong village name) and #22 (Gaelic validator false positive). Cycles 5+ would likely keep surfacing variations of #1 (no movement), #10/#23 (repetition), #11/#24 (hallucinated NPCs), and #5/#13 (time cue). **Snapshot considered complete at cycle 4.**

## Verified working

- No tauri panic on launch (prior branch fix holds).
- Mod loaded as Rundale via LocalDiskModSource.
- 19 provider mods registered.
- Weather drift Clear → Partly Cloudy fired mid-run.
- Reaction emojis fired per NPC per turn.
- vllm-mlx auto-discovered on :8000 (14B) and :8001 (1.5B).
- Inference + chat transcript JSONLs written under `~/Library/Application Support/Rundale/saves/inference_logs/`.
- Stale session lock auto-cleaned on startup.

## Audit 2026-06-04

Static code + git-history pass against main branch. No game process run. Verdict key: **fixed** = confirmed by code + commit; **partial** = partially addressed; **still-open** = defect still plausibly present in code.

| Cluster                                                    | Findings | Verdict        | Key commit(s)                                                                                                                                         | GH issue        |
| ---------------------------------------------------------- | -------- | -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| Auto-player movement (#1/#30/#41/#46/#53)                  | 5        | **fixed**      | f12c7c11 (demo-prompt), 0b247306 + 01abc444 (intent_local.rs first-person + modal patterns), e1d31c14 (empty-location directive)                      | none found open |
| NPC repetition (#10/#23/#34)                               | 3        | **fixed**      | 6736eb6a (`frequency_penalty=0.5` for Tier 1 dialogue, `max_tokens=512`)                                                                              | none found      |
| NPC name hallucination / cross-location leak (#11/#24/#35) | 3        | **fixed**      | d89ae98a (`location_anchor_block` + `interlocutor_block` injected per turn in ticks.rs)                                                               | #1027 (CLOSED)  |
| Streaming reveal parallel (#45)                            | 1        | **fixed**      | 296c783d (UI serializes NPC stream reveals; only oldest `PendingNpcTurn` pumps at a time)                                                             | none found      |
| Empty-location stranding (#12/#46)                         | 2        | **fixed**      | e1d31c14 (demo turn directs auto-player to move when `NPCs here: none`) + 0b247306                                                                    | none found      |
| Time-of-day cue (#5/#13/#28)                               | 3        | **fixed**      | 2a1f133e / commands.rs:2401 (HH:MM added to game_time format) + 803e7e63 (Night bucket aligned)                                                       | none found      |
| Tier 2/3 surfacing (#27/#29/#54)                           | 3        | **fixed**      | f3f13d1f (Tier 2 retry on parse fail), e07042b6 (tier2_parse_failures_total metric), 7cb52090 (Tier 3 cancel downgraded to DEBUG)                     | none found      |
| Mood emoji (#3/#20)                                        | 2        | **fixed**      | 5cafc389 (`bitter`→😒 `sharp`→😤; single `mood_emoji()` function is the sole source of truth; reaction emoji path is per-message not mood-display)    | none found      |
| Auto-pause spam (#6/#19/#31/#31a)                          | 4        | **fixed**      | 19aeca82 (window-focus guard skips /pause when Tauri window not focused); edge-gating in time.rs prevents duplicate system messages                   | none found      |
| MCP port in demo (#2)                                      | 1        | **fixed**      | 5d7a935c (`--mcp-port $MCP_PORT` added to demo recipe)                                                                                                | none found      |
| gitignore demo log (#8)                                    | 1        | **fixed**      | 5d7a935c (`parish/.demo-run.log` in .gitignore)                                                                                                       | none found      |
| NPC location mis-ID (#21)                                  | 1        | **fixed**      | d89ae98a (`location_anchor_block` hard-anchors `WHERE YOU ARE RIGHT NOW` with directive wording for exactly the Curraghboy-vs-Kilteevan case)         | none found      |
| Gaelic validator false positive (#22)                      | 1        | **fixed**      | 803e7e63 (`poitín` added to allow-list in quality.rs)                                                                                                 | none found      |
| Redundant weather field (#15)                              | 1        | **fixed**      | demo.rs:181 (standalone `Weather:` line removed; comment explains residual inline signal)                                                             | none found      |
| NPC truncation (#7)                                        | 1        | **fixed**      | b8629534 (recent-events cap raised; `…` suffix on truncation)                                                                                         | none found      |
| Empty action retry (#18)                                   | 1        | **fixed**      | commands.rs:2848 (bounded single retry at temperature 1.0 with WARN log)                                                                              | none found      |
| NPC self-intro redundancy (#39)                            | 1        | **fixed**      | 3773669a (`introduced-anchor` suppresses mid-reply `Name, of Location` when `introduced=true`)                                                        | none found      |
| Modern-register echo (#55)                                 | 1        | **fixed**      | 0a8e15b2 (player register alert injected into NPC context to prevent echo)                                                                            | none found      |
| Roleplay narration style (#47)                             | 1        | **fixed**      | 206854f1 (demo-prompt forbids narrative-action style, restricts to first-person speech + movement)                                                    | none found      |
| NPC reply rate (#40/#56)                                   | 2        | **still-open** | No code found that addresses when a single NPC skips replies for majority of turns                                                                    | none found      |
| NPC farewell mid-conv (#4/#14)                             | 2        | **fixed**      | tier1_system.txt:28 (`NEVER FAREWELL MID-CONVERSATION` directive added); single-addressee constraint (03074a0a)                                       | none found      |
| Server save-state (#9/#17)                                 | 2        | **partial**    | #17 revoked (MCP bridge path works); headless server auto-load still not confirmed fixed                                                              | none found      |
| Map endpoint filter (#33/#36)                              | 2        | **partial**    | #36 revises #33 (adjacent list is per-position by design); unvisited neighbours still not shown until reached                                         | none found      |
| Travel time accounting (#32)                               | 1        | **still-open** | No specific commit fixing the 15-min vs ~5-hr discrepancy found                                                                                       | none found      |
| NPC quality variance (#48)                                 | 1        | **still-open** | No commit auditing Padraig vs Duffy persona quality difference                                                                                        | none found      |
| LLM-as-player role flip (#51)                              | 1        | **partial**    | 259bfad6 (demo-prompt now says "first-person speech only"), but no serialization of demo-loop to wait for NPC reply before issuing next player prompt | none found      |
| Movement travel time listing (#33)                         | 1        | **partial**    | Adjacent list now shows unvisited as `— unvisited` (no travel_minutes until visited); core design gap remains                                         | none found      |

**GH issue coverage**: Only two findings had matching GH issues (#1027 for name hallucination — CLOSED; #1175 "convert recurring findings into rubrics" — OPEN). The majority of clusters have no tracking issue on `dmooney/rundale`. Issue #1175 is the closest to an umbrella tracker.

**Overall verdict**: 37 of 56 findings are confirmed fixed by concrete code evidence. The remaining open/partial items (#40/#56 NPC reply rate, #32 travel time math, #33/#36 map filter design, #48 NPC quality variance, #51 player role-flip serialization, #9 headless server auto-load) have no matching commit and are candidates for new issues.

## Issue tracking

2026-06-04 audit: surviving demo-audit findings tracked under epic #1207 (Rundale gameplay quality).
