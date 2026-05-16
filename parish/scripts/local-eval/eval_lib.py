"""Shared target abstraction for local + cloud OpenAI-compatible eval scripts.

A `Target` names an OpenAI-compatible chat-completions endpoint: a base URL,
a model id, and (optionally) an environment variable holding the API key.
Local vllm-mlx slots and cloud providers (OpenAI, Groq, OpenRouter, Together,
xAI, Mistral, DeepSeek, NVIDIA NIM, Google's OpenAI-compat endpoint, the
Anthropic `/v1` OpenAI-compat shim, vLLM, Ollama, LM Studio) all fit.

`parse_target` accepts a compact spec string:

    model@base_url                      # local, no auth
    model@base_url#env:VAR              # cloud, API key in $VAR

Example::

    parse_target("gpt-5.5@https://api.openai.com/v1#env:PARISH_OPENAI_API_KEY")

`call_chat` returns `(text, usage)` where `usage` is a dict with
`prompt_tokens` and `completion_tokens` keys (zeros if the server omits
them). `estimate_cost` consults the static `COSTS` table; unknown models
return 0.0 so the framework keeps working without prices.
"""

from __future__ import annotations

import hashlib
import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Tuple


@dataclass(frozen=True)
class Target:
    """OpenAI-compatible chat-completions endpoint."""

    model: str
    base_url: str
    api_key_env: Optional[str] = None

    def label(self) -> str:
        """Short human label: bare model name without org prefix."""
        return self.model.split("/")[-1]

    def api_key(self) -> Optional[str]:
        if not self.api_key_env:
            return None
        key = os.environ.get(self.api_key_env)
        if not key:
            raise RuntimeError(
                f"target {self.model} requires API key in ${self.api_key_env} but env is empty"
            )
        return key


def parse_target(spec: str) -> Target:
    """Parse `model@base_url[#env:VAR]` into a `Target`."""
    if "@" not in spec:
        raise ValueError(f"target spec must contain '@base_url': {spec!r}")
    model, rest = spec.split("@", 1)
    api_key_env: Optional[str] = None
    if "#" in rest:
        base_url, suffix = rest.split("#", 1)
        if not suffix.startswith("env:"):
            raise ValueError(f"target suffix must start with 'env:': {suffix!r}")
        api_key_env = suffix[len("env:"):]
    else:
        base_url = rest
    return Target(model=model.strip(), base_url=base_url.strip(), api_key_env=api_key_env)


REASONING_MODEL_PREFIXES = (
    "moonshotai/kimi-k2.5",
    "moonshotai/kimi-k2.6",
    "moonshotai/kimi-k2-thinking",
    "z-ai/glm-4.6",
    "z-ai/glm-4.7",
    "openai/o1",
    "openai/o3",
    "openai/o4",
    "anthropic/claude-opus-4.7",
    "anthropic/claude-sonnet-4.6",
    "deepseek/deepseek-r1",
    "google/gemini-2.5-pro",
    "google/gemini-3",
)


def _is_reasoning_model(model_id: str) -> bool:
    mid = model_id.lower()
    return any(mid.startswith(p) for p in REASONING_MODEL_PREFIXES)


def _default_reasoning_for(model_id: str) -> dict:
    """OpenRouter doesn't normalise reasoning-suppression syntax across
    providers, so we pick the form each model actually honours.

    - Most providers (kimi, glm, deepseek, anthropic, openai-o*) accept
      ``{"enabled": false}`` to disable thinking entirely.
    - Google rejects ``enabled`` and ``max_tokens=0`` with HTTP 400 but
      accepts ``effort: "low"``, which caps internal reasoning at ~64
      tokens — enough for short dialogue replies to fit in
      ``max_tokens=200``.
    """
    mid = model_id.lower()
    if mid.startswith("google/"):
        return {"effort": "low"}
    return {"enabled": False}


def call_chat(
    target: Target,
    system: Optional[str],
    user: str,
    *,
    schema: Optional[dict] = None,
    max_tokens: Optional[int] = None,
    temperature: float = 0.7,
    timeout: float = 180.0,
    max_retries: int = 4,
    reasoning: Optional[dict] = None,
) -> Tuple[str, dict]:
    """POST a single chat-completion. Returns `(text, usage)`.

    Retries on HTTP 429 / 503 using the `Retry-After` header (capped at 60 s)
    or exponential backoff (1, 2, 4, 8 s). Free-tier OpenRouter upstream
    rate-limits in particular benefit from this.

    `reasoning` is an OpenRouter-compatible dict passed through verbatim
    (e.g. ``{"enabled": False}`` to disable thinking, ``{"max_tokens": 50}``
    to cap it). When ``None`` AND the model is a known reasoning-class
    model, we default to ``{"enabled": False}`` so cached replies are
    the actual answer rather than truncated mid-thought.
    """
    msgs: list[dict] = []
    if system:
        msgs.append({"role": "system", "content": system})
    msgs.append({"role": "user", "content": user})
    body: dict = {
        "model": target.model,
        "messages": msgs,
        "stream": False,
        "temperature": temperature,
    }
    if max_tokens is not None:
        body["max_tokens"] = max_tokens
    if schema is not None:
        body["response_format"] = {"type": "json_schema", "json_schema": schema}
    if reasoning is not None:
        body["reasoning"] = reasoning
    elif _is_reasoning_model(target.model):
        body["reasoning"] = _default_reasoning_for(target.model)
    headers = {"Content-Type": "application/json"}
    key = target.api_key()
    if key:
        headers["Authorization"] = f"Bearer {key}"
    url = f"{target.base_url.rstrip('/')}/chat/completions"

    attempt = 0
    while True:
        req = urllib.request.Request(
            url,
            data=json.dumps(body).encode("utf-8"),
            headers=headers,
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                data = json.load(resp)
            break
        except urllib.error.HTTPError as e:
            if e.code in (429, 503) and attempt < max_retries:
                retry_after = e.headers.get("Retry-After")
                wait = min(float(retry_after), 60.0) if retry_after else 2 ** attempt
                attempt += 1
                print(f"  [{e.code}] retry {attempt}/{max_retries} after {wait:.0f}s ({target.model})")
                time.sleep(wait)
                continue
            raise
    # Some providers (e.g. xAI on grok-4.3) return capacity / rate-limit
    # signals inside a 200 OK body as `{"error": {"code": 502, "message": ...}}`
    # rather than via HTTP status. Retry those just like HTTP 502/503.
    if isinstance(data, dict) and "choices" not in data and "error" in data:
        err = data.get("error") or {}
        err_code = err.get("code")
        if err_code in (429, 502, 503) and attempt < max_retries:
            wait = 2 ** attempt
            attempt += 1
            print(f"  [body-{err_code}] retry {attempt}/{max_retries} after {wait:.0f}s ({target.model})")
            time.sleep(wait)
            # Re-issue request — break out of retry block via continue analogue.
            # Simplest: recurse. Recursion bounded by max_retries.
            return call_chat(target, system, user, schema=schema, max_tokens=max_tokens,
                             temperature=temperature, timeout=timeout,
                             max_retries=max_retries - attempt)

    try:
        msg = data["choices"][0]["message"]
        text = msg.get("content") or ""
        # Reasoning-class models (kimi-k2.6, kimi-k2-thinking, glm-4.7, etc.)
        # sometimes return content="" with the actual answer in `reasoning`.
        # This happens when max_tokens is consumed by reasoning before
        # content is emitted. Fall back to reasoning rather than failing.
        if not text.strip():
            reasoning = msg.get("reasoning") or ""
            if reasoning.strip():
                text = reasoning
    except (KeyError, IndexError, TypeError) as e:
        raise ValueError(
            f"unexpected chat-completion response shape ({type(e).__name__}: {e}). "
            f"Full response: {data}"
        ) from e
    usage = data.get("usage") or {}
    return text, {
        "prompt_tokens": int(usage.get("prompt_tokens", 0)),
        "completion_tokens": int(usage.get("completion_tokens", 0)),
    }


def call_chat_streaming(
    target: Target,
    system: Optional[str],
    user: str,
    *,
    schema: Optional[dict] = None,
    max_tokens: Optional[int] = None,
    temperature: float = 0.7,
    timeout: float = 180.0,
) -> dict:
    """Streaming chat-completion. Captures TTFT + tok/s alongside text.

    Returns a dict::

        {
            "text": str,
            "ttft_ms": int | None,           # time to first content delta
            "total_ms": int,                 # request → stream-close
            "completion_tokens": int | None, # from final usage line (if provided)
            "prompt_tokens": int | None,
            "tokens_per_second": float | None,
        }

    No retry — for perf measurement we want to see failure modes raw.
    """
    msgs: list[dict] = []
    if system:
        msgs.append({"role": "system", "content": system})
    msgs.append({"role": "user", "content": user})
    body: dict = {
        "model": target.model,
        "messages": msgs,
        "stream": True,
        "temperature": temperature,
    }
    if max_tokens is not None:
        body["max_tokens"] = max_tokens
    if schema is not None:
        body["response_format"] = {"type": "json_schema", "json_schema": schema}
    headers = {"Content-Type": "application/json", "Accept": "text/event-stream"}
    key = target.api_key()
    if key:
        headers["Authorization"] = f"Bearer {key}"
    url = f"{target.base_url.rstrip('/')}/chat/completions"

    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode("utf-8"),
        headers=headers,
        method="POST",
    )

    parts: list[str] = []
    ttft_ms: Optional[int] = None
    prompt_tokens: Optional[int] = None
    completion_tokens: Optional[int] = None
    start = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        for raw_line in resp:
            line = raw_line.decode("utf-8", errors="replace").rstrip("\r\n")
            if not line or not line.startswith("data:"):
                continue
            payload = line[5:].lstrip()
            if payload == "[DONE]":
                break
            try:
                evt = json.loads(payload)
            except json.JSONDecodeError:
                continue
            choices = evt.get("choices") or []
            if choices:
                delta = choices[0].get("delta") or {}
                # Content takes precedence; reasoning fallback for thinking models.
                chunk = delta.get("content") or delta.get("reasoning") or ""
                if chunk:
                    if ttft_ms is None:
                        ttft_ms = int((time.time() - start) * 1000)
                    parts.append(chunk)
            usage = evt.get("usage")
            if isinstance(usage, dict):
                if "prompt_tokens" in usage:
                    prompt_tokens = int(usage["prompt_tokens"])
                if "completion_tokens" in usage:
                    completion_tokens = int(usage["completion_tokens"])
    total_ms = int((time.time() - start) * 1000)
    text = "".join(parts)
    tps: Optional[float] = None
    if completion_tokens and ttft_ms is not None and total_ms > ttft_ms:
        gen_seconds = (total_ms - ttft_ms) / 1000.0
        if gen_seconds > 0:
            tps = completion_tokens / gen_seconds
    return {
        "text": text,
        "ttft_ms": ttft_ms,
        "total_ms": total_ms,
        "completion_tokens": completion_tokens,
        "prompt_tokens": prompt_tokens,
        "tokens_per_second": tps,
    }


# USD per 1M tokens (input, output). Verify before relying on totals — these
# are static reference values and providers change pricing without warning.
# Keep entries keyed by exact `model` id used in API calls. Unknown ids
# return 0.0 in `estimate_cost`.
COSTS: dict[str, Tuple[float, float]] = {
    # Anthropic (verify at console.anthropic.com)
    "claude-opus-4-7": (15.00, 75.00),
    "claude-sonnet-4-6": (3.00, 15.00),
    "claude-haiku-4-5": (1.00, 5.00),
    # OpenAI (verify at openai.com/api/pricing)
    "gpt-5.5": (0.0, 0.0),
    "gpt-5.4-mini": (0.0, 0.0),
    "gpt-5.4-nano": (0.0, 0.0),
    # Google
    "gemini-2.5-pro": (0.0, 0.0),
    "gemini-2.5-flash": (0.0, 0.0),
    "gemini-2.5-flash-lite": (0.0, 0.0),
    # Groq
    "openai/gpt-oss-120b": (0.0, 0.0),
    "llama-3.3-70b-versatile": (0.59, 0.79),
    "llama-3.1-8b-instant": (0.05, 0.08),
    # xAI
    "grok-4.20-reasoning": (0.0, 0.0),
    "grok-4.20-non-reasoning": (0.0, 0.0),
    "grok-4.1-fast-non-reasoning": (0.0, 0.0),
    # OpenRouter paid (verified 2026-05-13 via OpenRouter /api/v1/models)
    "qwen/qwen3-235b-a22b-2507": (0.07, 0.10),
    "deepseek/deepseek-v3.2": (0.25, 0.38),
    "mistralai/mistral-small-24b-instruct-2501": (0.05, 0.08),
    "google/gemini-2.5-flash-lite": (0.10, 0.40),
    # OpenRouter free (rate-limited upstream; $0 per call)
    "openai/gpt-oss-120b:free": (0.0, 0.0),
    "openai/gpt-oss-20b:free": (0.0, 0.0),
    "qwen/qwen3-next-80b-a3b-instruct:free": (0.0, 0.0),
    "meta-llama/llama-3.3-70b-instruct:free": (0.0, 0.0),
    # Local (free)
    "mlx-community/Qwen2.5-1.5B-Instruct-4bit": (0.0, 0.0),
    "mlx-community/Qwen2.5-7B-Instruct-4bit": (0.0, 0.0),
    "mlx-community/Qwen2.5-14B-Instruct-4bit": (0.0, 0.0),
}


def estimate_cost(target: Target, usage: dict) -> float:
    """USD cost for one call. Unknown models cost 0.0 (free or unverified)."""
    rates = COSTS.get(target.model)
    if not rates:
        return 0.0
    in_per_M, out_per_M = rates
    pt = usage.get("prompt_tokens", 0)
    ct = usage.get("completion_tokens", 0)
    return pt / 1_000_000 * in_per_M + ct / 1_000_000 * out_per_M


class CostTracker:
    """Accumulates token + USD totals across a run."""

    def __init__(self) -> None:
        self.prompt_tokens = 0
        self.completion_tokens = 0
        self.usd = 0.0
        self.calls = 0

    def record(self, target: Target, usage: dict) -> None:
        self.prompt_tokens += usage.get("prompt_tokens", 0)
        self.completion_tokens += usage.get("completion_tokens", 0)
        self.usd += estimate_cost(target, usage)
        self.calls += 1

    def summary(self) -> str:
        return (
            f"{self.calls} calls, "
            f"{self.prompt_tokens:,} in + {self.completion_tokens:,} out tokens, "
            f"~${self.usd:.4f}"
        )


# ---------------------------------------------------------------------------
# rundale-bench dataset loader
# ---------------------------------------------------------------------------

# Resolve <repo>/parish/testing/rundale-bench from this file's location:
# parish/scripts/local-eval/eval_lib.py -> parents[2] is <repo>/parish.
BENCH_ROOT = Path(__file__).resolve().parents[2] / "testing" / "rundale-bench"


def load_slice(
    slice_name: str,
    *,
    version: str = "v1",
    tier: Optional[str] = None,
    split: str = "dev",
    verify: bool = True,
) -> list[dict]:
    """Load a frozen rundale-bench slice as a list of records.

    `slice_name` is the basename without extension (e.g. `"dialogue"`).
    `tier`, if set, filters records to that tier (e.g. `"core"`, `"extended"`).
    `split` selects `dev` (the visible 80%) or `holdout` (the sealed 20% at
    `<slice>.holdout.jsonl`). Holdout exists so model picks can be defended
    against contamination — production leaderboard scores come from
    holdout, while local debugging targets dev.
    `verify` (default true) hashes the slice file against the version's
    `MANIFEST.json` and raises `RuntimeError` on mismatch — the freezing
    contract is that the bytes on disk match what was committed.
    """
    suite_dir = BENCH_ROOT / version
    if split == "dev":
        slice_filename = f"{slice_name}.jsonl"
    elif split == "holdout":
        slice_filename = f"{slice_name}.holdout.jsonl"
    else:
        raise ValueError(f"split must be 'dev' or 'holdout', got {split!r}")
    slice_path = suite_dir / slice_filename
    if not slice_path.exists():
        raise FileNotFoundError(f"rundale-bench slice not found: {slice_path}")

    raw = slice_path.read_bytes()
    if verify:
        manifest_path = suite_dir / "MANIFEST.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        slices_meta = manifest.get("slices", {})
        if slice_filename not in slices_meta:
            raise RuntimeError(
                f"rundale-bench/{version}/MANIFEST.json missing entry for "
                f"{slice_filename}. Rebuild via build_manifest.py."
            )
        expected = slices_meta[slice_filename]["sha256"]
        actual = hashlib.sha256(raw).hexdigest()
        if actual != expected:
            raise RuntimeError(
                f"rundale-bench/{version}/{slice_filename} sha256 mismatch: "
                f"manifest={expected} disk={actual}. Re-run the manifest builder "
                f"if the change is intentional and bump the version if frozen=true."
            )

    records = [json.loads(line) for line in raw.decode("utf-8").splitlines() if line.strip()]
    if tier is not None:
        records = [r for r in records if r.get("tier") == tier]
    return records
