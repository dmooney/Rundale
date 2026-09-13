# limerick-setup — agent scope

Backend-agnostic leaf crate that owns the full local-inference bootstrap lifecycle: Ollama installation detection and auto-install, cross-platform GPU/VRAM detection, VRAM-based model-tier selection (gemma4 e2b/e4b/26b/31b), managed child-process handles for Ollama/vllm-mlx/vllm, and the unified `setup_provider_client` entry point that resolves a configured provider into a live `limerick_providers::AnyClient`. Extracted from `limerick-inference` in #1410. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p limerick-setup                    # unit + integration tests (GPU parsers, model tiers, orchestration)
cargo test -p limerick-setup -- --nocapture     # with stdout for debugging
```

## Gotchas

- **Leaf-crate dependency rule (rule #1).** Depends only on `limerick-types`, `limerick-config`, `limerick-providers`, `reqwest` (json feature), `tokio`, `serde`/`serde_json`, and `tracing`. Never add `limerick-core`, `limerick-inference`, or any runtime crate (tauri, axum).
- **No dependency on `limerick-inference`.** The dependency is inverted: `limerick-inference` depends on this crate and re-exports `limerick_setup::*` at its former `limerick_inference::setup::*` path for downstream compatibility.
- **Apple Silicon unified-memory scaling.** `detect_gpu_info` reports `vram_free_mb` as ~70% of `hw.memsize` for Apple Silicon. `select_model` uses `vram_free_mb` directly — do not re-scale.
- **Windows AdapterRAM overflow.** WMI `AdapterRAM` is a 32-bit field and overflows at 4 GB. `orchestration` falls back to a registry `HardwareInformation.qwMemorySize` read for accurate VRAM on modern GPUs.
- **`rocm-smi` used-line ordering.** The `VRAM Total Used Memory` line also contains the substring "total", so the `used` check must run before the `total` check in `parse_rocm_smi_output` to avoid clobbering. Do not reorder those branches.
- **Model tier thresholds are free-VRAM budgets, not model sizes.** Thresholds (25 GB / 17 GB / 11 GB) include headroom above each model's disk size for context. Adjust both the threshold and the `vram_required_mb` together.
- **`OllamaProcess`/`VllmProcess` kill on Drop.** Handles stop the server they started; if Ollama was already running before Limerick, `child` is `None` and Drop is a no-op.
- **`SetupProgress` must be `Send + Sync`.** Implementations cross async boundaries. `TestProgress` in `progress::tests` is the canonical test double.

## Module map

`lib.rs` — crate root, public re-exports, and module declarations. `gpu_detect.rs` — `GpuVendor`/`GpuInfo` types, `detect_gpu_info` async entry point, platform-specific detection via `sysctl` (macOS), PowerShell/WMI + registry (Windows), `nvidia-smi` (NVIDIA), `rocm-smi` (AMD). `model_select.rs` — `ModelConfig`, `select_model`, static `MODEL_TIERS` table mapping VRAM budgets to gemma4 model tags. `orchestration.rs` — Ollama install/start/pull/warmup helpers, `setup_ollama`/`setup_ollama_with_config`, `setup_provider_client` unified entry point. `process.rs` — `OllamaProcess`, `VllmProcess`, `VllmSlot`, `VllmMlxProcess`, `VllmMlxSlot`, `RuntimeProcesses` (all with kill-on-Drop). `progress.rs` — `SetupProgress` trait, `StdoutProgress` impl, `TestProgress` test double.
