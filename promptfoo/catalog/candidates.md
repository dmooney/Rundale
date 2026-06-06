# Rundale benchmark candidates (REQ 1)

**265 viable candidates** made the cut (de-duped by family from 549 raw provider entries).

## Viability filter (explicit)

A model is viable iff ALL hold:

1. **Chat/instruct, text-in→text-out** — not embeddings/rerank/audio/image/video/moderation/base.
2. **Context window ≥ 8192 tokens** — fits the game's largest runtime prompt + headroom (floor measured from the runtime-faithful datasets).
3. **JSON-capable** — exposes `response_format`/`structured_outputs`, or tool-calling for forced JSON (needed by the intent + simulation slices).
4. **Interactive cost ceiling** — ≤ $16/game-hour (`pricing.gameplay_cost`), so it is affordable for real-time play.

De-dup: one logical model per family; cheapest viable provider kept as primary, others recorded as `alt_providers` (still perf-swept).

## Cost tiers (USD per game-hour)

- **free**: $0 (local MLX/GGUF + OpenRouter `:free`)
- **budget**: ≤ $0.20/hr
- **mid**: ≤ $1.50/hr
- **premium**: ≤ $16/hr

## Counts

| Tier | Count |
| --- | --- |
| free | 20 |
| budget | 88 |
| mid | 95 |
| premium | 62 |

**By source:** openrouter=244, anthropic=8, google=7, opencode-go=4, local=2

## Exclusion-reason histogram

| Reason | Count |
| --- | --- |
| price-unknown | 151 |
| dedup | 56 |
| no-json-capability | 36 |
| non-chat-modality | 21 |
| cost>16.0/hr | 8 |
| context<8192 | 8 |
| price-sentinel | 3 |
| meta-router | 1 |

## Viable candidates (sorted by tier, then cost)

| Model | Provider | Tier | $/game-hr | ctx | family |
| --- | --- | --- | --- | --- | --- |
| `inclusionai/ling-2.6-flash` | openrouter | budget | 0.0082 | 262144 | ling-2.6-flash |
| `meta-llama/llama-3.1-8b-instruct` | openrouter | budget | 0.0136 | 131072 | llama-3.1-8b-instruct |
| `mistralai/mistral-nemo` | openrouter | budget | 0.0136 | 131072 | mistral-nemo |
| `meta-llama/llama-3-8b-instruct` | openrouter | budget | 0.0253 | 8192 | llama-3-8b-instruct |
| `sao10k/l3-lunaris-8b` | openrouter | budget | 0.0262 | 8192 | l3-lunaris-8b |
| `google/gemma-3-4b-it` | openrouter | budget | 0.0291 | 131072 | gemma-3-4b-it |
| `amazon/nova-micro-v1` | openrouter | budget | 0.0321 | 128000 | nova-micro-v1 |
| `google/gemma-3-12b-it` | openrouter | budget | 0.0338 | 131072 | gemma-3-12b-it |
| `cohere/command-r7b-12-2024` | openrouter | budget | 0.0344 | 128000 | command-r7b-12-2024 |
| `mistralai/mistral-small-24b-instruct-2501` | openrouter | budget | 0.0345 | 32768 | mistral-small-24b-instruct-2501 |
| `qwen/qwen3.5-9b` | openrouter | budget | 0.0357 | 262144 | qwen3.5-9b |
| `ibm-granite/granite-4.1-8b` | openrouter | budget | 0.0364 | 131072 | granite-4.1-8b |
| `arcee-ai/trinity-mini` | openrouter | budget | 0.0384 | 131072 | trinity-mini |
| `qwen/qwen3-30b-a3b-instruct-2507` | openrouter | budget | 0.0442 | 131072 | qwen3-30b-a3b-instruct-2507 |
| `qwen/qwen3-235b-a22b-2507` | openrouter | budget | 0.0476 | 262144 | qwen3-235b-a22b-2507 |
| `microsoft/phi-4` | openrouter | budget | 0.0482 | 16384 | phi-4 |
| `tencent/hy3-preview` | openrouter | budget | 0.0538 | 262144 | hy3 |
| `amazon/nova-lite-v1` | openrouter | budget | 0.0550 | 300000 | nova-lite-v1 |
| `google/gemma-3-27b-it` | openrouter | budget | 0.0582 | 131072 | gemma-3-27b-it |
| `mistralai/mistral-small-3.2-24b-instruct` | openrouter | budget | 0.0593 | 128000 | mistral-small-3.2-24b-instruct |
| `qwen/qwen3.5-flash-02-23` | openrouter | budget | 0.0596 | 1000000 | qwen3.5-flash-02-23 |
| `qwen/qwen3-coder-30b-a3b-instruct` | openrouter | budget | 0.0632 | 160000 | qwen3-coder-30b-a3b-instruct |
| `mistralai/ministral-3b-2512` | openrouter | budget | 0.0632 | 131072 | ministral-3b-2512 |
| `qwen/qwen3-235b-a22b-thinking-2507` | openrouter | budget | 0.0632 | 262144 | qwen3-235b-a22b-thinking-2507 |
| `rekaai/reka-edge` | openrouter | budget | 0.0632 | 16384 | reka-edge |
| `z-ai/glm-4-32b` | openrouter | budget | 0.0632 | 128000 | glm-4-32b |
| `openai/gpt-5-nano` | openrouter | budget | 0.0648 | 400000 | gpt-5-nano |
| `qwen/qwen3-8b` | openrouter | budget | 0.0648 | 131072 | qwen3-8b |
| `bytedance-seed/seed-1.6-flash` | openrouter | budget | 0.0688 | 262144 | seed-1.6-flash |
| `gemini-2.0-flash-lite` | google | budget | 0.0688 | 1048576 | gemini-2.0-flash-lite |
| `gemini-2.0-flash-lite-001` | google | budget | 0.0688 | 1048576 | gemini-2.0-flash-lite-001 |
| `qwen/qwen3-32b` | openrouter | budget | 0.0696 | 131072 | qwen3-32b |
| `z-ai/glm-4.7-flash` | openrouter | budget | 0.0702 | 202752 | glm-4.7-flash |
| `meta-llama/llama-4-scout` | openrouter | budget | 0.0714 | 10000000 | llama-4-scout |
| `deepseek/deepseek-v4-flash` | openrouter | budget | 0.0715 | 1048576 | deepseek-v4-flash |
| `microsoft/phi-4-mini-instruct` | openrouter | budget | 0.0762 | 131072 | phi-4-mini-instruct |
| `qwen/qwen3-14b` | openrouter | budget | 0.0765 | 131702 | qwen3-14b |
| `stepfun/step-3.5-flash` | openrouter | budget | 0.0768 | 262144 | step-3.5-flash |
| `qwen/qwen3-30b-a3b-thinking-2507` | openrouter | budget | 0.0809 | 131072 | qwen3-30b-a3b-thinking-2507 |
| `mistralai/voxtral-small-24b-2507` | openrouter | budget | 0.0822 | 32000 | voxtral-small-24b-2507 |
| `xiaomi/mimo-v2-flash` | openrouter | budget | 0.0822 | 262144 | mimo-v2-flash |
| `qwen/qwen3-vl-8b-instruct` | openrouter | budget | 0.0904 | 256000 | qwen3-vl-8b-instruct |
| `qwen/qwen3-30b-a3b` | openrouter | budget | 0.0910 | 131072 | qwen3-30b-a3b |
| `bytedance-seed/seed-2.0-mini` | openrouter | budget | 0.0917 | 262144 | seed-2.0-mini |
| `gemini-2.0-flash` | google | budget | 0.0917 | 1048576 | gemini-2.0-flash |
| `gemini-2.0-flash-001` | google | budget | 0.0917 | 1048576 | gemini-2.0-flash-001 |
| `gemini-flash-lite-latest` | google | budget | 0.0917 | 1048576 | gemini-flash-lite |
| `google/gemini-2.5-flash-lite` | openrouter | budget | 0.0917 | 1048576 | gemini-2.5-flash-lite |
| `google/gemini-2.5-flash-lite-preview-09-2025` | openrouter | budget | 0.0917 | 1048576 | gemini-2.5-flash-lite-preview-09-2025 |
| `nvidia/llama-3.3-nemotron-super-49b-v1.5` | openrouter | budget | 0.0917 | 131072 | llama-3.3-nemotron-super-49b-v1.5 |
| `openai/gpt-4.1-nano` | openrouter | budget | 0.0917 | 1047576 | gpt-4.1-nano |
| `essentialai/rnj-1-instruct` | openrouter | budget | 0.0949 | 32768 | rnj-1-instruct |
| `mistralai/ministral-8b-2512` | openrouter | budget | 0.0949 | 262144 | ministral-8b-2512 |
| `qwen/qwen3-vl-32b-instruct` | openrouter | budget | 0.0954 | 262144 | qwen3-vl-32b-instruct |
| `inclusionai/ling-2.6-1t` | openrouter | budget | 0.0996 | 262144 | ling-2.6-1t |
| `inclusionai/ring-2.6-1t` | openrouter | budget | 0.0996 | 262144 | ring-2.6-1t |
| `mimo-v2.5` | opencode-go | budget | 0.1018 | 1000000 | mimo-v2.5 |
| `nousresearch/hermes-4-70b` | openrouter | budget | 0.1078 | 131072 | hermes-4-70b |
| `qwen/qwen3-vl-30b-a3b-instruct` | openrouter | budget | 0.1192 | 262144 | qwen3-vl-30b-a3b-instruct |
| `nex-agi/deepseek-v3.1-nex-n1` | openrouter | budget | 0.1200 | 131072 | deepseek-v3.1-nex-n1 |
| `qwen/qwen3-next-80b-a3b-thinking` | openrouter | budget | 0.1264 | 262144 | qwen3-next-80b-a3b-thinking |
| `mistralai/ministral-14b-2512` | openrouter | budget | 0.1265 | 262144 | ministral-14b-2512 |
| `allenai/olmo-3-32b-think` | openrouter | budget | 0.1280 | 65536 | olmo-3-32b-think |
| `baidu/ernie-4.5-vl-28b-a3b` | openrouter | budget | 0.1283 | 131072 | ernie-4.5-vl-28b-a3b |
| `tencent/hunyuan-a13b-instruct` | openrouter | budget | 0.1293 | 131072 | hunyuan-a13b-instruct |
| `thedrummer/rocinante-12b` | openrouter | budget | 0.1322 | 32768 | rocinante-12b |
| `qwen/qwen3-coder-next` | openrouter | budget | 0.1350 | 262144 | qwen3-coder-next |
| `cohere/command-r-08-2024` | openrouter | budget | 0.1375 | 128000 | command-r-08-2024 |
| `meta-llama/llama-4-maverick` | openrouter | budget | 0.1375 | 1048576 | llama-4-maverick |
| `mistralai/mistral-small-2603` | openrouter | budget | 0.1375 | 262144 | mistral-small-2603 |
| `openai/gpt-4o-mini-2024-07-18` | openrouter | budget | 0.1375 | 128000 | gpt-4o-mini |
| `openai/gpt-4o-mini-search-preview` | openrouter | budget | 0.1375 | 128000 | gpt-4o-mini-search |
| `upstage/solar-pro-3` | openrouter | budget | 0.1375 | 128000 | solar-pro-3 |
| `meta-llama/llama-3.2-11b-vision-instruct` | openrouter | budget | 0.1549 | 131072 | llama-3.2-11b-vision-instruct |
| `deepseek/deepseek-v3.2` | openrouter | budget | 0.1555 | 131072 | deepseek-v3.2 |
| `mistralai/mistral-saba` | openrouter | budget | 0.1644 | 32768 | mistral-saba |
| `qwen/qwen3.5-35b-a3b` | openrouter | budget | 0.1701 | 262144 | qwen3.5-35b-a3b |
| `qwen/qwen3.6-35b-a3b` | openrouter | budget | 0.1701 | 262144 | qwen3.6-35b-a3b |
| `deepseek/deepseek-chat-v3-0324` | openrouter | budget | 0.1805 | 163840 | deepseek-chat-v3-0324 |
| `deepseek/deepseek-r1-distill-qwen-32b` | openrouter | budget | 0.1834 | 128000 | deepseek-r1-distill-qwen-32b |
| `deepseek/deepseek-chat` | openrouter | budget | 0.1835 | 131072 | deepseek-chat |
| `deepseek/deepseek-chat-v3.1` | openrouter | budget | 0.1878 | 163840 | deepseek-chat-v3.1 |
| `minimax/minimax-m2.5` | openrouter | budget | 0.1897 | 204800 | minimax-m2.5 |
| `nousresearch/hermes-3-llama-3.1-70b` | openrouter | budget | 0.1897 | 131072 | hermes-3-llama-3.1-70b |
| `qwen/qwen3-vl-235b-a22b-instruct` | openrouter | budget | 0.1909 | 262144 | qwen3-vl-235b-a22b-instruct |
| `qwen/qwen3-vl-8b-thinking` | openrouter | budget | 0.1923 | 256000 | qwen3-vl-8b-thinking |
| `qwen/qwen3-coder-flash` | openrouter | budget | 0.1973 | 1000000 | qwen3-coder-flash |
| `arcee-ai/trinity-large-thinking` | openrouter | budget | 0.1988 | 262144 | trinity-large-thinking |
| `cognitivecomputations/dolphin-mistral-24b-venice-edition:free` | openrouter | free | 0.0000 | 32768 | dolphin-mistral-24b-venice-edition |
| `google/gemma-4-26b-a4b-it:free` | openrouter | free | 0.0000 | 262144 | gemma-4-26b-a4b-it |
| `google/gemma-4-31b-it:free` | openrouter | free | 0.0000 | 262144 | gemma-4-31b-it |
| `meta-llama/llama-3.3-70b-instruct:free` | openrouter | free | 0.0000 | 131072 | llama-3.3-70b-instruct |
| `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | vllmmlx | free | 0.0000 | 32768 | qwen2.5-1.5b-instruct |
| `mlx-community/Qwen2.5-7B-Instruct-4bit` | vllmmlx | free | 0.0000 | 32768 | qwen2.5-7b-instruct |
| `moonshotai/kimi-k2.6:free` | openrouter | free | 0.0000 | 262144 | kimi-k2.6 |
| `nvidia/nemotron-3-nano-30b-a3b:free` | openrouter | free | 0.0000 | 256000 | nemotron-3-nano-30b-a3b |
| `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free` | openrouter | free | 0.0000 | 256000 | nemotron-3-nano-omni-30b-a3b-reasoning |
| `nvidia/nemotron-3-super-120b-a12b:free` | openrouter | free | 0.0000 | 1000000 | nemotron-3-super-120b-a12b |
| `nvidia/nemotron-3-ultra-550b-a55b:free` | openrouter | free | 0.0000 | 1000000 | nemotron-3-ultra-550b-a55b |
| `nvidia/nemotron-nano-12b-v2-vl:free` | openrouter | free | 0.0000 | 128000 | nemotron-nano-12b-v2-vl |
| `nvidia/nemotron-nano-9b-v2:free` | openrouter | free | 0.0000 | 128000 | nemotron-nano-9b-v2 |
| `openai/gpt-oss-120b:free` | openrouter | free | 0.0000 | 131072 | gpt-oss-120b |
| `openai/gpt-oss-20b:free` | openrouter | free | 0.0000 | 131072 | gpt-oss-20b |
| `poolside/laguna-m.1:free` | openrouter | free | 0.0000 | 262144 | laguna-m.1 |
| `poolside/laguna-xs.2:free` | openrouter | free | 0.0000 | 262144 | laguna-xs.2 |
| `qwen/qwen3-coder:free` | openrouter | free | 0.0000 | 1048576 | qwen3-coder |
| `qwen/qwen3-next-80b-a3b-instruct:free` | openrouter | free | 0.0000 | 262144 | qwen3-next-80b-a3b-instruct |
| `z-ai/glm-4.5-air:free` | openrouter | free | 0.0000 | 131072 | glm-4.5-air |
| `inception/mercury-2` | openrouter | mid | 0.2055 | 128000 | mercury-2 |
| `qwen/qwen2.5-vl-72b-instruct` | openrouter | mid | 0.2055 | 131072 | qwen2.5-vl-72b-instruct |
| `qwen/qwen3.6-flash` | openrouter | mid | 0.2074 | 1000000 | qwen3.6-flash |
| `prime-intellect/intellect-3` | openrouter | mid | 0.2118 | 131072 | intellect-3 |
| `qwen/qwen-plus-2025-07-28:thinking` | openrouter | mid | 0.2137 | 1000000 | qwen-plus |
| `stepfun/step-3.7-flash` | openrouter | mid | 0.2165 | 256000 | step-3.7-flash |
| `qwen/qwen3-vl-30b-a3b-thinking` | openrouter | mid | 0.2178 | 131072 | qwen3-vl-30b-a3b-thinking |
| `qwen3.5-plus` | opencode-go | mid | 0.2213 | 262144 | qwen3.5-plus |
| `perceptron/perceptron-mk1` | openrouter | mid | 0.2228 | 32768 | perceptron-mk1 |
| `openai/gpt-5.4-nano` | openrouter | mid | 0.2260 | 400000 | gpt-5.4-nano |
| `qwen/qwen-2.5-72b-instruct` | openrouter | mid | 0.2315 | 131072 | qwen-2.5-72b-instruct |
| `minimax/minimax-m2` | openrouter | mid | 0.2319 | 204800 | minimax-m2 |
| `deepseek/deepseek-v3.1-terminus` | openrouter | mid | 0.2352 | 163840 | deepseek-v3.1-terminus |
| `minimax/minimax-m2.1` | openrouter | mid | 0.2460 | 204800 | minimax-m2.1 |
| `mistralai/codestral-2508` | openrouter | mid | 0.2466 | 256000 | codestral-2508 |
| `z-ai/glm-4.6v` | openrouter | mid | 0.2466 | 131072 | glm-4.6v |
| `qwen/qwen3.5-27b` | openrouter | mid | 0.2527 | 262144 | qwen3.5-27b |
| `anthropic/claude-3-haiku` | openrouter | mid | 0.2529 | 200000 | claude-3-haiku |
| `meta-llama/llama-3.1-70b-instruct` | openrouter | mid | 0.2530 | 131072 | llama-3.1-70b-instruct |
| `thedrummer/unslopnemo-12b` | openrouter | mid | 0.2530 | 32768 | unslopnemo-12b |
| `minimax/minimax-m2.7` | openrouter | mid | 0.2637 | 204800 | minimax-m2.7 |
| `kwaipilot/kat-coder-pro-v2` | openrouter | mid | 0.2750 | 256000 | kat-coder-pro-v2 |
| `minimax/minimax-m3` | openrouter | mid | 0.2750 | 1048576 | minimax-m3 |
| `google/gemini-3.1-flash-lite` | openrouter | mid | 0.2766 | 1048576 | gemini-3.1-flash-lite |
| `qwen/qwen3.5-plus-02-15` | openrouter | mid | 0.2877 | 1000000 | qwen3.5-plus-02-15 |
| `deepseek/deepseek-v4-pro` | openrouter | mid | 0.3163 | 1048576 | deepseek-v4-pro |
| `xiaomi/mimo-v2.5-pro` | openrouter | mid | 0.3163 | 1048576 | mimo-v2.5-pro |
| `bytedance-seed/seed-1.6` | openrouter | mid | 0.3240 | 262144 | seed-1.6 |
| `bytedance-seed/seed-2.0-lite` | openrouter | mid | 0.3240 | 262144 | seed-2.0-lite |
| `openai/gpt-5-mini` | openrouter | mid | 0.3240 | 400000 | gpt-5-mini |
| `openai/gpt-5.1-codex-mini` | openrouter | mid | 0.3240 | 400000 | gpt-5.1-codex-mini |
| `qwen/qwen3.5-122b-a10b` | openrouter | mid | 0.3370 | 262144 | qwen3.5-122b-a10b |
| `meta-llama/llama-3-70b-instruct` | openrouter | mid | 0.3443 | 8192 | llama-3-70b-instruct |
| `qwen/qwen3.6-plus` | openrouter | mid | 0.3596 | 1000000 | qwen3.6-plus |
| `openai/gpt-4.1-mini` | openrouter | mid | 0.3667 | 1047576 | gpt-4.1-mini |
| `qwen/qwen3.7-plus` | openrouter | mid | 0.3667 | 1000000 | qwen3.7-plus |
| `z-ai/glm-4.7` | openrouter | mid | 0.3809 | 202752 | glm-4.7 |
| `qwen/qwen3-vl-235b-a22b-thinking` | openrouter | mid | 0.3863 | 131072 | qwen3-vl-235b-a22b-thinking |
| `microsoft/wizardlm-2-8x22b` | openrouter | mid | 0.3921 | 65536 | wizardlm-2-8x22b |
| `moonshotai/kimi-k2.5` | openrouter | mid | 0.3952 | 262144 | kimi-k2.5 |
| `z-ai/glm-4.6` | openrouter | mid | 0.3961 | 202752 | glm-4.6 |
| `amazon/nova-2-lite-v1` | openrouter | mid | 0.3983 | 1000000 | nova-2-lite-v1 |
| `gemini-flash-latest` | google | mid | 0.3983 | 1048576 | gemini-flash |
| `google/gemini-2.5-flash` | openrouter | mid | 0.3983 | 1048576 | gemini-2.5-flash |
| `mimo-v2-omni` | opencode-go | mid | 0.4046 | 262144 | mimo-v2-omni |
| `mistralai/devstral-2512` | openrouter | mid | 0.4046 | 262144 | devstral-2512 |
| `mistralai/mistral-medium-3` | openrouter | mid | 0.4046 | 131072 | mistral-medium-3 |
| `mistralai/mistral-medium-3.1` | openrouter | mid | 0.4046 | 131072 | mistral-medium-3.1 |
| `mistralai/mistral-large-2512` | openrouter | mid | 0.4110 | 262144 | mistral-large-2512 |
| `openai/gpt-3.5-turbo` | openrouter | mid | 0.4110 | 16385 | gpt-3.5 |
| `google/gemma-2-27b-it` | openrouter | mid | 0.4111 | 8192 | gemma-2-27b-it |
| `qwen/qwen3-235b-a22b` | openrouter | mid | 0.4171 | 131072 | qwen3-235b-a22b |
| `sao10k/l3.3-euryale-70b` | openrouter | mid | 0.4205 | 131072 | l3.3-euryale-70b |
| `minimax/minimax-m1` | openrouter | mid | 0.4236 | 1000000 | minimax-m1 |
| `qwen/qwen3.5-397b-a17b` | openrouter | mid | 0.4315 | 262144 | qwen3.5-397b-a17b |
| `deepseek/deepseek-r1-distill-llama-70b` | openrouter | mid | 0.4522 | 131072 | deepseek-r1-distill-llama-70b |
| `qwen/qwen3.6-27b` | openrouter | mid | 0.4593 | 262144 | qwen3.6-27b |
| `deepseek/deepseek-r1-0528` | openrouter | mid | 0.4726 | 163840 | deepseek-r1-0528 |
| `z-ai/glm-4.5v` | openrouter | mid | 0.4932 | 65536 | glm-4.5v |
| `z-ai/glm-5` | openrouter | mid | 0.5046 | 202752 | glm-5 |
| `arcee-ai/virtuoso-large` | openrouter | mid | 0.5170 | 131072 | virtuoso-large |
| `moonshotai/kimi-k2` | openrouter | mid | 0.5245 | 131072 | kimi-k2 |
| `z-ai/glm-4.5` | openrouter | mid | 0.5311 | 131072 | glm-4.5 |
| `sao10k/l3.1-euryale-70b` | openrouter | mid | 0.5375 | 131072 | l3.1-euryale-70b |
| `google/gemini-3-flash-preview` | openrouter | mid | 0.5532 | 1048576 | gemini-3-flash |
| `moonshotai/kimi-k2-0905` | openrouter | mid | 0.5596 | 262144 | kimi-k2-0905 |
| `moonshotai/kimi-k2-thinking` | openrouter | mid | 0.5596 | 262144 | kimi-k2-thinking |
| `deepseek/deepseek-r1` | openrouter | mid | 0.6133 | 163840 | deepseek-r1 |
| `nousresearch/hermes-3-llama-3.1-405b` | openrouter | mid | 0.6324 | 131072 | hermes-3-llama-3.1-405b |
| `qwen/qwen3-coder-plus` | openrouter | mid | 0.6575 | 1000000 | qwen3-coder-plus |
| `~moonshotai/kimi-latest` | openrouter | mid | 0.6919 | 262144 | kimi |
| `x-ai/grok-build-0.1` | openrouter | mid | 0.7272 | 256000 | grok-build-0.1 |
| `amazon/nova-pro-v1` | openrouter | mid | 0.7334 | 300000 | nova-pro-v1 |
| `qwen/qwen3-max` | openrouter | mid | 0.7891 | 262144 | qwen3-max |
| `qwen/qwen3-max-thinking` | openrouter | mid | 0.7891 | 262144 | qwen3-max-thinking |
| `deepcogito/cogito-v2.1-671b` | openrouter | mid | 0.7905 | 128000 | cogito-v2.1-671b |
| `anthropic/claude-3.5-haiku` | openrouter | mid | 0.8093 | 200000 | claude-3.5-haiku |
| `z-ai/glm-5.1` | openrouter | mid | 0.8188 | 202752 | glm-5.1 |
| `mimo-v2-pro` | opencode-go | mid | 0.8220 | 1048576 | mimo-v2-pro |
| `nousresearch/hermes-4-405b` | openrouter | mid | 0.8220 | 131072 | hermes-4-405b |
| `relace/relace-search` | openrouter | mid | 0.8220 | 256000 | relace-search |
| `openai/gpt-5.4-mini` | openrouter | mid | 0.8298 | 400000 | gpt-5.4-mini |
| `~openai/gpt-mini-latest` | openrouter | mid | 0.8298 | 400000 | gpt-mini |
| `x-ai/grok-4.20` | openrouter | mid | 0.9090 | 2000000 | grok-4.20 |
| `x-ai/grok-4.3` | openrouter | mid | 0.9090 | 1000000 | grok-4.3 |
| `openai/o3-mini` | openrouter | mid | 1.0085 | 200000 | o3-mini |
| `openai/o3-mini-high` | openrouter | mid | 1.0085 | 200000 | o3-mini-high |
| `openai/o4-mini` | openrouter | mid | 1.0085 | 200000 | o4-mini |
| `openai/o4-mini-high` | openrouter | mid | 1.0085 | 200000 | o4-mini-high |
| `anthropic/claude-haiku-4.5` | openrouter | mid | 1.0116 | 200000 | claude-haiku-4.5 |
| `claude-haiku-4-5-20251001` | anthropic | mid | 1.0116 | 200000 | claude-haiku-4-5 |
| `~anthropic/claude-haiku-latest` | openrouter | mid | 1.0116 | 200000 | claude-haiku |
| `z-ai/glm-5v-turbo` | openrouter | mid | 1.0243 | 202752 | glm-5v |
| `qwen/qwen3.7-max` | openrouter | mid | 1.0275 | 1000000 | qwen3.7-max |
| `qwen/qwen3.6-max-preview` | openrouter | mid | 1.1507 | 262144 | qwen3.6-max |
| `mistralai/mistral-medium-3-5` | openrouter | premium | 1.5174 | 262144 | mistral-medium-3-5 |
| `google/gemini-2.5-pro` | openrouter | premium | 1.6200 | 1048576 | gemini-2.5-pro |
| `openai/gpt-5` | openrouter | premium | 1.6200 | 400000 | gpt-5 |
| `openai/gpt-5-chat` | openrouter | premium | 1.6200 | 128000 | gpt-5-chat |
| `openai/gpt-5-codex` | openrouter | premium | 1.6200 | 400000 | gpt-5-codex |
| `openai/gpt-5.1` | openrouter | premium | 1.6200 | 400000 | gpt-5.1 |
| `openai/gpt-5.1-chat` | openrouter | premium | 1.6200 | 128000 | gpt-5.1-chat |
| `openai/gpt-5.1-codex` | openrouter | premium | 1.6200 | 400000 | gpt-5.1-codex |
| `openai/gpt-5.1-codex-max` | openrouter | premium | 1.6200 | 400000 | gpt-5.1-codex-max |
| `mistralai/mistral-large` | openrouter | premium | 1.6440 | 128000 | mistral-large |
| `mistralai/mistral-large-2407` | openrouter | premium | 1.6440 | 131072 | mistral-large-2407 |
| `mistralai/mixtral-8x22b-instruct` | openrouter | premium | 1.6440 | 65536 | mixtral-8x22b-instruct |
| `x-ai/grok-4.20-multi-agent` | openrouter | premium | 1.6440 | 2000000 | grok-4.20-multi-agent |
| `google/gemini-3.5-flash` | openrouter | premium | 1.6596 | 1048576 | gemini-3.5-flash |
| `ai21/jamba-large-1.7` | openrouter | premium | 1.8336 | 256000 | jamba-large-1.7 |
| `openai/gpt-4.1` | openrouter | premium | 1.8336 | 1047576 | gpt-4.1 |
| `openai/o3` | openrouter | premium | 1.8336 | 200000 | o3 |
| `openai/o4-mini-deep-research` | openrouter | premium | 1.8336 | 200000 | o4-mini-deep-research |
| `openai/gpt-3.5-turbo-16k` | openrouter | premium | 1.9920 | 16385 | gpt-3.5-turbo-16k |
| `anthracite-org/magnum-v4-72b` | openrouter | premium | 2.0868 | 32768 | magnum-v4-72b |
| `gemini-3-pro-preview` | google | premium | 2.2128 | 1048576 | gemini-3-pro |
| `google/gemini-3.1-pro-preview` | openrouter | premium | 2.2128 | 1048576 | gemini-3.1-pro |
| `google/gemini-3.1-pro-preview-customtools` | openrouter | premium | 2.2128 | 1048756 | gemini-3.1-pro-preview-customtools |
| `~google/gemini-pro-latest` | openrouter | premium | 2.2128 | 1048576 | gemini-pro |
| `openai/gpt-5.2` | openrouter | premium | 2.2680 | 400000 | gpt-5.2 |
| `openai/gpt-5.2-chat` | openrouter | premium | 2.2680 | 128000 | gpt-5.2-chat |
| `openai/gpt-5.2-codex` | openrouter | premium | 2.2680 | 400000 | gpt-5.2-codex |
| `openai/gpt-5.3-chat` | openrouter | premium | 2.2680 | 128000 | gpt-5.3-chat |
| `openai/gpt-5.3-codex` | openrouter | premium | 2.2680 | 400000 | gpt-5.3-codex |
| `cohere/command-a` | openrouter | premium | 2.2920 | 256000 | command-a |
| `cohere/command-r-plus-08-2024` | openrouter | premium | 2.2920 | 128000 | command-r-plus-08-2024 |
| `openai/gpt-4o-2024-11-20` | openrouter | premium | 2.2920 | 128000 | gpt-4o |
| `openai/gpt-4o-search-preview` | openrouter | premium | 2.2920 | 128000 | gpt-4o-search |
| `amazon/nova-premier-v1` | openrouter | premium | 2.5290 | 1000000 | nova-premier-v1 |
| `openai/gpt-5.4` | openrouter | premium | 2.7660 | 1050000 | gpt-5.4 |
| `anthropic/claude-sonnet-4` | openrouter | premium | 3.0348 | 1000000 | claude-sonnet-4 |
| `anthropic/claude-sonnet-4.5` | openrouter | premium | 3.0348 | 1000000 | claude-sonnet-4.5 |
| `anthropic/claude-sonnet-4.6` | openrouter | premium | 3.0348 | 1000000 | claude-sonnet-4.6 |
| `claude-sonnet-4-5-20250929` | anthropic | premium | 3.0348 | 1000000 | claude-sonnet-4-5 |
| `claude-sonnet-4-6` | anthropic | premium | 3.0348 | 1000000 | claude-sonnet-4-6 |
| `perplexity/sonar-pro-search` | openrouter | premium | 3.0348 | 200000 | sonar-pro-search |
| `~anthropic/claude-sonnet-latest` | openrouter | premium | 3.0348 | 1000000 | claude-sonnet |
| `anthropic/claude-opus-4.5` | openrouter | premium | 5.0580 | 200000 | claude-opus-4.5 |
| `anthropic/claude-opus-4.6` | openrouter | premium | 5.0580 | 1000000 | claude-opus-4.6 |
| `anthropic/claude-opus-4.7` | openrouter | premium | 5.0580 | 1000000 | claude-opus-4.7 |
| `anthropic/claude-opus-4.8` | openrouter | premium | 5.0580 | 1000000 | claude-opus-4.8 |
| `claude-opus-4-5-20251101` | anthropic | premium | 5.0580 | 200000 | claude-opus-4-5 |
| `claude-opus-4-6` | anthropic | premium | 5.0580 | 1000000 | claude-opus-4-6 |
| `claude-opus-4-7` | anthropic | premium | 5.0580 | 1000000 | claude-opus-4-7 |
| `claude-opus-4-8` | anthropic | premium | 5.0580 | 1000000 | claude-opus-4-8 |
| `~anthropic/claude-opus-latest` | openrouter | premium | 5.0580 | 1000000 | claude-opus |
| `openai/gpt-5.5` | openrouter | premium | 5.5320 | 1050000 | gpt-5.5 |
| `openai/gpt-chat-latest` | openrouter | premium | 5.5320 | 400000 | gpt-chat |
| `~openai/gpt-latest` | openrouter | premium | 5.5320 | 1050000 | gpt |
| `openai/gpt-4-1106-preview` | openrouter | premium | 8.2200 | 128000 | gpt-4-1106 |
| `openai/gpt-4-turbo` | openrouter | premium | 8.2200 | 128000 | gpt-4 |
| `openai/o3-deep-research` | openrouter | premium | 9.1680 | 200000 | o3-deep-research |
| `anthropic/claude-opus-4.8-fast` | openrouter | premium | 10.1160 | 1000000 | claude-opus-4.8-fast |
| `openai/o1` | openrouter | premium | 13.7520 | 200000 | o1 |
| `anthropic/claude-opus-4` | openrouter | premium | 15.1740 | 200000 | claude-opus-4 |
| `anthropic/claude-opus-4.1` | openrouter | premium | 15.1740 | 200000 | claude-opus-4.1 |
| `claude-opus-4-1-20250805` | anthropic | premium | 15.1740 | 200000 | claude-opus-4-1 |
