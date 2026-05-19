Evidence type: gameplay transcript

# rundale-bench v1 leaderboard

Generated from the same JSON artifacts as [`leaderboard.html`](leaderboard.html). GitHub Markdown strips the dashboard JavaScript/CSS, so this file is a static Markdown snapshot and the HTML file is the interactive view.

## Summary

| Metric | Count |
| --- | --- |
| Cached candidates | 30 |
| Judged candidates | 29 |
| Unjudged backlog | 1 |
| Distinct judges | 2 |
| Quality rows | 57 |
| Perf rows | 30 |
| Gaeilge rows | 1 |

## Gaeilge fluency (1-5 rubric)

Latest `--slice gaeilge` run per candidate/base/split. Higher is better; English leakage is 5 when no English leaks.

| Candidate | Split | n | Err | Overall | Fluency | Grammar | Idiom | Task | No Eng | Cost | File |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| mlx-community/Qwen2.5-14B-Instruct-4bit | dev | 11 | 0 | 2.11 | 2.09 | 2.27 | 2.09 | 1.91 | 4.82 | $0.0529 | run_mlx_community_Qwen2_5_14B_Instruct_4bit_gaeilge_20260518T174855Z.json |

## Quality scores: cross-judge average

| Candidate | n | Total | Char | Auth | Lang | Resp | Craft | Judges |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| openai/gpt-5.5 | 30 | 8.90 | 9.04 | 9.30 | 8.50 | 8.84 | 8.84 | 2 |
| openai/gpt-5.4 | 30 | 8.86 | 8.87 | 9.37 | 8.73 | 8.66 | 8.66 | 2 |
| anthropic/claude-opus-4.7 | 30 | 8.85 | 8.90 | 9.33 | 8.87 | 8.57 | 8.56 | 2 |
| qwen/qwen3-max | 30 | 8.84 | 9.04 | 9.27 | 8.63 | 8.57 | 8.70 | 2 |
| mistralai/mistral-medium-3.1 | 30 | 8.84 | 9.03 | 9.16 | 8.90 | 8.57 | 8.54 | 2 |
| anthropic/claude-sonnet-4.6 | 30 | 8.83 | 9.07 | 9.27 | 8.36 | 8.93 | 8.54 | 2 |
| openai/gpt-5.4-mini | 30 | 8.83 | 8.83 | 9.30 | 8.54 | 8.87 | 8.60 | 2 |
| qwen/qwen3-235b-a22b-2507 | 29 | 8.79 | 9.06 | 9.37 | 8.34 | 8.59 | 8.61 | 2 |
| moonshotai/kimi-k2.5 | 30 | 8.76 | 9.04 | 9.23 | 8.40 | 8.60 | 8.53 | 2 |
| mistralai/mistral-large-2512 | 30 | 8.75 | 8.84 | 9.13 | 8.50 | 8.60 | 8.66 | 2 |
| google/gemma-3-27b-it | 26 | 8.72 | 8.73 | 9.21 | 8.50 | 8.62 | 8.54 | 2 |
| x-ai/grok-3-mini | 30 | 8.63 | 8.77 | 8.96 | 8.77 | 8.53 | 8.13 | 2 |
| meta-llama/llama-4-scout | 30 | 8.59 | 8.66 | 8.96 | 8.83 | 8.37 | 8.13 | 2 |
| google/gemini-2.5-pro | 30 | 8.54 | 8.57 | 9.04 | 7.83 | 8.73 | 8.57 | 2 |
| x-ai/grok-4.3 | 30 | 8.53 | 8.53 | 9.10 | 8.03 | 8.34 | 8.63 | 2 |
| anthropic/claude-haiku-4.5 | 30 | 8.52 | 8.60 | 9.07 | 8.06 | 8.54 | 8.37 | 2 |
| google/gemini-2.5-flash | 30 | 8.48 | 8.46 | 9.00 | 8.20 | 8.40 | 8.37 | 2 |
| meta-llama/llama-4-maverick | 30 | 8.41 | 8.43 | 9.10 | 8.60 | 7.73 | 8.20 | 2 |
| meta-llama/llama-3.3-70b-instruct | 30 | 8.34 | 8.13 | 8.80 | 8.63 | 8.04 | 8.07 | 2 |
| z-ai/glm-4.6 | 30 | 8.32 | 8.30 | 9.10 | 7.60 | 8.13 | 8.50 | 2 |
| deepseek/deepseek-v3.2 | 28 | 8.25 | 8.25 | 8.98 | 7.46 | 8.06 | 8.48 | 2 |
| mistralai/mistral-small-24b-instruct-2501 | 30 | 7.91 | 7.90 | 8.23 | 7.40 | 8.27 | 7.74 | 2 |
| openai/gpt-oss-120b | 26 | 7.84 | 8.08 | 8.77 | 7.04 | 7.77 | 7.54 | 2 |
| nousresearch/hermes-4-405b | 30 | 7.60 | 7.47 | 8.04 | 7.20 | 7.73 | 7.56 | 2 |
| openai/gpt-4o-mini | 30 | 7.43 | 7.17 | 8.03 | 6.70 | 7.73 | 7.54 | 2 |
| amazon/nova-pro-v1 | 30 | 7.38 | 7.04 | 8.23 | 7.04 | 6.83 | 7.77 | 2 |
| microsoft/phi-4 | 28 | 7.29 | 6.95 | 7.97 | 6.34 | 8.24 | 6.93 | 2 |

## Quality scores: by judge

| Candidate | Judge | n | Total | Char | Auth | Lang | Resp | Craft | File |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| openai/gpt-5.4 | mistralai/mistral-large-2512 | 15 | 9.17 | 9.33 | 9.73 | 8.80 | 9.00 | 9.00 | multiaxis_20260515T190416Z.json |
| anthropic/claude-sonnet-4.6 | mistralai/mistral-large-2512 | 15 | 9.13 | 9.40 | 9.53 | 8.53 | 9.13 | 9.07 | multiaxis_20260515T190416Z.json |
| mistralai/mistral-medium-3.1 | mistralai/mistral-large-2512 | 15 | 9.09 | 9.33 | 9.33 | 8.93 | 8.87 | 9.00 | multiaxis_20260515T190642Z.json |
| anthropic/claude-opus-4.7 | mistralai/mistral-large-2512 | 15 | 9.05 | 9.20 | 9.53 | 8.93 | 8.67 | 8.93 | multiaxis_20260515T190416Z.json |
| openai/gpt-5.5 | mistralai/mistral-large-2512 | 15 | 9.05 | 9.27 | 9.60 | 8.53 | 8.87 | 9.00 | multiaxis_20260515T190416Z.json |
| google/gemma-3-27b-it | mistralai/mistral-large-2512 | 15 | 9.03 | 9.20 | 9.60 | 8.73 | 8.60 | 9.00 | multiaxis_20260514T172222Z.json |
| qwen/qwen3-max | mistralai/mistral-large-2512 | 15 | 9.03 | 9.27 | 9.47 | 8.60 | 8.80 | 9.00 | multiaxis_20260514T174548Z.json |
| openai/gpt-5.4-mini | mistralai/mistral-large-2512 | 15 | 9.03 | 9.13 | 9.53 | 8.60 | 8.93 | 8.93 | multiaxis_20260515T190642Z.json |
| qwen/qwen3-235b-a22b-2507 | mistralai/mistral-large-2512 | 15 | 9.00 | 9.33 | 9.67 | 8.47 | 8.53 | 9.00 | multiaxis_20260514T172222Z.json |
| google/gemini-2.5-pro | mistralai/mistral-large-2512 | 15 | 8.97 | 9.13 | 9.40 | 8.33 | 8.93 | 9.07 | multiaxis_20260515T191140Z.json |
| moonshotai/kimi-k2.5 | mistralai/mistral-large-2512 | 15 | 8.96 | 9.20 | 9.33 | 8.40 | 8.73 | 9.13 | multiaxis_20260515T184612Z.json |
| anthropic/claude-haiku-4.5 | mistralai/mistral-large-2512 | 15 | 8.93 | 9.13 | 9.27 | 8.33 | 9.00 | 8.93 | multiaxis_20260514T170413Z.json |
| mistralai/mistral-large-2512 | mistralai/mistral-large-2512 | 15 | 8.88 | 9.07 | 9.27 | 8.40 | 8.67 | 9.00 | multiaxis_20260514T170413Z.json |
| meta-llama/llama-4-scout | mistralai/mistral-large-2512 | 15 | 8.87 | 9.00 | 9.20 | 8.93 | 8.60 | 8.60 | multiaxis_20260515T190642Z.json |
| x-ai/grok-3-mini | mistralai/mistral-large-2512 | 15 | 8.84 | 9.00 | 9.00 | 8.87 | 8.73 | 8.60 | multiaxis_20260514T170413Z.json |
| meta-llama/llama-3.3-70b-instruct | mistralai/mistral-large-2512 | 15 | 8.84 | 8.87 | 9.20 | 8.87 | 8.60 | 8.67 | multiaxis_20260515T190642Z.json |
| google/gemini-2.5-flash | mistralai/mistral-large-2512 | 15 | 8.81 | 8.93 | 9.20 | 8.40 | 8.53 | 9.00 | multiaxis_20260514T170413Z.json |
| x-ai/grok-4.3 | mistralai/mistral-large-2512 | 15 | 8.80 | 8.93 | 9.33 | 8.33 | 8.47 | 8.93 | multiaxis_20260515T190416Z.json |
| meta-llama/llama-4-maverick | mistralai/mistral-large-2512 | 15 | 8.80 | 8.87 | 9.33 | 8.87 | 8.27 | 8.67 | multiaxis_20260515T190642Z.json |
| z-ai/glm-4.6 | mistralai/mistral-large-2512 | 15 | 8.76 | 8.87 | 9.47 | 8.13 | 8.47 | 8.87 | multiaxis_20260515T191118Z.json |
| openai/gpt-5.5 | x-ai/grok-4.3 | 15 | 8.75 | 8.80 | 9.00 | 8.47 | 8.80 | 8.67 | multiaxis_20260515T190357Z.json |
| qwen/qwen3-max | x-ai/grok-4.3 | 15 | 8.65 | 8.80 | 9.07 | 8.67 | 8.33 | 8.40 | multiaxis_20260514T184902Z.json |
| anthropic/claude-opus-4.7 | x-ai/grok-4.3 | 15 | 8.64 | 8.60 | 9.13 | 8.80 | 8.47 | 8.20 | multiaxis_20260515T190357Z.json |
| openai/gpt-5.4-mini | x-ai/grok-4.3 | 15 | 8.63 | 8.53 | 9.07 | 8.47 | 8.80 | 8.27 | multiaxis_20260515T190615Z.json |
| mistralai/mistral-large-2512 | x-ai/grok-4.3 | 15 | 8.61 | 8.60 | 9.00 | 8.60 | 8.53 | 8.33 | multiaxis_20260514T184629Z.json |
| deepseek/deepseek-v3.2 | mistralai/mistral-large-2512 | 15 | 8.59 | 8.73 | 9.27 | 8.00 | 8.20 | 8.73 | multiaxis_20260514T172222Z.json |
| qwen/qwen3-235b-a22b-2507 | x-ai/grok-4.3 | 14 | 8.59 | 8.79 | 9.07 | 8.21 | 8.64 | 8.21 | multiaxis_20260514T182859Z.json |
| mistralai/mistral-medium-3.1 | x-ai/grok-4.3 | 15 | 8.59 | 8.73 | 9.00 | 8.87 | 8.27 | 8.07 | multiaxis_20260515T190615Z.json |
| moonshotai/kimi-k2.5 | x-ai/grok-4.3 | 15 | 8.56 | 8.87 | 9.13 | 8.40 | 8.47 | 7.93 | multiaxis_20260515T184523Z.json |
| openai/gpt-oss-120b | mistralai/mistral-large-2512 | 13 | 8.55 | 8.85 | 9.23 | 8.15 | 8.00 | 8.54 | multiaxis_20260514T172222Z.json |
| openai/gpt-5.4 | x-ai/grok-4.3 | 15 | 8.55 | 8.40 | 9.00 | 8.67 | 8.33 | 8.33 | multiaxis_20260515T190357Z.json |
| anthropic/claude-sonnet-4.6 | x-ai/grok-4.3 | 15 | 8.53 | 8.73 | 9.00 | 8.20 | 8.73 | 8.00 | multiaxis_20260515T190357Z.json |
| nousresearch/hermes-4-405b | mistralai/mistral-large-2512 | 15 | 8.51 | 8.47 | 9.07 | 7.87 | 8.53 | 8.60 | multiaxis_20260515T190642Z.json |
| x-ai/grok-3-mini | x-ai/grok-4.3 | 15 | 8.43 | 8.53 | 8.93 | 8.67 | 8.33 | 7.67 | multiaxis_20260514T184629Z.json |
| google/gemma-3-27b-it | x-ai/grok-4.3 | 11 | 8.42 | 8.27 | 8.82 | 8.27 | 8.64 | 8.09 | multiaxis_20260514T182859Z.json |
| mistralai/mistral-small-24b-instruct-2501 | mistralai/mistral-large-2512 | 15 | 8.32 | 8.33 | 8.67 | 7.80 | 8.33 | 8.47 | multiaxis_20260514T172222Z.json |
| meta-llama/llama-4-scout | x-ai/grok-4.3 | 15 | 8.32 | 8.33 | 8.73 | 8.73 | 8.13 | 7.67 | multiaxis_20260515T190615Z.json |
| microsoft/phi-4 | mistralai/mistral-large-2512 | 15 | 8.28 | 8.20 | 8.87 | 7.53 | 8.40 | 8.40 | multiaxis_20260514T172222Z.json |
| openai/gpt-4o-mini | mistralai/mistral-large-2512 | 15 | 8.27 | 8.27 | 8.93 | 7.67 | 8.00 | 8.47 | multiaxis_20260514T170413Z.json |
| x-ai/grok-4.3 | x-ai/grok-4.3 | 15 | 8.25 | 8.13 | 8.87 | 7.73 | 8.20 | 8.33 | multiaxis_20260515T190357Z.json |
| google/gemma-4-31b-it | x-ai/grok-4.3 | 15 | 8.19 | 8.00 | 8.67 | 7.67 | 8.40 | 8.20 | multiaxis_20260514T204402Z.json |
| google/gemini-2.5-flash | x-ai/grok-4.3 | 15 | 8.16 | 8.00 | 8.80 | 8.00 | 8.27 | 7.73 | multiaxis_20260514T184629Z.json |
| anthropic/claude-haiku-4.5 | x-ai/grok-4.3 | 15 | 8.12 | 8.07 | 8.87 | 7.80 | 8.07 | 7.80 | multiaxis_20260514T184629Z.json |
| google/gemini-2.5-pro | x-ai/grok-4.3 | 15 | 8.12 | 8.00 | 8.67 | 7.33 | 8.53 | 8.07 | multiaxis_20260515T191136Z.json |
| meta-llama/llama-4-maverick | x-ai/grok-4.3 | 15 | 8.03 | 8.00 | 8.87 | 8.33 | 7.20 | 7.73 | multiaxis_20260515T190615Z.json |
| amazon/nova-pro-v1 | mistralai/mistral-large-2512 | 15 | 7.93 | 7.87 | 8.87 | 7.80 | 7.13 | 8.00 | multiaxis_20260515T190642Z.json |
| deepseek/deepseek-v3.2 | x-ai/grok-4.3 | 13 | 7.91 | 7.77 | 8.69 | 6.92 | 7.92 | 8.23 | multiaxis_20260514T182859Z.json |
| z-ai/glm-4.6 | x-ai/grok-4.3 | 15 | 7.89 | 7.73 | 8.73 | 7.07 | 7.80 | 8.13 | multiaxis_20260515T191114Z.json |
| meta-llama/llama-3.3-70b-instruct | x-ai/grok-4.3 | 15 | 7.83 | 7.40 | 8.40 | 8.40 | 7.47 | 7.47 | multiaxis_20260515T190615Z.json |
| deepseek/deepseek-v4-pro | x-ai/grok-4.3 | 14 | 7.79 | 7.79 | 8.21 | 7.21 | 7.79 | 7.93 | multiaxis_20260514T180815Z.json |
| deepseek/deepseek-v4-pro | x-ai/grok-4.3 | 14 | 7.77 | 7.71 | 8.21 | 7.21 | 7.93 | 7.79 | multiaxis_20260514T185110Z.json |
| mistralai/mistral-small-24b-instruct-2501 | x-ai/grok-4.3 | 15 | 7.49 | 7.47 | 7.80 | 7.00 | 8.20 | 7.00 | multiaxis_20260514T182859Z.json |
| openai/gpt-oss-120b | x-ai/grok-4.3 | 13 | 7.12 | 7.31 | 8.31 | 5.92 | 7.54 | 6.54 | multiaxis_20260514T182859Z.json |
| amazon/nova-pro-v1 | x-ai/grok-4.3 | 15 | 6.83 | 6.20 | 7.60 | 6.27 | 6.53 | 7.53 | multiaxis_20260515T190615Z.json |
| nousresearch/hermes-4-405b | x-ai/grok-4.3 | 15 | 6.69 | 6.47 | 7.00 | 6.53 | 6.93 | 6.53 | multiaxis_20260515T190615Z.json |
| openai/gpt-4o-mini | x-ai/grok-4.3 | 15 | 6.60 | 6.07 | 7.13 | 5.73 | 7.47 | 6.60 | multiaxis_20260514T184629Z.json |
| microsoft/phi-4 | x-ai/grok-4.3 | 13 | 6.29 | 5.69 | 7.08 | 5.15 | 8.08 | 5.46 | multiaxis_20260514T182859Z.json |

## Perf probe

| Candidate | n_ok | TTFT p50 ms | TTFT p90 ms | Total p50 ms | Tok/s p50 | Tok/s p90 | JSON free | JSON schema | File |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| x-ai/grok-4.3 | 10 | 704 | 1561 | 3183 | 207.50 | 245.70 | 100.0% | 100.0% | perf_shardB_20260515T192852Z.json |
| openai/gpt-4o-mini | 10 | 816 | 1280 | 1243 | 192.70 | 669.00 | 100.0% | 100.0% | perf_shardA_20260515T192852Z.json |
| google/gemini-2.5-pro | 10 | 1925 | 2073 | 3331 | 143.30 | 165.30 | 0% | 100.0% | perf_shardB_20260515T192852Z.json |
| google/gemini-2.5-flash | 10 | 595 | 790 | 1032 | 99.50 | 197.10 | 100.0% | 100.0% | perf_shardB_20260515T192852Z.json |
| anthropic/claude-haiku-4.5 | 10 | 980 | 1399 | 1989 | 81.60 | 87.20 | 100.0% | 100.0% | perf_shardA_20260515T192852Z.json |
| mistralai/mistral-small-24b-instruct-2501 | 10 | 250 | 290 | 838 | 77.40 | 93.40 | 100.0% | 100.0% | perf_shardB_20260515T192852Z.json |
| openai/gpt-5.4-mini | 10 | 561 | 702 | 1389 | 75.50 | 88.40 | 100.0% | 100.0% | perf_shardA_20260515T192852Z.json |
| microsoft/phi-4 | 10 | 364 | 727 | 1674 | 67.80 | 77.50 | 0% | 100.0% | perf_shardD_20260515T192852Z.json |
| amazon/nova-pro-v1 | 10 | 478 | 643 | 855 | 65.90 | 97.60 | 100.0% | 100.0% | perf_shardD_20260515T192852Z.json |
| openai/gpt-oss-120b | 10 | 427 | 816 | 3072 | 62.70 | 600.60 | 100.0% | 100.0% | perf_shardC_20260515T192852Z.json |
| openai/gpt-5.5 | 10 | 2242 | 4549 | 4067 | 60.00 | 82.40 | 100.0% | 100.0% | perf_shardA_20260515T192852Z.json |
| qwen/qwen3-235b-a22b-2507 | 10 | 363 | 666 | 1696 | 54.20 | 65.30 | 100.0% | 100.0% | perf_20260514T202405Z.json |
| meta-llama/llama-4-scout | 10 | 279 | 389 | 1419 | 54.20 | 105.80 | 90.0% | 100.0% | perf_shardD_20260515T192852Z.json |
| meta-llama/llama-4-maverick | 10 | 412 | 521 | 1601 | 51.10 | 59.80 | 100.0% | 100.0% | perf_shardD_20260515T192852Z.json |
| mistralai/mistral-medium-3.1 | 10 | 390 | 521 | 2089 | 50.40 | 62.40 | 10.0% | 100.0% | perf_shardB_20260515T192852Z.json |
| anthropic/claude-sonnet-4.6 | 10 | 1328 | 5300 | 3221 | 44.10 | 46.10 | 100.0% | 100.0% | perf_shardA_20260515T192852Z.json |
| mistralai/mistral-large-2512 | 10 | 482 | 667 | 1714 | 44.10 | 51.70 | 100.0% | 100.0% | perf_shardB_20260515T192852Z.json |
| meta-llama/llama-3.3-70b-instruct | 10 | 516 | 1452 | 2339 | 43.20 | 55.80 | 90.0% | 90.0% | perf_shardD_20260515T192852Z.json |
| nousresearch/hermes-4-405b | 10 | 333 | 641 | 5623 | 39.40 | 41.10 | 100.0% | 100.0% | perf_shardD_20260515T192852Z.json |
| deepseek/deepseek-v4-pro | 10 | 1317 | 2125 | 6170 | 39.20 | 66.60 | 100.0% | 100.0% | perf_shardC_20260515T192852Z.json |
| anthropic/claude-opus-4.7 | 10 | 1421 | 1557 | 3861 | 39.00 | 41.90 | 100.0% | 100.0% | perf_shardA_20260515T192852Z.json |
| google/gemma-3-27b-it | 10 | 380 | 766 | 2328 | 37.90 | 48.50 | 100.0% | 100.0% | perf_20260514T202405Z.json |
| moonshotai/kimi-k2.5 | 10 | 942 | 2073 | 7220 | 37.60 | 70.70 | 100.0% | 100.0% | perf_shardC_20260515T192852Z.json |
| openai/gpt-5.4 | 10 | 715 | 843 | 2677 | 34.70 | 38.40 | 100.0% | 100.0% | perf_shardA_20260515T192852Z.json |
| z-ai/glm-4.6 | 10 | 1442 | 2844 | 16883 | 34.70 | 53.30 | 100.0% | 100.0% | perf_shardC_20260515T192852Z.json |
| qwen/qwen3-max | 10 | 1173 | 1410 | 2904 | 32.60 | 37.50 | 100.0% | 100.0% | perf_shardC_20260515T192852Z.json |
| deepseek/deepseek-v3.2 | 10 | 691 | 1554 | 2704 | 22.20 | 50.90 | 100.0% | 100.0% | perf_shardC_20260515T192852Z.json |
| google/gemma-4-31b-it | 10 | 1160 | 2111 | 3214 | 21.90 | 30.10 | 100.0% | 100.0% | perf_20260514T202405Z.json |
| qwen/qwen-2.5-72b-instruct | 1 | - | - | 5242 | - | - | 10.0% | 90.0% | perf_20260514T202405Z.json |
| x-ai/grok-3-mini | 0 | - | - | - | - | - | 0% | 0% | perf_shardB_20260515T192852Z.json |

## Unjudged backlog

`qwen/qwen-2.5-72b-instruct`
