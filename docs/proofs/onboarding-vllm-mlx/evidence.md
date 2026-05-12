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
