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

## What this PR does NOT verify (manual probe checklist)

These need a live Mac with ≥16 GB RAM and a network connection. I
ran them down to the point where Rust-side verification ends; the
remaining steps are gated on a real .app build, which requires
fixing the unrelated `@tauri-apps/api` vs `tauri` Rust crate
version mismatch on `main` before `cargo tauri build` will run.

1. `just build-vllm-mlx-bundle` (verified ✓)
2. `cargo tauri build --target aarch64-apple-darwin` — packages the
   bundle into `Rundale.app/Contents/Resources/vllm-mlx/python-runtime/`
3. Move .app to `/Applications`, launch from a clean test profile
   (`rm -rf ~/Library/Application\ Support/Rundale`)
4. SetupOverlay should render `LocalInferenceFork`. Click "Run
   locally".
5. Watch the progress bar fill as Qwen2.5-14B + 1.5B download
   (~9 GB total). Speed/ETA numbers should look sane.
6. After ~10–15 min on a fast connection the game appears.
7. `ps aux | grep python3` shows two python processes on `:8000`
   and `:8001`.
8. Play a few turns: NPC dialogue + reactions work.
9. Quit, relaunch: no wizard (sentinel set), instant game boot.
