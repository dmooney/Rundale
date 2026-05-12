Evidence type: gameplay transcript

# Onboarding wizard for local vllm-mlx — evidence bundle

End-to-end proof for the first-run wizard that ships vllm-mlx inside
the Mac .app and downloads model weights with a progress bar visible
to the user. Closes the "click one button, play the game" promise.

Companion PR adds:

- A two-pass HuggingFace Hub downloader in Rust (`hf_downloader.rs`).
- A bundle recipe + CI workflow that materialises a relocatable
  Python runtime with vllm-mlx pre-installed.
- A three-way SetupOverlay fork: local-recommended (Mac ≥16 GB),
  local-low-mem (Mac <16 GB), BYOK-only (elsewhere or after a prior
  successful onboarding). The wizard then picks the actual variant
  from live RAM: `ramGb >= 24` → two-slot (14B + 1.5B), otherwise
  small-only (1.5B everywhere). 14B 4-bit weights + KV cache +
  activations + 1.5B + host overhead is a ~20-24 GB working set;
  16 GB Macs OOM on the two-slot loadout, so the recommended fork
  on a 16-23 GB Mac still ships the small-only variant with an
  honest "consider BYOK" warning.

## Bundle build

`just build-vllm-mlx-bundle` produces a 356 MB compressed tarball
that pip-installs vllm-mlx into a relocatable python-build-standalone
runtime:

```
$ just build-vllm-mlx-bundle
fetching python-build-standalone .../cpython-3.13.1+20250115-aarch64-apple-darwin-install_only.tar.gz
extracting Python runtime…
installing vllm-mlx + hf_transfer into the runtime…
…
Successfully installed … vllm-mlx-0.3.0 …
packing tarball…

built: parish/dist/vllm-mlx-bundle.tar.zst
-rw-r--r--@ 1 dmooney  staff   356M May 11 16:51 parish/dist/vllm-mlx-bundle.tar.zst
c69022c57febe6d4863feb734c9ddf05b1a9dee26aa185ebb50a89a2fdb1d4ce  vllm-mlx-bundle.tar.zst
```

Unpacks to ~1.5 GB of site-packages and Python lib (torch + opencv +
mlx-metal are the bulk).

## Relocation + invocation shape

python-build-standalone's `install_only` tarball is relocatable, but
the pip-into-runtime layout (no venv) needed verification:

```
$ cp -R parish/dist/vllm-mlx/python-runtime /tmp/pbs-relo
$ /tmp/pbs-relo/bin/python3 -c "import vllm_mlx, sys; print('vllm_mlx', vllm_mlx.__version__); print('prefix:', sys.prefix)"
vllm_mlx 0.3.0
prefix: /tmp/pbs-relo
```

A venv would bake `/Users/dmooney/.../python-runtime` into
`pyvenv.cfg` and imports would fail after relocation. The pip-direct
layout sidesteps this entirely.

We invoke as `python3 -m vllm_mlx.cli serve …` rather than running
the pip-generated `bin/vllm-mlx` console script — that script has an
absolute shebang baked at install time and would point at the build
machine inside a shipped .app. Verified all the flags the Rust spawn
passes (`--port`, `--enable-prefix-cache`, `--continuous-batching`)
are accepted by `vllm_mlx.cli serve`.

(`python -m vllm_mlx` itself does NOT work — the package has no
`__main__.py`. Caught while sanity-checking; spawn switched to
`-m vllm_mlx.cli` in commit `246afe8f`.)

## Live MCP-driven probes — fully automated

Five clean-profile probes (2026-05-11/12) drove the wizard
end-to-end without manual clicks, using the same Mac display + MCP
bridge that real users have. Each probe spawned the same
`Rundale.app/Contents/MacOS/parish-tauri --mcp-port 3030` with a
fresh `HOME`, `PARISH_SAVES_DIR`, `PARISH_USER_CONFIG_DIR`.

### Probe 1: small-only end-to-end

```
$ curl 127.0.0.1:3030/api/onboarding-options
{"choice":"local-recommended","ram_gb":48}

$ curl -X POST 127.0.0.1:3030/api/start-local-inference -d '{"variant":"small-only"}'
{"ok":true}
```

HfModelDownloader pulled Qwen2.5-1.5B (880 MB across 11 files),
wrote `parish.toml` + `.onboarded` sentinel, persisted GameConfig,
cleared the gate. Progress events visible in SetupOverlay:

```
Downloading model.safetensors  | 278/839 MB (33%) done=False
…
The storyteller is ready.      | 880/880 MB (100%) done=True
```

After relaunch, vllm-mlx spawned within 2.5 s:

```
$ ps -p $(lsof -ti :8001) -ww -o command=
Rundale.app/Contents/Resources/vllm-mlx/python-runtime/bin/python3 \
    -m vllm_mlx.cli serve \
    mlx-community/Qwen2.5-1.5B-Instruct-4bit \
    --port 8001 --enable-prefix-cache --continuous-batching

$ curl -X POST 127.0.0.1:8001/v1/chat/completions -d '{ … "Say hello in one short sentence." …}'
"content":"Hello!"

$ curl -X POST 127.0.0.1:3030/api/submit-input -d '{"text":"look"}'
$ curl 127.0.0.1:3030/api/world-snapshot
{"location_name":"The Crossroads", "time_label":"Midday", … }
```

`HF_HUB_OFFLINE=1` is set, so vllm-mlx never re-checks the hub
after install.

### Probe 2: live NPC dialogue (small-only)

Proved the dialogue tier reaches the bundled server, not just
`/v1/chat/completions` in isolation. Walked player to The Crossroads
at 11:09 AM where Tommy O'Brien arrives per his `npcs.json`
schedule:

```
$ curl -X POST /api/submit-input -d '{"text":"Good day to you, Tommy. What brings you out to the Crossroads at this hour?","addressed_to":["Tommy O'\''Brien"]}'

$ curl /api/transcript
[
  {"speaker":"You",         "text":"Good day to you, Tommy. …"},
  {"speaker":"Tommy O'Brien","text":"Good day to ye, sir. I am here to see Colm Gallagher for a smithing job. He's hammering on a metalworki…"}
]
```

Period Hiberno-English, references another real NPC (Colm Gallagher,
the village smith). Truncated mid-word at the 80-token cap —
expected for the 1.5B small-only variant.

Saved to `transcript-tommy.json`. The `GET /api/transcript` route on
the MCP bridge was added in the same session so the conversation
ring-buffer is readable from outside the Tauri webview.

### Probe 3: wizard now produces playable game without relaunch

`do_start_local_inference_setup` previously wrote `parish.toml` +
emitted `setup-done`, but never called `bootstrap_inference_provider`
— so user saw "ready", clicked through, engine sat with no spawned
serve, no inference queue, no world tick. Only an app restart
re-entered `run()` and picked up the saved config.

Fixed by running the same post-gate bootstrap pipeline `run()` does
for returning users:

```
bootstrap_inference_provider → init_inference_queue → init_persistence →
  spawn_event_bus_fanin → spawn_world_tick → spawn_inactivity_tick →
  spawn_debug_tick → spawn_autosave_tick
```

Verified: clean profile → single POST → `curl :8001/v1/models`
returns in 3 s. No restart.

### Probe 4: Tier 2 / Tier 3 JSON-parse storm silenced

On small-only, the 1.5B can't reliably hold strict JSON for Tier 2
(Simulation) and Tier 3 (Reaction) schemas — prior probe logs
flooded with 12+ parse failures per 30 s. Three fixes:

1. **Per-category routing**: Sim+Reaction route to the in-process
   simulator. Intent stays on vllm-mlx (`parse_intent`'s `Unknown`
   fallback is a safer default than the simulator's keyword-match).
2. **Simulator JSON-detection shim**: `AnyClient::Simulator::generate_stream_with_format`
   used to ignore `response_format` and stream Markov text into a
   JSON parser. Now detects JSON-shaped asks (explicit
   `response_format`, "Respond with a JSON" / "Respond with JSON"
   markers, `"updates":` / `"npc_id":` schema fragments, "JSON" /
   "input parser" in system prompt) and streams a generic JSON
   object whose `#[serde(default)]`-compatible fields parse cleanly
   as `Tier2Response` / `Tier3Update` — worst case "uneventful tick"
   not parse error.
3. **`intent_json_for` word boundary**: simulator was matching `go`
   inside "Good morning" via `starts_with`, classifying as
   `Move`-to-"od morning". Fixed; regression test pins it.

Log diff: 12+ parse failures per 30 s → 0.

Multi-turn dialogue saved to `transcript-peig-fr-declan.json`.

### Probe 5: two-slot loadout live end-to-end

Recommended variant on 16+ GB Mac downloads both Qwen2.5-14B
(~7.7 GB) and Qwen2.5-1.5B (~880 MB), spawns two
`python3 -m vllm_mlx.cli serve` processes (14B on `:8000`, 1.5B
on `:8001`), routes:

- **Dialogue** → :8000 / 14B (full-quality player-facing replies)
- **Intent** → :8001 / 1.5B (fast classification)
- **Sim + Reaction** → in-process simulator

```
You: Brigid, my mother has a cough that won't leave her. Any remedy?
Brigid Ni Fhatharta: Ah, I've seen that cough before. Try a tea
  of marshmallow root and thyme. It'll soothe the throat and clear
  the chest. … Tá an tea sin go hóg an-laethúil é.
```

The 14B slips a Gaeilge sentence into Brigid's reply unprompted
("Tá an tea sin go hóg an-laethúil é" — "that tea is very useful…");
the small-only 1.5B never produced any Irish.

Tier 2 JSON-parse failures over the full probe: **0**.
Saved to `transcript-brigid-two-slot.json`.

## Bugs caught + fixed across probes

1. `python -m vllm_mlx` had no `__main__` → spawn `python -m vllm_mlx.cli`. (`246afe8f`)
2. `python -m venv` baked absolute build-host paths → drop venv,
   pip-install straight into the relocatable runtime. (`1f978447`)
3. `handle_set_provider_config` aborted on keychain platform errors
   during keyless local-provider wipe → tolerated with warn log. (`fd1be019`)
4. Wizard's persisted `parish.toml` never re-read at startup →
   `provider_config_from_env` now layers it under env-var overrides;
   relaunch picks up the saved choice.
5. `PARISH_HF_HOME` set only in-process during wizard → startup
   re-seeds it from `<user_config_dir>/models/` so vllm-mlx finds
   cached weights without network.
6. Wizard emitted `setup-done` without bootstrapping → wizard now
   runs the same post-gate pipeline as returning-user `run()`.
7. Tier 2/3 JSON-parse storm → routing + JSON-detection shim +
   `intent_json_for` word boundary (probe 4).
8. Tier 3 "boot-time race" was a missed shim marker → added
   `"Respond with JSON"` + `"updates":` + `"npc_id":` markers, plus
   regression test case.
9. Bundled vllm-mlx orphaned to launchd on Cmd+Q → hooked
   `RunEvent::ExitRequested` to call `runtime_processes.stop()`
   while tokio runtime is still alive (Drop on `AppState` was a
   catch-all but races runtime teardown).

## Wizard hardening

- **Feature flag** (AGENTS.md rule #6): `bootstrap_inference_provider`
  gates the wizard on `config.flags.is_disabled("local-inference-onboarding")`
  — default-on, explicit-disable falls back to legacy bootstrap.
  Documented in `docs/features.md`.
- **Idempotency guard**: `AppState::wizard_in_flight: AtomicBool` drops
  a second POST while the first is downloading. RAII guard clears
  the flag on every exit path (success or error).
- **Error-path UX**: every failing exit emits
  `setup-done(success=false)` with the error message so SetupOverlay
  drops out of the spinner.
- **Dev-mode fallback** verified: `cargo tauri dev` with no bundle →
  `resolve_bundled_vllm_mlx_paths` returns None → `VLLM_MLX_BIN`
  stays unset → `VllmMlxProcess::ensure_running` spawns `vllm-mlx`
  from PATH (`/Users/<user>/.local/bin/vllm-mlx` on a typical
  `uv tool install vllm-mlx` dev box). Error message at
  `parish-inference/src/setup.rs:372` directs the user if PATH also
  misses.

## Test coverage

```
$ cargo test --workspace
cargo test: 2637 passed, 16 ignored (66 suites, 38.90s)
```

New tests in this PR:

- `parish-inference/src/hf_downloader.rs` — 4 unit (allow-list filter,
  case-insensitivity, adapter running-total monotonic, clone semantics).
- `parish-inference/tests/hf_downloader_tests.rs` — 3 wiremock
  integration (404 manifest, empty allow-list, HEAD-sum-into-grand-total).
- `parish-inference/src/setup.rs` — 4 unit for `VllmMlxInvocation::resolve`
  python-vs-native discriminator.
- `parish-tauri/src/setup.rs` — 8 unit for `resolve_onboarding_choice`
  covering every short-circuit + every wizard variant.
- `parish-inference::tests::simulator_streams_json_when_format_or_prompt_requests_it`
  — pins JSON detection: 5 routing cases (explicit `response_format`,
  Tier 2 "Respond with a JSON" prompt, "input parser" system prompt,
  Tier 3 "Respond with JSON" + `"updates":` schema, plain prompt).
- `parish-npc::tier2_llm_integration::tier2_through_simulator_parses_as_empty_event`
  — runs `run_tier2_for_group` end-to-end against the in-process
  simulator; asserts result parses as an empty `Tier2Event`, not a
  JSON parse error.
- `parish-inference::simulator::intent_json_for_requires_word_boundary_on_move_verbs`
  — pins word-boundary fix so "Good morning" never classifies as
  `move`-to-"od morning" again.

## Out of scope

Apple Developer codesigning + notarization — tracked separately; end
users see a Gatekeeper warning on first launch until that lands.
Bundle hash verification at unpack, Linux/Windows bundling (Ollama
is the path there), and auto-update of the model cache are also
explicit non-goals for this PR.
