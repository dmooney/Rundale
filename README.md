# Rundale

Rundale is a text adventure set in rural Ireland in 1820, powered by the **Limerick**
game engine. You arrive in Kilteevan, County Roscommon, and explore through
conversation, observation and travel. The geography draws on historical Ireland;
the characters and establishments are fictional.

**Rundale is going mobile, with iPhone as the primary client.** The native SwiftUI
app centers on a chronological transcript, a compact status header and a native
composer. The shared Rust engine runs on device, local saves are authoritative,
and live inference goes through Limerick Endpoints. No provider credentials ship
in the app.

The illustrated notebook concept is shelved. Desktop and browser clients remain
part of Limerick, alongside terminal play and developer tools. Their capabilities
are reusable engine work, rather than a feature-parity target for the mobile game.
Rundale is building back up feature by feature, starting with a tiny world.

## A question for Róisín

“Would you teach me how to spin, Miss?” At Connolly Cottage, two people are
present. The game asks who you mean; choosing Róisín continues that same question
and streams her reply. Speech stays in the player's own words.

<p align="center">
  <a href="https://dmooney.github.io/rundale-pages/pr/2152/">
    <img src="https://dmooney.github.io/rundale-pages/pr/2152/natural-speech.gif" alt="Native Rundale gameplay: entering a natural spinning question, choosing Róisín, and reading her live response" width="360"/>
  </a>
</p>

[Watch the recording with playback controls](https://dmooney.github.io/rundale-pages/pr/2152/).
Captured October 4, 2026 in the native SwiftUI app on an iPhone simulator, with
the shared Limerick engine and live Limerick Endpoints. This is live gameplay,
not a scripted presentation fixture or physical-iPhone validation.

## Where we are going

The app now loads the canonical world—Kilteevan Village, Letter Office and Connolly
Cottage, with Peig, Mícheál and Róisín—through the shared engine. Named travel,
local observation commands, live dialogue, streaming and save/resume are wired in.
That implementation status does not establish completion of every milestone or
physical-iPhone acceptance gate.

- [Product specifications](docs/product-specs/README.md): six product milestones,
  from native interaction prototype through embedded gameplay, tiny-world navigation,
  mobile reliability, living-world proof and controlled expansion.
- [Mobile engine convergence plan](docs/plans/mobile-engine-convergence.md): the
  integration sequence and dependencies. Its dated status notes are historical;
  [GitHub milestones](https://github.com/dmooney/Rundale/milestones) and
  [issues](https://github.com/dmooney/Rundale/issues) track current delivery.
- [Native app implementation and setup](mobile/README.md),
  [test plans](docs/test-plans/README.md) and [phase demonstrations](docs/product-specs/phase-demo-plan.md).

As of October 3, 2026, shared-engine integration and its initial release gate
([#2046](https://github.com/dmooney/Rundale/issues/2046)) have landed. Re-acceptance
([#2047](https://github.com/dmooney/Rundale/issues/2047)) and lifting the feature freeze
([#2048](https://github.com/dmooney/Rundale/issues/2048)) remain open. Upcoming phone
work includes command argument completion, compass travel and clearer conversation
targeting. Background reactions and simulation need the host seam in
[#2025](https://github.com/dmooney/Rundale/issues/2025); the living-world milestone
must then demonstrate memory, gossip, a task, weather behavior and scheduled movement
inside the tiny world before content grows.

Product **Milestone** numbers and convergence **Mobile Phase** numbers are different.
Use the named GitHub milestones for current ordering; `mobile-verify --phase N`
refers to the product milestones.

## Ways to play

| Client                                     | Role                                                   | Run / setup                                                               |
| ------------------------------------------ | ------------------------------------------------------ | ------------------------------------------------------------------------- |
| **Native iPhone app**                      | Primary Rundale experience; engine and saves on device | [Build, simulator and internal TestFlight instructions](mobile/README.md) |
| Limerick desktop (Tauri + Svelte)          | Retained graphical engine client                       | `just run`                                                                |
| Limerick browser (Svelte + HTTP/WebSocket) | Retained graphical client backed by a server session   | `just web`                                                                |
| Limerick headless CLI                      | Terminal play with an engine in the same process       | `just run-headless`                                                       |
| Limerick HTTP CLI                          | Terminal client of a running Limerick server           | `cd limerick && just run-client`                                          |

The MCP bridge is an agent/developer interface to the server, rather than another
player UI. See [runtime driving](docs/agent/runtime-driving-reference.md).

## Feature availability

This matrix replaces the previous undifferentiated feature list. It describes the
checked-in clients and current roadmap, not a fresh end-to-end certification.

- **Implemented**: the client exposes the feature or uses the shared capability.
  Engine support does not imply a dedicated screen, full mobile parity or completed
  milestone acceptance.
- **Planned**: required by the current product milestones or an active delivery issue.
- **Not planned**: unavailable in that client and outside its current roadmap, or
  inapplicable to that interface. It can be reconsidered later; it is not a permanent
  rejection. In particular, initial mobile [non-requirements](docs/product-specs/product-technical-spec.md#15-initial-non-requirements)
  do not become promises simply because Limerick supports them.

Desktop and browser share the Svelte presentation. Terminal columns show text or
server behavior, without implying access to graphical panels. The headless REPL's
migration to the portable turn API remains [#2023](https://github.com/dmooney/Rundale/issues/2023),
so newer clarification and request-lifecycle behavior is marked Planned there.

| Feature                                                                          | iPhone      | Desktop     | Browser     | Headless CLI | HTTP CLI    |
| -------------------------------------------------------------------------------- | ----------- | ----------- | ----------- | ------------ | ----------- |
| Text transcript and free-text dialogue                                           | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Native SwiftUI composer, keyboard and touch interaction                          | Implemented | Not planned | Not planned | Not planned  | Not planned |
| Multiline input and editable command recall                                      | Implemented | Implemented | Implemented | Not planned  | Not planned |
| @mention NPC targeting                                                           | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Clarification of ambiguous addressees                                            | Implemented | Implemented | Implemented | Planned      | Implemented |
| Ask who unnamed speech addresses in a crowd (#2143)                              | Implemented | Implemented | Implemented | Planned      | Implemented |
| Short clarification-choice labels (#2128)                                        | Planned     | Planned     | Planned     | Planned      | Planned     |
| Local look, people, exits and help commands                                      | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Wait, pause and resume commands                                                  | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Full engine slash-command catalogue                                              | Not planned | Implemented | Implemented | Implemented  | Implemented |
| Command-name completion                                                          | Implemented | Implemented | Implemented | Not planned  | Not planned |
| Step-by-step argument completion on iPhone (#2146)                               | Planned     | Not planned | Not planned | Not planned  | Not planned |
| Streaming dialogue                                                               | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Stop, retry and durable request lifecycle                                        | Implemented | Implemented | Implemented | Planned      | Implemented |
| Inline emote styling                                                             | Implemented | Implemented | Implemented | Not planned  | Not planned |
| Emoji message reactions                                                          | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Irish vocabulary / pronunciation panel (Focail)                                  | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Status: location, time and weather                                               | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Nearby people in a separate sidebar                                              | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Illustrated scenes and NPC portraits                                             | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Graphical parish map, historic / modern tiles and worn paths                     | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Click-to-travel and animated map journeys                                        | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Multiple custom visual themes                                                    | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Desktop keyboard shortcuts and fullscreen                                        | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Accessible graphical controls                                                    | Implemented | Implemented | Implemented | Not planned  | Not planned |
| Canonical three-location / three-NPC world                                       | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Location graph, named travel and fuzzy resolution                                | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Compass-direction travel on iPhone (#2147)                                       | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Real, pinned and fictional geography / relative anchors                          | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Game clock and deterministic NPC schedules                                       | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Configurable clock speed (/speed)                                                | Not planned | Implemented | Implemented | Implemented  | Implemented |
| Seasonal schedules and festival hooks                                            | Not planned | Implemented | Implemented | Implemented  | Implemented |
| Weather changes and weather-aware route resolution                               | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Travel time, transport modes and encounters                                      | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Listen, omens and authored folklore commands                                     | Not planned | Implemented | Implemented | Implemented  | Implemented |
| Interactive NPC dialogue and personality / intelligence profiles                 | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Persisted memory: deliberate living-world proof                                  | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Gossip propagation: deliberate living-world proof                                | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Durable assigned work and task progression proof                                 | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Weather-dependent NPC behavior proof                                             | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Scheduled NPC movement proof                                                     | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Tier 2 nearby, tier 3 distant and tier 4 far simulation (#2025)                  | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Autonomous exchanges, arrival reactions and off-screen social simulation (#2025) | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Anachronism filtering and historical prompt guidance                             | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Local authoritative SQLite saves and journal recovery                            | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Automatic persistence and resume                                                 | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Manual save / load and git-style branching commands                              | Not planned | Implemented | Implemented | Implemented  | Implemented |
| Graphical save picker and branch DAG                                             | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Interactive terminal save picker                                                 | Not planned | Not planned | Not planned | Implemented  | Not planned |
| Cross-process save locking                                                       | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Limerick Endpoints inference with Auth / App Check                               | Implemented | Not planned | Not planned | Not planned  | Not planned |
| Direct provider choice, BYOK and per-category routing                            | Not planned | Implemented | Implemented | Implemented  | Implemented |
| Local models and offline inference simulator                                     | Not planned | Implemented | Implemented | Implemented  | Implemented |
| Offline deterministic actions (live dialogue needs inference)                    | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Priority inference lanes and background scheduling                               | Planned     | Implemented | Implemented | Implemented  | Implemented |
| Structured model output and prompt-injection defenses                            | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Player-facing debug panels and inference-call records                            | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Text debug / feature-flag inspection                                             | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Mobile bug reporting (#2022)                                                     | Planned     | Not planned | Not planned | Not planned  | Not planned |
| Existing graphical bug reporter                                                  | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Limerick Designer (mod, NPC, location, schedule and save editor)                 | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Data-driven world, NPCs, prompts, lore and Endpoint definitions                  | Implemented | Implemented | Implemented | Implemented  | Implemented |
| Runtime mod selection UI                                                         | Not planned | Implemented | Implemented | Not planned  | Not planned |
| Scripted / JSON command execution                                                | Not planned | Not planned | Not planned | Implemented  | Implemented |
| ANSI-colored terminal output                                                     | Not planned | Not planned | Not planned | Implemented  | Implemented |
| HTTP sessions, cookie persistence and isolated server saves                      | Not planned | Not planned | Implemented | Not planned  | Implemented |
| WebSocket UI events                                                              | Not planned | Not planned | Implemented | Not planned  | Not planned |
| Server authentication, metrics and container deployment                          | Not planned | Not planned | Implemented | Not planned  | Implemented |
| Controlled content expansion beyond the tiny world                               | Planned     | Planned     | Planned     | Planned      | Planned     |

Mobile details:

- The Commands button advertises `/look`, `/people`, `/exits` and `/help`.
  `/wait`, `/pause`, `/resume`, `/debug` and `/flags` also work. The rest of the
  engine command catalogue is not exposed; [#2146](https://github.com/dmooney/Rundale/issues/2146)
  extends completion for accepted commands.
- Shared movement, weather and schedule code is present on the phone. Arrival
  reactions requiring an unavailable Endpoint are not fulfilled, and background
  gossip and tier-4 progression are skipped until #2025. Full living-world proofs
  remain Planned even where some underlying state or logic already exists.
- The phone resumes its local save and journals committed turns. It has no branch
  picker, save DAG, provider configuration screen, graphical map or Focail panel.
- Mobile bug reporting is explicitly tracked in
  [#2022](https://github.com/dmooney/Rundale/issues/2022), although it was an initial
  non-requirement. [#2147](https://github.com/dmooney/Rundale/issues/2147),
  [#2143](https://github.com/dmooney/Rundale/issues/2143) and
  [#2128](https://github.com/dmooney/Rundale/issues/2128) track travel and conversation improvements.

The [Limerick capability reference](docs/limerick-feature-reference.md) retains the
detailed engine inventory, including modding, Designer, geo/NPC authoring tools,
scenario and quality harnesses, inference internals and established-client UI.
Those tools remain useful while the game grows; they are not mobile player features.

## Build and verification

Start with the [native app README](mobile/README.md) for Xcode, Rust targets,
private Firebase configuration and simulator setup. For development checks:

```sh
just mobile-verify --phase 1   # product milestone gates; use all for the full mobile suite
just mobile-build             # unsigned iOS Release build
just check                    # shared engine quality gates
just verify                   # engine gates plus harness walkthrough
```

Internal beta delivery uses `just testflight-update` and the
[TestFlight runbook](mobile/testflight.md). Simulator checks and upload success do
not replace physical-iPhone validation or a build available for testing.

For retained desktop, browser and terminal clients, use `just setup` and the run
commands above. Their prerequisites and packaging details are in
[build/test](docs/agent/build-test.md). Direct inference configuration for those
clients is separate from the phone's Endpoint integration.

## Architecture

The iPhone app embeds Limerick through `limerick-mobile-ffi`; Tauri and the server
host the same shared engine. `limerick-core` composes backend-independent world,
NPC, input, inference, mod and persistence crates. Desktop and browser share a
Svelte UI; the HTTP CLI and MCP bridge connect to the server. See the
[architecture reference](docs/agent/architecture.md),
[mobile architecture](docs/agent/mobile-architecture.md) and
[shared-engine decision](docs/adr/025-mobile-runtime-on-shared-engine.md).

| Directory       | Contents                                                                              |
| --------------- | ------------------------------------------------------------------------------------- |
| `mobile/`       | SwiftUI app, Swift presentation/Endpoint kits, Rust bridge, tests and release tooling |
| `limerick/`     | Shared Rust engine, desktop/web clients, CLI, harnesses and authoring tools           |
| `mods/rundale/` | Canonical tiny world, NPCs, prompts and Endpoint definitions                          |
| `endpoints/`    | Limerick Endpoints service                                                            |
| `docs/`         | Product specs, plans, test plans, decisions, research and engine references           |
| `promptfoo/`    | Model-quality benchmark and leaderboard tooling                                       |

## Model leaderboard

Rundale ships with its own reproducible LLM benchmark that scores models as the engine's NPC brain — in-character dialogue, reaction, world simulation, intent, and Gaeilge (Irish-language) fluency — then prices each candidate against real gameplay token volume. The **v2 promptfoo suite** is the benchmark of record; the v1 harness is archived.

- **Live results:** [dmooney.github.io/Rundale](https://dmooney.github.io/Rundale/) — the ranked v2 leaderboard: per-category scores with 95% bootstrap CIs, a quality-vs-cost efficiency frontier, cost tiers, per-model drill-downs, and the methodology. (Populates once the first funded run lands; until then it renders the schema.)
- **Reproducible harness:** [`promptfoo/`](promptfoo/) — the v2 benchmark of record. The v1 harness is archived under [`rundale-bench/`](rundale-bench/).
- **Static snapshot:** `promptfoo/leaderboard/leaderboard.md` + `leaderboard.jsonl` (append-only history), committed once the first funded run is recorded.

## AI disclosure

Rundale and Limerick are developed with AI coding agents and automated review,
regression and gameplay checks. Human play-testing is the final gate. Static world
content is AI-generated and human-reviewed; live NPC dialogue depends on the
configured inference service and authoritative game state. The game icon was
generated with OpenAI image generation.

## Documentation

- [Documentation hub](docs/index.md) and [repository layout](docs/repository-layout.md).
- [Product direction](docs/product-specs/README.md) and [delivery plan](docs/plans/mobile-engine-convergence.md).
- [Limerick capability reference](docs/limerick-feature-reference.md) and [troubleshooting](docs/troubleshooting.md).
- [Architecture decisions](docs/adr/README.md) and [historical research](docs/research/README.md).
- [Archived visual-client research](docs/graphics-v2/README.md), including the shelved notebook concept.
- [Agent guide](AGENTS.md).

## Licence

Rundale on the Limerick engine is © 2026 Dave Mooney and is licensed under the
[GNU General Public License v3.0](LICENSE) (`GPL-3.0-only`). Source code is
free to use, modify, and redistribute under the terms of that licence.

"Rundale" and "Limerick" are unregistered trademarks of Dave Mooney. The
GPL covers source reuse but not the project names or logos: forks must
rename. (A formal trademark policy lives at `TRADEMARK.md` once published.)

## Credits

Rundale is built on a stack of excellent open-source projects, including
[Rust](https://www.rust-lang.org/), [Tokio](https://tokio.rs/),
[Axum](https://github.com/tokio-rs/axum), [Tauri](https://tauri.app/),
[Svelte](https://svelte.dev/) / [SvelteKit](https://kit.svelte.dev/),
[MapLibre GL JS](https://maplibre.org/), [SQLite](https://www.sqlite.org/),
and [Phosphor Icons](https://phosphoricons.com/). Full attribution with
licence texts is in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md); run
`just notices` to regenerate the exhaustive transitive list.

Map data © [OpenStreetMap](https://www.openstreetmap.org/copyright)
contributors, licensed under the
[Open Database Licence 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
Historic 6″ Ordnance Survey Ireland tiles (1829–1842) reproduced with the
permission of the [National Library of Scotland](https://maps.nls.uk/),
licensed under [CC-BY](https://maps.nls.uk/copyright.html). UI icons use
[Phosphor Icons](https://phosphoricons.com/) under MIT. Map labels use
[Open Sans](https://github.com/googlefonts/opensans) under the SIL Open Font
License 1.1; its generated MapLibre glyph ranges and licence are bundled for
offline web and desktop rendering.
