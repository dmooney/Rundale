# Plan: LLM Demo / Auto-Player Mode

## Context

User wants an LLM to act as the human player in the Tauri desktop app — for observable playtesting and content generation. The LLM gets a structured text context each turn (status + location + NPCs + chat log), chooses an action, and submits it like a human would. Config via CLI flags at launch + a togglable F11 panel. Stop anytime with Esc or the panel's Stop button.

Tauri-only scope: the demo loop depends on the desktop app's observable state and `streamingActive` store for turn-complete detection. Architecture-fitness won't flag this because the new Tauri commands don't leak into backend-agnostic crates.

---

## Key Design Decisions (vs. Original Draft)

| Topic | Original Draft | This Plan | Reason |
|-------|---------------|-----------|--------|
| `InferenceCategory::Demo` | Add new variant | **Skip for MVP** | Cascades across 8+ files (`cat_idx`, `[Option;4]` arrays, `ALL`, `RateLimitConfig`, `presets.rs`, CLI, etc.) — add later if per-category config is needed |
| `EVENT_ACTION_COMPLETE` | New backend event | **Skip** | Not needed — frontend detects turn-complete via `streamingActive` store |
| Non-streaming LLM call | Queue via `token_tx: None` | **Direct `AnyClient::generate()`** | Same pattern as `parse_intent` (uses `state.client` directly); simpler |
| `recent_log` in context | Backend fetches | **Frontend fills before calling** | Backend has no text log; frontend owns the `textLog` store |
| `set_demo_config` | Backend-persisted | **Frontend state only** | Panel modifies `demoConfig` store; no need to round-trip through backend |

---

## Data Flow Diagram

```
CLI args (--demo, --demo-prompt, etc.)
       │
       ▼
 run() in lib.rs ──► parse DemoConfig ──► AppState.demo_config (read-only)
                                                │
                                                ▼
                               Frontend: get_demo_config() on mount
                                         │
                                         ├─► auto_start=true → startDemo()
                                         └─► F11 panel shows initial config

Demo loop (demo-player.ts):
  ┌─────────────────────────────────────────────┐
  │ 1. sleep(turn_pause_secs)                   │
  │ 2. getDemoContext() → DemoContextSnapshot   │
  │    (backend fills location/time/NPCs/adj)   │
  │ 3. ctx.recent_log = last 40 from textLog    │
  │ 4. getLlmPlayerAction(ctx) → action string  │
  │    (backend: AnyClient::generate())         │
  │ 5. subscribe streamingActive BEFORE submit  │
  │ 6. submitInput(action, [])                  │
  │ 7. wait until streamingActive=false (or     │
  │    50ms if streaming never started)         │
  │ 8. demoTurnCount++; check max_turns         │
  └──────────────── repeat ─────────────────────┘
```

---

## Phase 1 — Backend

### `parish/crates/parish-tauri/src/lib.rs`

**`DemoConfig` struct** (add in lib.rs, not in parish-config, to stay Tauri-scoped):
```rust
pub struct DemoConfig {
    pub auto_start: bool,
    pub extra_prompt: Option<String>,  // loaded from file, not the path
    pub turn_pause_secs: f32,          // default 2.0
    pub max_turns: Option<u32>,        // None = unlimited
}
```

**`AppState` additions** — two plain fields (no Mutex, read-only after startup):
```rust
pub demo_config: DemoConfig,
```

**CLI arg parsing** in `run()`, same pattern as `--screenshot`:
```rust
let demo_auto_start = args.iter().any(|a| a == "--demo");
let demo_extra_prompt = args.iter()
    .position(|a| a == "--demo-prompt")
    .and_then(|i| args.get(i + 1))
    .and_then(|p| std::fs::read_to_string(p).ok());
let demo_pause_secs = args.iter()
    .position(|a| a == "--demo-pause")
    .and_then(|i| args.get(i + 1))
    .and_then(|s| s.parse::<f32>().ok())
    .unwrap_or(2.0);
let demo_max_turns = args.iter()
    .position(|a| a == "--demo-max-turns")
    .and_then(|i| args.get(i + 1))
    .and_then(|s| s.parse::<u32>().ok());
```

Add `demo_config: DemoConfig { auto_start: demo_auto_start, extra_prompt: demo_extra_prompt, turn_pause_secs: demo_pause_secs, max_turns: demo_max_turns }` to `AppState { ... }`.

**Register new commands** in `.invoke_handler()`:
```rust
commands::get_demo_context,
commands::get_llm_player_action,
commands::get_demo_config,
```

### `parish/crates/parish-tauri/src/commands.rs`

**`DemoContextSnapshot`** struct (serde Serialize + Deserialize):
```rust
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoContextSnapshot {
    pub location_name: String,
    pub location_description: String,
    pub game_time: String,           // "Monday, 12 March 1820, Morning"
    pub season: String,
    pub weather: String,
    pub npcs_here: Vec<DemoNpcInfo>,
    pub adjacent: Vec<DemoAdjacentLocation>,
    // Frontend fills this before calling get_llm_player_action
    pub recent_log: Vec<String>,
    // Carried from AppState.demo_config for prompt assembly
    pub extra_prompt: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoNpcInfo {
    pub name: String,
    pub description: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoAdjacentLocation {
    pub name: String,
    pub travel_minutes: Option<u16>,
    pub visited: bool,
}
```

**`DemoConfigPayload`** struct for `get_demo_config` response:
```rust
#[derive(serde::Serialize, Clone)]
pub struct DemoConfigPayload {
    pub auto_start: bool,
    pub extra_prompt: Option<String>,
    pub turn_pause_secs: f32,
    pub max_turns: Option<u32>,
}
```

**`get_demo_context` command**: Locks `world` + `npc_manager` + `transport`, builds snapshot. Uses existing `world.clock.now()`, `world.graph`, `npc_manager.npcs_at()`. For NPC description: use `npc.brief_description` or `npc.personality` (check the Npc struct fields).

**`get_llm_player_action` command**: Takes `DemoContextSnapshot`, assembles prompt (system + user), calls `state.client.lock().await` to get the `AnyClient`, calls `client.generate(&model, &user_prompt, Some(&system_prompt), Some(200), Some(0.9))`. Returns cleaned action text (trim whitespace + strip surrounding quotes).

**Guard**: check `config.flags.is_enabled("demo-mode")` (register this flag name; the flag is auto-enabled when `--demo` is passed).

**`get_demo_config` command**: Reads `state.demo_config` and returns `DemoConfigPayload`.

**Prompt assembly** (in `get_llm_player_action`):

System prompt:
```
You are playing Rundale, an Irish living-world simulation set in 1820. You are a wandering
stranger exploring the townlands of east Roscommon. The world is populated by historical
Irish villagers — farmers, priests, weavers, matchmakers — each living their own life.

Explore naturally: talk to people, learn their stories, travel between locations, and respond
to whatever you encounter. Act as a curious outsider would.

{ctx.extra_prompt if present}

Reply with exactly one action — the text you would type as the player. Examples:
  Hello Brigid, what brings you here today?
  go to the mill
  look
  ask Seamus about the harvest
Do not include any explanation, just the action text.
```

User prompt assembled from `DemoContextSnapshot` fields: location, time, season, weather, NPCs here (bulleted), adjacent locations with travel times and visited status, recent log lines prefixed with `> `.

---

## Phase 2 — Frontend

### New file: `parish/apps/ui/src/stores/demo.ts`

```typescript
import { writable } from 'svelte/store';
export const demoEnabled = writable(false);
export const demoPaused = writable(false);
export const demoTurnCount = writable(0);
export const demoStatus = writable<'idle' | 'waiting' | 'thinking' | 'acting'>('idle');
export interface DemoConfig {
    auto_start: boolean;
    extra_prompt: string | null;
    turn_pause_secs: number;
    max_turns: number | null;
}
export const demoConfig = writable<DemoConfig>({
    auto_start: false,
    extra_prompt: null,
    turn_pause_secs: 2.0,
    max_turns: null
});
```

### New file: `parish/apps/ui/src/lib/demo-player.ts`

Turn-complete detection: subscribe to `streamingActive` before submitting. Track whether it ever went `true`. After `submitInput` resolves: if it went `true`, wait until `streamingActive` is `false` (covers dialog + movement-with-reactions). If never went `true`, yield 50ms.

```typescript
import { get } from 'svelte/store';
import { demoEnabled, demoPaused, demoTurnCount, demoStatus, demoConfig } from '../stores/demo';
import { streamingActive, textLog } from '../stores/game';
import { getDemoContext, getLlmPlayerAction, submitInput } from './ipc';

function sleep(ms: number) { return new Promise(r => setTimeout(r, ms)); }

function waitForFalse(store: Readable<boolean>): Promise<void> {
    return new Promise(resolve => {
        const unsub = store.subscribe(v => { if (!v) { unsub(); resolve(); } });
    });
}

export async function runDemoTurn(): Promise<void> {
    if (!get(demoEnabled) || get(demoPaused)) return;
    const config = get(demoConfig);

    demoStatus.set('waiting');
    await sleep(config.turn_pause_secs * 1000);
    if (!get(demoEnabled) || get(demoPaused)) return;

    demoStatus.set('thinking');
    const ctx = await getDemoContext();
    // Fill recent log from frontend store (last 40 lines)
    ctx.recent_log = get(textLog).slice(-40).map(e => `[${e.source}] ${e.content}`);
    if (config.extra_prompt) ctx.extra_prompt = config.extra_prompt;
    const action = (await getLlmPlayerAction(ctx)).trim().replace(/^["']|["']$/g, '');

    demoStatus.set('acting');
    // Detect whether streaming starts during submit
    let streamingStarted = false;
    const unsub = streamingActive.subscribe(v => { if (v) streamingStarted = true; });
    await submitInput(action, []);
    unsub();

    if (streamingStarted) {
        await waitForFalse(streamingActive);
    } else {
        await sleep(50);
    }

    demoTurnCount.update(n => n + 1);
    if (config.max_turns != null && get(demoTurnCount) >= config.max_turns) {
        demoEnabled.set(false);
        demoStatus.set('idle');
    }
}

let loopRunning = false;
export async function startDemoLoop(): Promise<void> {
    if (loopRunning) return;
    loopRunning = true;
    demoEnabled.set(true);
    while (get(demoEnabled)) {
        if (!get(demoPaused)) {
            try { await runDemoTurn(); } catch (e) { console.warn('Demo turn error:', e); await sleep(2000); }
        } else {
            await sleep(500);
        }
    }
    loopRunning = false;
    demoStatus.set('idle');
}

export function stopDemo(): void {
    demoEnabled.set(false);
}
```

### New file: `parish/apps/ui/src/components/DemoBanner.svelte`

Fixed overlay bar (top-center) shown when `demoEnabled`. Shows turn count, status, Pause/Resume + Stop buttons. Uses `demoEnabled`, `demoPaused`, `demoTurnCount`, `demoStatus` stores.

### New file: `parish/apps/ui/src/components/DemoPanel.svelte`

F11 config panel (same structural pattern as `DebugPanel.svelte`):
- Enable toggle
- Pause slider (0–30s)
- Max turns input (0 = unlimited)
- Extra prompt textarea
- Apply & Start / Stop buttons
- Turn counter and status display
On "Apply & Start": updates `demoConfig` store, calls `startDemoLoop()`.

### Modifications to `parish/apps/ui/src/routes/+page.svelte`

1. Import `DemoBanner`, `DemoPanel`, `demoVisible` (new store), `startDemoLoop`, `stopDemo`, `demoEnabled`
2. Add `demoVisible = writable(false)` store (or add to `demo.ts`)
3. `handleKeydown`: add F11 branch (toggle `demoVisible`)
4. `handleKeydown`: Esc branch: if `get(demoEnabled)`, call `stopDemo()` and `return`
5. In `setupMount` (after initial fetches): call `getDemoConfig()`, if `auto_start === true` call `startDemoLoop()`
6. Template: add `<DemoBanner />` and `{#if $demoVisible}<DemoPanel />{/if}` alongside `<DebugPanel />`

### Modifications to `parish/apps/ui/src/lib/ipc.ts`

```typescript
export const getDemoContext = () => command<DemoContextSnapshot>('get_demo_context');
export const getLlmPlayerAction = (ctx: DemoContextSnapshot) =>
    command<string>('get_llm_player_action', { ctx });
export const getDemoConfig = () => command<DemoConfigPayload>('get_demo_config');
```

Add `DemoContextSnapshot` and `DemoConfigPayload` types to `lib/types.ts`.

---

## Critical Files

| File | Change |
|------|--------|
| `parish/crates/parish-tauri/src/lib.rs` | `DemoConfig` struct, `AppState.demo_config`, CLI arg parsing, register 3 commands |
| `parish/crates/parish-tauri/src/commands.rs` | 3 new commands, `DemoContextSnapshot`, `DemoNpcInfo`, `DemoAdjacentLocation`, `DemoConfigPayload`, prompt assembly |
| `parish/apps/ui/src/lib/ipc.ts` | 3 new command wrappers |
| `parish/apps/ui/src/lib/types.ts` | `DemoContextSnapshot`, `DemoConfigPayload` types |
| `parish/apps/ui/src/routes/+page.svelte` | F11 handler, Esc stop, auto-start on mount, mount components |
| `parish/apps/ui/src/stores/demo.ts` | **new** — demo state stores |
| `parish/apps/ui/src/lib/demo-player.ts` | **new** — demo turn loop |
| `parish/apps/ui/src/components/DemoBanner.svelte` | **new** — always-on overlay banner |
| `parish/apps/ui/src/components/DemoPanel.svelte` | **new** — F11 config panel |

Existing NPC struct field to check: `parish/crates/parish-npc/src/lib.rs` — confirm `brief_description` or equivalent field name before writing `get_demo_context`.

---

## Verification

1. `just run -- --demo --demo-prompt mods/rundale/demo-prompt.txt --demo-pause 2`
2. Observe 5+ turns auto-complete: action chosen, submitted, NPC dialog reveals word-by-word, next turn starts after drain + 2s pause.
3. Verify context quality: actions reference current location, NPCs visible.
4. Press Esc — demo stops, status resets to idle.
5. Press F11 — DemoPanel opens; adjust pause to 5s, click Apply & Start, demo resumes at new pace.
6. Set max-turns to 3 — demo stops automatically after 3 turns. Banner shows "Turn 3".
7. `just check` passes (clippy, fmt, tests, architecture fitness).

---

## Deliberately Out of Scope (can add later)

- `InferenceCategory::Demo` — per-category provider/model override for demo generation
- Web-server parity for demo mode
- Irish word hints in context (not stored backend-side)
- `set_demo_config` backend command (panel manages frontend state only)
