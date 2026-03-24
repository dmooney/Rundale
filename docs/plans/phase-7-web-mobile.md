# Plan: Phase 7 — Web & Mobile Apps

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Planned**

## Goal

Deliver Parish as a full game client in web browsers and on mobile devices (iOS/Android) using Rust-native technologies: egui compiled to WebAssembly for the browser, and Tauri v2 for native mobile apps. A cloud-hosted game server (axum + WebSocket) manages game state and routes LLM inference through cloud providers.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Cloud Server                             │
│  ┌──────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │  axum    │  │ Game Engine  │  │ Cloud LLM (OpenRouter/   │  │
│  │  HTTP +  │←→│ WorldState   │←→│ Anthropic/OpenAI)        │  │
│  │  WS      │  │ NpcManager   │  │                          │  │
│  └────┬─────┘  │ GameClock    │  └──────────────────────────┘  │
│       │        └──────────────┘                                 │
│       │        ┌──────────────┐                                 │
│       │        │ SQLite       │                                 │
│       │        │ Persistence  │                                 │
│       │        └──────────────┘                                 │
└───────┼─────────────────────────────────────────────────────────┘
        │ WebSocket (JSON messages)
        │
   ┌────┴──────────────────────────────────┐
   │            Thin Clients               │
   ├───────────────────┬───────────────────┤
   │  Web (WASM)       │  Mobile (Tauri)   │
   │  egui + eframe    │  egui + Tauri v2  │
   │  wasm-bindgen     │  iOS + Android    │
   │  Browser runtime  │  Native webview   │
   └───────────────────┴───────────────────┘
```

### Key Design Decisions

1. **Thin client, thick server**: All game logic (world simulation, NPC ticks, inference routing, persistence) runs server-side. Clients are pure rendering + input.
2. **egui everywhere**: The existing `src/gui/` module already renders via egui. eframe supports WASM compilation natively. Reuse the same GUI code for web and mobile.
3. **Tauri v2 for mobile**: Tauri 2.0 supports iOS and Android alongside desktop. The egui frontend runs inside Tauri's webview (via `eframe`'s WASM target) or natively via `egui` + `winit` on mobile.
4. **WebSocket protocol**: Real-time bidirectional communication between client and server. JSON-serialized game messages. Supports token streaming for LLM responses.
5. **Cloud-only inference**: Web/mobile clients connect to a cloud-hosted server that uses cloud LLM providers (per ADR-013). No local Ollama dependency for remote play.

## Prerequisites

- Phase 4 complete: persistence layer needed for server-side game state management
- ADR-013 cloud LLM support: server must route inference to cloud providers
- Phase 3 complete: NPC system, world graph, and game loop are the core being served

## Tasks

### Part A: Game Protocol & Server (Rust)

1. **Define the client-server protocol in `src/protocol/mod.rs`** (new module)
   - `ClientMessage` enum (serde): `Connect { player_name }`, `PlayerInput { text }`, `Command { name, args }`, `Ping`
   - `ServerMessage` enum (serde): `Welcome { world_snapshot }`, `TextLog { entries: Vec<TextEntry> }`, `StreamToken { token }`, `StreamEnd`, `WorldUpdate { location, time, weather, npcs_present }`, `MapUpdate { locations, edges, player_pos }`, `Pong`, `Error { message }`
   - `TextEntry` struct: `source: TextSource`, `content: String`, `timestamp: GameTime`
   - `TextSource` enum: `Narrator`, `Npc { name }`, `Player`, `System`
   - Use `serde_json` for serialization over WebSocket text frames

2. **Extract game engine into a reusable `GameSession` struct in `src/session.rs`**
   - Encapsulates: `WorldState`, `NpcManager`, `GameClock`, `InferenceClients`, `Database`
   - `async fn process_input(&mut self, input: &str) -> Vec<ServerMessage>` — runs the existing game loop pipeline (intent parse → inference → world update → response messages)
   - `fn world_snapshot(&self) -> WorldSnapshot` — serializable snapshot for `Welcome` message
   - `fn current_state(&self) -> WorldUpdate` — current location, time, weather, NPCs for UI sync
   - This refactor separates game logic from UI, replacing the current direct coupling in `main.rs`

3. **Implement the game server in `src/server/mod.rs`** (new module)
   - Framework: `axum` with `axum::extract::ws::WebSocket` for WebSocket support
   - `async fn main_server(config: ServerConfig)` — binds to `0.0.0.0:8080`, serves static WASM files and WebSocket endpoint
   - Route `/ws` — WebSocket upgrade, spawns per-connection `handle_session` task
   - Route `/` — serves the WASM web client (static files from `web/dist/`)
   - Route `/health` — health check endpoint for load balancers
   - `ServerConfig` struct: `port`, `cloud_provider`, `max_sessions`, `static_dir`

4. **Implement per-session WebSocket handler in `src/server/session_handler.rs`**
   - `async fn handle_session(ws: WebSocket, engine: Arc<Mutex<GameSession>>)`
   - On `ClientMessage::Connect` → create `GameSession`, send `Welcome` + `WorldUpdate` + `MapUpdate`
   - On `ClientMessage::PlayerInput` → call `session.process_input()`, stream tokens via `StreamToken` messages, then send `StreamEnd` + `WorldUpdate`
   - Background task: send `WorldUpdate` on game clock ticks (NPC movements, time changes)
   - Heartbeat: respond to `Ping` with `Pong`, disconnect on 30s timeout

5. **Implement session management in `src/server/manager.rs`**
   - `SessionManager` struct: manages multiple concurrent game sessions
   - `fn create_session(&mut self, player_id: &str) -> SessionId`
   - `fn get_session(&self, id: SessionId) -> Option<&GameSession>`
   - Session lifecycle: create on connect, persist on disconnect, resume on reconnect
   - Memory limits: cap at `max_sessions` (configurable, default 50)
   - Idle timeout: save and drop sessions after 30 minutes of inactivity

6. **Add `--server` CLI flag in `src/main.rs`**
   - New mode alongside `--gui`, `--headless`, `--script`
   - `cargo run -- --server` starts the game server
   - `--server-port <PORT>` (default 8080)
   - `--server-static <DIR>` (default `web/dist/`)

### Part B: Web Client (egui + WASM)

7. **Create web client workspace member in `web/`**
   - `web/Cargo.toml`: workspace member, depends on `parish` (library), `eframe` with `wasm` feature, `wasm-bindgen`, `web-sys`, `gloo-net` (WebSocket)
   - `web/src/lib.rs`: WASM entry point via `#[wasm_bindgen(start)]`
   - `web/index.html`: minimal HTML shell loading the WASM bundle
   - Build with `trunk` (Rust WASM bundler): `trunk build --release` → outputs to `web/dist/`

8. **Implement `WebClient` networking layer in `web/src/net.rs`**
   - `WebClient` struct: wraps `gloo-net::websocket::futures::WebSocket`
   - `async fn connect(url: &str) -> Result<Self>` — connects to server WebSocket
   - `async fn send(&self, msg: ClientMessage) -> Result<()>`
   - `async fn recv(&self) -> Result<ServerMessage>` — returns next message from server
   - Reconnection logic: exponential backoff (1s, 2s, 4s, 8s) on disconnect, resend `Connect` message

9. **Adapt `GuiApp` for thin-client mode in `web/src/app.rs`**
   - `WebGuiApp` struct: mirrors `src/gui/mod.rs` `GuiApp` but receives state from server instead of local engine
   - Reuse `src/gui/` panel modules: `chat_panel`, `map_panel`, `sidebar`, `status_bar`, `input_field`, `theme`
   - On input submit → send `ClientMessage::PlayerInput` via WebSocket
   - On `ServerMessage::StreamToken` → append to chat panel (same streaming UX as local)
   - On `ServerMessage::WorldUpdate` → update status bar, sidebar, NPC list
   - On `ServerMessage::MapUpdate` → update map panel
   - Map click-to-move → sends `PlayerInput { text: "go to <location>" }`

10. **Configure eframe for WASM target**
    - `eframe::WebOptions` with canvas ID matching `index.html`
    - Set `max_size_points` for responsive sizing
    - Handle browser events: window resize, tab visibility, beforeunload (save)
    - Touch input support: egui handles this natively, but verify scroll and tap behavior

11. **Build pipeline and static serving**
    - Add `Makefile` or `justfile` recipe: `make web` → `cd web && trunk build --release`
    - Server serves `web/dist/` at `/` — `index.html`, `*.wasm`, `*.js`
    - Cache headers: WASM files get content-hash filenames for cache busting

### Part C: Mobile Client (Tauri v2)

12. **Create Tauri v2 project in `mobile/`**
    - `mobile/` directory with Tauri v2 project structure
    - `mobile/src-tauri/` — Rust backend (Tauri commands, app config)
    - `mobile/src/` — Frontend (loads the same egui WASM bundle from Part B)
    - `mobile/src-tauri/tauri.conf.json` — app name "Parish", bundle ID `com.parish.app`, permissions

13. **Configure Tauri for iOS and Android**
    - iOS: `tauri ios init` → Xcode project in `mobile/src-tauri/gen/apple/`
    - Android: `tauri android init` → Gradle project in `mobile/src-tauri/gen/android/`
    - Both targets load the egui WASM frontend in Tauri's webview
    - Deep link support: `parish://` URL scheme for save sharing (future)

14. **Implement mobile-specific adaptations**
    - Touch-optimized input: larger tap targets on map nodes, virtual keyboard management
    - `input_field` adjustments: auto-focus on tap, keyboard dismiss on send
    - Responsive layout: stack panels vertically on narrow screens (chat above map)
    - Status bar: compact single-line format for small screens
    - Sidebar: swipe-to-reveal gesture on mobile (egui `SidePanel` with animation)

15. **Mobile networking and lifecycle**
    - Reuse `WebClient` from Part B (same WebSocket protocol)
    - Handle app lifecycle: `on_pause` → save session ID, `on_resume` → reconnect WebSocket
    - Background: disconnect WebSocket when app is backgrounded, reconnect on foreground
    - Push notification hook (future): server can notify when interesting NPC events occur

16. **Build and distribution setup**
    - iOS: `tauri ios build` → `.ipa` for TestFlight / App Store
    - Android: `tauri android build` → `.apk`/`.aab` for Play Store
    - CI recipe: GitHub Actions workflow for building both targets

### Part D: Shared Infrastructure

17. **Extract GUI panels into a shared crate `parish-ui`**
    - Move `src/gui/theme.rs`, `chat_panel.rs`, `map_panel.rs`, `sidebar.rs`, `status_bar.rs`, `input_field.rs` to `crates/parish-ui/src/`
    - These modules render with egui and take data structs as input (not game engine references)
    - Define `UiState` trait or struct: text log, location info, map data, NPC list, time/weather
    - Desktop GUI (`src/gui/`): populates `UiState` from local `WorldState`
    - Web/Mobile GUI: populates `UiState` from `ServerMessage` payloads
    - Both frontends call the same panel rendering functions

18. **Authentication and session tokens**
    - Simple token-based auth: server generates session token on `Connect`, client stores in `localStorage` (web) or Tauri secure storage (mobile)
    - Reconnect with token to resume session without re-creating game state
    - No user accounts in Phase 7 — anonymous sessions with optional player name
    - Future: OAuth or passkey auth for persistent accounts

19. **Server deployment configuration**
    - `Dockerfile`: multi-stage build (Rust builder → minimal runtime image with WASM assets)
    - `docker-compose.yml`: server + volume for SQLite persistence
    - Environment variables: `PARISH_CLOUD_API_KEY`, `PARISH_CLOUD_MODEL`, `PARISH_SERVER_PORT`, `PARISH_MAX_SESSIONS`
    - Health check endpoint at `/health` for orchestrators (Kubernetes, ECS, etc.)

20. **Monitoring and observability**
    - `tracing` with `tracing-subscriber` JSON output for structured logging
    - Metrics: active sessions, WebSocket messages/sec, inference latency, error rate
    - Optional: `prometheus` metrics endpoint at `/metrics` for monitoring
    - Rate limiting: per-session message rate limit (prevent inference abuse)

## New Dependencies

| Crate | Purpose | Used In |
|-------|---------|---------|
| `axum` | HTTP/WebSocket server | Server |
| `axum-extra` | WebSocket utilities | Server |
| `tower` | Middleware (CORS, rate limiting) | Server |
| `tower-http` | Static file serving, CORS | Server |
| `tokio-tungstenite` | WebSocket protocol | Server |
| `trunk` | WASM build tool | Web (build-time) |
| `wasm-bindgen` | Rust↔JS interop | Web client |
| `web-sys` | Browser API bindings | Web client |
| `gloo-net` | WebSocket client for WASM | Web client |
| `tauri` (v2) | Mobile app framework | Mobile |

## Workspace Structure

```
Parish/
├── Cargo.toml              # Workspace root
├── crates/
│   └── parish-ui/          # Shared egui panels (theme, chat, map, sidebar)
│       ├── Cargo.toml
│       └── src/
├── src/                    # Main binary (TUI, GUI, headless, server modes)
│   ├── server/             # axum game server
│   │   ├── mod.rs
│   │   ├── session_handler.rs
│   │   └── manager.rs
│   ├── protocol/           # Client↔server message types
│   │   └── mod.rs
│   └── session.rs          # GameSession (extracted game engine)
├── web/                    # WASM web client
│   ├── Cargo.toml
│   ├── index.html
│   ├── Trunk.toml
│   └── src/
│       ├── lib.rs
│       ├── app.rs          # WebGuiApp (thin client)
│       └── net.rs          # WebSocket client
├── mobile/                 # Tauri v2 mobile app
│   ├── src-tauri/
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   └── src/
│   └── src/                # Loads WASM frontend
└── Dockerfile
```

## Implementation Order

1. **Tasks 1–2**: Protocol + GameSession extraction (foundational, unblocks everything)
2. **Task 17**: Extract shared UI crate (must happen before web/mobile use the panels)
3. **Tasks 3–6**: Game server (axum + WebSocket + session management)
4. **Tasks 7–11**: Web client (WASM + egui + networking)
5. **Tasks 12–16**: Mobile client (Tauri v2 wrapping the web client)
6. **Tasks 18–20**: Auth, deployment, monitoring (polish)

## Testing Strategy

- **Protocol**: Unit tests for serialization/deserialization of all message types
- **GameSession**: Unit tests for `process_input` with mock inference (reuse `GameTestHarness`)
- **Server**: Integration tests with `axum::test` — connect WebSocket, send messages, verify responses
- **Web client**: Manual browser testing + Playwright/Selenium for automated E2E
- **Mobile**: Manual device testing + Tauri's built-in test utilities
- **Load testing**: `k6` or `locust` script simulating 50 concurrent WebSocket sessions

## Open Questions

1. **Single-player or multiplayer?** — This plan assumes single-player sessions (one player per game world). Multiplayer (shared world) is a significant extension deferred to a future phase.
2. **Mobile app store approval** — Text adventure games with AI-generated content may require content moderation disclosure for App Store / Play Store review.
3. **WASM bundle size** — egui WASM builds can be 5–10 MB. May need `wasm-opt` optimization and lazy loading for acceptable mobile load times.
4. **Offline mobile play** — Could embed a small local model (via ONNX Runtime) for offline mobile play. Significant complexity; deferred.
