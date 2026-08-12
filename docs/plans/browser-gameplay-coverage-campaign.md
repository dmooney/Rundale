# Browser Gameplay Coverage Campaign

Status: active reusable runbook  
Primary surface: Rundale web UI in the Codex in-app browser  
Stopping rule: coverage complete, not a fixed turn count  
Expected size: 85–110 gameplay turns plus UI-only interactions

## Purpose and boundaries

This campaign turns every player-facing claim in the root README feature list into a live
browser scenario with a visual or authoritative oracle. Use one coherent character history,
temporary comparison branches, and one calendar branch. A hard failure gates only the
affected run; persist it, start a recovery run from the last valid checkpoint, and continue
the unfinished coverage ledger.

Inventory, currency, meals, and lodging are conversational role-play only. Do not require
canonical state for them. Durable assigned work is explicitly advertised as a mechanic and
must enter and advance the authoritative task ledger.

This is a browser campaign, not the strict Tauri-only quality-harness lane. Tauri windowing,
native menus, keychain, F2 capture, and the embedded desktop bridge require a separate native
pass. Also excluded: testing all provider integrations, cloud authentication, deployment and
metrics, CLI/headless/client modes, mod-authoring utilities, Parish Designer editing,
developer tooling, and architecture fitness claims. Rundale currently configures walking
only, so horse/cart comparison is not applicable; walking travel and road encounters remain
covered. Mythology hooks are prompt context only, matching the README's reserved-for-future
wording.

## Evidence contract

- Drive the shipping web UI with real local inference.
- Validate through same-origin browser requests so the oracle shares the page's `parish_sid`.
  Use `/api/engine-state`, `/api/debug-snapshot`, `/api/save-state`, and the page's WebSocket
  stream; do not compare against a separate MCP session.
- After every gameplay input, save a distinct browser screenshot, visible transcript lines,
  authoritative state delta, and relevant events. Preserve full prompt/response records for
  every dialogue turn before shutting down.
- Attribute every visible line to player speech, player command, system/time narration, NPC
  dialogue, or autonomous world event before scoring it.
- File every distinct game/UI defect from the live browser session with exact evidence and a
  stable signature. Do not file browser-controller, host-display, or harness failures as game
  bugs.
- Allowed ledger statuses are exactly: `pending`, `pass`, `fail`, `not observed`, and
  `not applicable`.
- Stochastic behavior gets the bounded attempts stated below. Exhaustion becomes
  `not observed`, not an automatic bug.
- Ingest every completed or gated segment into the quality dashboard and record its run ID.

## Campaign acts

### Act 1 — Browser, UI, and input baseline (about 10 gameplay turns)

1. Start a fresh named character, explicitly pause time, and capture the initial desktop UI
   plus same-session engine state.
2. Exercise Look, Examine, Move, Talk, and Interact with natural free text.
3. Use `@mention` in a crowded location; only the named NPC may answer.
4. Exercise slash/location/NPC completion, command history, multiline input, and map
   quick-travel.
5. Capture one dialogue mid-stream and after completion. Require incremental rendering,
   sticky bottom-follow, and no candidate JSON or metadata leakage.
6. Obtain an emote and an Irish phrase. Verify italic emote rendering and Focail
   name/pronunciation accumulation. Allow three dialogue attempts for each.
7. React to a message and retain that reaction for the persistence act.
8. Open and close Map, Save/Load, Debug, Mod, Help, and shortcuts surfaces. Focus must return
   to the initiating control.
9. Verify approved scene, NPC, and map images are loaded, nonblank, and responsive.
10. Repeat the essential send/stream/scroll/Map/People & Words flow at a mobile viewport.

### Act 2 — Geography, travel, map, clock, and weather (about 20 turns)

1. Move using a deliberately misspelled destination to prove fuzzy resolution.
2. Visit Hodson Bay (real), Kilteevan Village (manual), and The Forge
   (fictional/relative). Require spatially coherent map positions and travel.
3. Traverse one edge five times and reopen the map after trips two and five. Require the
   traversal count and worn-path presentation to increase.
4. Travel on at least ten eligible edges across morning, afternoon, dusk, and night. Record
   time, prose narration, and any encounter. No encounter after ten trips is `not observed`.
5. Click a destination on the map and capture intermediate and final pan/zoom states.
6. Switch historic and OSM maps without losing player or location state.
7. Compare `/speed slow` and `/speed ludicrous` over equal five-second intervals, then restore
   normal speed and pause.
8. Reach every time phase and verify status, scene prose, NPC schedule, and engine state:
   Dawn 05:00, Morning 07:00, Midday 12:00, Afternoon 14:00, Dusk 17:00, Night 19:00, and
   Midnight 23:00.
9. From one saved origin, create `weather-clear`, `weather-heavy-rain`, `weather-fog`, and
   `weather-storm` branches. Compare the same route in clear/heavy rain/fog; attempt a
   flood/exposed route during storm and require rejection or alternate routing.
10. In heavy rain, wait 30 game-minutes and require exposed NPCs to seek plausible shelter.
11. Observe natural weather for six successive two-hour advances. Transitions must be
    adjacent and dwell at least two game-hours.
12. Visit the Fairy Fort or Holy Well and ask a grounded folklore question. Require authored
    significance in context without inventing a new supernatural mechanic.

### Act 3 — NPC cognition, dialogue, memory, and gossip (about 30 turns)

1. Ask the same practical and abstract questions of contrasting intelligence profiles.
   Compare vocabulary, reasoning, empathy, and creativity against authored traits.
2. Build at least 21 meaningful memories with one NPC around a distinctive topic, leave long
   enough for tier deflation, return, and ask for recall. Verify short-term rollover,
   long-term promotion, and persistence in Debug.
3. Tell one NPC a distinctive harmless fact, then follow it through at least three social
   hops. Record exact and distorted retellings.
4. Leave a populated location, advance time, and inspect events for Tier-2/Tier-3 off-screen
   activity, mood/relationship changes, and later gossip.
5. Observe Tier-4 events over the calendar tour. None after the full observation window is
   `not observed` unless logs prove the subsystem did not execute.
6. Try up to five conversations around strongly related co-located NPCs. If a follow-on chain
   occurs, it must stop at three exchanges.
7. Recheck one NPC's location/activity at every seasonal checkpoint.
8. Submit one known anachronism and require period-appropriate confusion.
9. Submit one prompt-injection attempt and one fabricated person/place premise. Require no
   instruction leak, raw JSON, false confirmation, or unauthorized state effect.
10. Inspect inference records for intent/dialogue/simulation routing, model, latency, bounded
    history, errors, and structured-output validation.
11. Start interactive dialogue while background simulation is active. Interactive work must
    not wait behind lower-priority simulation.

### Act 4 — Work, role-play follow-through, and persistence (about 15 turns)

1. Ask a grounded employer for work and explicitly accept one concrete task.
2. Require the task in `active_tasks` and in visible player status independent of the input.
3. Perform the matching physical action; require assigned → in-progress state and semantic
   events.
4. Wait several hours, report completion, and ask the employer to inspect it. Inspection is
   conversational role-play but must acknowledge established work and must not invent
   inventory or payment.
5. Save manually, react to the employer's message, and fork `work-done`.
6. Diverge `work-done`, load `main`, and verify original location, task, conversation, and
   reaction. Reload `work-done` and verify its divergent state.
7. Open the F5 save picker; its DAG, names, and current branch must match save state.
8. Keep the session active beyond 45 seconds and require autosave without dialogue/streaming
   interruption.
9. Restart the web server and revisit with the same browser cookie. Session, branch, task,
   reaction, and journal-derived state must recover.

### Act 5 — Calendar, seasons, festivals, and long-range simulation (about 15 turns)

Create `calendar-tour` from the saved campaign. Advance whole days in this exact sequence:

| From | To | `/wait` minutes | Expected |
| --- | --- | ---: | --- |
| 20 March 1820 | 1 May 1820 | 60480 | Spring, Bealtaine |
| 1 May 1820 | 1 June 1820 | 44640 | Summer |
| 1 June 1820 | 1 August 1820 | 87840 | Summer, Lughnasa |
| 1 August 1820 | 1 September 1820 | 44640 | Autumn |
| 1 September 1820 | 1 November 1820 | 87840 | Autumn, Samhain |
| 1 November 1820 | 1 December 1820 | 43200 | Winter |
| 1 December 1820 | 1 February 1821 | 89280 | Winter, Imbolc |

At every checkpoint run `/time`, inspect status and same-session engine state, speak to a
scheduled NPC, compare activity/location to the previous season, and record weather,
Tier-3/Tier-4, relationship, story, and gossip events. The NPC must ground the current
season/festival and reject a different festival as currently active. Time-box each bulk
advance at 90 seconds; a timeout gates that run and recovery continues from the last valid
checkpoint.

### Act 6 — Web sessions, themes, accessibility, and finish (about 10 turns)

1. Open a second isolated browser context, use a different player name/location, and prove
   transcript, task, save, and world state do not cross sessions.
2. Reload/reconnect the first context and verify retained transcript and WebSocket continuity.
3. Observe WebSocket messages for streaming tokens, world updates, theme changes, and map
   switches.
4. Test default, Solarized Light, and Solarized Dark; reload after each and require persisted
   selection with no wrong-theme flash.
5. Exercise keyboard-only Tab, Enter, Escape, `M`, `?`, and Save/Load controls.
6. Check semantic roles, accessible names, visible focus, modal focus trap/restoration, and
   WCAG-AA computed contrast on desktop and mobile.
7. Inspect all eight Debug tabs and compare displayed values to same-session debug state.
8. Use the UI bug reporter for the first genuine finding, attaching the exact record. If no
   finding exists, exercise it in dry-run mode rather than filing a fake issue.

## README coverage ledger

Update rows immediately after each scenario. Never reconstruct them from memory at the end.

| Feature | Scenario | Oracle | Run/turn | Status | Issue | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| Fuzzy location graph | Misspelled movement | Correct unique destination and state change | — | pending | — | — |
| Prose edges and worn paths | Five repeated traversals | Narration plus increased count/edge weight | — | pending | — | — |
| Hybrid geography | Real/manual/fictional visits | Coherent coordinates and travel | — | pending | — | — |
| Game clock and speed | Seven phases; slow/ludicrous comparison | Status/state phase and relative rate | — | pending | — | — |
| Four seasons | Calendar checkpoints | Engine season plus schedule differences | — | pending | — | — |
| Weather state machine | Six two-hour advances | Adjacent transitions and minimum dwell | — | pending | — | — |
| Weather-gated travel | Clear/rain/fog/storm branches | Time penalties and route block/alternate | — | pending | — | — |
| Travel and encounters | Ten varied journeys | Time/prose; encounter or bounded exhaustion | — | pending | — | — |
| Festivals | Four exact festival dates | Status, hooks, relationship/event evidence | — | pending | — | — |
| Mythology hooks | Fairy Fort/Holy Well dialogue | Authored prompt context only | — | pending | — | — |
| Tier 1 dialogue | Direct NPC conversations | Full grounded streamed response | — | pending | — | — |
| Tier 2 simulation | Nearby wait | Event/mood/relationship delta | — | pending | — | — |
| Tier 3 simulation | Distant long wait | Batch/off-screen event evidence | — | pending | — | — |
| Tier 4 simulation | Calendar tour | Life events or bounded not-observed result | — | pending | — | — |
| NPC memory | 21-turn topic and return | Promotion and recall after deflation | — | pending | — | — |
| Gossip network | Three social hops | Propagation and optional distortion | — | pending | — | — |
| Intelligence profiles | Matched questions | Authored differences visible in replies | — | pending | — | — |
| Seasonal schedules | Same NPC at checkpoints | Canonical activity/location changes | — | pending | — | — |
| Autonomous NPC chains | Five bounded attempts | Follow-on chain no longer than three | — | pending | — | — |
| Off-screen social simulation | Leave, wait, return | Persisted events and surfaced gossip | — | pending | — | — |
| Anachronism filter | Known modern term | Authentic confusion, no false acceptance | — | pending | — | — |
| Per-category inference | Debug during live turns | Correct route/model/category | — | pending | — | — |
| Priority queue | Dialogue during simulation | Interactive response not blocked | — | pending | — | — |
| Token streaming | Mid/final captures | Incremental bounded rendering | — | pending | — | — |
| Structured output validation | Debug plus visible reply | No raw metadata; valid effects only | — | pending | — | — |
| Inference timeout/logging | Debug inspection | Bounded records with latency/error fields | — | pending | — | — |
| Prompt-injection defence | Injection and fabrication probes | No leak, false claim, or unauthorized effect | — | pending | — | — |
| Five free-text intents | Natural inputs | Correct presentation and state behavior | — | pending | — | — |
| `@mention` targeting | Crowded conversation | Only selected NPC responds | — | pending | — | — |
| Slash-command surface | Representative commands | Correct command presentation/effect | — | pending | — | — |
| Chat-first play surface | Desktop/mobile use | Transcript/input/context remain usable | — | pending | — | — |
| Responsive illustrated context | Desktop/mobile images | Loaded, nonblank, responsive assets | — | pending | — | — |
| Coordinated secondary surfaces | Open/close each surface | Correct destination and focus restoration | — | pending | — | — |
| Emote rendering | NPC action/emote | Italic inline presentation | — | pending | — | — |
| Message reactions | React, save, reload | Reaction persists | — | pending | — | — |
| Enriched input | Completion/history/multiline/travel | Each input behavior works | — | pending | — | — |
| Focail panel | Irish phrase/name | Word, name, pronunciation accumulate | — | pending | — | — |
| Durable assigned work | Assign, act, wait, inspect | Task state/events persist; inspection coherent | — | pending | — | — |
| Manual save and autosave | Save plus >45 seconds | Save identity/timestamp advances | — | pending | — | — |
| Branching and journal recovery | Fork/load/restart | Exact divergent state recovers | — | pending | — | — |
| Save picker DAG | F5 after branching | UI graph matches authoritative branches | — | pending | — | — |
| Map overlay and sources | M, map card, source switch | Correct markers/edges/source | — | pending | — | — |
| Animated travel | Click-to-travel | Intermediate and final framing | — | pending | — | — |
| Status/context chrome | All phases/weather/festival | Legible and state-consistent | — | pending | — | — |
| Themes | Three themes plus reload | Correct palette and persistence | — | pending | — | — |
| Debug records | Eight tabs | Values match same-session debug state | — | pending | — | — |
| Bug reporter | First real finding or dry-run | Exact record, screenshot, state, diagnostic | — | pending | — | — |
| Keyboard shortcuts | Keyboard-only pass | Correct action, focus, and dismissal | — | pending | — | — |
| Accessibility | Desktop/mobile audit | Roles, names, focus, contrast | — | pending | — | — |
| Web session isolation | Two browser contexts | No state crosses cookies | — | pending | — | — |
| WebSocket events | Stream/world/theme/map actions | Expected event classes observed | — | pending | — | — |
| Restart persistence | Server restart, same cookie | Session/save state resumes | — | pending | — | — |
| Horse/cart travel comparison | Active Rundale transport config | Rundale ships walking only | — | not applicable | — | `mods/rundale/transport.toml` |

## Run history

Append one row per completed or gated segment and keep unfinished coverage explicit.

| Date | Commit | Browser/server configuration | Dashboard run IDs | Findings | Unfinished coverage |
| --- | --- | --- | --- | --- | --- |
| — | — | — | — | — | All rows initially pending |

## Completion checklist

- [ ] Every ledger row is non-`pending`.
- [ ] Every `fail` row links a filed issue.
- [ ] Every `not observed`/`not applicable` row states why.
- [ ] All gameplay turns have distinct screenshots and transcript/state evidence.
- [ ] Every dialogue turn has a complete raw prompt/response artifact.
- [ ] Every completed or gated segment is ingested and listed in run history.
- [ ] Final report lists passes, issues, gates/recoveries, stochastic non-observations,
      exclusions, dashboard run IDs, and this runbook path.
