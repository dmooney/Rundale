# iOS Port — Fully On-Device Rundale

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) |
> ADRs: [005 — Ollama Local Inference](../adr/005-ollama-local-inference.md), [014 — Web/Mobile Architecture](../adr/014-web-mobile-architecture.md), [016 — Tauri + Svelte GUI](../adr/016-tauri-svelte-gui.md) |
> Related: [Phase 7 — Web & Mobile](../plans/phase-7-web-mobile.md) (alternative thin-client design)
>
> **Status: Design** — no implementation work has started.

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
│                        crates/parish-core                               │
│  WorldGraph · NpcManager · GameClock · SimulationTiers · Persistence    │
│  InferenceClients (trait-based; impl chosen at compile time)            │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │ shared, unchanged across modes
   ┌───────────────────────┼─────────────────────────────────┐
   │                       │                                 │
   ▼                       ▼                                 ▼
┌─────────────┐  ┌──────────────────┐  ┌──────────────────────────────┐
│ parish-cli  │  │ parish-tauri     │  │ parish-tauri                 │
│ (headless)  │  │ desktop          │  │ iOS (new)                    │
│ Ollama HTTP │  │ Ollama HTTP      │  │ llama.cpp + Metal in-process │
└─────────────┘  └──────────────────┘  └──────────────────────────────┘
                 ┌──────────────────┐
                 │ parish-server    │
                 │ Axum + cloud LLM │
                 └──────────────────┘
```

iOS becomes a fourth mode alongside the headless CLI, Tauri desktop, and the
Axum web server. All four consume `crates/parish-core/` unchanged. The only
iOS-specific code lives in three places:

1. **The inference backend** — `llama.cpp` linked statically into the Rust core
2. **The save-path resolver** — iOS sandbox instead of relative `saves/`
3. **The Tauri shell glue** — the Xcode project, bundle resources, and a one-line override that forces the embedded backend on iOS

Everything else — the world graph, NPC tiers, conversation log, anachronism
filter, save/load, time advancement — already compiles for `aarch64-apple-ios`
today.

## The Inference Replacement

This is the only hard problem. Everything else is plumbing.

### Why Ollama can't ship to iOS

Parish's inference layer today (`crates/parish-core/src/inference/`) assumes a
desktop OS that can spawn processes and host a multi-GB model server:

- `setup.rs` shells out to `Command::new("ollama")` to bootstrap and pull models, and runs `nvidia-smi` / `rocm-smi` for GPU detection
- `client.rs` wraps an `OllamaProcess` (`std::process::Child`) for lifecycle management
- `openai_client.rs` sends HTTP requests to a local OpenAI-compatible endpoint

iOS forbids subprocess spawning. There is no localhost daemon to talk to. The
HTTP path can't reach anywhere useful. The entire bootstrap layer is
unusable.

### The trait

Introduce an `InferenceBackend` trait in `crates/parish-core/src/inference/`
exposing the two methods `spawn_inference_worker` actually calls today on
`OpenAiClient`:

```rust
trait InferenceBackend: Send + Sync {
    async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<String, ParishError>;

    async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::UnboundedSender<String>,
        max_tokens: Option<u32>,
    ) -> Result<String, ParishError>;
}
```

`InferenceClients` and `spawn_inference_worker` change to consume
`Box<dyn InferenceBackend>` (or are made generic over a concrete type). The
worker's queue / log / streaming machinery stays exactly as it is — only the
type parameter changes.

The existing `OpenAiClient` becomes one impl (HTTP path, used by every
non-iOS mode). A new `LlamaCppClient` becomes the other (embedded path),
gated behind a `ios-inference` Cargo feature on `parish-core`. No mode
gets both at once: the choice is made at compile time.

### The embedded backend

The realistic option is **llama.cpp via the [`llama-cpp-2`](https://crates.io/crates/llama-cpp-2)
Rust binding (or hand-rolled FFI), statically linked into the Rust core.**
It already supports Metal on Apple Silicon, runs quantized GGUF models, and
compiles cleanly for `aarch64-apple-ios`. `LlamaCppClient::new` takes the
filesystem path of a GGUF model so the Swift layer (or Rust startup code)
can hand it whichever file was downloaded for this device.

Alternatives considered for v1 and rejected:

- **Apple MLX** — solid Apple-Silicon backend, but its Rust story is immature and it would force a Swift-side inference path with a second IPC hop.
- **Core ML / Apple Neural Engine** — see [Q&A](#qa--common-decisions). Skip for v1.

### Model choice

A Q4_K_M quantization of a 3B–4B class instruction model:

| Model            | Size on disk | Resident RAM | Notes                          |
|------------------|--------------|--------------|--------------------------------|
| Qwen2.5-3B Q4    | ~1.9 GB      | ~3 GB        | Strong roleplay quality        |
| Llama-3.2-3B Q4  | ~2.0 GB      | ~3 GB        | Solid instruction-following    |
| Phi-3.5-mini Q4  | ~2.3 GB      | ~3.2 GB      | Stronger reasoning, weaker RP  |

All three fit comfortably on an 8 GB device alongside `WKWebView` and game
state.

The current `qwen3:14b`-tuned prompts in
`mods/rundale/prompts/tier1_system.txt` will need a revision pass for
the smaller model — expect more rigid scaffolding, fewer open-ended
instructions, more concrete examples. The existing tiered prompt system
(`tier1_system.txt`, `tier1_context.txt`, `tier2_system.txt`) is exactly the
right shape for this; nothing about the file layout needs to change.

### Cleanup

`#[cfg(not(target_os = "ios"))]`-gate everything in
`crates/parish-core/src/inference/setup.rs` (Ollama bootstrap, GPU probe, all
`Command::new` paths) and the `OllamaProcess` lifecycle wrapper in
`crates/parish-core/src/inference/client.rs`. With the trait in place,
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
- Add `"iOS"` to `tauri.conf.json` bundle targets. Add iOS icons and Info.plist entries (camera/mic usage strings if relevant; the game currently needs neither).
- In `crates/parish-tauri/src/lib.rs`, the provider construction path around line 899 reads the `PARISH_PROVIDER` env var and chooses between Ollama and OpenAI-compatible cloud providers. On `target_os = "ios"`, force the embedded backend regardless of env — the env var concept doesn't really apply when there's only one possible backend.
- The `--screenshot <dir>` CLI flag in `lib.rs` is a desktop dev affordance. `#[cfg]`-gate the whole flag-parsing block out on iOS (the binary on iOS receives no command line).

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
database layer in `crates/parish-core/src/persistence/database.rs` needs zero
changes.

The change is in `crates/parish-core/src/persistence/picker.rs::ensure_saves_dir`
(line 79), which today returns:

```rust
pub fn ensure_saves_dir() -> PathBuf {
    let saves_dir = PathBuf::from(SAVES_DIR);  // "saves"
    std::fs::create_dir_all(&saves_dir).ok();
    ...
}
```

A relative path is fine on desktop and the headless CLI but meaningless on
iOS — the app's working directory is not where its writable storage lives.
The fix is a small `saves_dir()` helper in `parish-core` that returns the
platform-correct base path:

- Desktop / CLI: current behaviour (relative `saves/`, or whatever existing convention)
- iOS: the app's Application Support directory, resolved through Tauri's path API or directly via `dirs`/Foundation bridging

`ensure_saves_dir()` then calls the helper. Every other persistence
path-handling function continues to work unchanged because they all take an
explicit `&Path` already.

### Mod assets

`mods/rundale/` is currently loaded by relative path. On iOS the app
sandbox has no concept of "the directory the binary was launched from", so
the mod has to ship as a Tauri *resource*:

- Add `mods/rundale/**` to `tauri.conf.json` → `bundle.resources`
- Replace any direct relative-path mod loading in the Tauri startup (`crates/parish-tauri/src/lib.rs`) with Tauri's resource resolver
- The resolver returns the correct on-disk path on every platform — desktop reads from the dev directory, iOS reads from inside the app bundle

The mod is small (~13 files, hundreds of KB) so bundling it has no meaningful
size impact.

## What Gets Dropped on iOS

- The Ollama auto-installer and GPU probe in `crates/parish-core/src/inference/setup.rs`
- The `OllamaProcess` lifecycle wrapper in `crates/parish-core/src/inference/client.rs`
- The Axum web-server mode (`crates/parish-server/`) — not built for iOS. The iOS build only touches `parish-tauri` + `parish-core`, so `parish-server` and `parish-cli` are excluded naturally. No Cargo manifest surgery required.
- The `--screenshot <dir>` CLI flag in `crates/parish-tauri/src/lib.rs` (a desktop dev affordance)

## Model Download UX

**Don't ship the model in the IPA.** A 2 GB binary in an IPA pushes the app
over the Apple cellular-download limit and bloats the initial download for
users who would be fine waiting for the model to fetch later.

Apple gives us two clean mechanisms:

1. **On-Demand Resources (ODR)** — tag GGUF variants in the Xcode project and request them at runtime. They don't count against the initial IPA size, download on first launch, and Apple handles the CDN. Recommended.
2. **Background `URLSession` from a CDN we control** — write to the app's Application Support directory. More work but gives us full control over hosting and updates.

Either way:

- Detect device class on first launch (`ProcessInfo.processInfo.physicalMemory`, device model) and pick a quant: 3B-Q4 for 6 GB devices, 4B-Q4 or 7B-Q4 for 8 GB devices (15 Pro / 16 / 16 Pro).
- `LlamaCppClient::new` takes the resolved model path so the Swift layer can hand it whichever file it downloaded.
- Show a one-time "Downloading parish brain (~2 GB)" screen on first launch. Reuse the existing `LoadingAnimation` from `crates/parish-core/src/loading.rs` for visual continuity with the desktop boot experience.

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

The ANE is reachable exclusively through Core ML, and Core ML's transformer
support is geared toward small/medium models with fixed shapes. For
autoregressive LLM decoding with a sliding KV cache, the practical state of
the art on Apple Silicon is still Metal/MPS via `llama.cpp` or MLX, both of
which run on the **GPU**, not the ANE. `llama.cpp`'s Metal backend on an
A17 Pro / A18 hits roughly 20–30 tokens/sec for a 3B Q4, which is plenty for
Rundale's tier-1 dialogue.

If we wanted ANE specifically, the path would be: convert the model to Core
ML packages with `coremltools` (stateful KV-cache support landed in iOS 18),
then call it from Swift. That's a real research project on its own and would
re-introduce a Swift inference path with a second IPC hop. Defer to v2; only
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
workflow that runs on `macos-latest`, gated to only run on PRs that touch
`crates/parish-tauri/`, `crates/parish-core/src/inference/`, `apps/ui/`, or
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

- **LLM quality on a 3B model is the dominant risk.** Current prompts and the anachronism pipeline (`crates/parish-core/src/npc/anachronism.rs`) were tuned against a 14B model. Expect a real prompt-engineering pass and possibly more frequent fallback to Tier-2 cognition. This is the only piece that can't be derisked just by writing the integration — it has to be measured against real player conversations.
- **Memory pressure.** 3B Q4 + WKWebView + game state ≈ 3.5–4 GB resident. Fine on 8 GB devices, marginal on 6 GB. Don't target pre-iPhone-15-Pro hardware.
- **App Store review.** Bundling a multi-GB model is allowed but pushes the IPA over the cellular-download limit. ODR sidesteps this but adds a first-launch download-screen requirement that reviewers will check.
- **Tauri iOS maturity.** `WKWebView` quirks (no `eval`, stricter CSP) sometimes bite Svelte apps. Budget time for a shakedown pass. `tauri.conf.json` currently sets `security.csp: null`, which simplifies dev but may need tightening for App Store review.
- **Background termination.** iOS aggressively kills backgrounded apps with high RAM usage. The session-resume path (load from latest snapshot) needs to be fast and reliable on a cold start.

## Verification Plan

When this design is implemented:

1. `cargo build --target aarch64-apple-ios --features ios-inference -p parish-core` — core compiles for the device.
2. `cd crates/parish-tauri && cargo tauri ios build` — produces an `.ipa`.
3. **Install on a physical iPhone 15 Pro** (the simulator can't run Metal-accelerated `llama.cpp` realistically) and smoke-test:
   - Start a new game, walk between two locations, talk to an NPC. Confirm token streaming arrives in `apps/ui/src/components/ChatPanel.svelte`.
   - Save, kill the app from the multitasker, relaunch, load. Confirm `rusqlite` round-trips through the iOS sandbox path resolved by `ensure_saves_dir`.
   - Confirm the anachronism filter still fires, the conversation log persists across turns, and time advances.
4. **Measure on-device:**
   - Tokens/sec for tier-1 dialogue (target ≥15 t/s)
   - Peak RSS over a 10-minute session
   - Battery drain over a 10-minute session
   - Thermal state at the end of the session
5. **Confirm no desktop regressions** from the trait refactor:
   - `cd apps/ui && npx vitest run`
   - `cargo test -p parish-core`
   - `just check` and `just verify` (CLI / Tauri desktop / web server still build and pass)

## Files That Would Be Modified

Forward reference for whoever picks this up:

| Path                                                              | Change                                                          |
|-------------------------------------------------------------------|-----------------------------------------------------------------|
| `crates/parish-core/src/inference/mod.rs`                         | `InferenceBackend` trait, generic worker                        |
| `crates/parish-core/src/inference/llama_cpp_client.rs` *(new)*    | Embedded backend behind `ios-inference` feature                 |
| `crates/parish-core/src/inference/setup.rs`                       | `cfg`-gate Ollama bootstrap and GPU probe                       |
| `crates/parish-core/src/inference/client.rs`                      | `cfg`-gate `OllamaProcess`                                      |
| `crates/parish-core/src/persistence/picker.rs`                    | iOS sandbox branch in `ensure_saves_dir` (and a `saves_dir()` helper) |
| `crates/parish-core/Cargo.toml`                                   | `ios-inference` feature, conditional `llama-cpp-2` dep          |
| `crates/parish-tauri/tauri.conf.json`                             | iOS bundle target, `bundle.resources` for the mod, iOS icons    |
| `crates/parish-tauri/src/lib.rs`                                  | Force embedded backend on `target_os = "ios"`; gate `--screenshot` flag parsing |
| `crates/parish-tauri/gen/apple/` *(generated)*                    | Xcode project from `cargo tauri ios init`                       |
| `mods/rundale/prompts/tier1_system.txt`                    | Slim down for 3B-class model                                    |
| `apps/ui/src/app.css`                                             | `env(safe-area-inset-*)` rules                                  |
