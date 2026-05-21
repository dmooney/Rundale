# Demo API Request Profile

Generated: 2026-05-24T02:17:29Z

## Configuration

- Command: `just demo 10 30`
- Duration target: 300s
- Observed window: 300.4s
- Human reading pause: 10s between demo turns
- Provider forced for run: `custom`
- Parish base URL: `http://127.0.0.1:53496`
- Main upstream (dialogue/simulation/demo-player): `http://localhost:8000/v1`
- Small upstream (intent/reaction): `http://localhost:8001/v1`
- Main model requested: `mlx-community/Qwen2.5-14B-Instruct-4bit`
- Small model requested: `mlx-community/Qwen2.5-1.5B-Instruct-4bit`
- Demo process return code: `143`
- Stopped after requested duration: `True`
- Events JSONL: `/Users/dmooney/Rundale/.worktrees/demo-api-profile/docs/proofs/demo-api-profile/20260524T021229Z/events.jsonl`
- Summary JSON: `/Users/dmooney/Rundale/.worktrees/demo-api-profile/docs/proofs/demo-api-profile/20260524T021229Z/summary.json`
- Demo log: `/Users/dmooney/Rundale/.worktrees/demo-api-profile/docs/proofs/demo-api-profile/20260524T021229Z/demo.log`

## Requests By Category

| Category | Requests | Req/min | p50 ms | p95 ms | Errors | Est. input tok | Est. output tok | Prompt chars | Response chars |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| demo-player | 14 | 2.80 | 2089 | 4107 | 0 | 20809 | 373 | 83219 | 1188 |
| intent | 12 | 2.40 | 1081 | 1574 | 0 | 5962 | 412 | 23827 | 1254 |
| dialogue | 7 | 1.40 | 6555 | 26290 | 0 | 16027 | 1107 | 64096 | 4041 |
| simulation | 27 | 5.39 | 2272 | 6240 | 1 | 25787 | 1572 | 103092 | 6032 |
| reaction | 19 | 3.79 | 332 | 549 | 0 | 10467 | 194 | 41844 | 618 |
| travel | 2 | 0.40 | 281 | 590 | 0 | 558 | 64 | 2229 | 284 |
| unknown | 0 | 0.00 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| total_gameplay (excludes demo-player) | 67 | 13.38 | 1437 | 7032 | 1 | 58801 | 3349 | 235088 | 12229 |
| total_observed (includes demo-player) | 81 | 16.18 | 1674 | 6555 | 1 | 79610 | 3722 | 318307 | 13417 |

## Cost Examples

These are estimates from observed/estimated text tokens only. They exclude prompt caching, batch discounts, higher-context surcharges, tools, images, audio, retries outside the proxy, and provider taxes. Local inference cost is `$0.00` API spend.
Static price table last checked: 2026-05-20. Verify provider pages before budget decisions.

| Example model | Input $/1M | Output $/1M | Estimated run cost | Estimated per hour |
|---|---:|---:|---:|---:|
| OpenAI GPT-5.4 mini | $0.75 | $4.50 | $0.059171 | $0.7091 |
| OpenAI GPT-5.4 | $2.50 | $15.00 | $0.197238 | $2.3636 |
| Anthropic Claude Sonnet 4.6 | $3.00 | $15.00 | $0.226638 | $2.7159 |
| Anthropic Claude Haiku 4.5 | $1.00 | $5.00 | $0.075546 | $0.9053 |
| Google Gemini 2.5 Flash | $0.30 | $2.50 | $0.026013 | $0.3117 |
| Google Gemini 2.5 Flash-Lite | $0.10 | $0.40 | $0.007220 | $0.0865 |
| xAI Grok 4.3 | $1.25 | $2.50 | $0.081874 | $0.9811 |
| Mistral Large 3 | $0.50 | $1.50 | $0.034424 | $0.4125 |

Price source URLs checked:
- OpenAI: https://openai.com/api/pricing/
- Anthropic: https://platform.claude.com/docs/en/about-claude/pricing
- Google Gemini: https://ai.google.dev/gemini-api/docs/pricing
- xAI: https://docs.x.ai/developers/models
- Mistral: https://docs.mistral.ai/models/model-cards/mistral-large-3-25-12

## Regression Check

No baseline supplied. Use `--write-baseline <path>` after a trusted run, then pass `--baseline <path>` in later runs.

## Request Events

| # | +s | Category | Model | Stream | Status | ms | Est. in tok | Est. out tok | Error |
|---:|---:|---|---|---:|---:|---:|---:|---:|---|
| 1 | 13.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 3996 | 977 | 38 |  |
| 3 | 17.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 3264 | 865 | 34 |  |
| 4 | 19.4 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 1569 | 918 | 21 |  |
| 6 | 21.0 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1294 | 490 | 25 |  |
| 5 | 20.7 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 6240 | 947 | 40 |  |
| 7 | 22.3 | dialogue | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 6555 | 2094 | 78 |  |
| 8 | 28.9 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 345 | 510 | 6 |  |
| 2 | 13.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 21571 | 845 | 400 | client disconnected while proxy was streaming response body: BrokenPipeError |
| 9 | 38.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1619 | 962 | 43 |  |
| 10 | 40.0 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1345 | 947 | 41 |  |
| 11 | 41.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 2134 | 977 | 45 |  |
| 12 | 42.1 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 1747 | 980 | 29 |  |
| 14 | 43.9 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1184 | 499 | 27 |  |
| 13 | 43.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1674 | 865 | 38 |  |
| 15 | 45.0 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 190 | 519 | 6 |  |
| 16 | 53.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 2924 | 962 | 63 |  |
| 17 | 55.1 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 2572 | 1036 | 24 |  |
| 19 | 57.7 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1472 | 494 | 40 |  |
| 18 | 56.3 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 3046 | 940 | 47 |  |
| 20 | 59.1 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 251 | 514 | 6 |  |
| 21 | 68.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 2551 | 962 | 48 |  |
| 22 | 69.2 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 1757 | 1084 | 27 |  |
| 24 | 71.0 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1574 | 496 | 44 |  |
| 23 | 71.0 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1793 | 942 | 49 |  |
| 25 | 72.5 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 219 | 516 | 6 |  |
| 26 | 82.6 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 1722 | 1136 | 22 |  |
| 27 | 83.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 2272 | 865 | 52 |  |
| 28 | 84.3 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1361 | 491 | 34 |  |
| 29 | 85.7 | dialogue | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 7032 | 2243 | 87 |  |
| 30 | 92.7 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 227 | 511 | 6 |  |
| 31 | 103.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1660 | 977 | 45 |  |
| 32 | 105.1 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1309 | 962 | 40 |  |
| 33 | 106.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 2357 | 942 | 44 |  |
| 34 | 107.3 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 1956 | 1207 | 25 |  |
| 36 | 109.3 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 983 | 498 | 20 |  |
| 37 | 110.3 | travel | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 590 | 283 | 36 |  |
| 35 | 108.8 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 2185 | 940 | 47 |  |
| 38 | 110.9 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | true | 200 | 230 | 725 | 18 |  |
| 39 | 113.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1532 | 942 | 40 |  |
| 40 | 121.2 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 2089 | 1347 | 30 |  |
| 41 | 123.3 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 325 | 499 | 42 |  |
| 42 | 123.6 | dialogue | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 6059 | 2091 | 70 |  |
| 43 | 129.7 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 263 | 522 | 9 |  |
| 44 | 133.4 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1742 | 942 | 48 |  |
| 45 | 142.1 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 2212 | 1393 | 34 |  |
| 46 | 144.3 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1086 | 501 | 46 |  |
| 47 | 145.4 | dialogue | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 6081 | 2203 | 103 |  |
| 48 | 148.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 3159 | 942 | 49 |  |
| 49 | 151.5 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 332 | 524 | 6 |  |
| 50 | 163.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1722 | 942 | 45 |  |
| 51 | 168.5 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 2441 | 1494 | 38 |  |
| 52 | 170.9 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 513 | 511 | 26 |  |
| 53 | 171.4 | dialogue | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 10345 | 2405 | 182 |  |
| 54 | 181.8 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 356 | 535 | 7 |  |
| 55 | 188.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1993 | 942 | 53 |  |
| 56 | 202.4 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 4107 | 1651 | 40 |  |
| 57 | 203.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 4521 | 1074 | 36 |  |
| 58 | 206.5 | dialogue | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 26290 | 2654 | 512 |  |
| 59 | 232.8 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 274 | 534 | 6 |  |
| 60 | 243.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 1437 | 1074 | 38 |  |
| 61 | 246.9 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 1839 | 1905 | 11 |  |
| 62 | 248.7 | travel | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 281 | 275 | 28 |  |
| 63 | 249.0 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | true | 200 | 263 | 730 | 29 |  |
| 64 | 249.3 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | true | 200 | 340 | 728 | 44 |  |
| 65 | 249.6 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 147 | 507 | 7 |  |
| 66 | 253.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 3019 | 1076 | 51 |  |
| 67 | 260.0 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 2949 | 2193 | 24 |  |
| 68 | 262.9 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1055 | 494 | 39 |  |
| 70 | 264.0 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 548 | 515 | 6 |  |
| 69 | 264.0 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 549 | 515 | 7 |  |
| 71 | 264.0 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 549 | 513 | 6 |  |
| 73 | 274.1 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 3832 | 2219 | 25 |  |
| 72 | 273.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 4910 | 988 | 52 |  |
| 74 | 277.9 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1013 | 496 | 31 |  |
| 75 | 278.9 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 405 | 515 | 6 |  |
| 76 | 278.9 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 405 | 517 | 7 |  |
| 77 | 278.9 | reaction | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 405 | 517 | 6 |  |
| 79 | 289.0 | demo-player | `mlx-community/Qwen2.5-14B-Instruct-4bit` | false | 200 | 3044 | 2246 | 23 |  |
| 78 | 288.5 | simulation | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 3693 | 988 | 46 |  |
| 80 | 292.0 | intent | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` | false | 200 | 1081 | 493 | 38 |  |
| 81 | 293.1 | dialogue | `mlx-community/Qwen2.5-14B-Instruct-4bit` | true | 200 | 5291 | 2337 | 75 |  |
