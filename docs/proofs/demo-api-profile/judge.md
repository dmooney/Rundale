# Judge: demo-api-profile

Acceptance criteria: met

Verdict: sufficient

Technical debt: clear

The profiler runs the existing `just demo` command with a human-paced 10 second pause, forces local custom inference through a localhost proxy, writes temporary Tauri category overrides for the macOS vLLM-MLX two-slot setup, and emits repeatable Markdown, JSON summary, JSONL event, and demo log artifacts.

The generated report includes per-category and total request rates, latency, error counts, token estimates, and example cloud model cost estimates. The live five-minute vLLM-MLX run produced 81 request events over a 285.0 second API activity window measured from first proxied request start through last request end. Intent, reaction, and travel requests used the 1.5B small slot; dialogue, simulation, and demo-player requests used the 14B main slot. All HTTP statuses were 200; one simulation stream was recorded as a client-disconnect warning after cancellation.

Residual risk: the cost table is intentionally static and should be refreshed against provider pricing pages before budget decisions.
