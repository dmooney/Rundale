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
import re
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Target:
    """OpenAI-compatible chat-completions endpoint."""

    model: str
    base_url: str
    api_key_env: str | None = None

    def label(self) -> str:
        """Short human label: bare model name without org prefix."""
        return self.model.split("/")[-1]

    def api_key(self) -> str | None:
        if not self.api_key_env:
            return None
        # Strip surrounding whitespace: secret stores / `.env` files routinely
        # leave a stray leading space or trailing newline on an exported key,
        # which sails through `/v1/models` (often unauthenticated) but gets the
        # candidate *and* judge chat calls rejected with a 401 (an extra space
        # after `Bearer ` is enough — OpenRouter returns "Missing Authentication
        # header"). Normalise here so both paths are immune.
        key = os.environ.get(self.api_key_env)
        if key:
            key = key.strip()
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
    api_key_env: str | None = None
    if "#" in rest:
        base_url, suffix = rest.split("#", 1)
        if not suffix.startswith("env:"):
            raise ValueError(f"target suffix must start with 'env:': {suffix!r}")
        api_key_env = suffix[len("env:") :]
    else:
        base_url = rest
    return Target(model=model.strip(), base_url=base_url.strip(), api_key_env=api_key_env)


def _is_deepseek_direct(target: Target) -> bool:
    """Return true only for DeepSeek's first-party API hostname."""
    return (urllib.parse.urlparse(target.base_url).hostname or "").lower() == "api.deepseek.com"


def _is_google_direct(target: Target) -> bool:
    return (
        urllib.parse.urlparse(target.base_url).hostname or ""
    ).lower() == "generativelanguage.googleapis.com"


def _uses_latest_gemini_sampling_contract(target: Target) -> bool:
    return _is_google_direct(target) and target.model in {
        "gemini-3.6-flash",
        "gemini-3.5-flash-lite",
    }


def _apply_reasoning_request(
    body: dict,
    target: Target,
    *,
    reasoning: dict | None,
    enable_thinking: bool | None,
) -> None:
    """Apply the target's native reasoning controls to an OpenAI-style body."""
    if _is_deepseek_direct(target):
        effort = (reasoning or {}).get("effort")
        if effort == "none" or (effort is None and enable_thinking is False):
            body["thinking"] = {"type": "disabled"}
            return
        if effort is not None or enable_thinking is True:
            body["thinking"] = {"type": "enabled"}
        if effort in {"minimal", "low", "medium", "high"}:
            body["reasoning_effort"] = "high"
        elif effort in {"xhigh", "max"}:
            body["reasoning_effort"] = "max"
        return
    if _is_google_direct(target):
        effort = (reasoning or {}).get("effort")
        if effort in {"minimal", "low", "medium"}:
            body["reasoning_effort"] = effort
        elif effort in {"high", "xhigh", "max"}:
            body["reasoning_effort"] = "high"
        return
    if reasoning is not None:
        body["reasoning"] = reasoning
    elif _is_reasoning_model(target.model):
        body["reasoning"] = _default_reasoning_for(target.model)


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
    # opencode.ai/zen/go/v1 — bare ids (no org prefix). Every model the
    # gateway exposes declares reasoning=true in its registry, so listing
    # the families is simpler than enumerating each.
    "kimi-k2.5",
    "kimi-k2.6",
    "qwen3.5-plus",
    "qwen3.6-plus",
    "qwen3.7-max",
    "glm-5",
    "glm-5.1",
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "minimax-m2.5",
    "minimax-m2.7",
    "mimo-v2.5",
    "mimo-v2.5-pro",
)


def _is_reasoning_model(model_id: str) -> bool:
    mid = model_id.lower()
    return any(mid.startswith(p) for p in REASONING_MODEL_PREFIXES)


# Local mlx_lm.server-hosted models whose chat template defaults to thinking
# mode. For these we inject `chat_template_kwargs={"enable_thinking": False}`
# so the reply we score is the actual answer rather than the leaked reasoning
# trace. (Cloud reasoning models use the `reasoning` body field above instead;
# mlx_lm.server doesn't honour that, only chat_template_kwargs.)
THINKING_MLX_PREFIXES = (
    "mlx-community/Qwen3-",
    "mlx-community/Qwen3.5-",
    "mlx-community/Qwen3.6-",
)


def _is_thinking_mlx_model(model_id: str) -> bool:
    return any(model_id.startswith(p) for p in THINKING_MLX_PREFIXES)


# Local mlx-served models that are *mandatory* reasoners: their chat template
# always opens a thought channel and `chat_template_kwargs={"enable_thinking":
# False}` does NOT suppress it through vllm-mlx (unlike the Qwen3 family above).
# gemma-4 ("gemma4_unified") spends ~250 tokens reasoning before its reply, so
# we serve it with `--reasoning-parser gemma4` (thought → `reasoning_content`,
# answer → `content`) and bump max_tokens so the reply phase has headroom rather
# than truncating mid-thought. Mirrors the opencode-go reasoning bump below.
REASONING_MLX_PREFIXES = ("mlx-community/gemma-4-",)


def _is_reasoning_mlx_model(model_id: str) -> bool:
    mid = model_id.lower()
    return any(mid.startswith(p) for p in REASONING_MLX_PREFIXES)


# Markers a model writes when it leaks its own planning prose into the
# visible `content` field instead of emitting the in-character reply.
# Tuned against the 11 bench-bugs surfaced in the opencode-go 2026-05-25
# sweep — mostly mimo-v2.5-pro / minimax-m2.*. Tracked for a principled
# replacement in #1085 — these heuristics are brittle by design.
#
# Each entry is a regex anchored at the start of the response. Use full
# verb phrases ("hmm, the user is...", "okay, so...") rather than bare
# interjections like "Hmm," — Brigid legitimately opens dialogue with
# those, so matching them in isolation is a false-positive risk.
_COT_OPENERS = (
    r"the user is (asking|telling|requesting)",
    r"the player is (asking|telling|requesting|wondering)",
    r"the person is (asking|telling)",
    r"we (need|are|have) to respond",
    r"i (need|have|should) to respond",
    r"let me (think|draft|craft|consider|plan)\b",
    r"hmm,?\s+(the|so|let me|i (need|should)|we)\b",
    r"okay,?\s+(the|so|let me|i (need|should)|we)\b",
    r"alright,?\s+(the|so|let me|i (need|should)|we)\b",
    r"key (elements|constraints|considerations):",
    r"constraints to (remember|consider):",
    r"steps:",
    r"plan:",
    r"approach:",
)
# Drop the trailing `\b` from the alternation: many openers end in `,`
# or `:` which are non-word characters, and `\b` after a non-word char
# requires a following word char — that never matches when the opener is
# followed by whitespace (the common case). Anchor with `^\s*` only.
_COT_PREFIX_RE = re.compile(
    r"^\s*(?:" + "|".join(_COT_OPENERS) + r")",
    re.IGNORECASE,
)
# In-character dialogue markers — when CoT is detected, scan forward for
# the first one. Reachable from either a line start, a sentence boundary
# on the same line (`. Ah,` / `? Aye,`), or a paragraph break — so
# planning prose ending in "...respond as Brigid. Ah, sure now..." on a
# single line is still recovered. Persona-specific (rundale midwife);
# see #1085 for a persona-agnostic replacement.
_RESUMER_BOUNDARY = r"(?:^|[.!?]\s+|\n\s*)"
_RESUMER_TOKENS = (
    r"Ah[,!]",
    r"Aye[,.]",
    r"Sure[,!]",
    r"Mhuise",
    r"'Tis",
    r"Tis\s",
    r"Mayhap",
    r"Well now",
    r"Now,",
    r"\"",  # quoted dialogue
    r"Brigid[:\s]",
    r"Dia dhuit",
    r"Mo chara",
    r"A leanbh",
    r"A chroí",
)
_DIALOGUE_RESUMER_RE = re.compile(
    _RESUMER_BOUNDARY + r"(" + "|".join(_RESUMER_TOKENS) + r")",
    re.IGNORECASE | re.MULTILINE,
)


def _scrub_chain_of_thought(text: str) -> str:
    """Strip chain-of-thought leak from the start of a reply.

    Returns the original text unchanged if no CoT marker is found, the
    in-character reply if planning prose is followed by actual dialogue,
    or an empty string if the whole response is planning prose (judge
    will then flag the empty reply as a bench-bug).

    Same-line dialogue (planning then `... Ah, sure I'll fetch ye some`
    on the same line, no newline) is also recovered — search starts at
    the end of the matched CoT prefix, not the first newline.
    """
    if not text:
        return text
    m_cot = _COT_PREFIX_RE.match(text)
    if not m_cot:
        return text
    # Scan for a resumer marker anywhere after the CoT prefix. Cover both
    # newline-separated planning blocks AND same-line planning preambles
    # ("Hmm, the user is asking. Ah, sure now, here's some chamomile…").
    # MULTILINE makes the resumers' leading `^` also match after `\n`,
    # but we still need to inject one synthetic line start so a same-line
    # resumer that doesn't appear after a real newline is reachable —
    # do that by anchoring the search at the end of the CoT prefix.
    tail = text[m_cot.end() :]
    m = _DIALOGUE_RESUMER_RE.search(tail)
    if not m:
        return ""  # never resumed in-character → bench-bug
    # group 1 is the marker itself; m.start(1) skips the boundary chars
    # (newline / sentence-end punctuation) so the returned text begins
    # at the in-character marker.
    return tail[m.start(1) :].lstrip()


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


# opencode-go exposes some models *only* via the Anthropic Messages format
# (POST /v1/messages with x-api-key). Calling /v1/chat/completions returns
# 401 "Model X is not supported for format oa-compat". Route these through
# `_call_messages_anthropic` below. Extend the set as the gateway adds more
# Anthropic-only models.
OPENCODE_GO_ANTHROPIC_ONLY: set[str] = {
    "qwen3.7-max",
}


def _is_opencode_go_anthropic_only(target: Target) -> bool:
    return "opencode.ai" in target.base_url and target.model in OPENCODE_GO_ANTHROPIC_ONLY


def _anthropic_headers(target: Target) -> dict:
    headers = {
        "Content-Type": "application/json",
        "anthropic-version": "2023-06-01",
        "User-Agent": "rundale-bench/1.0 (+https://github.com/davidmooney/Rundale)",
    }
    key = target.api_key()
    if key:
        headers["x-api-key"] = key
    return headers


def _anthropic_body(
    target: Target,
    system: str | None,
    user: str,
    *,
    max_tokens: int | None,
    temperature: float,
    schema: dict | None = None,
    stream: bool = False,
) -> dict:
    """Build a /v1/messages request body. When `schema` is given, enforce the
    JSON shape via Anthropic tool_use (forced tool_choice on a single tool
    whose input_schema is the requested JSON schema). The caller extracts
    the tool input as a JSON string from the response."""
    body: dict = {
        "model": target.model,
        "messages": [{"role": "user", "content": user}],
        "max_tokens": max_tokens if max_tokens is not None else 1024,
        "temperature": temperature,
    }
    if system:
        body["system"] = system
    if stream:
        body["stream"] = True
    if schema is not None:
        tool_name = schema.get("name") or "respond"
        body["tools"] = [
            {
                "name": tool_name,
                "description": "Emit the response as structured JSON matching the input_schema.",
                "input_schema": schema.get("schema") or schema,
            }
        ]
        body["tool_choice"] = {"type": "tool", "name": tool_name}
        # opencode-go (Alibaba DashScope upstream) rejects forced tool_choice
        # when the model is in thinking mode: "The tool_choice parameter does
        # not support being set to required or object in thinking mode". Drop
        # the thinking trace when we're enforcing structured output — the
        # tool_use input is the answer; chain-of-thought is wasted tokens here.
        body["thinking"] = {"type": "disabled"}
    return body


def _coerce_nullable_empty_strings(value, schema):
    """Walk a tool_use input alongside its schema and replace `""` with `None`
    on properties whose type list includes "null". Models on opencode-go's
    Anthropic route emit `""` for absent nullable fields even when the system
    prompt asks for `null`; downstream consumers (graders, parsers) treat
    `""` and `None` differently, costing partial credit. This restores the
    semantic intent of the schema's `["string", "null"]` typing.

    Recurses through nested dicts AND arrays: an array of objects whose items
    schema declares nullable properties needs every element coerced, not just
    the top-level container."""
    if not isinstance(schema, dict):
        return value
    if isinstance(value, list):
        item_schema = schema.get("items") or {}
        for i, item in enumerate(value):
            value[i] = _coerce_nullable_empty_strings(item, item_schema)
        return value
    if not isinstance(value, dict):
        return value
    props = schema.get("properties") or {}
    for key, val in list(value.items()):
        prop_schema = props.get(key) or {}
        ptype = prop_schema.get("type")
        nullable = isinstance(ptype, list) and "null" in ptype
        if nullable and val == "":
            value[key] = None
        elif isinstance(val, (dict, list)):
            value[key] = _coerce_nullable_empty_strings(val, prop_schema)
    return value


def _parse_retry_after(header_value, default: float) -> float:
    """Parse a `Retry-After` HTTP response header to a wait in seconds.

    RFC 7231 allows either a non-negative delta-seconds integer or an
    HTTP-date (e.g. `Wed, 21 Oct 2015 07:28:00 GMT`). `float(...)` raises
    `ValueError` on the HTTP-date form and would crash the retry loop. Fall
    back to `default` whenever the header is missing or unparseable as a
    delta-seconds value — we never received an HTTP-date Retry-After from
    the gateways we target in practice, but a future provider could."""
    if not header_value:
        return default
    try:
        return float(header_value)
    except (TypeError, ValueError):
        return default


def _unwrap_raw_arguments(payload):
    """opencode-go's gateway returns `{"raw_arguments": "<partial-JSON>"}` when
    the underlying model's tool_use call produced invalid JSON (usually mid-
    truncation at max_tokens). When `raw_arguments` *does* parse as valid
    JSON — sometimes the gateway wraps a fully-formed payload despite no
    truncation — unwrap it so downstream graders see the intended structure.
    Truly truncated payloads fail to parse here and the raw shape is returned
    unchanged so the grader can flag the failure."""
    if (
        isinstance(payload, dict)
        and set(payload.keys()) == {"raw_arguments"}
        and isinstance(payload["raw_arguments"], str)
    ):
        try:
            return json.loads(payload["raw_arguments"])
        except json.JSONDecodeError:
            return payload
    return payload


def _extract_anthropic_text(data: dict, schema: dict | None = None) -> str:
    """Pull a single text payload from a non-streaming /v1/messages response.

    - For tool_use responses: stringify the `input` field of the tool_use block,
      after unwrapping `raw_arguments` (gateway wrap-around) and coercing
      empty strings to null on schema-nullable properties.
    - Otherwise: concatenate every `text` block, dropping `thinking` blocks.
    """
    blocks = data.get("content") or []
    for b in blocks:
        if b.get("type") == "tool_use":
            payload = _unwrap_raw_arguments(b.get("input", {}))
            if schema is not None:
                inner = schema.get("schema") or schema
                payload = _coerce_nullable_empty_strings(payload, inner)
            return json.dumps(payload)
    return "".join(b.get("text", "") for b in blocks if b.get("type") == "text")


def _call_messages_anthropic(
    target: Target,
    system: str | None,
    user: str,
    *,
    schema: dict | None,
    max_tokens: int | None,
    temperature: float,
    timeout: float,
    max_retries: int,
) -> tuple[str, dict]:
    """POST /v1/messages in Anthropic Messages format. Returns `(text, usage)`.

    Used for opencode-go models that refuse the OpenAI-compat path. Maps the
    Anthropic response shape (content blocks + input_tokens/output_tokens)
    back to the (text, {prompt_tokens, completion_tokens}) contract that the
    rest of eval_lib expects. When `schema` is given, enforces JSON output
    via forced tool_use and returns the tool input as a JSON string.
    """
    body = _anthropic_body(
        target,
        system,
        user,
        max_tokens=max_tokens,
        temperature=temperature,
        schema=schema,
    )
    headers = _anthropic_headers(target)
    url = f"{target.base_url.rstrip('/')}/messages"

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
            # opencode-go's Anthropic route returns transient 500s ~10-30% of
            # the time on complex tool_use schemas (tier3-sim batches);
            # retrying clears most. Treat 500 like 503 for retry purposes.
            if e.code in (429, 500, 503) and attempt < max_retries:
                wait = min(_parse_retry_after(e.headers.get("Retry-After"), 2**attempt), 60.0)
                attempt += 1
                print(
                    f"  [{e.code}] retry {attempt}/{max_retries} after {wait:.0f}s ({target.model})"
                )
                time.sleep(wait)
                continue
            raise

    try:
        text = _extract_anthropic_text(data, schema=schema)
    except (KeyError, IndexError, TypeError) as e:
        raise ValueError(
            f"unexpected anthropic-messages response shape ({type(e).__name__}: {e}). "
            f"Full response: {data}"
        ) from e
    usage = data.get("usage") or {}
    return text, {
        "prompt_tokens": int(usage.get("input_tokens", 0)),
        "completion_tokens": int(usage.get("output_tokens", 0)),
    }


def _stream_messages_anthropic(
    target: Target,
    system: str | None,
    user: str,
    *,
    schema: dict | None,
    max_tokens: int | None,
    temperature: float,
    timeout: float,
) -> dict:
    """Streaming /v1/messages. Returns the same shape as `call_chat_streaming`.

    SSE event grammar (Anthropic):
      event: message_start    → message envelope with usage.input_tokens
      event: content_block_start  → block has type=text | tool_use | thinking
      event: content_block_delta  → delta.type=text_delta|input_json_delta|thinking_delta
      event: content_block_stop
      event: message_delta    → delta + usage.output_tokens
      event: message_stop
    """
    body = _anthropic_body(
        target,
        system,
        user,
        max_tokens=max_tokens,
        temperature=temperature,
        schema=schema,
        stream=True,
    )
    headers = _anthropic_headers(target)
    headers["Accept"] = "text/event-stream"
    url = f"{target.base_url.rstrip('/')}/messages"

    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode("utf-8"),
        headers=headers,
        method="POST",
    )

    text_parts: list[str] = []
    tool_parts: list[str] = []
    block_types: dict[int, str] = {}
    current_event = ""
    ttft_ms: int | None = None
    prompt_tokens: int | None = None
    completion_tokens: int | None = None
    start = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        for raw_line in resp:
            line = raw_line.decode("utf-8", errors="replace").rstrip("\r\n")
            if not line:
                current_event = ""
                continue
            if line.startswith("event:"):
                current_event = line[6:].strip()
                continue
            if not line.startswith("data:"):
                continue
            payload = line[5:].lstrip()
            try:
                evt = json.loads(payload)
            except json.JSONDecodeError:
                continue
            etype = evt.get("type") or current_event
            if etype == "message_start":
                msg = evt.get("message") or {}
                usage = msg.get("usage") or {}
                if "input_tokens" in usage:
                    prompt_tokens = int(usage["input_tokens"])
            elif etype == "content_block_start":
                idx = evt.get("index", 0)
                block_types[idx] = (evt.get("content_block") or {}).get("type", "text")
            elif etype == "content_block_delta":
                idx = evt.get("index", 0)
                btype = block_types.get(idx, "text")
                delta = evt.get("delta") or {}
                dtype = delta.get("type")
                if dtype == "text_delta" and btype == "text":
                    chunk = delta.get("text", "")
                    if chunk:
                        if ttft_ms is None:
                            ttft_ms = int((time.time() - start) * 1000)
                        text_parts.append(chunk)
                elif dtype == "input_json_delta" and btype == "tool_use":
                    chunk = delta.get("partial_json", "")
                    if chunk:
                        if ttft_ms is None:
                            ttft_ms = int((time.time() - start) * 1000)
                        tool_parts.append(chunk)
                # thinking_delta intentionally ignored
            elif etype == "message_delta":
                usage = evt.get("usage") or {}
                if "output_tokens" in usage:
                    completion_tokens = int(usage["output_tokens"])
            elif etype == "message_stop":
                break

    total_ms = int((time.time() - start) * 1000)
    if tool_parts:
        text = "".join(tool_parts)
        if schema is not None:
            try:
                parsed = json.loads(text)
                parsed = _unwrap_raw_arguments(parsed)
                inner = schema.get("schema") or schema
                parsed = _coerce_nullable_empty_strings(parsed, inner)
                text = json.dumps(parsed)
            except (json.JSONDecodeError, AttributeError):
                pass
    else:
        text = "".join(text_parts)
    tps: float | None = None
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


def call_chat(
    target: Target,
    system: str | None,
    user: str,
    *,
    schema: dict | None = None,
    max_tokens: int | None = None,
    temperature: float = 0.7,
    timeout: float = 180.0,
    max_retries: int = 4,
    reasoning: dict | None = None,
    messages: list[dict] | None = None,
    response_format: dict | None = None,
    frequency_penalty: float | None = None,
    enable_thinking: bool | None = None,
) -> tuple[str, dict]:
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
    if _is_opencode_go_anthropic_only(target):
        # opencode-go Anthropic-only models can't speak OpenAI-compat. Route
        # to the /messages adapter, which returns the same (text, usage) tuple.
        # When a schema is provided, the adapter enforces JSON output via
        # forced tool_use and serialises the tool input as the returned text.
        return _call_messages_anthropic(
            target,
            system,
            user,
            schema=schema,
            max_tokens=max_tokens,
            temperature=temperature,
            timeout=timeout,
            max_retries=max_retries,
        )

    # Runtime-faithful path: a captured `messages` array (multi-turn / verbatim
    # roles) overrides the system+user pair; a captured `response_format`
    # (e.g. {"type":"json_object"}, or None) overrides the schema-derived one so
    # the candidate sees exactly the request the live engine sends.
    if messages is not None:
        msgs = list(messages)
    else:
        msgs = []
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
    if frequency_penalty is not None:
        body["frequency_penalty"] = frequency_penalty
    if _uses_latest_gemini_sampling_contract(target):
        body.pop("temperature", None)
        body.pop("frequency_penalty", None)
    if enable_thinking is not None and not (
        _is_deepseek_direct(target) or _is_google_direct(target)
    ):
        body["enable_thinking"] = enable_thinking
        body["chat_template_kwargs"] = {"enable_thinking": enable_thinking}
    if response_format is not None:
        body["response_format"] = response_format
    elif schema is not None:
        body["response_format"] = {"type": "json_schema", "json_schema": schema}
    # Reasoning-suppression syntax is not standardised across OpenAI-compat
    # gateways. Pick the form each target accepts:
    # - opencode.ai/zen — strict OpenAI schema; rejects `reasoning: {...}`
    #   ("Extra inputs are not permitted"). Some downstream providers behind
    #   the gateway accept `reasoning_effort` but with mutually incompatible
    #   enum sets: kimi/qwen/glm accept "none", DeepSeek/Xiaomi reject
    #   "none" (only low|medium|high|max), and Minimax forbids disabling at
    #   all. Sending NOTHING is the only universally-safe choice; chain-of-
    #   thought is handled at parse time via the `reasoning_content`
    #   fallback and the `<think>` regex strip below.
    # - OpenRouter / direct vendor APIs — `reasoning: {enabled|effort|...}`.
    is_opencode_go = "opencode.ai" in target.base_url
    if is_opencode_go:
        # The opencode-go gateway exposes models from many vendors, each
        # with mutually incompatible reasoning controls. Probed 2026-05-25:
        #   kimi-k2.5/k2.6, qwen3.5/3.6-plus, glm-5/5.1 → "none" works
        #     (without it, kimi dumps pure chain-of-thought into content).
        #   deepseek-v4-flash/pro, mimo-v2.5/v2.5-pro → only low|medium|high|max;
        #     "low" is the only level that consistently emits non-empty
        #     content at dialogue's max_tokens=200 (others bleed reasoning).
        #   minimax-m2.5/m2.7 → reasoning cannot be disabled; "low" empties
        #     content; omitting the field yields clean replies.
        mid = target.model.lower()
        if mid.startswith(("kimi-", "qwen3.", "glm-")):
            body["reasoning_effort"] = "none"
        elif mid.startswith(("deepseek-v4-", "mimo-v2")):
            body["reasoning_effort"] = "low"
        # minimax: deliberately omit reasoning_effort.
        # Several opencode-go families burn most of their token budget on
        # internal reasoning (deepseek-v4-* in reasoning_content; mimo-v2.*
        # and minimax-m2.* often leak it into content as "We need to respond
        # as Brigid…" planning prose, or run out before emitting any reply).
        # At dialogue's max_tokens=200 the candidate response is either
        # blank, a single token, or pure chain-of-thought. Bump to 3000
        # across these families so dialogue / reaction / gaeilge / sim
        # have headroom to actually emit a reply. Cost trivial — even at
        # max output 12k tokens × $4/M = $0.05 per call worst case, real
        # usage is <$0.01 per slice.
        if mid.startswith(("deepseek-v4-", "mimo-v2", "minimax-m2")) and (
            max_tokens is None or max_tokens < 3000
        ):
            body["max_tokens"] = 3000
    else:
        _apply_reasoning_request(
            body,
            target,
            reasoning=reasoning,
            enable_thinking=enable_thinking,
        )
    # Local mlx-served mandatory reasoners (gemma-4): the thought lands in
    # reasoning_content via --reasoning-parser, but the model burns ~250-550
    # tokens reasoning before it emits the in-character reply, so dialogue's
    # max_tokens=200 / intent's 100 truncate it mid-thought and leave content
    # empty. Give the reply phase headroom (1500 — normal completions land
    # ~400-600 tokens; this caps the ~20% of prompts that run away reasoning at
    # ~65s rather than letting them burn far longer). Mirrors the opencode-go
    # bump above. vllm-mlx ignores every enable_thinking knob for this arch, so
    # headroom + reasoning-parser is the only available path.
    if _is_reasoning_mlx_model(target.model) and (max_tokens is None or max_tokens < 1500):
        body["max_tokens"] = 1500
    headers = {
        "Content-Type": "application/json",
        # Some providers front their API with Cloudflare (e.g. opencode.ai)
        # which 403s the default Python-urllib UA via firewall rule 1010.
        "User-Agent": "rundale-bench/1.0 (+https://github.com/davidmooney/Rundale)",
    }
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
                wait = min(_parse_retry_after(e.headers.get("Retry-After"), 2**attempt), 60.0)
                attempt += 1
                print(
                    f"  [{e.code}] retry {attempt}/{max_retries} after {wait:.0f}s ({target.model})"
                )
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
            wait = 2**attempt
            attempt += 1
            print(
                f"  [body-{err_code}] retry {attempt}/{max_retries} after {wait:.0f}s ({target.model})"
            )
            time.sleep(wait)
            # Re-issue request — break out of retry block via continue analogue.
            # Simplest: recurse. Recursion bounded by max_retries.
            return call_chat(
                target,
                system,
                user,
                schema=schema,
                max_tokens=max_tokens,
                temperature=temperature,
                timeout=timeout,
                max_retries=max_retries - attempt,
            )

    try:
        msg = data["choices"][0]["message"]
        text = msg.get("content") or ""
        # Reasoning fallback:
        # - OpenRouter exposes a `reasoning` field that, for some models,
        #   holds the actual answer when `content` is empty (kimi-k2-thinking
        #   via OR is the canonical case). Fall back to it.
        # - opencode-go reports chain-of-thought in `reasoning_content` —
        #   that's the *thinking*, not the answer. Falling back would feed
        #   raw "We need to respond as Brigid…" to the judge, scoring 1.0.
        #   So skip it on this gateway; let an empty reply stay empty so
        #   the failure mode is visible. (We bump deepseek-v4-* max_tokens
        #   to 2000 above so the reply phase has headroom in the first
        #   place.)
        if not text.strip():
            if is_opencode_go or _is_reasoning_mlx_model(target.model):
                # opencode-go / local gemma-4 report chain-of-thought in
                # reasoning_content — that's the *thinking*, not the answer.
                # Falling back would feed "* Character: Brigid…" planning prose
                # to the judge. Leave empty so the failure mode stays visible.
                pass
            else:
                for field in ("reasoning_content", "reasoning"):
                    trace = msg.get(field) or ""
                    if trace.strip():
                        text = trace
                        break
        # Some providers emit the thinking trace inline in content, wrapped
        # in <think>…</think>. Strip so the judge scores the actual reply.
        # The `</think>|$` alternation also handles truncated traces where
        # max_tokens cut the model off mid-thought (no closing tag emitted).
        if "<think>" in text:
            text = re.sub(r"<think>.*?(?:</think>|$)\s*", "", text, flags=re.DOTALL)
        # opencode-go mimo-v2.5-pro / minimax-m2.* leak chain-of-thought
        # planning prose into content even with reasoning_effort tuned.
        # Telltale openers ("The user is asking…", "The player wants…",
        # "We need to respond as Brigid…") are recoverable: the model
        # usually delivers the actual reply after the planning block, or
        # not at all. Try to extract the in-character segment; if none
        # found, leave content empty so the judge bench-bug-flags it.
        text = _scrub_chain_of_thought(text) if is_opencode_go else text
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
    system: str | None,
    user: str,
    *,
    schema: dict | None = None,
    max_tokens: int | None = None,
    temperature: float = 0.7,
    timeout: float = 180.0,
    messages: list[dict] | None = None,
    response_format: dict | None = None,
    frequency_penalty: float | None = None,
    enable_thinking: bool | None = None,
    reasoning: dict | None = None,
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
    if _is_opencode_go_anthropic_only(target):
        return _stream_messages_anthropic(
            target,
            system,
            user,
            schema=schema,
            max_tokens=max_tokens,
            temperature=temperature,
            timeout=timeout,
        )
    if messages is not None:
        msgs = list(messages)
    else:
        msgs = []
        if system:
            msgs.append({"role": "system", "content": system})
        msgs.append({"role": "user", "content": user})
    body: dict = {
        "model": target.model,
        "messages": msgs,
        "stream": True,
        "temperature": temperature,
    }
    # Both vllm-mlx and mlx_lm.server support the standard usage trailer, but
    # omit it unless explicitly requested. The perf harness needs real token
    # counts for throughput; keep this local-only because some cloud-compatible
    # gateways reject `stream_options` even though OpenAI documents it.
    if target.base_url.startswith(("http://127.0.0.1", "http://localhost")):
        body["stream_options"] = {"include_usage": True}
    if max_tokens is not None:
        body["max_tokens"] = max_tokens
    if frequency_penalty is not None:
        body["frequency_penalty"] = frequency_penalty
    if _uses_latest_gemini_sampling_contract(target):
        body.pop("temperature", None)
        body.pop("frequency_penalty", None)
    if enable_thinking is not None and not (
        _is_deepseek_direct(target) or _is_google_direct(target)
    ):
        body["enable_thinking"] = enable_thinking
        body["chat_template_kwargs"] = {"enable_thinking": enable_thinking}
    if response_format is not None:
        body["response_format"] = response_format
    elif schema is not None:
        body["response_format"] = {"type": "json_schema", "json_schema": schema}
    is_local = target.base_url.startswith(("http://127.0.0.1", "http://localhost"))
    is_opencode_go = "opencode.ai" in target.base_url
    if not is_opencode_go:
        _apply_reasoning_request(
            body,
            target,
            reasoning=reasoning,
            enable_thinking=enable_thinking,
        )
    headers = {
        "Content-Type": "application/json",
        "Accept": "text/event-stream",
        "User-Agent": "rundale-bench/1.0 (+https://github.com/davidmooney/Rundale)",
    }
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
    ttft_ms: int | None = None
    prompt_tokens: int | None = None
    completion_tokens: int | None = None
    usage_cost: float | None = None
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
                # Player-facing TTFT begins at the first content delta. Cloud
                # gateways may stream hidden reasoning first; counting it made
                # mandatory reasoners look faster than the game can display and
                # polluted the measured output with chain-of-thought. Local and
                # opencode-compatible servers still need the legacy reasoning
                # fallback because some expose the final answer only there.
                chunk = delta.get("content") or (
                    (delta.get("reasoning") or "")
                    if is_local or is_opencode_go
                    else ""
                )
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
                if usage.get("cost") is not None:
                    usage_cost = float(usage["cost"])
    total_ms = int((time.time() - start) * 1000)
    text = "".join(parts)
    tps: float | None = None
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
        "cost": usage_cost,
    }


# USD per 1M tokens (input, output). Verify before relying on totals — these
# are static reference values and providers change pricing without warning.
# Keep entries keyed by exact `model` id used in API calls. Unknown ids
# return 0.0 in `estimate_cost`.
COSTS: dict[str, tuple[float, float]] = {
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
    # xAI via OpenRouter (verify at openrouter.ai/api/v1/models)
    "x-ai/grok-4.3": (3.00, 15.00),
    "x-ai/grok-3-mini": (0.30, 0.50),
    "x-ai/grok-4-fast": (0.20, 0.50),
    # OpenCode Go (flat-rate subscription — opencode.ai/go).
    # Per-call cost reported as $0 since the platform doesn't bill per-token.
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
    "mimo-v2-pro": (0.0, 0.0),
    "mimo-v2-omni": (0.0, 0.0),
    "mimo-v2.5-pro": (0.0, 0.0),
    "mimo-v2.5": (0.0, 0.0),
    "hy3-preview": (0.0, 0.0),
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

# Resolve <repo>/rundale-bench from this file's location:
# parish/scripts/local-eval/eval_lib.py -> parents[3] is <repo>.
BENCH_ROOT = Path(__file__).resolve().parents[3] / "rundale-bench"


def load_slice(
    slice_name: str,
    *,
    version: str = "v1",
    tier: str | None = None,
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


# ---------------------------------------------------------------------------
# Runtime-equivalent dialogue system prompt (rundale persona: Brigid O'Brien)
# ---------------------------------------------------------------------------
#
# The bench is the score-of-record for dialogue regressions. Its system
# prompt must track what the live runtime sends to the LLM — otherwise
# prompt-quality work doesn't show up in the scoreboard (issue #994).
#
# Source of truth is `mods/rundale/prompts/tier1_system.txt`, mirroring
# `parish_npc::build_tier1_system_prompt`. The bench fills the persona
# slots with the synthetic "Brigid O'Brien, 42, midwife" character used
# across rundale-bench scripts and appends the same `language_directive`
# the runtime emits for en-IE / ga-IE (see `parish_npc::language_directive`).

_REPO_ROOT = Path(__file__).resolve().parents[3]
_RUNDALE_TIER1_TEMPLATE = _REPO_ROOT / "mods" / "rundale" / "prompts" / "tier1_system.txt"

# Mirrors `parish_npc::GA_IE_PHRASE_GUIDE` (parish/crates/parish-npc/src/lib.rs).
# Keep in sync when the runtime guide changes.
_GA_IE_PHRASE_GUIDE = (
    "\n    Preferred ga-IE phrases (use these where natural; do not confabulate "
    "other Irish): "
    'Greetings: "Dia dhuit" (hello), "Dia is Muire dhuit" (reply), '
    '"Conas atá tú?" (how are you), "Slán" (goodbye), '
    '"Slán abhaile" (safe home). '
    'Blessings / thanks: "Go raibh maith agat" (thank you), '
    '"Le cúnamh Dé" (with God\'s help), "Buíochas le Dia" (thank God), '
    '"Beannacht Dé ort" (God bless you), "Go n-éirí leat" (good luck to you). '
    'Exclamations: "Mo ghrá" (my love), "A chroí" (dear, sweetheart), '
    '"A stór" (treasure / dear), "A leanbh" (child), "Mhuise" (well, indeed), '
    '"Faith", "Bedad", "Bedambut". '
    'Concepts: "sídhe" (fairy folk), "sí" (fairy mound), '
    '"seanchaí" (storyteller), "céilí" (gathering), '
    '"poitín" (illicit spirits), "piseog" (superstition).'
)


def _language_directive(player: str, native: str | None) -> str:
    """Python mirror of `parish_npc::language_directive`.

    Reproduces the locale clause the runtime appends to every Tier-1
    dialogue system prompt. Defined here so the bench's system prompt
    matches what the real game sends to the LLM.
    """
    directive = (
        f"LANGUAGE: Speak in {player}. "
        f"Use spelling, idioms, and conventions appropriate to that BCP 47 locale."
    )
    player_lower = player.lower()
    if player_lower.startswith("en") and player_lower != "en-us":
        directive += (
            f' Never use en-US spellings such as "color", "realize", '
            f'"favor", "neighbor", or "-ize" verb endings '
            f"— use the spelling appropriate to {player}."
        )
    if native:
        directive += (
            f" Where a native speaker would naturally code-switch, sprinkle words "
            f"and short phrases from {native} into your dialogue and record them "
            f"in the `language_hints` metadata array. "
            f"CRITICAL: {native} is a SPRINKLE only — at most one short phrase "
            f"(1-5 words) per reply, woven into otherwise-{player} prose. "
            f"{player} must carry the meaning of every sentence. "
            f"NEVER reply entirely in {native}, even if the player's question "
            f"seems to invite it. The player may not speak {native}; the meaning "
            f"of your reply must be clear to a {player} speaker who knows zero "
            f"{native}. "
            f"Use ONLY {player} and {native} — no other language under any "
            f"circumstances."
        )
        if native.lower() in ("ga-ie", "ga"):
            directive += _GA_IE_PHRASE_GUIDE
    else:
        directive += f" Stay in {player} — do not invent or import other languages."
    directive += (
        " Every character you emit must be Latin script (a-z, A-Z, accented "
        "Latin such as á é í ó ú ü ñ ç ß) or standard punctuation. "
        "Do NOT emit Cyrillic (Russian), Han (Chinese), Hiragana / Katakana "
        "(Japanese), Hangul (Korean), Arabic, Hebrew, Greek, or Devanagari "
        "characters — replace any tempted non-Latin word with its English or "
        "native-language equivalent, or omit it."
    )
    return directive


# Brigid is the canonical bench persona; her personality string is bench-
# provided (she does not appear in `mods/rundale/npcs.json`).
_BRIGID_PERSONALITY = (
    "kind but direct, with a deep knowledge of local plants and folk medicine. "
    "Has known the player's family for years."
)


def build_dialogue_system_prompt(
    *,
    name: str = "Brigid O'Brien",
    age: int = 42,
    occupation: str = "midwife",
    personality: str = _BRIGID_PERSONALITY,
    mood: str = "content",
    improv: bool = False,
    player_language: str = "en-IE",
    native_language: str | None = "ga-IE",
) -> str:
    """Render the rundale-bench dialogue system prompt.

    Reads `mods/rundale/prompts/tier1_system.txt`, substitutes the persona
    slots, and appends the same language directive the runtime emits. The
    defaults reproduce the historical bench persona (Brigid O'Brien, 42,
    midwife) with the rundale mod's player/native language pair (en-IE /
    ga-IE).
    """
    template = _RUNDALE_TIER1_TEMPLATE.read_text(encoding="utf-8")
    body = template.format(
        name=name,
        age=age,
        occupation=occupation,
        personality=personality,
        mood=mood,
        improv_section="" if not improv else "\n\n[improv-craft guidance enabled]",
        intel_guidance="",
        tone_guidance="",
    )
    return body.rstrip() + "\n\n" + _language_directive(player_language, native_language)
