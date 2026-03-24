# ADR-014: Web & Mobile Architecture

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-23)

## Context

Parish currently supports three UI modes — TUI (Ratatui), GUI (egui/eframe), and headless — all running as local desktop processes. The game engine, LLM inference, and persistence are tightly coupled to the local process. To reach players on web browsers and mobile devices (iOS/Android), we need a client-server architecture where thin clients connect to a cloud-hosted game server.

Key constraints:
- The existing egui GUI already renders the full game interface (map, chat, sidebar, theme).
- eframe natively supports WASM compilation, making browser deployment straightforward.
- Tauri v2 supports iOS and Android alongside desktop, using webview-based rendering.
- LLM inference for web/mobile users must use cloud providers (per ADR-013) since there's no local Ollama.
- The game simulation (NPC ticks, world state, persistence) is computationally significant and must run server-side.

## Decision

Adopt a **thin-client, thick-server** architecture using Rust-native technologies throughout:

### Server
- **axum** HTTP/WebSocket server hosts the game engine
- Each player gets an isolated `GameSession` (world state, NPC manager, persistence)
- Communication via WebSocket with JSON-serialized message protocol
- Cloud LLM providers handle all inference (extending ADR-013)
- SQLite persistence per session (extending Phase 4)

### Web Client
- **egui compiled to WebAssembly** via eframe's native WASM support
- Built with `trunk` (Rust WASM bundler)
- Connects to server via WebSocket (`gloo-net`)
- Reuses the same panel rendering code as the desktop GUI

### Mobile Client
- **Tauri v2** wraps the WASM web client for iOS and Android
- Same egui frontend runs inside Tauri's webview
- Native app lifecycle management (pause/resume, push notifications)
- Touch-optimized layout adaptations

### Shared UI Crate
- `parish-ui` crate extracts egui panel components (chat, map, sidebar, status bar, theme)
- Desktop GUI, web client, and mobile client all depend on this crate
- Panels accept data structs as input, decoupled from game engine internals

## Consequences

- **Code reuse**: ~80% of GUI code is shared across desktop, web, and mobile via `parish-ui`.
- **Rust everywhere**: No JavaScript/TypeScript in the stack. All clients are Rust compiled to native or WASM.
- **Server cost**: Cloud hosting + cloud LLM inference adds operational cost per player session.
- **Latency**: WebSocket adds network latency to every player action. Mitigated by token streaming for LLM responses and optimistic UI updates.
- **Complexity**: New modules (protocol, server, session management) and a workspace restructure. Justified by reaching browser and mobile audiences.
- **Offline play**: Web/mobile clients require an internet connection. Desktop modes (TUI/GUI/headless) continue to work offline with local Ollama.
- **WASM bundle size**: egui WASM builds are typically 5–10 MB. Acceptable for web; fine for mobile app bundles.

## Alternatives Considered

1. **React/TypeScript web + React Native mobile**: Maximum ecosystem support and developer pool, but introduces a second language and duplicates all UI work. Rejected in favor of Rust-native code reuse.
2. **Web-only (responsive, no native mobile app)**: Simpler, but native mobile apps provide better performance, offline potential, and app store presence. Tauri v2's mobile support makes native apps low-cost.
3. **Compile full game engine to WASM (peer-to-peer)**: Eliminates the server, but WASM can't run Ollama or manage SQLite efficiently. LLM inference must go through cloud APIs anyway, making a server the natural home for game logic.
4. **Bevy + WebGPU**: More powerful rendering, but Parish is a text adventure — egui's immediate-mode model is a better fit than a game engine, and eframe's WASM support is more mature.
5. **Flutter for mobile**: Cross-platform mobile framework, but requires Dart and doesn't share code with the Rust backend. Tauri v2 keeps everything in Rust.
