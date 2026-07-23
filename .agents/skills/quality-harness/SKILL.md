---
name: quality-harness
description: Run a game quality-control playtest — YOU drive the LIVE Rundale game via the parish MCP against real models, play in-character for N turns, observe the world, then judge it CRITICALLY (anchored rubric, discrete findings) and file bugs. Trigger when the user says "run the quality harness", "do a harness run", "playtest the game", "QA the game", "drive a playtest", or similar. NOT for model benchmarking (that is /rundale-bench) and NOT for scripted bug-probing (that is /demo-audit-mcp).
argument-hint: '[turns N] [persona "..."] [goal "..."]'
---

# quality-harness — agent-driven critical playtest

**You are the harness.** You drive the live game through the parish MCP, play it like a real
player, observe the world (dialogue **and** the async simulation), then judge it as a
**hard-to-please human playtester** and file every defect. No standalone binary, no `/api`, no
headless, no reading a database. Full background:
[`docs/agent/driving-the-game-via-mcp.md`](../../../docs/agent/driving-the-game-via-mcp.md).

Args (all optional): `turns N` (default 12), `persona "..."`, `goal "..."`.

## Hard rules (non-negotiable)

- **Tauri + parish MCP only.** Drive via `mcp__parish__*`. Never the headless `parish-server`,
  never raw `/api/*` curl, never read a SQLite DB. The cloud env runs the desktop app.
- **Real models.** Dialogue is real (vllm-mlx Qwen). Do not "simulate" a run.
- **Control time explicitly** (see below). Never rely on window focus.
- **Judge critically.** Default skeptical. See the rubric — inflated scores are a failure of
  the harness.
- **Judge what the player SEES, not just the API.** Empty `exchanges` proves the engine
  produced no NPC reply — it does **not** prove the turn was rendered correctly. After every
  non-dialogue input (look / examine / move / system command), screenshot and confirm the UI
  renders it in a distinct command/narration style with **no** "You" speech bubble and **no**
  NPC speaker chip. A command drawn as dialogue, or pinned to an NPC who never replied, is a
  real defect that the API transcript hides — `parish_turn` / `get_transcript` will look clean.
- **Attribute every line to its source before scoring.** Tag each transcript/screen line as one
  of: player speech, player command, **system/time narration**, NPC dialogue, or autonomous
  world event. The time levers emit narration — `/resume` → "Time stirs again in the parish",
  `/wait N` → "You wait for N minutes… It is now HH:MM", `/pause` → "The clocks of the parish
  stand still" — that is **your** scaffolding, not world life. Never credit it to
  `world_responsiveness` or to an NPC.

## 1. Preflight (do this first, every time)

1. Confirm the MCP tools exist: call `mcp__parish__parish_engine_state`.
   - **If `mcp__parish__*` is unavailable**, the parish MCP server did not register. It only
     spawns at **session init** and there is **no in-session reload**. Tell the user to **start a
     fresh session** (Tauri must be running first); pre-build with
     `cargo build -p parish-mcp`. Do NOT fall back to headless/`/api`. (See #1352.)
2. Confirm the game is up: `parish_engine_state` returns a scene. If it errors with a transport
   error, the Tauri app isn't running — launch it with
   `bash parish/scripts/launch-tauri-screenshottable.sh 3030` (it builds the current worktree UI
   and launches Tauri against static frontend assets, so no Vite lifecycle can leave the window
   blank). Plain `cargo run -p parish-tauri -- --mcp-port 3030` uses `devUrl` and is not a
   graphical-harness runtime. For SCREENSHOTS specifically: use the helper above, and
   note the in-app fix wakes a **slept** display before capture — a screen that idled off reports
   as locked and used to fast-fail; it now wakes + holds the display (`caffeinate -u -d`). The
   launch helper **additionally** holds a `caffeinate -d -i -s` assertion bound to the app's
   lifetime, so the display never sleeps/locks mid-run and per-turn captures don't degrade to
   placeholders. §7's close releases it.
3. Disable focus-auto-pause so window/focus events can't toggle game time during the run
   (once #1357 lands): the harness owns pause state. Until then, just always set `/pause`
   explicitly each loop and never foreground the window except to screenshot (then restore).

## 2. Set up the run

- `mcp__parish__parish_new_game` for a clean transcript.
- `parish_submit_input("/pause")` — freeze the world so it can't drift while you think.
- Read the opening: `parish_engine_state` + `parish_world_snapshot` (scene description) +
  `parish_npcs_here`.
- Adopt the persona/goal (default persona: a curious newcomer to the parish, 1820; default
  goal: meet villagers and find your feet). Play in-character — a real player, not a command
  fuzzer.

## 3. Turn loop (repeat for `turns`)

For each turn:

1. **OBSERVE** — `parish_engine_state` (+ `parish_world_snapshot` / `parish_npcs_here` as
   needed). Once #1356 lands, use `tauri_invoke("get_turn")` (or `parish_turn`) for the slim
   bundle (last exchanges + world events + state) — **do not** pull `get_debug_snapshot`
   per turn (≈357 KB).
2. **ACT** — choose one meaningful in-character input; `parish_submit_input(text, addressed_to:
[npc])` for dialogue. Vary: greet, ask, move (`go to X`), act, use a slash command.
3. **RESULT** — read the NPC reply. Once #1356 lands, `submit_input` returns the exchange
   directly; until then read `tauri_invoke("get_transcript")` (NOTE: it is **location-scoped** —
   it clears on movement; full history is `get_debug_snapshot.conversations`).
4. **ADVANCE THE WORLD** (so autonomous life happens): `/resume` → `/wait N` → `/pause`, then
   re-read state. NPCs arrive/leave, gossip spreads, weather/mood shift — capture these deltas.
   The transcript will NOT show them; engine_state + events will.
5. **SCREENSHOT (every turn)** — after the reply has rendered and the log has autoscrolled to
   the bottom (sticky-bottom #1529 lands new dialogue at the fold), call `parish_take_screenshot`
   and save the returned PNG straight to **this turn's** `turns/NNN/frame.png`. One real,
   distinct capture per turn — do **not** reuse a prior turn's frame. (A single capture fanned
   across the run is the "every screenshot looks the same" bug — proven in the artifacts: runs
   had 1 distinct frame across all 25 turns even when the capture itself succeeded.) Capture
   **without foregrounding** the window — foregrounding toggles game time (#1277). The launch
   helper holds the display awake (`caffeinate`, §1.2) so the backgrounded capture path stays
   alive. If a capture still fails for a turn, write the shared placeholder for that ONE turn and
   **note in the run log that turn N is a placeholder** — never present a placeholder as real
   (rule #18). If captures fail every turn, raise the window once, capture, then **restore
   `/pause`** (foregrounding can resume the clock pre-#1357) and record it.
6. **RECORD** — note input, reply, state delta, and any defect you'd flag as a player.

## 4. Judge — be a HARD critic

Score the whole session on 7 axes, **0–100**, using these anchors. **Default skeptical; require
evidence to go high; never round up.** A category that fails consistently caps its axis at 60.

- **90–100** — indistinguishable from a skilled human author; zero immersion breaks.
- **75–89** — solid; only minor, isolated blemishes.
- **60–74** — flaws a real player notices (mood/voice mismatch, mild incoherence, verbosity).
- **40–59** — repeated or serious flaws; immersion regularly broken.
- **< 40** — broken.

Axes (weights for the quality mean in parens): `narrative_coherence` (1.5),
`character_fidelity` (1.5), `world_responsiveness` (1.0), `intent_fidelity` (0.75),
`immersion` (1.0), `progression` (1.0), `common_sense` (0.75).

**Itemize EVERY defect as a discrete finding** — do not summarize them away. Each finding:
`{category, turn, severity (low|med|high|critical), description, evidence (exact quote),
signature}`. Examples of things a critical playtester MUST flag (not an exhaustive list):

- NPC **mood tag not reflected** in dialogue tone (e.g. a `sharp`/`bitter` NPC speaking warmly)
  — this is a character-fidelity **failure**, not a nitpick; cap `character_fidelity` ≤ 60 if it
  happens across NPCs.
- **Unfounded familiarity** — an NPC implying prior knowledge of a stranger.
- **Command treated as dialogue** / intent misfire (#1351). Two distinct flavours, both
  fileable: (a) the engine generates an NPC reply to a non-dialogue input; (b) the engine makes
  no reply (`exchanges` empty) but the **UI still renders the command as a "You" speech bubble
  and/or attaches an NPC speaker chip** — a presentation-layer misroute you can only see in the
  screenshot. Always screenshot a look/examine/system turn and check its rendering; do not pass
  it just because the API transcript shows no NPC line.
- **Source/attribution slip in your own judging** — scoring system/time narration
  (`/resume` / `/wait` / `/pause` lines) as world life, or crediting a line to the wrong
  speaker. A finding built on a misattributed line is a false finding; verify the line's origin
  first.
- **Small-model verbosity** — rambling, repetition, multiple questions crammed in one reply.
- Any anachronism, contradiction, retcon, teleport, or scaffolding/JSON leak.
- A turn where the world did **not** respond when it should have.

Compute the weighted-mean quality. **Gate** the run (quality = N/A) on any hard fail: a crash
(MCP transport error mid-run), a parser reject, a turn timeout, or the player stuck with no
state change for several turns.

## 5. Output + file bugs

Produce: per-turn log, the 7 axis scores + rationale, the weighted quality (or GATED + reason),
and the full findings list. Then **file every finding** via
`mcp__parish__parish_file_bug(title, description, context)` — it bundles a screenshot + logs +
state into a GitHub issue labeled for the `/backlog` drain and **returns the issue URL**.
**Record that URL against the finding's `signature`** — §6 step 2 writes it into the payload so
the dashboard links the finding to its issue.

**File all of them, not just the headline ones.** The rule is dedup, not triage: collapse only
genuine duplicates (the same defect seen twice). **Do not skip a finding because it is
low-severity** — a `low` is still a real defect and still gets an issue; severity sets priority,
not whether it is filed. Recurring model-quality findings (mood-blind dialogue, verbosity) are
real bugs too. The only findings that may go unfiled are ones folded into another issue as an
exact duplicate; say so explicitly in the output ("folded into #NNNN"). Every finding you carry
into the ingest payload should have an `issue_url` unless it is such a dup — a payload finding
with no `issue_url` and no dup note is a filing miss.

## 6. Persist to the dashboard

A skill run is invisible to the `parish-harness` dashboard unless you ingest it. Do this at the
end of every run so it shows on `serve` (`http://localhost:8787`) next to binary runs.

1. **Lay out an artifact dir.** Pick a `uuid` for the run and create
   `<root>/runs/<uuid>/turns/NNN/frame.png` for each turn, where `<root>` is the same
   `--artifacts` dir the dashboard serves (default: next to `harness.db`). Each turn's
   `frame.png` is **that turn's own** capture from §3 step 5 — do not fan one screenshot across
   turns (every-frame-identical is the bug this fixes). Sanity-check before ingest:
   `find runs/<uuid>/turns -name frame.png -print0 | xargs -0 md5 -q | sort -u | wc -l` should be
   close to the turn count, not 1 (a few dupes are fine when the world genuinely didn't change;
   all-identical means the fan-out regressed). Use the shared placeholder **only** for a turn
   whose live capture failed, and only when you logged that fallback. Also write
   `turns/NNN/lines.json` (the turn's narrative lines, `[]` is fine). Every `frame.png` must be
   non-empty (the ingest validates this — rule #14).

   **Per-turn inference log (clickable on the run page) — MANDATORY for every dialogue turn.**
   `ingest` only rejects a _dangling_ `llm_transcript_path`; it does **not** reject a dialogue
   turn that omits one, so a logless run validates green and the dashboard renders blank,
   non-clickable turns. Do not rely on the validator — author the logs yourself. For **every
   turn that produced an NPC exchange** write `turns/NNN/llm.json` and reference it from that
   turn's payload as `"llm_transcript_path": "turns/NNN/llm.json"`. Capture the inference logs
   **live, per turn, before you close the app** (§7) — once Rundale quits, `get_debug_snapshot`
   is gone and the raw prompt/response is unrecoverable, leaving exchanges-only logs. Also
   populate `turns/NNN/lines.json` with the turn's narrative lines (an empty `[]` renders an
   empty, useless panel — fill it with the look/movement/system narration even when there is no
   dialogue). Omit `llm_transcript_path` **only** for turns with no NPC exchange (movement,
   `look`, a system command) — those stay non-clickable by design. The dashboard makes a turn
   with a log clickable, opening a panel that shows the dialogue exchange by default with a
   collapsible raw prompt/response section. Capture the raw model I/O from the Tauri black-box (the
   `get_debug_snapshot.conversations` history / the session `inference_logs/<ts>.jsonl` gen_ai
   spans) for the calls that fired during the turn. Schema:

   ```json
   { "turn_index": 0,
     "player_input": "…",
     "exchanges": [ { "speaker": "You" | "<npc-name>", "text": "…" } ],
     "inferences": [ { "category": "intent" | "dialogue" | "reaction" | "…",
                       "model": "mlx-community/Qwen2.5-14B-Instruct-4bit",
                       "prompt": "<full system+user prompt>", "response": "<raw completion>",
                       "latency_ms": 1234, "tokens": { "prompt": 0, "completion": 0 } } ] }
   ```

   A referenced `llm_transcript_path` **must** exist in the bundle or ingest rejects the run
   (no dangling reference). Omit the field for turns where you captured no inference (movement,
   `look`, a system command). Turns without a log render non-clickable.

2. **Emit the payload JSON** (schema in
   [`parish/crates/parish-harness/README.md`](../../../parish/crates/parish-harness/README.md)
   under `ingest`). Fill `git` from the worktree (`git rev-parse HEAD` / `--abbrev-ref HEAD` /
   `status --porcelain`), set `rubric_sha256` to the binary's pinned rubric sha
   (`cargo run -p parish-harness -- ...` records it; or read the rubric file hash), include all
   `turns`, the 7 `axes` with rationales, every `finding` (with the same `signature` you used
   when filing the issue **and its `issue_url`** — the URL `parish_file_bug` returned in §5, so
   ingest links the finding on the dashboard), and a `cost` tally. On a hard fail set `gate` and
   omit `quality_score`.

3. **Ingest, then backfill any missing issue links:**

   ```sh
   cargo run -p parish-harness -- ingest --payload <run.json> --artifacts <root>
   # safety net: link any finding whose issue_url wasn't set inline (e.g. a dedup against a
   # prior run's issue) by matching its signature to the filed issue body.
   cargo run -p parish-harness -- backfill-issues
   ```

   Ingest prints `ingested run <id>`. Surface that id and `http://localhost:8787` to the user so
   they can open the run on the dashboard.

## 7. Close Rundale (always, once the run is complete)

A run **owns** the Rundale desktop app it drove — shut it down once the run is finished and
persisted, so the bundled models and window are released. This is the final step, after the
ingest in §6; do it whether the run completed or hard-failed (a gated run still closes the app).
**Order matters:** ingest first, then close — quitting the app drops the MCP bridge on
`127.0.0.1:3030`, so no `mcp__parish__*` call will work afterward.

```sh
# Graceful quit of the packaged desktop app, then a fallback for the dev binary:
osascript -e 'quit app "Rundale"' 2>/dev/null || true
pkill -f 'parish-tauri' 2>/dev/null || true
# Release the display-awake hold from the launch helper. It self-exits when the app dies
# (`caffeinate -w "$APP_PID"`), but kill the pidfile too in case the bridge stayed up:
kill "$(cat "/tmp/parish-caffeinate-${USER:-shared}.pid" 2>/dev/null)" 2>/dev/null || true
```

Do **not** touch the dashboard `serve` process (port 8787) — only the game app is closed, so the
user can still open the run you just ingested. Confirm it is down (`curl -sf
http://127.0.0.1:3030/api/health` should fail) and tell the user Rundale was closed.

## Calibration example (be this harsh)

A 5-turn run with coherent multi-NPC plot but **every** NPC ignoring its mood tag, one
"I've seen ye carry yer own weight" said to a stranger, and one rambling reply scores roughly:
`narrative_coherence 85, character_fidelity 58 (mood-blind across NPCs), world_responsiveness
88, intent_fidelity 92, immersion 72, progression 78, common_sense 62` → **quality ≈ 75**, with
3+ filed findings. A generous reviewer would have said 82 — that is exactly the inflation to
avoid.
