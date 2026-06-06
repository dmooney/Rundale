"""Pricing + game-time token profile for rundale-bench v2 (promptfoo).

`COSTS` is a snapshot of `parish/scripts/local-eval/eval_lib.py::COSTS`, copied
here so the promptfoo suite is reproducible without the legacy harness (USD per
1M tokens, `(input, output)`; unknown ids → 0.0).

`GAME_TIME_PROFILE` mirrors the normal-play request profile that
`rundale-bench/build_site_data.py::_estimate_gameplay_cost` consumes: how many
input / output tokens the live engine spends per minute of wall-clock gameplay,
split by inference category. Multiplying these by a provider's price yields the
`gameplay_cost_usd_per_minute` / `_per_hour` figures. The numbers below are a
committed point-in-time profile from a five-minute vLLM-MLX demo run (the same
source `build_normal_play_profile` reads); refresh when the engine's per-turn
token economy shifts.
"""

from __future__ import annotations

# USD per 1M tokens, (input, output). Snapshot — verify before trusting totals.
COSTS: dict[str, tuple[float, float]] = {
    # Anthropic
    "claude-opus-4-7": (15.00, 75.00),
    "claude-sonnet-4-6": (3.00, 15.00),
    "claude-haiku-4-5": (1.00, 5.00),
    # OpenAI
    "gpt-5.5": (0.0, 0.0),
    "gpt-5.4-mini": (0.0, 0.0),
    "gpt-5.4-nano": (0.0, 0.0),
    "gpt-5-mini": (0.25, 2.00),
    # Google
    "gemini-2.5-pro": (0.0, 0.0),
    "gemini-2.5-flash": (0.0, 0.0),
    "gemini-2.5-flash-lite": (0.0, 0.0),
    # Groq
    "openai/gpt-oss-120b": (0.0, 0.0),
    "llama-3.3-70b-versatile": (0.59, 0.79),
    "llama-3.1-8b-instant": (0.05, 0.08),
    # OpenRouter paid
    "qwen/qwen3-235b-a22b-2507": (0.07, 0.10),
    "deepseek/deepseek-v3.2": (0.25, 0.38),
    "mistralai/mistral-small-24b-instruct-2501": (0.05, 0.08),
    "google/gemini-2.5-flash-lite": (0.10, 0.40),
    "meta-llama/llama-3.3-70b-instruct": (0.13, 0.39),
    "deepseek/deepseek-chat": (0.28, 1.10),
    # OpenRouter free
    "openai/gpt-oss-120b:free": (0.0, 0.0),
    "openai/gpt-oss-20b:free": (0.0, 0.0),
    "qwen/qwen3-next-80b-a3b-instruct:free": (0.0, 0.0),
    "meta-llama/llama-3.3-70b-instruct:free": (0.0, 0.0),
    # xAI
    "x-ai/grok-4.3": (3.00, 15.00),
    "x-ai/grok-3-mini": (0.30, 0.50),
    "x-ai/grok-4-fast": (0.20, 0.50),
    "grok-4-fast": (0.20, 0.50),
    # OpenCode Go (flat-rate subscription — $0 per token)
    "qwen3.7-max": (0.0, 0.0),
    "qwen3.6-plus": (0.0, 0.0),
    "qwen3.5-plus": (0.0, 0.0),
    "kimi-k2.6": (0.0, 0.0),
    "kimi-k2.5": (0.0, 0.0),
    "glm-5.1": (0.0, 0.0),
    "glm-5": (0.0, 0.0),
    "deepseek-v4-pro": (0.0, 0.0),
    "deepseek-v4-flash": (0.0, 0.0),
    "minimax-m2.7": (0.0, 0.0),
    "minimax-m2.5": (0.0, 0.0),
    "mimo-v2.5-pro": (0.0, 0.0),
    "mimo-v2.5": (0.0, 0.0),
    # Local (free)
    "mlx-community/Qwen2.5-1.5B-Instruct-4bit": (0.0, 0.0),
    "mlx-community/Qwen2.5-7B-Instruct-4bit": (0.0, 0.0),
    "mlx-community/Qwen2.5-14B-Instruct-4bit": (0.0, 0.0),
}


def estimate_cost(model: str, prompt_tokens: int, completion_tokens: int) -> float:
    """USD for one call. Unknown models cost 0.0. Mirrors eval_lib.estimate_cost."""
    rates = COSTS.get(model)
    if not rates:
        return 0.0
    in_per_m, out_per_m = rates
    return prompt_tokens / 1_000_000 * in_per_m + completion_tokens / 1_000_000 * out_per_m


# Tokens consumed per MINUTE of wall-clock gameplay, by inference category.
# Ported from a committed normal-play demo profile (gameplay categories only;
# the synthetic demo-player bucket is excluded, matching build_site_data.py).
# {category: (input_tokens_per_min, output_tokens_per_min)}.
GAME_TIME_PROFILE: dict[str, tuple[float, float]] = {
    "intent": (1850.0, 110.0),
    "dialogue": (2600.0, 420.0),
    "reaction": (900.0, 130.0),
    "simulation": (3400.0, 880.0),
    "travel": (210.0, 40.0),
}

# Convenience totals across all gameplay categories.
GAME_TIME_TOTAL_INPUT_PER_MIN = sum(v[0] for v in GAME_TIME_PROFILE.values())
GAME_TIME_TOTAL_OUTPUT_PER_MIN = sum(v[1] for v in GAME_TIME_PROFILE.values())

GAME_TIME_PROFILE_LABEL = "Five-minute vLLM-MLX demo run, gameplay categories only"


def gameplay_cost(input_price_per_mtok: float, output_price_per_mtok: float) -> dict:
    """USD/min, USD/hr and per-category USD/min for a provider's price.

    Port of rundale-bench/build_site_data.py::_estimate_gameplay_cost. Local
    ($0) providers return all-zero costs; rank those by throughput instead.
    """
    per_category: dict[str, float] = {}
    for name, (cin, cout) in GAME_TIME_PROFILE.items():
        per_category[name] = (cin * input_price_per_mtok + cout * output_price_per_mtok) / 1_000_000
    per_minute = (
        GAME_TIME_TOTAL_INPUT_PER_MIN * input_price_per_mtok
        + GAME_TIME_TOTAL_OUTPUT_PER_MIN * output_price_per_mtok
    ) / 1_000_000
    return {
        "gameplay_cost_usd_per_minute": per_minute,
        "gameplay_cost_usd_per_hour": per_minute * 60.0,
        "gameplay_cost_by_category_usd_per_minute": per_category,
    }
