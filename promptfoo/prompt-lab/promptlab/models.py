"""Models under test. Standard library HTTP only; spend metered from usage."""

from __future__ import annotations

import json
import os
import ssl
import time
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass

from .render import Request

GEMINI_URL = "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
OPENAI_URL = "https://api.openai.com/v1/chat/completions"


class ModelConfigError(RuntimeError):
    pass


@dataclass
class Reply:
    model: str
    ok: bool
    dialogue: str | None
    raw: str | None
    error: str | None
    input_tokens: int
    output_tokens: int
    cost_usd: float
    latency_s: float
    finish: str | None
    # The request never completed (HTTP error after retries, timeout): an
    # outage, not a judgement on the prompt, so it is never scored.
    transport_failed: bool = False

    def to_json(self) -> dict:
        return asdict(self)


def price_of(cfg: dict) -> tuple[float, float]:
    price = cfg.get("price")
    if not price or "input" not in price or "output" not in price:
        raise ModelConfigError(f"{cfg.get('model')} has no price; refusing to spend uncounted")
    return float(price["input"]), float(price["output"])


def cost(cfg: dict, input_tokens: int, output_tokens: int) -> float:
    pin, pout = price_of(cfg)
    return (input_tokens * pin + output_tokens * pout) / 1_000_000


def _ssl_context() -> ssl.SSLContext:
    cafile = os.environ.get("SSL_CERT_FILE") or os.environ.get("REQUESTS_CA_BUNDLE")
    if not cafile and os.path.exists("/root/.ccr/ca-bundle.crt"):
        cafile = "/root/.ccr/ca-bundle.crt"
    return ssl.create_default_context(cafile=cafile) if cafile else ssl.create_default_context()


def post_json(
    url: str, body: dict, headers: dict, timeout: float = 120.0, attempts: int = 3
) -> dict:
    data = json.dumps(body).encode()
    last: Exception | None = None
    for attempt in range(attempts):
        req = urllib.request.Request(
            url, data=data, headers={"Content-Type": "application/json", **headers}
        )
        try:
            with urllib.request.urlopen(req, timeout=timeout, context=_ssl_context()) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as err:
            detail = err.read().decode(errors="replace")[:500]
            last = RuntimeError(f"HTTP {err.code}: {detail}")
            if err.code not in (429, 500, 502, 503, 504):
                break
        except (urllib.error.URLError, TimeoutError) as err:
            last = err
        time.sleep(2 ** (attempt + 1))
    raise RuntimeError(str(last))


def _dialogue(text: str | None) -> str | None:
    if not text:
        return None
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        return None
    if (
        isinstance(parsed, dict)
        and set(parsed) == {"dialogue"}
        and isinstance(parsed["dialogue"], str)
        and parsed["dialogue"].strip()
    ):
        return parsed["dialogue"]
    return None


def _gemini_key() -> str:
    key = os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY")
    if not key:
        raise ModelConfigError("set GEMINI_API_KEY (or GOOGLE_API_KEY) to call Gemini")
    return key


def _call_gemini(cfg: dict, request: Request) -> tuple[str | None, int, int, str | None]:
    body = {
        "systemInstruction": {"parts": [{"text": request.system}]},
        "contents": [{"role": "user", "parts": [{"text": request.user}]}],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseJsonSchema": request.output_schema,
            "maxOutputTokens": cfg.get("max_output_tokens", 1024),
        },
    }
    resp = post_json(GEMINI_URL.format(model=cfg["model"]), body, {"x-goog-api-key": _gemini_key()})
    usage = resp.get("usageMetadata", {})
    out_tokens = int(usage.get("candidatesTokenCount", 0)) + int(usage.get("thoughtsTokenCount", 0))
    cand = (resp.get("candidates") or [{}])[0]
    text = "".join(
        p.get("text", "") for p in cand.get("content", {}).get("parts", []) if not p.get("thought")
    )
    return text or None, int(usage.get("promptTokenCount", 0)), out_tokens, cand.get("finishReason")


def _call_openai(cfg: dict, request: Request) -> tuple[str | None, int, int, str | None]:
    key = os.environ.get("OPENAI_API_KEY")
    if not key:
        raise ModelConfigError("set OPENAI_API_KEY to use the OpenAI stand-in")
    body = {
        "model": cfg["model"],
        "messages": [
            {"role": "system", "content": request.system},
            {"role": "user", "content": request.user},
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {"name": "dialogue", "schema": request.output_schema, "strict": False},
        },
        "max_completion_tokens": cfg.get("max_output_tokens", 2048),
    }
    if cfg["model"].startswith("gpt-5"):
        body["reasoning_effort"] = "minimal"
    resp = post_json(OPENAI_URL, body, {"Authorization": f"Bearer {key}"})
    usage = resp.get("usage", {})
    choice = (resp.get("choices") or [{}])[0]
    return (
        choice.get("message", {}).get("content"),
        int(usage.get("prompt_tokens", 0)),
        int(usage.get("completion_tokens", 0)),
        choice.get("finish_reason"),
    )


_PROVIDERS = {"google": _call_gemini, "openai": _call_openai}


def generate(cfg: dict, request: Request) -> Reply:
    price_of(cfg)  # refuse before spending
    call = _PROVIDERS.get(cfg["provider"])
    if call is None:
        raise ModelConfigError(f"unknown provider {cfg['provider']!r}")
    started = time.monotonic()
    try:
        text, tin, tout, finish = call(cfg, request)
    except ModelConfigError:
        raise
    except Exception as err:
        return Reply(
            cfg["model"],
            False,
            None,
            None,
            str(err)[:500],
            0,
            0,
            0.0,
            time.monotonic() - started,
            None,
            transport_failed=True,
        )
    dialogue = _dialogue(text)
    finish_ok = finish in (None, "STOP", "stop")
    ok = dialogue is not None and finish_ok
    error = None if ok else f"no valid dialogue (finish={finish})"
    return Reply(
        cfg["model"],
        ok,
        dialogue,
        text,
        error,
        tin,
        tout,
        cost(cfg, tin, tout),
        time.monotonic() - started,
        finish,
    )
