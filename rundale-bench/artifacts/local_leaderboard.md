

## Local MLX sweeps

Local MLX runs via `local_runner.py`. `peak_RAM_GB` is the live-sampled RSS peak of the mlx_lm.server pid and children. `params_B` is total parameters in billions (with active count for MoE).

| Date (UTC) | hf_repo | slot | quant | params_B | peak_RAM_GB | slice | split | metric | $/run | judge | harness_sha |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 20260528T200716Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 6.82 | intent | dev | label_match=1.000 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T200716Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 6.82 | dialogue | dev | overall=4.60 (c=4.6/a=4.6/l=5.0/r=4.3/cr=4.2) | $0.0003 | judge_v1 | 444a2b53 |
| 20260528T201015Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | intent | dev | label_match=0.800 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T201015Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | dialogue | dev | overall=4.78 (c=5.0/a=5.0/l=5.0/r=4.7/cr=4.3) | $0.0004 | judge_v1 | 444a2b53 |
| 20260528T201015Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | reaction | dev | mean_in_character=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T201015Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | tier2-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T201015Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.60 | tier3-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T201300Z | mlx-community/EXAONE-3.5-7.8B-Instruct-4bit | large | 4bit | 7.8 | 7.05 | intent | dev | label_match=0.000 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T201300Z | mlx-community/EXAONE-3.5-7.8B-Instruct-4bit | large | 4bit | 7.8 | 7.05 | dialogue | dev | overall=0.00 (c=0.0/a=0.0/l=0.0/r=0.0/cr=0.0) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T201300Z | mlx-community/EXAONE-3.5-7.8B-Instruct-4bit | large | 4bit | 7.8 | 7.05 | reaction | dev | mean_in_character=0.00 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T220733Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | intent | dev | label_match=1.000 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T220753Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | intent | dev | label_match=0.800 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T220753Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | dialogue | dev | overall=4.72 (c=4.7/a=4.7/l=5.0/r=4.9/cr=4.5) | $0.0003 | judge_v1 | 444a2b53 |
| 20260528T220753Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | reaction | dev | mean_in_character=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T220753Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | tier2-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T220753Z | mlx-community/OLMo-2-1124-7B-Instruct-4bit | large | 4bit | 7.0 | 4.59 | tier3-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T201300Z | mlx-community/EXAONE-3.5-7.8B-Instruct-4bit | large | 4bit | 7.8 | 7.05 | tier2-sim | dev | schema_valid=0.00 plausibility=0.00 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T221113Z | mlx-community/EXAONE-3.5-7.8B-Instruct-4bit | large | 4bit | 7.8 | 4.77 | intent | dev | label_match=0.000 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T201300Z | mlx-community/EXAONE-3.5-7.8B-Instruct-4bit | large | 4bit | 7.8 | 7.05 | tier3-sim | dev | schema_valid=0.00 plausibility=0.00 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T224303Z | mlx-community/Llama-3.1-Tulu-3-8B-4bit | large | 4bit | 8.0 | 6.76 | intent | dev | label_match=0.900 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T224303Z | mlx-community/Llama-3.1-Tulu-3-8B-4bit | large | 4bit | 8.0 | 6.76 | dialogue | dev | overall=4.60 (c=4.6/a=4.7/l=5.0/r=4.6/cr=4.2) | $0.0003 | judge_v1 | 444a2b53 |
| 20260528T224303Z | mlx-community/Llama-3.1-Tulu-3-8B-4bit | large | 4bit | 8.0 | 6.76 | reaction | dev | mean_in_character=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T224303Z | mlx-community/Llama-3.1-Tulu-3-8B-4bit | large | 4bit | 8.0 | 6.76 | tier2-sim | dev | schema_valid=1.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T224303Z | mlx-community/Llama-3.1-Tulu-3-8B-4bit | large | 4bit | 8.0 | 6.76 | tier3-sim | dev | schema_valid=0.40 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T224625Z | mlx-community/Ministral-8B-Instruct-2410-4bit | large | 4bit | 8.0 | 7.10 | intent | dev | label_match=0.900 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T224625Z | mlx-community/Ministral-8B-Instruct-2410-4bit | large | 4bit | 8.0 | 7.10 | dialogue | dev | overall=4.44 (c=4.2/a=4.8/l=5.0/r=3.9/cr=4.4) | $0.0003 | judge_v1 | 444a2b53 |
| 20260528T224625Z | mlx-community/Ministral-8B-Instruct-2410-4bit | large | 4bit | 8.0 | 7.10 | reaction | dev | mean_in_character=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T224625Z | mlx-community/Ministral-8B-Instruct-2410-4bit | large | 4bit | 8.0 | 7.10 | tier2-sim | dev | schema_valid=0.80 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T224625Z | mlx-community/Ministral-8B-Instruct-2410-4bit | large | 4bit | 8.0 | 7.10 | tier3-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T225045Z | mlx-community/OLMo-2-1124-13B-Instruct-4bit | large | 4bit | 13.0 | 11.13 | intent | dev | label_match=0.900 | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T225045Z | mlx-community/OLMo-2-1124-13B-Instruct-4bit | large | 4bit | 13.0 | 11.13 | dialogue | dev | overall=4.56 (c=4.4/a=4.6/l=5.0/r=4.2/cr=4.6) | $0.0004 | judge_v1 | 444a2b53 |
| 20260528T225045Z | mlx-community/OLMo-2-1124-13B-Instruct-4bit | large | 4bit | 13.0 | 11.13 | reaction | dev | mean_in_character=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T225045Z | mlx-community/OLMo-2-1124-13B-Instruct-4bit | large | 4bit | 13.0 | 11.13 | tier2-sim | dev | schema_valid=1.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T225045Z | mlx-community/OLMo-2-1124-13B-Instruct-4bit | large | 4bit | 13.0 | 11.13 | tier3-sim | dev | schema_valid=0.20 plausibility=0.00 (pending_judge) | $0.0000 | judge_v1 | 444a2b53 |
| 20260528T231939Z | mlx-community/Qwen3-30B-A3B-4bit | large | 4bit | 30.0 (3.0 active) | 13.56 | intent | dev | label_match=0.700 | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T231939Z | mlx-community/Qwen3-30B-A3B-4bit | large | 4bit | 30.0 (3.0 active) | 13.56 | dialogue | dev | overall=0.00 (c=0.0/a=0.0/l=0.0/r=0.0/cr=0.0) | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T231939Z | mlx-community/Qwen3-30B-A3B-4bit | large | 4bit | 30.0 (3.0 active) | 13.56 | reaction | dev | mean_in_character=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T231939Z | mlx-community/Qwen3-30B-A3B-4bit | large | 4bit | 30.0 (3.0 active) | 13.56 | tier2-sim | dev | schema_valid=1.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T231939Z | mlx-community/Qwen3-30B-A3B-4bit | large | 4bit | 30.0 (3.0 active) | 13.56 | tier3-sim | dev | schema_valid=0.70 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T232742Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 14.03 | intent | dev | label_match=0.000 | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T232742Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 14.03 | dialogue | dev | overall=0.00 (c=0.0/a=0.0/l=0.0/r=0.0/cr=0.0) | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T232742Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 14.03 | reaction | dev | mean_in_character=0.00 | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T232742Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 14.03 | tier2-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 992ad2a8 |
| 20260528T235513Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 16.18 | intent | dev | label_match=0.000 | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260528T235513Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 16.18 | dialogue | dev | overall=0.00 (c=0.0/a=0.0/l=0.0/r=0.0/cr=0.0) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260528T235513Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 16.18 | reaction | dev | mean_in_character=0.00 | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260528T235513Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 16.18 | tier2-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260528T235513Z | mlx-community/gemma-2-27b-it-4bit | large | 4bit | 27.0 | 16.18 | tier3-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T000349Z | mlx-community/Yi-1.5-34B-Chat-4bit | large | 4bit | 34.0 | 17.32 | intent | dev | label_match=0.000 | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T000349Z | mlx-community/Yi-1.5-34B-Chat-4bit | large | 4bit | 34.0 | 17.32 | dialogue | dev | overall=0.00 (c=0.0/a=0.0/l=0.0/r=0.0/cr=0.0) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T000349Z | mlx-community/Yi-1.5-34B-Chat-4bit | large | 4bit | 34.0 | 17.32 | reaction | dev | mean_in_character=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T000349Z | mlx-community/Yi-1.5-34B-Chat-4bit | large | 4bit | 34.0 | 17.32 | tier2-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T000349Z | mlx-community/Yi-1.5-34B-Chat-4bit | large | 4bit | 34.0 | 17.32 | tier3-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T003123Z | mlx-community/Qwen2.5-Coder-32B-Instruct-4bit | large | 4bit | 32.0 | 15.24 | intent | dev | label_match=0.800 | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T003123Z | mlx-community/Qwen2.5-Coder-32B-Instruct-4bit | large | 4bit | 32.0 | 15.24 | dialogue | dev | overall=0.00 (c=0.0/a=0.0/l=0.0/r=0.0/cr=0.0) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T003123Z | mlx-community/Qwen2.5-Coder-32B-Instruct-4bit | large | 4bit | 32.0 | 15.24 | reaction | dev | mean_in_character=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T003123Z | mlx-community/Qwen2.5-Coder-32B-Instruct-4bit | large | 4bit | 32.0 | 15.24 | tier2-sim | dev | schema_valid=0.60 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 8b715111 |
| 20260529T003123Z | mlx-community/Qwen2.5-Coder-32B-Instruct-4bit | large | 4bit | 32.0 | 15.24 | tier3-sim | dev | schema_valid=0.00 plausibility=0.00 (pending_judge) | $0.0000 | judge_sonnet_v1 | 8b715111 |
