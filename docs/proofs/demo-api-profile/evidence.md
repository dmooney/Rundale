# Evidence: demo-api-profile

Evidence type: live gameplay transcript

## Run Artifacts

- Five-minute profiler run: `docs/proofs/demo-api-profile/20260524T021229Z/report.md`
- Request events: `docs/proofs/demo-api-profile/20260524T021229Z/events.jsonl`
- Request summary: `docs/proofs/demo-api-profile/20260524T021229Z/summary.json`
- Demo transcript: `docs/proofs/demo-api-profile/20260524T021229Z/demo.log`

## Commands

```sh
python3 parish/scripts/profile-demo-requests.py --dry-run --duration-secs 300 --pause 10
python3 parish/scripts/profile-demo-requests.py --self-test
python3 -m py_compile parish/scripts/profile-demo-requests.py
just --list
just demo-profile
cargo run --manifest-path parish/Cargo.toml -p parish -- --script parish/testing/fixtures/play_demo-api-profile.txt
```

## Acceptance Criteria Mapping

- Invokes `just demo`: dry run printed `command: just demo 10 30`; the live demo log begins with `$ just demo 10 30`.
- Local inference only: the report shows `Provider forced for run: custom`, `Parish base URL: http://127.0.0.1:53496`, main upstream `http://localhost:8000/v1`, small upstream `http://localhost:8001/v1`, main model `mlx-community/Qwen2.5-14B-Instruct-4bit`, and small model `mlx-community/Qwen2.5-1.5B-Instruct-4bit`.
- Category and total report: the generated report has rows for `demo-player`, `intent`, `dialogue`, `simulation`, `reaction`, `travel`, `unknown`, `total_gameplay`, and `total_observed`, including requests/minute and latency columns.
- Request-level JSONL: `events.jsonl` contains 81 request records with timestamp, category, model, stream flag, status, duration, prompt characters, and response characters.
- Cost examples: the report includes estimated run and hourly costs for OpenAI, Anthropic, Google, xAI, and Mistral example models based on observed token estimates.
- `just demo-profile` wrapper: `just --list` shows `demo-profile DURATION="300" PAUSE="10" MODEL="mlx-community/Qwen2.5-14B-Instruct-4bit" UPSTREAM="http://localhost:8000/v1"`.

## Measured Result

- Observed API activity window: 285.0 seconds, measured from first proxied request start through last request end.
- Gameplay requests: 67 total, 14.10 requests/minute.
- Observed requests including demo auto-player: 81 total, 17.05 requests/minute.
- Category rates: `demo-player` 2.95/minute, `intent` 2.53/minute, `dialogue` 1.47/minute, `simulation` 5.68/minute, `reaction` 4.00/minute, `travel` 0.42/minute, `unknown` 0.00/minute.
- HTTP failures: 0. The profiler recorded 1 cancelled simulation stream as a client-disconnect warning.

## Notes

The vLLM-MLX run used separate local slots for main narrative/simulation traffic and small intent/reaction/travel traffic. Local API spend is `$0.00`; the cloud cost table is a static estimate and should be refreshed before budget decisions.
