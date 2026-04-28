# Regression Audit: LLM / Inference

Scope: 13 providers, 4 inference categories, `resolve_config` precedence,
Ollama bootstrap (binary detection, GPU detection, VRAM-tiered model
selection, auto-pull, warmup), token streaming, inference logging ring buffer.

## 1. Sub-features audited

- 13 provider backends (Simulator, Ollama, LM Studio, vLLM, OpenRouter, OpenAI, Google Gemini, Groq, xAI, Mistral, DeepSeek, Together, Anthropic, Custom)
- 4 inference categories (Dialogue, Simulation, Intent, Reaction)
- `resolve_config` precedence: defaults < TOML < env < CLI
- Ollama bootstrap (binary detection, GPU detect, VRAM model selection, auto-pull)
- Token-by-token streaming
- Inference logging ring buffer (default capacity 50)

## 2. Coverage matrix

| Sub-feature | Unit | HTTP-mocked integration | Rubric | UI |
|---|---|---|---|---|
| Provider enum / name parsing | `parish-config/src/provider.rs:568` `test_provider_from_str_loose`; `:680` `test_provider_default_base_url`; `:726` `test_provider_requirements`; `:761` vllm | none | none | none |
| Ollama HTTP client | `parish-inference/src/openai_client.rs` (in-source) | `parish-inference/tests/http_mock_tests.rs:22-280` (10 tests: response, system-prompt, 500/404, empty, streaming chunk/empty/malformed/no-trailing-newline, JSON typed payload, parse error) | none | none |
| OpenAI HTTP client | (in-source) | `http_mock_tests.rs:280-560` (12 tests: choice content, bearer present/absent, 401 mapping, empty choices, SSE chunks, done sentinel, comments/blank lines, JSON content, malformed inner, max_tokens present/absent) | none | none |
| Anthropic HTTP client | `parish-inference/src/anthropic_client.rs` has 54 in-source `#[test]` markers but **no mocked HTTP integration test in `tests/`** | none | none | none |
| Simulator provider | `parish-inference/src/simulator.rs` (13 in-source tests) | exercised implicitly by every fixture-driven test | none | none |
| **Other 9 providers** (LM Studio, vLLM, OpenRouter, Gemini, Groq, xAI, Mistral, DeepSeek, Together, Custom) | only via the OpenAI-compatible client path; **no provider-specific HTTP mocks** | none | none | none |
| 4 inference categories (Dialogue/Simulation/Intent/Reaction) | `parish-config/src/provider.rs:1053-1194` cloud-config per-category tests; `parish-config/src/engine.rs:189` `for_category` rate limit | `parish-input/tests/llm_fallback_integration.rs` (6 tests: LLM fallback for ambiguous intent) | none | none |
| `resolve_config` precedence (defaults < TOML < env < CLI) | `parish-config/src/provider.rs:809` defaults; `:822` from-toml; `:848` cli-overrides-toml; `:877` openrouter-requires-api-key; `:911` builtin-cloud; `:1002` empty-strings-filtered | none | none | none |
| Ollama binary detection / install | `parish-inference/src/setup.rs` (in-source — install/check) | none — relies on real subprocess | none | none |
| GPU detection (nvidia-smi, rocm-smi, sysctl, Windows PowerShell) | `setup.rs:1422-1611` (parsers for nvidia-smi/rocm-smi/Windows JSON; `is_discrete_gpu`; AMD/NVIDIA env builders) | none | none | none |
| VRAM-tiered model selection | `setup.rs:1416` `select_model_for_vram` (in-source) | none | none | none |
| Ollama model availability / pull | `setup.rs:1614-1806` (8 async tests: exact match, latest-suffix, missing, empty list, malformed JSON, pull progress, 404, status-only lines, skip-when-present) | none | none | none |
| Token streaming (channel + cursor) | `parish-inference/src/utf8_stream.rs` (in-source) | `http_mock_tests.rs:116` Ollama stream chunks; `:381` OpenAI SSE | none | `apps/ui/src/lib/stream-pacing.test.ts` |
| Inference logging ring buffer | `parish-inference/src/lib.rs` (in-source — 210 tests in crate) | `parish-server/tests/isolation.rs:118` `debug_snapshot_call_log_has_prompt_len_not_prompt_text` | none | none |

## 3. Strong spots

- `resolve_config` precedence is **exhaustively** unit-tested
  (`provider.rs:775-1218` — every layer, every branch, every category).
  This is the pattern other configuration code should follow.
- The OpenAI HTTP client has 12 mocked-HTTP tests covering bearer/no-bearer,
  401, SSE parsing, max_tokens, malformed bodies — production-grade.
- Ollama setup is unusually well tested for a system-integration
  surface: 28 tests covering parsers, model discovery, pull progress,
  `latest`-suffix matching.
- The Simulator default means CI never needs the network — the harness
  is Simulator-driven by default.

## 4. Gaps

- **[P0] Anthropic provider has zero mocked-HTTP test.** Despite 54
  in-source `#[test]` markers in `anthropic_client.rs`, no entry in
  `parish-inference/tests/http_mock_tests.rs` mocks an Anthropic
  request/response. Given Anthropic is a first-tier cloud provider,
  this is a real risk. Suggested: extend `http_mock_tests.rs` with
  `anthropic_generate_*` mirroring the OpenAI suite.
- **[P0] 9 of 13 providers (LM Studio, vLLM, OpenRouter, Gemini, Groq,
  xAI, Mistral, DeepSeek, Together, Custom) ride on the OpenAI-compatible
  path with no provider-specific smoke test.** A subtle URL- or
  header-shape difference (e.g. Gemini's `?key=` vs bearer) would
  silently break only that provider. Suggested: a single parameterized
  test in `http_mock_tests.rs` that hits a stub server per provider with
  expected URL+headers, asserting the request shape. Ship as one
  `#[rstest]`-style table.
- **[P1] No test asserts streaming cancellation / mid-stream errors
  cleanly drop the channel.** UI input is supposed to re-enable when
  streaming stops — if the channel hangs, the UI freezes. Suggested
  unit test in `parish-inference/src/utf8_stream.rs`:
  `test_stream_cancellation_signals_consumer`.
- **[P1] Inference ring buffer overflow / capacity boundary untested.**
  Default capacity 50; no test asserts entry 51 evicts entry 1.
  Suggested unit test in `parish-inference/src/lib.rs`:
  `test_inference_log_evicts_oldest_at_capacity`.
- **[P1] No test of GPU-detection fallback chain.** Each parser is
  tested in isolation but no test exercises "nvidia missing → rocm
  missing → sysctl returns memsize → CPU fallback". Suggested:
  `test_detect_gpu_info_fallback_order`.
- **[P2] VRAM-tier boundary tests partial.** `select_model_for_vram`
  has tests around `10_999` but no explicit tests at all four
  thresholds (≥25/≥17/≥11/<11 GB). Suggested:
  `test_select_model_for_vram_at_thresholds`.
- **[P2] Per-category override merging into a single resolved config
  bundle.** Cloud-per-category is tested; the four-category dispatcher
  in the engine layer has no integration test asserting that
  `dialogue→openai, simulation→ollama` actually routes per-call.

## 5. Recommendations

1. **Add Anthropic HTTP mocks** mirroring the OpenAI suite — the
   first-tier provider with the largest gap.
2. **Add a parameterized provider-shape test** that asserts every
   non-OpenAI-compatible provider builds its request correctly. One
   table-driven test closes 9 silent-regression vectors.
3. **Test streaming cancellation and ring-buffer overflow** — both are
   one-test fixes for hard-to-reproduce production hangs.
4. **Extend `select_model_for_vram` boundary tests** to all four tiers.
