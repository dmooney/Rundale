# iOS Port — Fully On-Device Rundale

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) |
> ADRs: [005 — Ollama Local Inference](../adr/005-ollama-local-inference.md), [014 — Web/Mobile Architecture](../adr/014-web-mobile-architecture.md), [016 — Tauri + Svelte GUI](../adr/016-tauri-svelte-gui.md) |
> Related: [Phase 7 — Web & Mobile](../plans/phase-7-web-mobile.md) (alternative thin-client design)
>
> **Status: Design (implementation-ready)** — open decisions closed; FFI, save-path, model-delivery, and migration order committed. No code work has started.

## Goal

Ship the entire Rundale game — UI, simulation, persistence, and LLM inference —
running fully on-device on a modern iPhone (15 Pro / 16 class), with no
network calls and no companion server. The player downloads the app, the app
downloads its language model on first launch, and from then on Rundale runs
offline.

This is deliberately the *opposite* of the [Phase 7 — Web & Mobile](../plans/phase-7-web-mobile.md)
design, which puts the game on a cloud server and ships a thin client to
mobile. Both approaches are viable; this doc describes the on-device path so
the trade-off is explicit.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│       parish-core · parish-inference · parish-persistence (+ others)    │
│  WorldGraph · NpcManager · GameClock · SimulationTiers · Persistence    │
│  InferenceBackend trait (Box<dyn>; impl chosen at compile time)         │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │ shared, unchanged across modes
   ┌───────────────────────┼─────────────────────────────────┐
   │                       │                                 │
   ▼                       ▼                                 ▼
┌─────────────┐  ┌──────────────────┐  ┌──────────────────────────────┐
│ parish-cli  │  │ parish-tauri     │  │ parish-tauri                 │
│ (headless)  │  │ desktop          │  │ iOS (new)                    │
│ Ollama HTTP │  │ Ollama HTTP      │  │ LiteRT-LM + iOS GPU in-proc  │
└─────────────┘  └──────────────────┘  └──────────────────────────────┘
                 ┌──────────────────┐
                 │ parish-server    │
                 │ Axum + cloud LLM │
                 └──────────────────┘
```

iOS becomes a fourth mode alongside the headless CLI, Tauri desktop, and the
Axum web server. All four consume the shared crates (`parish-core`,
`parish-inference`, `parish-persistence`, and siblings) unchanged. The only
iOS-specific code lives in three places:

1. **The inference backend** — LiteRT-LM linked into `parish-inference` via a thin C FFI bridge
2. **The save-path resolver** — iOS sandbox instead of relative `saves/`
3. **The Tauri shell glue** — the Xcode project, bundle resources, and a one-line override that forces the embedded backend on iOS

Everything else — the world graph, NPC tiers, conversation log, anachronism
filter, save/load, time advancement — already compiles for `aarch64-apple-ios`
today.

## The Inference Replacement

This is the only hard problem. Everything else is plumbing.

### Why Ollama can't ship to iOS

Parish's inference layer today (`crates/parish-inference/src/`) assumes a
desktop OS that can spawn processes and host a multi-GB model server:

- `setup.rs` shells out to `Command::new("ollama")` to bootstrap and pull models, and runs `nvidia-smi` / `rocm-smi` for GPU detection
- `client.rs` wraps an `OllamaProcess` (`std::process::Child`) for lifecycle management
- `openai_client.rs` sends HTTP requests to a local OpenAI-compatible endpoint
- `lib.rs` exposes an `AnyClient` enum (`OpenAi` | `Simulator`) which both `InferenceClients` and `spawn_inference_worker` consume by concrete type

iOS forbids subprocess spawning. There is no localhost daemon to talk to. The
HTTP path can't reach anywhere useful. The entire bootstrap layer is
unusable.

### The trait

Introduce an `InferenceBackend` trait in `crates/parish-inference/src/`
mirroring the three async methods every current caller already uses on
`OpenAiClient` / `SimulatorClient` / `AnyClient`. Keep `temperature` in the
surface — it's already threaded through the real signatures:

```rust
#[async_trait::async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError>;

    async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::UnboundedSender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError>;

    /// Default impl calls `generate` and `serde_json::from_str`; backends
    /// that support native structured output (OpenAI JSON mode) override it.
    async fn generate_json_raw(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        self.generate(model, prompt, system, max_tokens, temperature).await
    }
}
```

A free function `generate_json::<T>` layered on top of `generate_json_raw`
keeps the generic-over-`T` convenience without polluting the `dyn`-safe
trait.

**`InferenceClients` and `spawn_inference_worker` move to `Box<dyn InferenceBackend>`, not generics.** `InferenceClients` today is a
collection of per-category overrides (`interactive`, `background`, `batch`,
…): a single monomorphized `T` would force every slot to be the same
concrete type, which collapses the point of the struct. Dynamic dispatch
also lets `parish-tauri` swap backends at runtime for non-iOS (env-driven)
modes without re-parameterizing the rest of the worker. The per-call cost
is negligible against network or on-device inference latency.

**Delete the `AnyClient` enum.** The existing arms (`OpenAiClient`,
`SimulatorClient`) each `impl InferenceBackend` directly. Every call site
that names `AnyClient` today gets replaced with `Box<dyn InferenceBackend>`
or `Arc<dyn InferenceBackend>` as ownership demands.

Concrete call sites to update (from a repo sweep):

- `crates/parish-inference/src/lib.rs` — definition of `AnyClient`, `InferenceClients`, `spawn_inference_worker`
- `crates/parish-tauri/src/lib.rs` — `build_client_from_env` and `build_cloud_client_from_env` (both construct `AnyClient`); setup hook that wires `InferenceClients`
- `crates/parish-tauri/src/commands.rs` — dynamic `InferenceClients` rebuild path when the user changes provider at runtime
- `crates/parish-cli/` — any direct `AnyClient` use
- `crates/parish-server/` — any direct `AnyClient` use

The worker's queue / log / streaming machinery does not change — only the
type of the `client` field and of each `InferenceClients` slot.

The existing `OpenAiClient` becomes one impl (HTTP path, used by every
non-iOS mode). `SimulatorClient` becomes another. A new `LiteRtLmClient`
becomes the third (embedded path), gated behind an `ios-inference` Cargo
feature on `parish-inference`. No mode gets all at once: the iOS-specific
backend is compile-time.

### The embedded backend

The v1 option is **LiteRT-LM via a thin C shim, statically linked into
`parish-inference`.** Google positions LiteRT-LM as the production-ready
on-device LLM runtime for Android/iOS/web/desktop, and specifically ships
Gemma 4 E2B/E4B edge variants with iOS GPU acceleration.

Web-validated references (checked April 16, 2026):

- LiteRT-LM README: Gemma 4 support and cross-platform (including iOS) positioning — <https://github.com/google-ai-edge/LiteRT-LM>
- Google Developers blog benchmark post: iOS GPU decode numbers for Gemma4 E2B/E4B — <https://developers.googleblog.com/en/bringing-agentic-ai-to-edge-devices-with-gemma-3n/>
- Google AI Edge iOS LLM guide: older MediaPipe API is deprecated in favor of LiteRT-LM — <https://ai.google.dev/edge/mediapipe/solutions/genai/llm_inference/ios>

Alternatives considered for v1 and rejected:

- **`llama.cpp` + GGUF** — still viable fallback, but no longer the primary plan now that Gemma 4 has first-party LiteRT-LM packaging and published iOS GPU numbers.
- **Apple MLX** — solid Apple-Silicon backend, but its Rust story is immature and it would force a Swift-side inference path with a second IPC hop.

### Build + FFI

The workspace has **no existing C/C++ FFI today** (only `tauri_build::build()`
in `parish-tauri`'s `build.rs`), so this is greenfield. Commit to:

- **Source vendoring:** LiteRT-LM pinned as a git submodule at `crates/parish-inference/vendor/litert-lm/`. Submodule pinning is preferred over `cmake` `FetchContent` because the workspace has no precedent for network fetches in `build.rs`.
- **Bridge language:** a thin **C** (not C++) shim at `crates/parish-inference/vendor/bridge/litert_lm_bridge.{h,cc}`. C ABI avoids name mangling and keeps `bindgen` trivial. The `.cc` file is the only C++ in the tree; it is compiled with `-fno-exceptions -fno-rtti` and linked with the LiteRT-LM static library.
- **Shim surface** — five entry points over an opaque `LiteRtLmHandle*`:

  ```c
  // Returns NULL on failure; call litert_lm_last_error() for details.
  LiteRtLmHandle* litert_lm_create(const char* model_path);
  void litert_lm_destroy(LiteRtLmHandle*);

  // Non-streaming: fills out_buf up to out_cap, writes bytes-written to out_len.
  // Returns 0 on success, non-zero error code otherwise.
  int litert_lm_generate(
      LiteRtLmHandle*, const char* system, const char* prompt,
      uint32_t max_tokens, float temperature,
      char* out_buf, size_t out_cap, size_t* out_len);

  // Streaming: pull model. Call _start, then _next in a loop until EOS.
  int litert_lm_stream_start(
      LiteRtLmHandle*, const char* system, const char* prompt,
      uint32_t max_tokens, float temperature);
  // Returns 0 = token available in out_buf/out_len,
  //         1 = EOS (stream complete),
  //        <0 = error.
  int litert_lm_stream_next(
      LiteRtLmHandle*, char* out_buf, size_t out_cap, size_t* out_len);

  // Thread-local last error message (owned by the library).
  const char* litert_lm_last_error(void);
  ```

- **Rust side:** `bindgen` in a new `crates/parish-inference/build.rs` generates bindings from the shim header. The `cc` crate compiles the shim. Both are gated on `cfg(feature = "ios-inference")`. `parish-tauri`'s `build.rs` stays untouched.
- **Rust wrapper** at `crates/parish-inference/src/litert_lm_client.rs` holds an `Arc<Mutex<NonNull<LiteRtLmHandle>>>` (the underlying runtime is not `Sync`) and implements `InferenceBackend`. The wrapper owns the `CString` for the model path for the lifetime of the handle.
- **Async/sync bridge:** streaming runs on `tokio::task::spawn_blocking`. Inside the blocking task, a loop calls `litert_lm_stream_next` and forwards each token through the existing `mpsc::UnboundedSender<String>` — matching the contract `spawn_inference_worker` already consumes. Non-streaming `generate` also runs on `spawn_blocking` (a single call) to avoid parking the Tokio runtime.
- **Error mapping:** any non-zero status code from the shim yields `ParishError::Inference(String)` populated from `litert_lm_last_error()`. Null-handle creation failures yield `ParishError::Setup(String)` to match existing conventions.
- **Model-path ownership:** `LiteRtLmClient::new(path: &Path) -> Result<Self, ParishError>` clones the path into an owned `CString` and passes it to `litert_lm_create`. The model file must outlive the client; this is the caller's responsibility. On iOS the caller is the Tauri setup hook, which reads the path from the `PARISH_MODEL_PATH` env var (set by the Swift layer after ODR resolves the tag).

### Model choice

Use **Gemma 4 edge variants** as the default local model family:

| Model         | Size on disk | iOS 17 Pro GPU decode | Notes |
|---------------|--------------|------------------------|-------|
| Gemma4-E2B-it | 2.58 GB      | ~56–57 tok/s           | Best default for responsiveness + memory |
| Gemma4-E4B-it | 3.65 GB      | ~25 tok/s              | Better quality; higher memory/thermal pressure |

Both are explicitly benchmarked by Google AI Edge on iOS GPU. Start with E2B
as the shipping default and keep E4B as an opt-in "high quality" setting for
latest Pro-class devices.

The current `qwen3:14b`-tuned prompts in
`mods/rundale/prompts/tier1_system.txt` will need a revision pass for
the smaller model — expect more rigid scaffolding, fewer open-ended
instructions, more concrete examples. The existing tiered prompt system
(`tier1_system.txt`, `tier1_context.txt`, `tier2_system.txt`) is exactly the
right shape for this; nothing about the file layout needs to change.

### Cleanup

`#[cfg(not(target_os = "ios"))]`-gate everything in
`crates/parish-inference/src/setup.rs` (Ollama bootstrap, GPU probe, all
`Command::new` paths) and the `OllamaProcess` lifecycle wrapper in
`crates/parish-inference/src/client.rs`. With the trait in place,
`spawn_inference_worker` no longer cares which backend it's holding, so the
gating is local to those two files.

## The Tauri 2 iOS Shell

Tauri 2 supports iOS out of the box. Rundale just hasn't enabled it. Most of
the prep work has, however, already happened organically.

### What's already done

- `crates/parish-tauri/Cargo.toml` already isolates `gdk` and `glib` behind `[target.'cfg(target_os = "linux")'.dependencies]`. iOS builds skip them automatically.
- `crates/parish-tauri/src/lib.rs` already gates `capture_gdk_screenshot` and `dispatch_screenshot` behind `#[cfg(target_os = "linux")]` with `bail!` stubs for other targets.
- `tauri.conf.json` already uses identifier `ie.parish.app`, which is reusable for iOS.
- `apps/ui/` already builds to a static bundle via the SvelteKit static adapter, which is exactly what `WKWebView` wants.

### What's left

- Run `cargo tauri ios init` from `crates/parish-tauri/` to generate the Xcode project under `crates/parish-tauri/gen/apple/`. Today only `gen/schemas/` exists.
- `tauri.conf.json` currently has `"bundle.targets": "all"` (a string). Change the schema to an explicit array and include `"iOS"`:

  ```jsonc
  "bundle": {
    "active": true,
    "targets": ["deb", "appimage", "nsis", "app", "dmg", "iOS"],
    "icon": ["icons/icon.png"],
    "resources": ["../../mods/rundale/**"]
  }
  ```

  Add iOS icons and Info.plist entries (no camera/mic usage strings needed; the game requires neither).
- In `crates/parish-tauri/src/lib.rs`, `build_client_from_env` reads the `PARISH_PROVIDER` env var and dispatches between Ollama, OpenAI-compatible cloud providers, and the simulator. On `target_os = "ios"`, short-circuit that function to construct a `LiteRtLmClient` from `PARISH_MODEL_PATH` regardless of env. `build_cloud_client_from_env` is desktop-only and is `cfg`-gated out.
- `crates/parish-tauri/src/commands.rs` contains a **second** `InferenceClients` construction path used when the user changes provider at runtime. Apply the same iOS override there (or better: collapse both call sites onto a single helper before adding the iOS branch, so the override lives in one place).
- The `--screenshot <dir>` CLI flag parsing in `crates/parish-tauri/src/lib.rs` is a desktop dev affordance. `#[cfg(not(target_os = "ios"))]`-gate the whole flag-parsing block (the iOS binary receives no command-line arguments).

### Touch input

`InputField.svelte` and `FullMapOverlay.svelte` already use pointer events,
so they should work on touch out of the box. Two things to verify on a real
device:

- Pinch-zoom on `MapPanel.svelte` and `FullMapOverlay.svelte`
- That the on-screen keyboard doesn't cover the input field

`apps/ui/src/app.css` has zero `env(safe-area-inset-*)` rules today (verified
via grep). Add at minimum `env(safe-area-inset-bottom)` to the input bar and
`env(safe-area-inset-top)` to the status bar / chat panel header. This is the
standard iOS safe-area dance.

## Persistence and Asset Bundling

### Save files

`rusqlite` with the `bundled` feature already cross-compiles to iOS, so the
database layer in `crates/parish-persistence/src/database.rs` needs zero
changes.

The change is in `crates/parish-persistence/src/picker.rs::ensure_saves_dir`,
which today hard-codes a relative path:

```rust
pub fn ensure_saves_dir() -> PathBuf {
    let saves_dir = PathBuf::from(SAVES_DIR);  // const SAVES_DIR = "saves"
    std::fs::create_dir_all(&saves_dir).ok();
    ...
}
```

A relative path is fine on desktop and the headless CLI but meaningless on
iOS — the app's working directory is not where its writable storage lives.

**Commit:** change `ensure_saves_dir` to accept an explicit base directory,
and resolve that base in `parish-tauri` via `tauri::Manager::path().app_data_dir()`.

```rust
// parish-persistence
pub fn ensure_saves_dir(base: &Path) -> PathBuf {
    let saves_dir = base.join(SAVES_DIR);
    std::fs::create_dir_all(&saves_dir).ok();
    // ... legacy migration stays here, but walks relative to `base`
    saves_dir
}
```

Call-site handling:

- **`parish-cli`** passes `Path::new(".")` — preserves today's relative-`saves/` behaviour.
- **`parish-tauri` (desktop + iOS)** passes `app_handle.path().app_data_dir()?`. On desktop this resolves to the OS-native app-data directory (XDG on Linux, `Application Support` on macOS, `%APPDATA%` on Windows); on iOS it resolves to the sandboxed Application Support directory. One mechanism, no iOS-only branch.
- **`parish-server`** passes its existing configured data directory.

This avoids introducing the `dirs` crate (which does not resolve a useful
iOS sandbox path) and keeps `parish-persistence` free of any `tauri`
dependency. Every other persistence function already takes an explicit
`&Path`, so no other signatures change.

### Mod assets

`mods/rundale/` is currently located via `parish_core::game_mod::find_default_mod`,
which walks up from `std::env::current_dir()` looking for a `mods/rundale/mod.toml`.
On iOS the app sandbox has no concept of "the directory the binary was launched
from", so `current_dir()` is useless and the mod has to ship as a Tauri
*resource*:

- Add `"../../mods/rundale/**"` to `tauri.conf.json` → `bundle.resources` (shown in §"What's left" above).
- In `parish-tauri/src/lib.rs`, replace the `find_default_mod()` / `GameMod::load(&dir)` pairing in the Tauri startup hook with `app_handle.path().resolve("mods/rundale", BaseDirectory::Resource)?` before calling `GameMod::load`. Keep `find_default_mod` untouched for the CLI and server binaries.
- The resolver returns the correct on-disk path on every platform — desktop reads from the dev directory, iOS reads from inside the app bundle.

Other callers of `GameMod::load` to audit (from a repo sweep):
`parish-tauri/src/commands.rs`, `parish-tauri/src/ipc/handlers.rs`,
plus the CLI entry point. Only the Tauri entry points need the resource-resolver
change; the CLI keeps its existing discovery.

The mod is small (~13 files, hundreds of KB) so bundling it has no meaningful
size impact.

## What Gets Dropped on iOS

- The Ollama auto-installer and GPU probe in `crates/parish-inference/src/setup.rs`
- The `OllamaProcess` lifecycle wrapper in `crates/parish-inference/src/client.rs`
- The `AnyClient` enum — replaced by the `InferenceBackend` trait for every mode
- The Axum web-server mode (`crates/parish-server/`) — not built for iOS. The iOS build only touches `parish-tauri`, `parish-core`, `parish-inference`, `parish-persistence`, so `parish-server` and `parish-cli` are excluded naturally. No Cargo manifest surgery required.
- The `--screenshot <dir>` flag parsing in `crates/parish-tauri/src/lib.rs` (a desktop dev affordance)

## Model Download UX

**Don't ship the model in the IPA.** A 2 GB binary in an IPA pushes the app
over the Apple cellular-download limit and bloats the initial download for
users who would be fine waiting for the model to fetch later.

**Commit: On-Demand Resources (ODR)** — tag `.litertlm` model variants in the
Xcode project and request them at runtime. They don't count against the
initial IPA size, Apple handles CDN + retry, and the tag-download flow is
already well-understood territory for App Store review. Reject a bespoke
`URLSession` + self-hosted-CDN path for v1 on the grounds that it requires
standing up hosting, implementing resume, and duplicating what ODR already
gives us for free.

Flow:

1. **Swift layer on cold start** runs `NSBundleResourceRequest` with the tag for the selected model tier (see thresholds below). Shows the progress UI while the download runs.
2. **On success**, Swift resolves the bundle URL (`Bundle.main.url(forResource:withExtension:)` for the tag's resource), writes it to the process environment as `PARISH_MODEL_PATH`, and proceeds with Tauri init.
3. **Rust setup hook** reads `PARISH_MODEL_PATH` and constructs `LiteRtLmClient::new(path)`. If the env var is missing or the file is unreadable, surface `ParishError::Setup` and return to the Swift layer so it can retry the ODR request.
4. **On ODR failure**, Swift retries once; on second failure, it blocks at the loading screen with a user-visible error and an explicit "Retry" button. No silent fallback.

Device tiering (evaluated once, persisted in `UserDefaults`):

- `ProcessInfo.processInfo.physicalMemory >= 8 * 1024 * 1024 * 1024` **and** device model is iPhone 15 Pro or newer → **Gemma4-E4B** (3.65 GB, ~25 tok/s).
- Otherwise → **Gemma4-E2B** (2.58 GB, ~56–57 tok/s).
- Pre-iPhone-15-Pro hardware is out of scope; do not attempt to run.

Show a one-time "Downloading parish brain (~2 GB)" screen on first launch.
Reuse the existing `LoadingAnimation` from `crates/parish-core/src/loading.rs`
for visual continuity with the desktop boot experience.

## Q&A — Common Decisions

### Apple Developer account?

Yes, paid ($99/yr). Required for:

- Installing on a physical device beyond the 7-day free-provisioning window
- TestFlight
- Submitting to the App Store

A free Apple ID is enough to run the app in the simulator, but the simulator
can't meaningfully test on-device LLM perf (see below), so plan on the paid
account from day one.

### Should we use the Apple Neural Engine (ANE)?

Skip for v1.

LiteRT-LM's current iOS path is GPU-first and already publishes strong Gemma 4
decode throughput on iPhone-class hardware, which is enough for Rundale's
tier-1 dialogue UX. ANE-specific tuning would require a separate Core ML-first
inference path and a different model packaging workflow. Defer to v2; only
revisit if battery or thermal measurements demand it.

### iPhone Simulator?

Useful for some things, not for others.

**Use the simulator for:**

- Svelte layout work
- Safe-area inset CSS
- Tauri IPC plumbing
- Save/load round-trip testing
- Touch event sanity checks

**Do not use the simulator for:**

- Tokens/sec measurement
- Memory headroom
- Thermal / battery behaviour

The iOS Simulator runs x86_64/arm64 macOS code under the hood — it does not
emulate the A-series GPU, has no Metal Performance Shaders parity with a
real device, and can't represent real memory pressure or thermal throttling.
Tokens/sec, RSS, and battery measurements **must** happen on a physical
iPhone 15 Pro or newer. Which is the other reason the paid developer
account is non-negotiable.

## Shipping Process

The technical work above is only half the story. The other half is Apple's
distribution machinery, which is mostly a one-time tax but rewards knowing
what's coming.

### Prerequisites

- **A Mac with Xcode.** The iOS toolchain only runs on macOS. `xcode-select --install` for the command-line tools.
- **Apple Developer Program enrollment** ($99/yr) at developer.apple.com. 24–48 hours for individual accounts; longer for company accounts (DUNS number required). A free Apple ID is enough for the simulator and a 7-day on-device install, but everything else (real signing, TestFlight, App Store) needs the paid program.

### Inner loop: simulator

`cargo tauri ios dev` builds the app, opens the iOS simulator, and rebuilds
on change. No signing required. Fast iteration. Use this for layout, IPC,
save/load, touch event sanity checks. Do **not** use this for inference
benchmarking — see the [iPhone Simulator Q&A](#iphone-simulator) above.

### Inner loop: physical device

Two paths.

**Free path (7-day expiry):** plug the iPhone into the Mac, sign in with an
Apple ID under Xcode → Settings → Accounts, pick the personal team in the
project's Signing & Capabilities tab, and let Xcode auto-generate a
provisioning profile. First run requires trusting the developer certificate
on the phone (Settings → General → VPN & Device Management). The app
expires after 7 days and must be reinstalled. Good enough to verify Tauri +
WKWebView + Svelte + touch input work end to end.

**Paid path (1-year expiry):** same flow but pick the paid Developer team.
Builds last a year and survive reboots.

Under the hood every iOS build is signed with a **certificate** (proves
*who* built it) and bundled with a **provisioning profile** (proves *what
device* and *what entitlements* it has). Xcode hides this when "Automatically
manage signing" is on, which you should leave on until you have a reason
not to.

### Beta distribution: TestFlight

TestFlight is Apple's official beta channel — the way to get builds onto
other people's phones without going through full App Store review.

1. `cargo tauri ios build --release` produces a signed `.ipa` (or use Xcode's Product → Archive)
2. Upload via Xcode's Organizer or `xcrun altool` / `fastlane pilot`
3. The build appears in **App Store Connect** under TestFlight after a few minutes of processing
4. Add testers:
   - **Internal testers** (up to 100, must be on the dev team) — instant access, no review
   - **External testers** (up to 10,000, anyone with an email) — requires a one-time "beta app review" by Apple, usually under 24 hours
5. Testers install the **TestFlight app** from the App Store, accept the invite, and get builds. Updates push automatically.

TestFlight builds expire after 90 days; upload a new build to refresh. This
is how Rundale should be distributed to playtesters before any App Store
submission.

### CI

Three viable options:

| Option                          | Pros                                              | Cons                                                          |
|---------------------------------|---------------------------------------------------|---------------------------------------------------------------|
| **Xcode Cloud**                 | First-party, integrated with App Store Connect; generous free tier (25 hr/month) | Less flexible than YAML-based CI; locks you into Apple        |
| **GitHub Actions** (`macos-latest`) | Familiar, flexible YAML, plays well with the rest of Rundale CI | macOS minutes burn ~10× faster than Linux; ~5–15 min per build |
| **Self-hosted Mac**             | Cheapest at scale; fast                           | Babysit the machine; certificate management is on you         |

A typical iOS CI pipeline:

```
1. Checkout
2. Install Ruby + bundler + fastlane
3. Set up signing (decrypt certs from a private repo via fastlane match)
4. Build & test:           cargo tauri ios build (or xcodebuild test ...)
5. Build for distribution: cargo tauri ios build --release
6. Upload to TestFlight:   fastlane pilot upload
```

The signing-in-CI part is the gnarly bit. The clean answer is
**[`fastlane match`](https://docs.fastlane.tools/actions/match/)**, which
stores certificates and provisioning profiles in a private (encrypted) git
repo. Any CI machine runs `fastlane match` to fetch them. Without `match`
you end up doing manual keychain dances in CI that break every time Apple
rotates a certificate.

For Rundale specifically: the existing GitHub Actions workflows for
desktop/CLI tests should not change. Add a separate `ios-build.yml`
workflow that runs on `macos-latest`, gated with path filters to only run
on PRs that touch `crates/parish-tauri/**`, `crates/parish-inference/**`,
`crates/parish-core/**`, `crates/parish-persistence/**`, `apps/ui/**`, or
the workflow itself. This avoids burning macOS minutes on unrelated PRs.

### App Store submission

Once a TestFlight build is solid:

1. In **App Store Connect**, create the app listing (bundle ID `ie.parish.app`, name, primary language, SKU)
2. Fill out metadata: description, keywords, support URL, marketing URL, age rating questionnaire, category, pricing
3. Upload **screenshots** for every required device size (currently 6.7" and 6.5" iPhone are mandatory; iPad if supported). Apple is strict about pixel dimensions.
4. Provide a **privacy policy URL** and complete the **App Privacy** disclosures. A fully on-device Rundale should be a clean "no data collected" declaration, which is the easy case.
5. Pick a build from TestFlight as the release candidate
6. Submit for review

App Review reality:

- Typical turnaround is **24–48 hours**, occasionally same-day for small updates
- Roughly 30–40% of first submissions get rejected. Common reasons: crashes the reviewer triggered, missing privacy disclosures, in-app purchases that bypass StoreKit, "minimum functionality" rejections for thin apps, broken links in metadata
- Rejection comes with a written reason; fix and resubmit (no fee)
- After approval, release immediately or schedule a date
- **Phased releases** roll out to a percentage of users over 7 days — use this to catch crashes early
- **Expedited review** is available a few times a year for genuine emergencies

### Operational gotchas

- **You can never test "what App Store users see" before submitting.** TestFlight uses the same binary but a different distribution path; some bugs only surface in production builds.
- **Provisioning profiles expire** every year, or whenever a certificate rotates. Builds that worked yesterday will fail today with `No matching provisioning profiles found`. Budget time for this every ~12 months.
- **Apple's review guidelines change.** A pattern that was fine last submission can be a rejection reason on the next one. Re-read the [App Store Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) before any submission after a long gap.
- **Bundle identifiers are forever.** Once `ie.parish.app` ships to the store, it can't be changed without releasing a new app and migrating saves manually.
- **Release builds are slow.** A clean Rust + Tauri release build is 10–20 minutes on a Mac mini. CI minutes add up fast.
- **iOS aggressively kills backgrounded apps with high RAM usage.** Rundale + a 3 GB resident model is exactly the kind of app iOS will reap. The session-resume path (load from latest snapshot on cold start) needs to be fast and reliable.

## Risks

- **LLM quality on a 3B model is the dominant risk.** Current prompts and the anachronism pipeline (`crates/parish-npc/src/anachronism.rs`) were tuned against a 14B model. Expect a real prompt-engineering pass and possibly more frequent fallback to Tier-2 cognition. This is the only piece that can't be derisked just by writing the integration — it has to be measured against real player conversations.
- **Memory pressure.** Gemma4-E2B plus `WKWebView` + game state should be manageable on 8 GB devices, but E4B can push thermal and memory headroom. Keep pre-iPhone-15-Pro hardware out of scope.
- **App Store review.** Bundling a multi-GB model is allowed but pushes the IPA over the cellular-download limit. ODR sidesteps this but adds a first-launch download-screen requirement that reviewers will check.
- **Tauri iOS maturity.** `WKWebView` quirks (no `eval`, stricter CSP) sometimes bite Svelte apps. Budget time for a shakedown pass. `tauri.conf.json` currently sets `security.csp: null`, which simplifies dev but may need tightening for App Store review.
- **Background termination.** iOS aggressively kills backgrounded apps with high RAM usage. The session-resume path (load from latest snapshot) needs to be fast and reliable on a cold start.

## Migration Order

An explicit sequence so a single implementation pass can execute end-to-end
without partial-state breakage. Each step leaves the tree green on the
existing desktop/CLI/web targets before moving on.

1. Introduce `InferenceBackend` trait in `crates/parish-inference/src/lib.rs`.
2. `impl InferenceBackend for OpenAiClient` (override `generate_json_raw` for native JSON mode) and `impl InferenceBackend for SimulatorClient`.
3. Delete `AnyClient`. Replace every occurrence with `Box<dyn InferenceBackend>` (or `Arc<dyn …>` where shared). Update `InferenceClients` and `spawn_inference_worker` signatures accordingly. Touch both construction sites in `parish-tauri` (`src/lib.rs` and `src/commands.rs`) — consider collapsing them onto a single helper first.
4. Add the `ios-inference` Cargo feature to `crates/parish-inference/Cargo.toml` with the LiteRT-LM C-shim build deps (`bindgen` build-dep, `cc` build-dep, `async-trait`).
5. Vendor LiteRT-LM as a submodule at `crates/parish-inference/vendor/litert-lm/`. Add the C shim at `vendor/bridge/`. Add the `crates/parish-inference/build.rs` that compiles the shim when the feature is on.
6. Add `crates/parish-inference/src/litert_lm_client.rs` implementing `InferenceBackend` via the shim + `spawn_blocking`.
7. `#[cfg(not(target_os = "ios"))]`-gate Ollama bootstrap (`setup.rs`) and `OllamaProcess` (`client.rs`).
8. In `parish-tauri`, branch the setup hook on `target_os = "ios"` to build a `LiteRtLmClient` from `PARISH_MODEL_PATH` instead of calling `build_client_from_env`.
9. Change `ensure_saves_dir` to `ensure_saves_dir(base: &Path)`. Update every call site (CLI, Tauri, server) to pass its platform base. In `parish-tauri`, resolve the base via `app_handle.path().app_data_dir()`.
10. Add `../../mods/rundale/**` to `bundle.resources` in `tauri.conf.json`. In the Tauri setup hook, replace `find_default_mod()` with `app_handle.path().resolve("mods/rundale", BaseDirectory::Resource)`.
11. `cargo tauri ios init` → commit the generated Xcode project under `crates/parish-tauri/gen/apple/`.
12. Update `bundle.targets` to the array form including `"iOS"`; add iOS icon assets.
13. `env(safe-area-inset-*)` pass in `apps/ui/src/app.css` (at minimum the input bar and the status bar / chat panel header).
14. Add ODR tag entries for the two model tiers to the Xcode project. Write the Swift bootstrapper that resolves the tag, populates `PARISH_MODEL_PATH`, and shows the first-launch download UI.
15. Add `.github/workflows/ios-build.yml` — macOS runner, path-filtered, `fastlane match` for signing, `cargo tauri ios build --release` + `fastlane pilot upload` on `main`.
16. Prompt-tuning pass for Gemma4-E2B on `mods/rundale/prompts/{tier1_system,tier1_context,tier2_system}.txt`.

## Prerequisites for Execution

This design cannot be implemented end-to-end by a headless automated agent.
The following are required and are not available from CI-only or Linux-only
contexts:

- **A Mac with Xcode + command-line tools.** Steps 11–15 and all on-device testing require it.
- **A physical iPhone 15 Pro or newer.** The simulator is unfit for tokens/sec, memory, and thermal measurement (see [iPhone Simulator Q&A](#iphone-simulator)).
- **A paid Apple Developer Program account ($99/yr).** Required for TestFlight, stable signing, and App Store submission.
- **An ODR-tagged model build pipeline.** Gemma4-E2B and E4B `.litertlm` files must be placed into the Xcode project with ODR tags before a first-launch download will resolve.
- **An iterative prompt-tuning loop.** Retuning the tier-1 prompts for a 3B-class model is measurement-driven and cannot be one-shot.

A headless run can complete steps 1–10 and 16 on its own — the trait
refactor, the FFI scaffolding (compiles and links when the feature is
off), the save-path and mod-resource plumbing, and the prompt edits.
Everything past that requires a human with the hardware above.

## Verification Plan

When this design is implemented:

1. `cargo build --target aarch64-apple-ios --features ios-inference -p parish-inference` — embedded backend compiles for the device.
2. `cargo build --target aarch64-apple-ios -p parish-core -p parish-persistence` — shared crates compile for the device.
3. `cd crates/parish-tauri && cargo tauri ios build` — produces an `.ipa`.
4. **Install on a physical iPhone 15 Pro or newer** (the simulator can't represent real on-device GPU LLM perf) and smoke-test:
   - Start a new game, walk between two locations, talk to an NPC. Confirm token streaming arrives in `apps/ui/src/components/ChatPanel.svelte`.
   - Save, kill the app from the multitasker, relaunch, load. Confirm `rusqlite` round-trips through the iOS sandbox path resolved by `ensure_saves_dir`.
   - Confirm the anachronism filter still fires, the conversation log persists across turns, and time advances.
5. **Measure on-device:**
   - Tokens/sec for tier-1 dialogue (target ≥15 t/s)
   - Peak RSS over a 10-minute session
   - Battery drain over a 10-minute session
   - Thermal state at the end of the session
6. **Confirm no desktop regressions** from the trait refactor:
   - `cd apps/ui && npx vitest run`
   - `cargo test -p parish-core -p parish-inference -p parish-persistence`
   - `just check` and `just verify` (CLI / Tauri desktop / web server still build and pass)

## Implementation Spec (Execution-Ready)

This section turns the design above into a concrete v1 implementation contract so
the work can be delivered in one pass.

### Locked v1 decisions

- **Inference runtime:** LiteRT-LM (no `llama.cpp` fallback in v1)
- **Model delivery:** **On-Demand Resources (ODR)** (no custom CDN path in v1)
- **Default model:** `Gemma4-E2B-it` (E4B as opt-in device-gated setting)
- **Streaming UX:** token-streaming required (no "wait for full response" fallback)
- **Target devices:** iPhone 15 Pro / 16-class and newer only for launch

### Rust ↔ C++ FFI contract (normative)

Add a C ABI bridge consumed by `crates/parish-core/src/inference/litert_lm_client.rs`.

#### Ownership and lifetime

- Rust owns request buffers and passes UTF-8 pointers + lengths into C.
- C++ must copy request text before returning from the FFI call.
- C++ owns model/session state behind an opaque handle.
- Rust owns the opaque handle pointer and must call `parish_litert_free` exactly once.
- All FFI functions return a status code; failures are mapped to `ParishError`.

#### Threading

- `parish_litert_generate*` calls are serialized per model handle.
- Creating multiple handles is allowed, but v1 uses a single shared handle.
- Streaming callbacks may be invoked on a LiteRT worker thread; callback
  implementation must be `Send + 'static` and non-blocking.

#### C ABI surface

```c
typedef struct ParishLiteRtHandle ParishLiteRtHandle;

typedef enum {
  PARISH_LITERT_OK = 0,
  PARISH_LITERT_INVALID_ARG = 1,
  PARISH_LITERT_MODEL_LOAD_FAILED = 2,
  PARISH_LITERT_INFERENCE_FAILED = 3,
  PARISH_LITERT_CANCELLED = 4,
  PARISH_LITERT_INTERNAL = 255
} ParishLiteRtStatus;

typedef void (*ParishTokenCallback)(const char* token_utf8,
                                    size_t token_len,
                                    void* user_data);

typedef int (*ParishCancelCallback)(void* user_data); // non-zero => cancel

ParishLiteRtStatus parish_litert_new(const char* model_path_utf8,
                                     size_t model_path_len,
                                     ParishLiteRtHandle** out_handle);

ParishLiteRtStatus parish_litert_generate(ParishLiteRtHandle* handle,
                                          const char* system_utf8,
                                          size_t system_len,
                                          const char* prompt_utf8,
                                          size_t prompt_len,
                                          uint32_t max_tokens,
                                          char** out_text_utf8,
                                          size_t* out_text_len);

ParishLiteRtStatus parish_litert_generate_stream(
    ParishLiteRtHandle* handle,
    const char* system_utf8,
    size_t system_len,
    const char* prompt_utf8,
    size_t prompt_len,
    uint32_t max_tokens,
    ParishTokenCallback on_token,
    ParishCancelCallback should_cancel,
    void* user_data,
    char** out_text_utf8,
    size_t* out_text_len);

void parish_litert_string_free(char* s);
void parish_litert_free(ParishLiteRtHandle* handle);
```

#### Error mapping in Rust

- `INVALID_ARG` -> `ParishError::InvalidInput`
- `MODEL_LOAD_FAILED` -> `ParishError::Config`
- `INFERENCE_FAILED` / `INTERNAL` -> `ParishError::Inference`
- `CANCELLED` -> `ParishError::Inference` with `"cancelled"` marker text

`generate_stream` must always return the final accumulated text, even if token
callbacks were emitted.

### ODR model download spec (normative)

Use ODR only in v1 with two asset packs:

- tag `model-e2b` => `Gemma4-E2B-it.litertlm`
- tag `model-e4b` => `Gemma4-E4B-it.litertlm`

#### Selection rule

At first launch:

1. Probe device memory class via `ProcessInfo.processInfo.physicalMemory`.
2. Default to `model-e2b`.
3. Offer `model-e4b` only on devices with >= 8 GB RAM and A17 Pro / newer.

#### First-launch state machine

`NotStarted -> Downloading -> Verifying -> Ready -> Failed`

- **Downloading:** show progress UI with bytes downloaded.
- **Verifying:** check file exists, non-zero size, and extension `.litertlm`.
- **Ready:** persist selected model tag + resolved absolute path.
- **Failed:** show retry action and keep app unusable for gameplay until model is ready.

#### Retry/backoff

- Automatic retries: 3 attempts (1s, 3s, 10s)
- Then surface manual "Retry download" control.

### Persistence path contract

Implement `saves_dir()` in `parish-core`:

- non-iOS: unchanged relative `saves/`
- iOS: app Application Support directory + `/saves`

`ensure_saves_dir()` must:

1. Resolve `saves_dir()`
2. `create_dir_all`
3. Return absolute path
4. Log resolved path at `info` level once at startup

### UI acceptance criteria (must pass)

- Input bar is never occluded by the software keyboard in portrait mode.
- Status/header and bottom input respect iOS safe area insets.
- Pinch zoom works on map panel and full map overlay.
- Chat token streaming remains progressive (new tokens visible within 250 ms of emission).

### Performance SLOs and fail criteria

On physical iPhone 15 Pro (release build):

- Tier-1 dialogue decode throughput: **>= 15 tokens/sec** (median over 5 prompts)
- Peak RSS over 10-minute session: **< 6.5 GB**
- Battery drain over 10-minute scripted session: **<= 8%**
- No jetsam termination during 10-minute session + one background/foreground cycle

If any fail criterion is missed, do not ship v1 without an explicit waiver.

### Prompt-retuning done definition

`mods/rundale/prompts/tier1_system.txt` update is complete when:

1. 20-turn scripted conversation corpus passes with no malformed outputs.
2. Anachronism filter triggers on all known fixture prompts.
3. NPC response coherence is rated "acceptable or better" in at least 16/20 turns by two reviewers.

Store the scripted corpus and outcomes in `testing/fixtures/ios_prompt_eval/`.

## Files That Would Be Modified

Forward reference for whoever picks this up:

| Path                                                              | Change                                                          |
|-------------------------------------------------------------------|-----------------------------------------------------------------|
| `crates/parish-inference/src/lib.rs`                              | Introduce `InferenceBackend` trait; delete `AnyClient`; `InferenceClients` + `spawn_inference_worker` move to `Box<dyn InferenceBackend>` |
| `crates/parish-inference/src/openai_client.rs`                    | `impl InferenceBackend for OpenAiClient` (override `generate_json_raw` for native JSON mode) |
| `crates/parish-inference/src/simulator.rs`                        | `impl InferenceBackend for SimulatorClient`                     |
| `crates/parish-inference/src/litert_lm_client.rs` *(new)*         | Embedded LiteRT-LM backend behind `ios-inference` feature       |
| `crates/parish-inference/src/setup.rs`                            | `cfg`-gate Ollama bootstrap and GPU probe                       |
| `crates/parish-inference/src/client.rs`                           | `cfg`-gate `OllamaProcess`                                      |
| `crates/parish-inference/build.rs` *(new)*                        | `bindgen` + `cc` for the C shim, gated on `ios-inference`       |
| `crates/parish-inference/vendor/litert-lm/` *(new submodule)*     | Pinned upstream LiteRT-LM source                                |
| `crates/parish-inference/vendor/bridge/litert_lm_bridge.{h,cc}` *(new)* | Thin C shim over LiteRT-LM                                 |
| `crates/parish-inference/Cargo.toml`                              | `ios-inference` feature; `async-trait`, `bindgen` (build-dep), `cc` (build-dep) |
| `crates/parish-persistence/src/picker.rs`                         | `ensure_saves_dir(base: &Path)` — explicit base, no iOS branch in this crate |
| `crates/parish-tauri/tauri.conf.json`                             | `bundle.targets` → array with `"iOS"`; `bundle.resources` for the mod; iOS icons |
| `crates/parish-tauri/src/lib.rs`                                  | Resolve mod via Tauri resource API; pass `app_data_dir` to `ensure_saves_dir`; force embedded backend on `target_os = "ios"` via `PARISH_MODEL_PATH`; `cfg`-gate `--screenshot` parsing |
| `crates/parish-tauri/src/commands.rs`                             | Same iOS override at the dynamic `InferenceClients` rebuild path; unify both construction sites on one helper |
| `crates/parish-tauri/gen/apple/` *(generated)*                    | Xcode project from `cargo tauri ios init`                       |
| `mods/rundale/prompts/tier1_system.txt`                           | Slim down for 3B-class model                                    |
| `mods/rundale/prompts/tier1_context.txt`                          | Same; tighten scaffolding                                       |
| `mods/rundale/prompts/tier2_system.txt`                           | Same                                                            |
| `apps/ui/src/app.css`                                             | `env(safe-area-inset-*)` rules                                  |
| `.github/workflows/ios-build.yml` *(new)*                         | macOS runner, path-filtered triggers (`crates/parish-tauri/**`, `crates/parish-inference/**`, `apps/ui/**`, self); `fastlane match` for signing |
