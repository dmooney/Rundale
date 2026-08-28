---
name: parish-engine
description: Drive the Rundale engine to see gameplay actually working — the script harness, feature proofs, autonomous play-tests, eval rubrics/baselines, the live LLM auto-player, browser UI sessions, and GUI screenshots. Use after changing world, movement, NPC, input, inference, UI, or mod code, and whenever you need to prove a gameplay change is live rather than that tests pass.
disable-model-invocation: false
argument-hint: '[harness|prove|play|rubric|demo|browser|screenshot] [scenario or feature]'
---

One skill for every way of running the engine to observe behaviour. Pick the section that matches the
job; they share a build step (`cargo build` first) and the same `--script` JSON harness underneath.

| You want to…                                                   | Section                      |
| -------------------------------------------------------------- | ---------------------------- |
| Run fixture scripts and check JSON output                      | **Script harness**           |
| Prove a specific feature is live at runtime                    | **Prove a feature**          |
| Autonomously explore / stress the game                         | **Exploratory play-test**    |
| Get a deterministic, no-LLM regression sensor                  | **Eval rubrics & baselines** |
| See a feature in the live Tauri app (dialogue, streaming, IPC) | **Auto-player / demo**       |
| Click through the web UI in a real browser                     | **Browser UI session**       |
| Regenerate GUI screenshots                                     | **GUI screenshots**          |

`/prove`, `/play`, and `/rubric` form a ladder: rubric (deterministic, machine-checked) → prove (you read
the JSON) → play (autonomous exploration). Demo and browser sessions cover paths the headless harness can't
reach (Tauri IPC, streaming, the rendered UI). None of them replace `/check`.

---

## Script harness

Run fixture scripts through `--script` mode, which emits structured JSON per command.

1. **Build first**: `cargo build`.
2. **Run the script**:
   - Specific fixture: `cargo run -- --script <script-file>`
   - Default walkthrough: `cargo run -- --script testing/fixtures/test_walkthrough.txt`
3. **Inspect the JSON line by line.** Check that:
   - Movement results have valid `to` locations and reasonable `minutes`
   - Look results contain non-empty descriptions
   - System commands return expected, non-empty responses
   - No unexpected errors, panics, or `"result": "unknown_input"`
4. **Run additional fixtures** if the default passed:
   - `cargo run -- --script testing/fixtures/test_movement_errors.txt`
   - `cargo run -- --script testing/fixtures/test_commands.txt`
   - `cargo run -- --script testing/fixtures/test_speed.txt`
5. **Report** which scripts passed and flag any anomalies.

Script command reference:

- Movement: `go to <location>`, `walk to <location>`
- Look: `look`, `look around`
- System: `/status`, `/time`, `/map`, `/npcs`, `/wait [N]`, `/tick`
- Place attention: `/listen` (ambient soundscape), `/omen` (cautious sign, never prophecy),
  `/folklore` (exact authored tradition)
- Persistence: `/save`, `/fork <name>`, `/load <name>`, `/branches`, `/log`
- Speed: `/speed fast`, `/speed normal`, `/speed slow`
- Control: `/pause`, `/resume`, `/new`
- Debug: `/debug`, `/debug npcs`, `/debug clock`, `/debug here`, `/debug schedule`

Location names come from `/map` or `mods/rundale/world.json`.

---

## Prove a feature

Prove a gameplay feature works at runtime — not just that tests pass.

1. **Write a targeted script** at `testing/proofs/play_prove.txt` that exercises the feature from a
   player's perspective. Use `/wait` to advance time, move between locations, and use `/time`, `/status`,
   `/debug clock`, `/debug npcs`, `look`, `/npcs` to make the feature's impact visible.
2. **Run it**: `cargo run -- --script testing/proofs/play_prove.txt`
3. **Read the JSON critically.** For each line ask:
   - Do values change when expected (weather transitions, NPC relocations)?
   - Do descriptions read naturally — grammatical and immersive to a player?
   - Does NPC behaviour respond correctly to the new feature?
   - Are any fields empty, nonsensical, or stuck at their initial value?
   - For place-attention work, do `/listen`, `/omen`, and `/folklore` produce three distinct
     player-visible results at the same frozen scene? Does a natural discussion of those topics add
     its atmospheric cue while the underlying conversational turn still completes?
4. **Fix what you find.** Common issues:
   - New tick/update logic added to `parish-server` and `headless` but **not** the test harness
     (`crates/parish-cli/src/testing.rs`) — the harness has its own game loop in `advance_time()` and
     `Command::Tick`.
   - Large `/wait` jumps that call your logic once at the final timestamp instead of each intermediate step.
   - Template interpolation producing ungrammatical text when a new enum variant has a multi-word Display.
   - Features that silently no-op because a required field isn't wired up in a constructor.
5. **Re-run until the output proves the feature is live.** Don't stop at "tests pass" — stop at "I can see
   it working in the game output."
6. **Report** what you tested, what the output showed, and any fixes made.

**Think like the player.** Would someone who doesn't know the code understand what's happening? Would a game
creator accept this output quality? If not, the feature isn't done.

---

## Exploratory play-test

Autonomous play-test to evaluate the gameplay experience.

1. **Build first**: `cargo build`.
2. **Determine what to test**:
   - If given a `.txt` path, use it as the script directly.
   - If given a scenario ("explore all locations", "talk to every NPC", "test time passage"), generate a
     script at `testing/proofs/play_session.txt`.
   - If nothing is given, generate a comprehensive exploration script that checks `/status`, `/time`,
     `/map`, `/npcs`; visits every reachable location; uses `/wait` and `/tick` to advance time and observe
     schedule changes; tests `/save`, `/fork`, `/branches`, `/log`; and toggles `/speed fast` / `/speed normal`.
3. **Run it**: `cargo run -- --script <script-file>`.
4. **Analyze the JSON** per the same checks as the Script-harness section (movement, system, map, NPCs,
   time, wait, errors).
5. **Report** a play-test summary: locations visited and whether descriptions generated, NPCs encountered
   and where, time/season progression, anomalies/bugs/missing features, and an overall assessment.

---

## Eval rubrics & baselines

The **deterministic** half of the harness: snapshot baselines + structural rubrics, no human reading and no
LLM judge required.

What this checks:

1. **Snapshot baselines.** Each fixture in `BASELINED_FIXTURES` (`crates/parish-cli/tests/eval_baselines.rs`)
   is run through `run_script_captured`, serialized to JSON, and diffed against
   `testing/evals/baselines/<fixture>.json`. Any drift fails with a "live | baseline" diff window.
2. **Structural rubrics** asserted on every baselined fixture:
   - `rubric_anachronisms_are_empty` — no NpcResponse surfaces anachronistic terms.
   - `rubric_movement_minutes_are_positive` — no Moved with `minutes == 0` (catches a frozen clock).
   - `rubric_look_descriptions_are_non_empty` — no Looked with empty description (catches a silent renderer failure).

Steps:

1. Run the suite: `cargo test -p parish --test eval_baselines`.
2. **Baseline test fails** — read the "live | baseline" diff:
   - Unintentional drift (a regression): fix the code, rerun.
   - Intentional drift (you changed gameplay deliberately): confirm the new output is correct, run
     `just baselines` to regenerate, and review `git diff testing/evals/baselines/`.
3. **Rubric test fails** — the panic names the fixture, step, and canonical fix. Fix the code, rerun.

Adding a fixture to the baseline set:

1. Confirm its output is deterministic — run it twice and `diff` the JSON. Differences in `new_log_lines`
   are fine (not part of `ScriptResult`); differences elsewhere are not.
2. Add the stem to `BASELINED_FIXTURES` and a matching `#[test] fn baseline_<fixture>()`.
3. Run `just baselines` to capture the initial JSON.
4. Commit the test entry alongside its baseline.

Use this after editing `parish-world`, `parish-npc`, `parish-cli/src/testing.rs`, or `mods/rundale/`;
before opening/updating a gameplay PR; or as a faster, lower-noise alternative to **Prove a feature** when
the change is structural.

---

## Auto-player / demo

The LLM auto-player drives the **live Tauri app** — use it for paths the headless harness can't exercise:
Tauri IPC, NPC dialogue quality, frontend streaming behaviour.

```sh
just demo 2 5    # 5 turns, 2s pause — fast smoke test
just demo 4 20   # 20 turns, 4s pause — content generation / sustained observation
just demo 3      # unlimited turns
```

Capture logs to read the transcript:

```sh
DEMO_LOG=$(mktemp) && just demo 2 5 > "$DEMO_LOG" 2>&1
grep -E "chat \[|demo turn|WARN" "$DEMO_LOG"
```

`chat [player]` / `chat [npc]` lines show the conversation; `demo turn: LLM chose action` shows the
auto-player's pick each turn.

Verify:

- Player actions are single-line natural language — no reasoning preamble or JSON artifacts.
- NPC dialogue is Irish-authentic and responds to what the player actually said.
- `demo turn` fires each turn — if absent, the LLM call is hanging or failing.
- No `waitForFalse timed out` warnings — streaming completed cleanly.
- The clock advances between turns — game is not paused.

Bugs demo surfaces that the headless prove misses: streaming freezes (input stays disabled, 30s timeout
fires), thinking blocks leaking into player actions, NPC saying nothing (429 rate limit or JSON field-name
typo), game clock paused throughout. Demo is **observational** — it surfaces live behaviour, it does not
assert. It does not replace `just check`, the prove flow, or Playwright.

`just demo` opens the MCP bridge on `--mcp-port 3030`, so while it runs (or after launching the Tauri app
yourself with `cargo run -p parish-tauri -- --mcp-port 3030`) you can drive and inspect the live game with
`mcp__parish__*`: `parish_world_snapshot` / `parish_npcs_here` / `parish_submit_input` to probe, and
**`mcp__parish__parish_file_bug`** to file any bug as a GitHub issue that auto-bundles a screenshot (a native
window capture — the minimap renders, #1160), recent logs, and game state. Dedup against open issues first
(`gh issue list --search`); set `PARISH_BUG_REPORT_DRY_RUN=1` to write the report to disk instead of filing
while testing. For the full bug-hunting loop see `/demo-audit`.

---

## Browser UI session

Interactive Chrome test against the web server via browser MCP tools. Follow the plan in
`docs/plans/archive/chrome-test-plan.md`.

Setup:

1. **Build frontend**: `cd apps/ui && npm run build`.
2. **Check server**: `curl -s -o /dev/null -w "%{http_code}" http://localhost:3001/`.
3. **Start if needed**: `cargo run -- --web 3001` (background; wait for 200 on health check).
4. **Connect browser tooling** via the available browser MCP tools. If no extension is connected, ask the
   user to enable it and retry.
5. **Navigate** a tab to `http://127.0.0.1:3001`.

Test execution — take screenshots at key points, track pass/fail:

- **Required:** Page Load (status bar, map, NPCs sidebar, chat panel, input render); Navigation (travel to
  ≥2 locations, verify map/status/NPCs update); Edge Cases (invalid location, already-here, empty submit);
  System Commands (`/help`, `/status`, `/pause`, `/resume`); Console Check (read console for errors at start
  and end).
- **If LLM provider configured in .env:** NPC Conversation (streaming response); Irish Words (Focail panel
  populates); Idle Message (atmospheric message at empty location).
- **If explicitly requested:** Debug Panel tabs; Speed Commands; Theme Updates over time.

Reporting: print a pass/fail table; list bugs with repro steps; report console errors; check server logs.
If bugs are found, file them with **`mcp__parish__parish_file_bug`** (auto-bundles screenshot + logs +
state) — dedup against open issues first (`gh issue list --search`). Write results to
`docs/reviews/chrome-testing-session.md` (append a dated section if it exists).

---

## GUI screenshots

Regenerate Rundale GUI screenshots after UI changes via headless Playwright — no X11/GDK required.

1. **Install deps** (if needed): `cd apps/ui && npm install`.
2. **Capture**: `cd apps/ui && npx playwright test e2e/screenshots.spec.ts` — captures the GUI at 4 times of
   day (morning, midday, dusk, night) on headless Chromium with mocked Tauri IPC.
3. **Verify output**: check `apps/ui/e2e/screenshots/baseline/` for updated PNGs; list files and sizes.
4. **Report** which baselines changed; remind the user to commit them alongside the UI change.

Notes:

- To update visual-regression baselines: `npx playwright test e2e/screenshots.spec.ts --update-snapshots`.
- `docs/screenshots/` contains separately curated, referenced documentation images; do not copy every
  Playwright baseline there.
- Full E2E suite: `npx playwright test` or `just ui-e2e`.
