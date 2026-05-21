# Acceptance Criteria: demo-api-profile

## Task

Add a repeatable profiling harness that runs the existing `just demo` auto-player for a human-paced five-minute sample using local OpenAI-compatible inference, counts every proxied inference API request by category and in total, and writes a regression-friendly report including example cloud costs for common major-lab model rates.

## Criteria

- The harness invokes `just demo` rather than a separate gameplay loop, with configurable duration, pause, and max-turn settings — observable via: running the profiler with `--dry-run` and checking the printed command.
- The default run is local-inference oriented: it points Parish at a local OpenAI-compatible proxy and requires no cloud API key — observable via: the generated report's configuration section showing `provider=custom`, localhost proxy/upstream URLs, and no cloud auth.
- The report groups requests into at least `demo-player`, `intent`, `dialogue`, `simulation`, `reaction`, `travel`, `unknown`, and total rows, with requests/minute and latency columns — observable via: the generated Markdown report table.
- The profiler records request-level JSONL events for regression analysis, including timestamp, category, model, stream flag, status, duration, prompt characters, and response characters — observable via: the JSONL file written beside the report.
- The report includes example costs for popular cloud models using the observed request mix and static per-million-token rates, clearly marked as estimates — observable via: the generated Markdown cost table.
- A `just demo-profile` wrapper runs the profiler with the repository defaults — observable via: `just --list` showing `demo-profile` and the wrapper executing the Python script.

## Verification script

Run: `cargo run --manifest-path parish/Cargo.toml -p parish -- --script parish/testing/fixtures/play_demo-api-profile.txt`

Expected signals in output:
- The script harness starts and accepts `/status`, showing the fixture remains valid.
- The output includes a location/status update after `look`, proving the gameplay harness is still executable while the profiler tooling is developed.

Primary tooling verification:
- `python3 parish/scripts/profile-demo-requests.py --dry-run --duration-secs 300 --pause 10`
- `python3 parish/scripts/profile-demo-requests.py --self-test`
- `just --list`
