# ADR-016: Replace egui/eframe with Tauri 2 + Svelte GUI

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-24)

## Context

Parish currently ships a windowed GUI built on **egui/eframe** (`src/gui/`, ~2,000 LOC). egui is
an immediate-mode Rust GUI toolkit that renders directly via the OS window compositor (via wgpu/glow
on desktop). It has served the project well for rapid iteration but shows hard limits as the game
matures:

- **Aesthetic ceiling**: egui's painter API makes rich visual design laborious. The Irish cultural
  theme calls for decorative typography, textured panel backgrounds, animated transitions, and
  authentic Celtic visual motifs — none of which egui handles naturally.
- **Map limitations**: The current `map_panel.rs` places locations in a fixed circular arrangement
  that ignores the real OSM-derived geography already extracted by `geo-tool`. A proper interactive
  map (SVG, canvas, or WebGL) is not achievable in egui without substantial bespoke code.
- **Text rendering**: The adventure log needs markdown-like formatting (bold NPC names, italicised
  narration, coloured speaker attribution). egui treats text as flat paragraphs; rich inline styling
  requires workarounds.
- **Font & glyph handling**: Supporting Ogham script, fada-accented Irish characters, and decorative
  display fonts requires careful font fallback chains that egui's font system does not manage well.
- **Web target**: Phase 7 plans a browser-playable version. egui compiles to WASM via `eframe`, but
  browser egui has known rendering and input quirks; a native web frontend (HTML/CSS) is a
  first-class citizen in browsers rather than a guest.
- **Mobile (Phase 7)**: Tauri 2.0 targets iOS and Android natively. A Tauri-based desktop app today
  is the cheapest path to mobile in future phases.

The decision is to remove `src/gui/` entirely and replace it with a **Tauri 2 desktop application**
where the frontend is written in **Svelte + TypeScript** and the Rust game engine acts as the Tauri
backend, communicating via the Tauri IPC bridge.

## Decision

Replace the egui/eframe GUI with **Tauri 2 + Svelte**:

- The project becomes a **Cargo workspace** with two crates: `parish-core` (all game logic) and
  `src-tauri` (the Tauri backend shell).
- Game logic (`world/`, `npc/`, `inference/`, `input/`, `persistence/`, `headless/`, `testing/`)
  moves into `crates/parish-core/` as a reusable library crate.
- The Tauri backend (`src-tauri/src/lib.rs`) wires Tauri commands and events to the `parish-core`
  engine. The Tokio runtime lives here alongside all async game tasks.
- The frontend (`ui/src/`) is a **Svelte 5 + TypeScript** single-page app bundled by **Vite**.
  It communicates with the Rust backend exclusively through typed Tauri `invoke()` calls and
  `listen()` event subscriptions.
- The existing CLI binary (TUI + headless modes) is preserved as a separate workspace binary that
  depends on `parish-core` directly.
- `eframe` and `egui` are removed from `Cargo.toml`.

## Consequences

**Positive:**

- Full HTML/CSS design freedom: Celtic typography, textured backgrounds, CSS animations, SVG
  illustrations, and responsive layout are all first-class.
- Real interactive map: The OSM coordinates from `geo-tool` can drive a proper SVG or canvas map
  with zoom, pan, and click-to-travel.
- Rich text: Markdown-formatted adventure log, speaker-coloured dialogue, inline Irish word
  tooltips — all trivial in HTML.
- Path to mobile: Tauri 2 targets iOS and Android; the Svelte frontend runs unchanged.
- Path to web: The same Svelte frontend can target a browser via the Phase 7 WebSocket server
  (ADR-014) with minimal adaptation.
- Clear separation of concerns: The IPC boundary enforces a clean game-logic / rendering split that
  `mod.rs`'s current 44 KB monolith lacks.
- Frontend hot-reload: Vite HMR makes UI iteration dramatically faster than full Rust rebuilds.

**Negative:**

- **IPC latency for streaming**: Every LLM token crosses the Tauri IPC bridge as a JSON event. The
  per-token overhead is measurable (~1–2 ms/event) and may require batching tokens before emitting.
- **Build complexity**: The project now requires Node.js, `npm`/`pnpm`, Tauri CLI, and a two-step
  build (`ui` bundle then `cargo tauri build`). CI pipelines need updating.
- **Larger binary / slower startup**: Tauri bundles a WebView process. Cold startup is slower than
  an egui binary, though this is imperceptible for a game.
- **WebView platform variance**: WebKitGTK on Linux can be outdated; WebView2 on Windows requires a
  runtime (ships with Windows 11). egui rendered identically everywhere.
- **Rewrite cost**: ~2,000 LOC of GUI Rust and 40+ GUI tests are discarded. All visual behaviour
  must be re-implemented in Svelte. Estimated scope: 2–3 weeks of focused development.
- **Screenshot mechanism changes**: The current egui `ViewportCommand::Screenshot` approach is
  gone; screenshots require a different mechanism (see plan).

## Alternatives Considered

- **Keep egui and extend it**: Considered adding a custom renderer or using egui's painter API for
  richer visuals. Rejected — the aesthetic and map requirements push well beyond what incremental
  egui work can deliver. The ceiling is architectural.
- **egui compiled to WASM (eframe web)**: The Phase 7 plan originally assumed this for browser
  play. Rejected in favour of Tauri because egui-WASM has input lag, no CSS, and the mobile story
  would still require Tauri anyway. Two renderers for the same content is wasteful.
- **Slint**: A Rust-native declarative GUI toolkit with a design-friendly DSL. Evaluated and
  rejected: smaller ecosystem, no browser target, limited CSS-style flexibility.
- **Dioxus**: A Rust-native React-like framework that compiles to desktop, web, and mobile.
  Attractive, but the Rust + JS split in Tauri is preferred because it gives the frontend team full
  access to the JS/npm ecosystem (maps, animations, fonts) without waiting on Rust crate parity.
- **React or Vue instead of Svelte**: Svelte is chosen for its minimal bundle size, lack of VDOM
  overhead, and suitability for stateful game UIs with reactive stores. React and Vue are larger
  without meaningful benefit for this use case.

## Related

- [Plan: Tauri GUI Rewrite](../plans/phase-8-tauri-gui.md)
- [ADR-014: Web & Mobile Architecture](014-web-mobile-architecture.md) — Phase 7 mobile plans that
  this decision accelerates
- [docs/design/overview.md](../design/overview.md) — architecture overview, to be updated post-migration
