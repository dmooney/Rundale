# Driving the live game via the limerick MCP (for QA / the harness)

How an agent drives the **real game** for playtests, QA, and the quality harness. Hard-won
notes — read before automating gameplay. Companion to
[`limerick-mcp/README.md`](../../limerick/crates/limerick-mcp/README.md) and
[`limerick-tauri/src/mcp_bridge.rs`](../../limerick/crates/limerick-tauri/src/mcp_bridge.rs).

**To run a full critical playtest, invoke the `/quality-harness` skill**
([`.agents/skills/quality-harness/SKILL.md`](../../.agents/skills/quality-harness/SKILL.md)) —
it encodes this procedure step-by-step (preflight → `/pause` time control → in-character turn
loop → anchored critical judge → file findings). This doc is its reference manual.

## Golden rules

- **Drive the Tauri desktop app via the limerick MCP (`mcp__limerick__*`).** Never the headless
  `limerick-server` for real play — it has no model (simulator dialogue is garbage) and a
  different HTTP surface (`/api/command`, which the Tauri bridge does **not** serve).
- **Boot:** `cargo run -p limerick-tauri -- --mcp-port 3030` (auto-starts bundled vllm-mlx Qwen
  14B dialogue + 1.5B intent/reaction, and the MCP bridge on `:3030`). The MCP server
  (`.mcp.json` → `limerick-mcp-launch.sh`) bridges your `mcp__limerick__*` calls to it.
  **If you need screenshots, boot with `bash limerick/scripts/launch-tauri-screenshottable.sh 3030`
  instead** — it builds this worktree's static UI and forces Tauri to load `frontendDist`, so the
  graphical harness never depends on a Vite server. (Display sleep is handled in-app: the capture path
  wakes + holds a slept screen, which otherwise reports as locked and fast-fails.)
- **The bridge `:3030` ≠ the limerick-server `/api/command` surface.** Bridge routes:
  `submit-input`, `engine-state`, `world-snapshot`, `npcs-here`, `transcript`,
  `debug-snapshot`, `new-game`, `take-screenshot`, `submit-byok`, `map`, save/load, etc. There
  is **no `/api/command`** on the bridge.

## Tool surface

`mcp__limerick__*` (16 curated tools) + `tauri_invoke` (escape hatch = any bridge command, e.g.
`get_transcript`, `get_debug_snapshot`).

- **Drive:** `limerick_new_game`, `limerick_submit_input(text, addressed_to?)`, `limerick_save_game`,
  `limerick_load_branch`.
- **Read state:** `limerick_engine_state` (small, canonical), `limerick_world_snapshot` (small;
  scene description + clock), `limerick_npcs_here`, `limerick_map`, `limerick_save_state`.
- **Screenshots:** `limerick_take_screenshot`, `limerick_latest_screenshot`.
- **Setup/BYOK:** `limerick_setup_status`, `limerick_setup_byok` (engine model A/B),
  `limerick_byok_env_keys`.
- **Bugs:** `limerick_file_bug`.

## Reading a turn's result

- **`limerick_submit_input` blocks until the turn is processed, then returns `null`.** The NPC
  reply already exists on return — it's just not returned. Read it via
  `tauri_invoke("get_transcript")` (flat `[{speaker,text}]`) or
  `tauri_invoke("get_debug_snapshot").conversations.exchanges` (pairs
  `player_input`↔`npc_dialogue`). (Issue #1353 / #1356 propose returning it directly.)
- **`get_debug_snapshot` is ~357 KB / 10k lines.** Do **not** pull it per turn — it blows the
  tool-result token cap. Use the small reads (`engine_state`, `world_snapshot`, `transcript`)
  per turn; reserve `debug-snapshot` for rare deep dives, and `jq` the saved file.

## Time is asynchronous — control it explicitly

The world sim runs in **real time at ~36× (`speed_factor`)** when unpaused: NPCs arrive/leave
on schedule, gossip spreads, reactions fire, weather changes — **independent of player input**.
The dialogue transcript does **not** capture these; world events live in
`debug-snapshot.events`.

Control time with **slash commands via `limerick_submit_input`** (verified):

- `/pause` → `engine_state.clock.paused = true`, sim **frozen** (idle = no drift).
- `/resume` → `paused = false`, sim runs at `speed_factor`.
- `/wait N` → advance a fixed amount.

**Harness pattern** (deterministic, captures the async world, no event-push needed):

```text
/pause                      # freeze
obs0 = engine_state
action = decide(obs0)
limerick_submit_input(action) # blocks; reply ready on return (read via transcript)
/resume ; /wait N ; /pause  # advance the world a controlled N for autonomous NPC life
delta = new dialogue + new debug-snapshot.events + state delta   # frozen → nothing lost
```

> **MCP push does not fit the agent loop.** Claude Code is request/response: a server-sent
> notification does not wake the model mid-turn to "react". Freeze + checkpoint-read is the
> right model, and is token-cheaper.

### Focus toggles time (gotcha)

Window focus is coupled to the clock — alt-tabbing **pauses on blur / resumes on focus** (the
coupling behind closed issue #1277; the `paused` flag can read `true` while the clock still
advances). Observed: raising the window to foreground resumed the clock and ran it ~6 game-hours
during an otherwise-idle run. **Do not rely on focus; set `/pause` explicitly each loop.**
Issue #1357 adds a feature flag to disable focus-auto-pause (off for the harness).

## Screenshots

- `limerick_take_screenshot` prefers a fresh native-window capture so the WebGL minimap appears in
  the image. If the desktop window is backgrounded / minimized / off-Space and a previous verified
  screenshot exists, the bridge returns that latest path with a `fallback: "latest_screenshot"`
  warning instead of a generic HTTP 500 (#1522). If no verified fallback exists, the endpoint
  returns a structured `503 screenshot capture unavailable`.
- **For a fresh capture:** wake the display + raise the window, then capture:

```sh
caffeinate -u -t 3 &
osascript -e 'tell application "System Events" to set frontmost of (first process whose name contains "limerick") to true'
```

- **Collision:** foregrounding to capture can **resume the clock** (focus coupling above). After
  a screenshot, restore `/pause`.

## MCP connection / teleport gotcha

- The `limerick` MCP server is spawned by Claude Code at **session init only** (`.mcp.json` →
  `limerick-mcp-launch.sh`); **there is no in-session reload**. If the init spawn fails, the
  session is stuck without `mcp__limerick__*`.
- **Teleporting into a fresh worktree breaks it** (#1352): the launch script resolves the binary
  from `CARGO_TARGET_DIR` env (unset here — the shared target is set via `~/.cargo/config.toml`
  `target-dir`), looks in the wrong `limerick/target`, finds nothing, cold-builds, loses the
  handshake race. **Fix/workaround:** pre-build `limerick-mcp` and stage it where the script looks
  (`limerick/target/debug/limerick-mcp`), then start a fresh session.

## Known issues surfaced via MCP playtests

| #     | What                                                                       |
| ----- | -------------------------------------------------------------------------- |
| #1351 | Commands (`look`) intermittently classified as player dialogue; NPC reacts |
| #1352 | Teleport breaks MCP (target-dir mismatch in launch script)                 |
| #1353 | No first-class dialogue read (`submit_input` → `null`)                     |
| #1354 | `infer_player_message_reaction` Serialization errors + 2s timeout          |
| #1355 | `limerick_take_screenshot` times out when window backgrounded              |
| #1356 | MCP token efficiency: compact `submit_input` result + slim per-turn read   |
| #1357 | Feature-flag focus auto-pause/resume; disable for the harness              |
