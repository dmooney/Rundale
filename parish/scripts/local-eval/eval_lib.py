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
) -> Tuple[str, dict]:
    """POST a single chat-completion. Returns `(text, usage)`.

    Retries on HTTP 429 / 503 using the `Retry-After` header (capped at 60 s)
    or exponential backoff (1, 2, 4, 8 s). Free-tier OpenRouter upstream
    rate-limits in particular benefit from this.
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
    try:
        text = data["choices"][0]["message"]["content"] or ""
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
    verify: bool = True,
) -> list[dict]:
    """Load a frozen rundale-bench slice as a list of records.

    `slice_name` is the basename without extension (e.g. `"dialogue"`).
    `tier`, if set, filters records to that tier (e.g. `"core"`, `"extended"`).
    `verify` (default true) hashes the slice file against the version's
    `MANIFEST.json` and raises `RuntimeError` on mismatch — the freezing
    contract is that the bytes on disk match what was committed.
    """
    suite_dir = BENCH_ROOT / version
    slice_path = suite_dir / f"{slice_name}.jsonl"
    if not slice_path.exists():
        raise FileNotFoundError(f"rundale-bench slice not found: {slice_path}")

    raw = slice_path.read_bytes()
    if verify:
        manifest_path = suite_dir / "MANIFEST.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        expected = manifest["slices"][f"{slice_name}.jsonl"]["sha256"]
        actual = hashlib.sha256(raw).hexdigest()
        if actual != expected:
            raise RuntimeError(
                f"rundale-bench/{version}/{slice_name}.jsonl sha256 mismatch: "
                f"manifest={expected} disk={actual}. Re-run the manifest builder "
                f"if the change is intentional and bump the version if frozen=true."
            )

    records = [json.loads(line) for line in raw.decode("utf-8").splitlines() if line.strip()]
    if tier is not None:
        records = [r for r in records if r.get("tier") == tier]
    return records
