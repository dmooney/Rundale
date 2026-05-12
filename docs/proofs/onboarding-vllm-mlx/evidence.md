Evidence type: gameplay transcript

# Onboarding wizard for local vllm-mlx — evidence bundle

Captures what was verified end-to-end for the first-run wizard that
ships vllm-mlx inside the Mac .app and downloads model weights with
a progress bar visible to the user. The companion PR adds:

- A two-pass HuggingFace Hub downloader in Rust (`hf_downloader.rs`).
- A bundle recipe + CI workflow that materialises a relocatable
  Python runtime with vllm-mlx pre-installed.
- A three-way SetupOverlay fork: local-recommended (Mac ≥16 GB),
  local-low-mem (Mac <16 GB), BYOK-only (elsewhere or after a prior
  successful onboarding).

## Bundle build — end-to-end

`just build-vllm-mlx-bundle` runs cleanly on an M-series Mac after
two rebuilds during development (commits b13bbeea and 1f978447):

```
$ just build-vllm-mlx-bundle
fetching python-build-standalone .../cpython-3.13.1+20250115-aarch64-apple-darwin-install_only.tar.gz
extracting Python runtime…
installing vllm-mlx + hf_transfer into the runtime…
Collecting vllm-mlx
  Downloading vllm_mlx-0.3.0-py3-none-any.whl.metadata (10 kB)
Collecting hf_transfer
  Downloading hf_transfer-0.1.9-cp38-abi3-macosx_11_0_arm64.whl.metadata (4.6 kB)
…
Successfully installed … vllm-mlx-0.3.0 …
packing tarball…

built: parish/dist/vllm-mlx-bundle.tar.zst
-rw-r--r--@ 1 dmooney  staff   356M May 11 16:51 parish/dist/vllm-mlx-bundle.tar.zst
c69022c57febe6d4863feb734c9ddf05b1a9dee26aa185ebb50a89a2fdb1d4ce  vllm-mlx-bundle.tar.zst
```

Final compressed bundle is ~356 MB (torch + opencv + mlx-metal are
the bulk). Unpacks to ~1.5 GB of site-packages and Python lib.

## Relocation test

The .app ships Contents/Resources/vllm-mlx/python-runtime/ — i.e.
the bundle is copied out of the build host and into the app
sandbox. python-build-standalone's `install_only` tarball is
relocatable, but we wanted to verify our pip-into-runtime layout
(no venv) survives a move:

```
$ cp -R parish/dist/vllm-mlx/python-runtime /tmp/pbs-relo
$ /tmp/pbs-relo/bin/python3 -c "import vllm_mlx, sys; print('vllm_mlx', vllm_mlx.__version__); print('prefix:', sys.prefix)"
vllm_mlx 0.3.0
prefix: /tmp/pbs-relo
```

The interpreter resolves `sys.prefix` to the new location and
`vllm_mlx` imports cleanly. This is the proof we don't need a
venv (a venv would bake `/Users/dmooney/.../python-runtime` into
`pyvenv.cfg` and the imports would fail after relocation).

## vllm-mlx CLI shape

The pip-generated `bin/vllm-mlx` console script has an absolute
shebang baked at install time, which would point at the build
machine inside a shipped .app. We sidestep it by invoking the
module directly. Verified:

```
$ python-runtime/bin/python3 -m vllm_mlx.cli serve --help
usage: cli.py serve [-h] [--models-config MODELS_CONFIG]
                    [--served-model-name SERVED_MODEL_NAME] [--host HOST]
                    [--port PORT] ...
                    [--enable-prefix-cache] [--disable-prefix-cache]
                    [--continuous-batching]
```

All the flags the Rust spawn passes (`--port`, `--enable-prefix-cache`,
`--continuous-batching`) are accepted by `vllm_mlx.cli serve`.

(`python -m vllm_mlx` itself does NOT work — the package has no
`__main__.py`. This was caught while sanity-checking and fixed in
commit 246afe8f.)

## Test coverage

```
$ cargo test --workspace --tests
cargo test: 2627 passed, 7 ignored (51 suites, 14.10s)
```

New tests added by this PR:

- `parish-inference/src/hf_downloader.rs` — 4 unit tests (allow-list
  filter, case-insensitivity, adapter running-total monotonic,
  clone semantics)
- `parish-inference/tests/hf_downloader_tests.rs` — 3 wiremock
  integration tests:
  - `download_models_errors_when_allow_list_skips_every_file`
  - `download_models_surfaces_manifest_fetch_failure`
  - `download_models_sums_head_content_length_into_grand_total`
- `parish-inference/src/setup.rs` — 4 unit tests for the
  `VllmMlxInvocation::resolve` python-vs-native discriminator
- `parish-tauri/src/setup.rs` — 8 unit tests for
  `resolve_onboarding_choice` covering every short-circuit and
  every wizard variant

All green; clippy clean; fmt clean.

## Live MCP-driven probe — fully automated

The Mac has both eyes and hands here: a display, an MCP bridge, and
the same `python3 -m vllm_mlx.cli serve` invocation that runs in a
shipped .app. So we drove the wizard end-to-end without any manual
clicks.

### Setup

```
just build-vllm-mlx-bundle               # 1.5 GB python-runtime + vllm-mlx
cargo tauri build --debug --bundles app  # Rundale.app with Resources/vllm-mlx/
```

Launch from outside the worktree (so `.env`'s `PARISH_PROVIDER=ollama`
doesn't auto-resolve a provider before the gate fires):

```
$ cd /tmp
$ env HOME=/tmp/parish-clean-probeE \
      PARISH_SAVES_DIR=/tmp/parish-clean-probeE/saves \
      PARISH_USER_CONFIG_DIR=/tmp/parish-clean-probeE/parish-cfg \
      /Users/.../Rundale.app/Contents/MacOS/parish-tauri --mcp-port 3030
```

### Gate fires correctly (clean profile)

```
$ curl 127.0.0.1:3030/api/onboarding-options
{"choice":"local-recommended","ram_gb":48}

$ curl 127.0.0.1:3030/api/setup-snapshot
{ "needs_onboarding":true, "onboarding_choice":"local-recommended",
  "current_message":"Preparing the storyteller...", ... }
```

48 GB Mac → LocalRecommended.

### Driving the wizard via MCP

```
$ curl -X POST 127.0.0.1:3030/api/start-local-inference \
       -d '{"variant":"small-only"}'
{"ok":true}
```

That single POST runs the full path: HfModelDownloader pulls
Qwen2.5-1.5B (880 MB across 11 files), writes
`parish-cfg/parish.toml` + `.onboarded` sentinel, persists the
GameConfig, clears the onboarding gate.

Mid-flight progress (polled by the SetupOverlay on the real
desktop):

```
Downloading model.safetensors  | 278/839 MB (33%) done=False
...
Downloading vocab.json         | 880/880 MB (100%) done=False
The storyteller is ready.      | 880/880 MB (100%) done=True
```

### Relaunch picks up the saved config + spawns vllm-mlx serve

```
$ <same env> /path/to/parish-tauri --mcp-port 3030
INFO parish_tauri_lib::setup: Starting inference provider setup...
INFO parish_inference::setup: vllm-mlx not detected, starting vllm-mlx serve...
INFO parish_inference::setup: vllm-mlx ready after ~2500ms
INFO parish_tauri_lib::setup: Restored from parish_001.db (branch: main)

$ ps -p $(lsof -ti :8001) -ww -o command=
.../Rundale.app/Contents/Resources/vllm-mlx/python-runtime/bin/python3 \
    -m vllm_mlx.cli serve \
    mlx-community/Qwen2.5-1.5B-Instruct-4bit \
    --port 8001 --enable-prefix-cache --continuous-batching
```

vllm-mlx server is up, listening on :8001, serving the cached
Qwen1.5B weights with `HF_HUB_OFFLINE=1` (so no network calls).

### Real inference completes through bundled vllm-mlx

```
$ curl -X POST 127.0.0.1:8001/v1/chat/completions \
       -d '{ "model":"mlx-community/Qwen2.5-1.5B-Instruct-4bit",
              "messages":[{"role":"user","content":"Say hello in one short sentence."}],
              "max_tokens":40 }'
{ "id":"chatcmpl-e259545c",
  "model":"mlx-community/Qwen2.5-1.5B-Instruct-4bit",
  "choices":[{"message":{"role":"assistant","content":"Hello!"}, "finish_reason":"stop"}],
  "usage":{"prompt_tokens":36,"completion_tokens":3,"total_tokens":39} }
```

### Real player input dispatches to the game engine

```
$ curl -X POST 127.0.0.1:3030/api/submit-input -d '{"text":"look"}'

INFO parish_tauri_lib::commands: chat [player] input=look

$ curl 127.0.0.1:3030/api/world-snapshot
{ "location_name":"The Crossroads",
  "location_description":"A quiet crossroads where four narrow roads
   meet. A weathered stone wall lines the eastern side, half-hidden
   by brambles. To the north, smoke rises from a cluster of cottages.
   The air smells of turf and wet grass.",
  "time_label":"Midday", "weather":"Partly Cloudy", "season":"Spring",
  ... }
```

The desktop session, the bundled python interpreter, the downloaded
Qwen weights, the spawned vllm-mlx server, the MCP bridge, and the
game-engine event loop are all the same process tree. No manual
steps.

### Bugs caught + fixed during this probe

1. `python -m vllm_mlx` fails (no `__main__.py`) — switched spawn
   to `python -m vllm_mlx.cli`. (`246afe8f`)
2. `python -m venv` baked absolute build-host paths — dropped venv,
   pip-installed straight into the relocatable runtime. (`1f978447`)
3. `handle_set_provider_config` aborted on keychain platform errors
   for keyless local providers — tolerated. (`fd1be019` or
   equivalent)
4. Wizard's persisted parish.toml never re-read at startup —
   `provider_config_from_env` now layers it under env-var
   overrides; relaunch picks up the saved choice. (this PR)
5. `PARISH_HF_HOME` not re-seeded on relaunch — startup re-points
   it at `<user_config_dir>/models/` so vllm-mlx finds the cached
   weights without network. (this PR)

## Live NPC dialogue through the bundled runtime

Second clean-profile probe (2026-05-12) drove the full first-run
flow against a different save and walked the player into an NPC
exchange to prove the dialogue tier — not just `/v1/chat/completions`
in isolation — is wired through the spawned vllm-mlx serve.

```
$ env HOME=/tmp/parish-clean-peig-… \
      PARISH_SAVES_DIR=…/saves \
      PARISH_USER_CONFIG_DIR=…/parish-cfg \
      PARISH_MODS_DIR=/path/to/repo/mods \
      Rundale.app/Contents/MacOS/parish-tauri --mcp-port 3030

$ curl 127.0.0.1:3030/api/onboarding-options
  → {"choice":"local-recommended","ram_gb":48}
$ curl -X POST 127.0.0.1:3030/api/start-local-inference \
       -d '{"variant":"small-only"}'
  → cache-hit (HF_HOME from earlier probe), done in <2 s
$ curl -X POST 127.0.0.1:3030/api/new-game   → fresh save written
```

After relaunch (`vllm-mlx ready after ~3000ms`, save restored), the
game lands at Kilteevan Village 8:00 AM Friday. NPC schedules in
`mods/rundale/npcs.json` route Tommy O'Brien (Retired Farmer) to
the Crossroads at 11:00 AM — verified by walking the player there:

```
$ curl -X POST 127.0.0.1:3030/api/submit-input -d '{"text":"go to The Crossroads"}'
  → game time advances to 11:09 AM, player at The Crossroads
$ curl 127.0.0.1:3030/api/npcs-here
  → [{"real_name":"Tommy O'Brien","occupation":"Retired Farmer", … }]

$ curl -X POST 127.0.0.1:3030/api/submit-input \
       -d '{"text":"Good day to you, Tommy. What brings you out to the Crossroads at this hour?",
            "addressed_to":["Tommy O'\''Brien"]}'

$ curl 127.0.0.1:3030/api/transcript
[
  {"speaker":"You",         "text":"Good day to you, Tommy. What brings you out to the Crossroads at this hour?"},
  {"speaker":"Tommy O'Brien","text":"Good day to ye, sir. I am here to see Colm Gallagher for a smithing job. He's hammering on a metalworki…"}
]
```

In-character: 1820 rural Hiberno-English ("Good day to ye, sir"),
references another real NPC (Colm Gallagher, the village smith,
defined a few hundred lines up in `npcs.json`). Reply truncated
mid-word at the small-slot's 80-token cap — expected behaviour for
the 1.5B Qwen on the small-only variant; the proof here is that
the dialogue tier reached the bundled server and streamed a
context-aware response, not that the 1.5B writes Booker-prize prose.

Saved transcript: `docs/proofs/onboarding-vllm-mlx/transcript-tommy.json`.

A new MCP route — `GET /api/transcript` — was added in the same
session so the local conversation ring-buffer is readable from
outside the Tauri webview; the dialogue stream emits Svelte events
that Playwright/MCP can't tap directly.

## Three follow-up fixes — wizard now produces a playable game

The first follow-up probe shipped the wizard but exposed three
shipping blockers. A third probe (2026-05-12) drove them out:

### 1. Wizard now spawns vllm-mlx without a relaunch

`do_start_local_inference_setup` used to write the saved
`parish.toml` + emit `setup-done`, but never called
`bootstrap_inference_provider` — so the user saw "ready", clicked
through, and the engine sat with no spawned `vllm_mlx.cli serve`,
no inference queue, no world tick. Only a manual app restart
re-entered `run()` and picked up the saved config.

The fix runs the same post-gate bootstrap pipeline `run()` does on
a returning user (bootstrap → init_inference_queue →
init_persistence → spawn_event_bus_fanin →
spawn_world_tick → spawn_inactivity_tick → spawn_debug_tick →
spawn_autosave_tick) so the wizard hands back a fully-live game.

Verified: a clean `PARISH_USER_CONFIG_DIR` profile, a single POST
to `/api/start-local-inference`, and `curl 127.0.0.1:8001/v1/models`
reports the bundled python serving Qwen1.5B inside 3 seconds —
no restart.

### 2. Multi-turn dialogue against the 1.5B small-only loadout

```
You: Good morning, Peig. Fine day, isn't it?
Peig Hannigan: Good morning, friend. Fine day, indeed. And yourself? What brings you to Kilteevan?
You: What news of the village this morning?
Peig Hannigan: Good morning, friend. Fine day, indeed. …
You: My mother has the cough something terrible. Have you any remedy?
Fr. Declan Tierney: Good morning, brother. Fine weather indeed. Perhaps you are here to seek the aid of a doctor? … if you are looking for a remedy, I may have something that might help your mother. …
```

The middle turn shows the 1.5B at its limit — prefix-cache hits
make near-identical prompts repeat near-identical replies. The
third turn proves the dialogue tier is actually re-deciding
content on materially different player input (sick-mother prompt
elicits a specific remedy response from the priest who has just
arrived at the village). Saved to `transcript-peig-fr-declan.json`.

### 3. Tier 2 / Tier 3 JSON-parse storm silenced

On the small-only loadout the 1.5B can't reliably hold the strict
JSON schema Tier 2 (Simulation) and Tier 3 (Reaction) expect, so
the prior probe's logs flooded with one parse failure every 1–2
seconds across every nearby location. The fix routes Sim+Reaction
to the in-process simulator (Intent stays on vllm-mlx — see why
below) AND fixes a latent simulator bug where
`AnyClient::Simulator::generate_stream_with_format` ignored
`response_format` and streamed plain Markov text into a JSON
parser. The simulator now detects JSON-shaped asks (via system
prompt keywords plus the `Respond with a JSON` boilerplate Tier 2
uses) and streams a generic JSON object with
`#[serde(default)]`-compatible fields, so `Tier2Response` parses
to an "uneventful tick" instead of erroring.

Intent stays on vllm-mlx because the simulator's `intent_json_for`
matches verb prefixes via `starts_with("go")` without a word
boundary, so "Good morning" gets classified as `Move`-to-"od
morning" and the actual dialogue path never fires. That latent
bug is also fixed (regression test in
`parish-inference/src/simulator.rs::intent_json_for_requires_word_boundary_on_move_verbs`),
so a future small-only loadout can route Intent to the simulator
without re-introducing the regression — but until then,
parse_intent's `Unknown` fallback (which trickles down into
`handle_npc_conversation` regardless) is a safer default.

Log diff from the previous probe (same wizard, same player input):

```
before: 12+ "Tier 2 inference failed at <loc>: Tier 2 JSON parse
        failed: expected value at line 1 column 1" per 30 s
after:  0 Tier 2 JSON parse failures; "Tier 2 cancelled
        mid-stream" entries when sim_cancel preempts a tick on
        player input (expected behaviour).
```

## Two-slot loadout — live end-to-end

The wizard's `recommended` variant on a 16+ GB Mac downloads
**both** Qwen2.5-14B (~7.7 GB) and Qwen2.5-1.5B (~880 MB), spawns
two `python3 -m vllm_mlx.cli serve` processes (14B on `:8000`,
1.5B on `:8001`), and routes the inference categories:

- **Dialogue** → :8000 / 14B (full-quality player-facing replies)
- **Intent** → :8001 / 1.5B (fast classification; `parse_intent`'s
  `Unknown` fallback covers the occasional JSON-parse failure)
- **Sim + Reaction** → in-process simulator (the 1.5B can't hold
  strict JSON for Tier 2 / Tier 3 schemas reliably; routing those
  categories to the simulator avoids the same parse-failure storm
  the `small-only` loadout was hitting)

Probed 2026-05-12 against a clean profile with both models
pre-cached locally (HF cache symlinked from `~/.cache/huggingface`).
After the wizard completes, `curl /v1/models` on both ports lists
the right model, and an NPC dialogue exchange produces in-character
period speech from the 14B big slot:

```
You: Brigid, my mother has a cough that won't leave her. Any remedy?
Brigid Ni Fhatharta: Ah, I've seen that cough before. Try a tea
  of marshmallow root and thyme. It'll soothe the throat and clear
  the chest. A bit of rest and warmth too, if ye can get it. How's
  yer mother's strength holding up, mind ye don't tire her too
  much with the tea-making if she's weak from the coughing. Tá an
  tea sin go hóg an-laethúil é.
```

The 14B is fluent enough to slip a Gaeilge sentence into Brigid's
reply unprompted ("Tá an tea sin go hóg an-laethúil é" — "that
tea is very useful…"); the small-only Qwen1.5B never produced any
Irish in the previous probe.

Tier 2 JSON-parse failures in the log over the full probe: **0**.
Tier 3 boot-time failures: 1 (same race the small-only probe saw —
first tier-3 tick fires within the 5-second world-tick window
before the simulator override settles end-to-end; engine moves on,
no subsequent failures, NPCs continue to schedule and disperse
normally).

Saved to `transcript-brigid-two-slot.json`.

## Wizard hardening — three smaller fixes alongside

### Feature flag (AGENTS.md rule #6)

`bootstrap_inference_provider` now gates the wizard on
`config.flags.is_disabled("local-inference-onboarding")` — default-on,
explicit-disable falls back to the legacy bootstrap. Operators can
ship a build that suppresses the wizard without code changes (for
example when running the engine pointed at a managed server).

### Idempotency guard

`do_start_local_inference_setup` now uses
`AppState::wizard_in_flight: AtomicBool` to drop a second POST
while the first is downloading. The in-flight wizard keeps running;
the duplicate caller sees `local-inference setup already in progress`.
RAII guard clears the flag on every exit path (success or error).

### Error-path UX

Every failing exit (HF download failure, bootstrap failure, etc.)
now emits a `setup-done` event with `success=false` + the error
message, so the SetupOverlay drops out of the "Downloading…"
spinner and the user sees what went wrong. Without this the wizard
hung on the spinner forever and the user had to restart the app
to see anything.

## Tests

- `parish-inference::tests::simulator_streams_json_when_format_or_prompt_requests_it`
  — pins the simulator's JSON detection so the four routing cases
  (explicit `response_format`, "Respond with a JSON" prompt
  marker, "input parser" system prompt, plain prompt) keep
  producing the right body shape (JSON vs Markov text).
- `parish-npc::tier2_llm_integration::tier2_through_simulator_parses_as_empty_event`
  — runs `run_tier2_for_group` end-to-end against the in-process
  simulator and asserts the result parses cleanly as an empty
  `Tier2Event`, not a JSON parse error.
- `parish-inference::simulator::intent_json_for_requires_word_boundary_on_move_verbs`
  — pins the simulator's intent verb-boundary fix so "Good
  morning" never classifies as `move`-to-"od morning" again.
