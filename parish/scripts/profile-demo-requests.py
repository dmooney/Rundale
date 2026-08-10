#!/usr/bin/env python3
"""Profile inference request volume during `just demo`.

The harness starts a transparent proxy, points Parish at it, runs `just demo`,
and records every inference request. It understands both OpenAI-compatible
chat completions and Google's native Interactions request/response/SSE usage.
The default loadout matches the macOS local-inference default:
vLLM-MLX with the larger dialogue/simulation slot on port 8000 and the small
intent/reaction slot on port 8001.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import errno
import json
import math
import os
import signal
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Protocol

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_REPORT_DIR = REPO_ROOT / "docs" / "proofs" / "demo-api-profile"
GEMINI_CAPABILITY_SNAPSHOT = json.loads(
    (REPO_ROOT / "parish" / "config" / "gemini-3.6-flash-capabilities.json").read_text()
)
GEMINI_STANDARD_RATES = GEMINI_CAPABILITY_SNAPSHOT["pricing_usd_per_million_tokens"]["standard"]
DEFAULT_UPSTREAM = "http://localhost:8000/v1"
DEFAULT_SMALL_UPSTREAM = "http://localhost:8001/v1"
DEFAULT_MODEL = os.environ.get(
    "PARISH_PROFILE_MODEL",
    "mlx-community/Qwen2.5-14B-Instruct-4bit",
)
DEFAULT_SMALL_MODEL = os.environ.get(
    "PARISH_PROFILE_SMALL_MODEL",
    "mlx-community/Qwen2.5-1.5B-Instruct-4bit",
)
DEFAULT_DURATION_SECS = 300
DEFAULT_PAUSE_SECS = 10.0
SMALL_SLOT_CATEGORIES = {"intent", "reaction"}
CATEGORY_ORDER = [
    "demo-player",
    "intent",
    "dialogue",
    "simulation",
    "reaction",
    "travel",
    "unknown",
]
GAMEPLAY_CATEGORIES = [c for c in CATEGORY_ORDER if c != "demo-player"]
HOP_BY_HOP_HEADERS = {
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
}
CLIENT_DISCONNECT_ERRNOS = {
    errno.EPIPE,
    errno.ECONNRESET,
    errno.ECONNABORTED,
}

# Static USD rates per 1M tokens. These are report examples, not procurement
# truth; refresh them when provider pricing changes.
EXAMPLE_MODEL_PRICES = [
    ("OpenAI GPT-5.4 mini", 0.75, 4.50),
    ("OpenAI GPT-5.4", 2.50, 15.00),
    ("Anthropic Claude Sonnet 4.6", 3.00, 15.00),
    ("Anthropic Claude Haiku 4.5", 1.00, 5.00),
    (
        "Google Gemini 3.6 Flash (Standard)",
        GEMINI_STANDARD_RATES["input"],
        GEMINI_STANDARD_RATES["output_and_thought"],
    ),
    ("Google Gemini 2.5 Flash-Lite", 0.10, 0.40),
    ("xAI Grok 4.3", 1.25, 2.50),
    ("Mistral Large 3", 0.50, 1.50),
]
PRICE_TABLE_CHECKED = GEMINI_CAPABILITY_SNAPSHOT["checked_at"]
PRICE_SOURCES = [
    ("OpenAI", "https://openai.com/api/pricing/"),
    ("Anthropic", "https://platform.claude.com/docs/en/about-claude/pricing"),
    ("Google Gemini", "https://ai.google.dev/gemini-api/docs/pricing"),
    ("xAI", "https://docs.x.ai/developers/models"),
    ("Mistral", "https://docs.mistral.ai/models/model-cards/mistral-large-3-25-12"),
]


@dataclass
class ApiEvent:
    request_id: int
    started_at: str
    elapsed_since_run_start_secs: float
    category: str
    method: str
    path: str
    model: str
    stream: bool
    response_format: str
    api_mode: str
    status: int
    duration_ms: int
    ttft_ms: int | None
    prompt_chars: int
    system_chars: int
    response_chars: int
    prompt_tokens_reported: int | None
    completion_tokens_reported: int | None
    cached_tokens_reported: int | None
    thought_tokens_reported: int | None
    total_tokens_reported: int | None
    terminal_status: str | None
    provider_request_id: str | None
    effective_service_tier: str | None
    input_tokens_estimated: int
    output_tokens_estimated: int
    error: str | None


class Recorder:
    def __init__(self) -> None:
        self._events: list[ApiEvent] = []
        self._lock = threading.Lock()
        self._next_id = 1
        self.started_monotonic = time.monotonic()

    def next_id(self) -> int:
        with self._lock:
            request_id = self._next_id
            self._next_id += 1
            return request_id

    def record(self, event: ApiEvent) -> None:
        with self._lock:
            self._events.append(event)

    def events(self) -> list[ApiEvent]:
        with self._lock:
            return list(self._events)


class _RoutingConfig(Protocol):
    """Structural interface for objects that carry upstream routing config.

    Both ``ProfilingServer`` and the lightweight ``argparse.Namespace`` stub
    used in ``self_test`` satisfy this protocol.
    """

    upstream: str
    small_upstream: str
    small_model: str


class ProfilingServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def __init__(
        self,
        server_address: tuple[str, int],
        recorder: Recorder,
        upstream: str,
        small_upstream: str,
        small_model: str,
        timeout_secs: float,
    ) -> None:
        super().__init__(server_address, ProxyHandler)
        self.recorder = recorder
        self.upstream = upstream.rstrip("/")
        self.small_upstream = small_upstream.rstrip("/")
        self.small_model = small_model
        self.timeout_secs = timeout_secs
        self.quiet: bool = False


class ProxyHandler(BaseHTTPRequestHandler):
    server_version = "ParishDemoProfiler/1.0"
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args: object) -> None:
        if getattr(self.server, "quiet", True):
            return
        super().log_message(fmt, *args)

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        self._forward_without_recording()

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        recorder: Recorder = self.server.recorder  # type: ignore[attr-defined]
        request_id = recorder.next_id()
        started = time.monotonic()
        raw_body = self.rfile.read(int(self.headers.get("Content-Length", "0") or 0))
        body = parse_json(raw_body)
        category = classify_request(body)
        model = str(body.get("model") or "")
        stream = bool(body.get("stream"))
        response_format = response_format_label(body.get("response_format"))
        api_mode = "google-interactions" if self.path.endswith("/interactions") else "openai-compat"
        prompt_chars, system_chars = prompt_char_counts(body)
        status = 502
        response_bytes = b""
        error: str | None = None

        try:
            status, response_bytes, error, ttft_ms = self._forward_bytes(raw_body, category, model)
        except Exception as exc:  # pragma: no cover - exercised manually.
            error = str(exc)
            status = 502
            payload = json.dumps({"error": error}).encode("utf-8")
            try:
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.send_header("Connection", "close")
                self.end_headers()
                self.wfile.write(payload)
            except Exception as write_exc:
                if not client_disconnect_message("sending proxy error", write_exc):
                    raise
            response_bytes = payload
            ttft_ms = None

        metrics = response_metrics(response_bytes, stream)
        duration_ms = int((time.monotonic() - started) * 1000)
        event = ApiEvent(
            request_id=request_id,
            started_at=utc_now(),
            elapsed_since_run_start_secs=round(
                started - recorder.started_monotonic,
                3,
            ),
            category=category,
            method="POST",
            path=self.path,
            model=model,
            stream=stream,
            response_format=response_format,
            api_mode=api_mode,
            status=status,
            duration_ms=duration_ms,
            ttft_ms=ttft_ms,
            prompt_chars=prompt_chars,
            system_chars=system_chars,
            response_chars=metrics["response_chars"],
            prompt_tokens_reported=metrics["prompt_tokens_reported"],
            completion_tokens_reported=metrics["completion_tokens_reported"],
            cached_tokens_reported=metrics["cached_tokens_reported"],
            thought_tokens_reported=metrics["thought_tokens_reported"],
            total_tokens_reported=metrics["total_tokens_reported"],
            terminal_status=metrics["terminal_status"],
            provider_request_id=metrics["provider_request_id"],
            effective_service_tier=metrics["effective_service_tier"],
            input_tokens_estimated=estimate_tokens(prompt_chars),
            output_tokens_estimated=metrics["output_tokens_estimated"],
            error=error or metrics["error"],
        )
        recorder.record(event)

    def _forward_without_recording(self) -> None:
        try:
            upstream_url = build_upstream_url(self.server.upstream, self.path)  # type: ignore[attr-defined]
            req = urllib.request.Request(upstream_url, method="GET")
            copy_request_headers(self.headers, req)
            with urllib.request.urlopen(req, timeout=self.server.timeout_secs) as resp:  # type: ignore[attr-defined]
                data = resp.read()
                self.send_response(resp.status)
                copy_response_headers(resp.headers, self, len(data))
                self.end_headers()
                self.wfile.write(data)
        except urllib.error.HTTPError as exc:
            data = exc.read()
            self.send_response(exc.code)
            copy_response_headers(exc.headers, self, len(data))
            self.end_headers()
            self.wfile.write(data)
        except Exception as exc:
            data = json.dumps({"error": str(exc)}).encode("utf-8")
            self.send_response(502)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(data)

    def _forward_bytes(
        self, raw_body: bytes, category: str, model: str
    ) -> tuple[int, bytes, str | None, int | None]:
        forward_started = time.monotonic()
        ttft_ms: int | None = None
        upstream = upstream_for_request(self.server, category, model)  # type: ignore[arg-type]
        upstream_url = build_upstream_url(upstream, self.path)
        req = urllib.request.Request(upstream_url, data=raw_body, method="POST")
        copy_request_headers(self.headers, req)
        chunks: list[bytes] = []
        client_connected = True
        forward_error: str | None = None
        try:
            with urllib.request.urlopen(req, timeout=self.server.timeout_secs) as resp:  # type: ignore[attr-defined]
                try:
                    self.send_response(resp.status)
                    copy_response_headers(resp.headers, self, None)
                    self.end_headers()
                except Exception as exc:
                    disconnect = client_disconnect_message("sending response headers", exc)
                    if not disconnect:
                        raise
                    client_connected = False
                    forward_error = disconnect
                while True:
                    chunk = resp.read(8192)
                    if not chunk:
                        break
                    if ttft_ms is None:
                        ttft_ms = int((time.monotonic() - forward_started) * 1000)
                    chunks.append(chunk)
                    if not client_connected:
                        continue
                    try:
                        self.wfile.write(chunk)
                        self.wfile.flush()
                    except Exception as exc:
                        disconnect = client_disconnect_message("streaming response body", exc)
                        if not disconnect:
                            raise
                        client_connected = False
                        forward_error = disconnect
                return resp.status, b"".join(chunks), forward_error, ttft_ms
        except urllib.error.HTTPError as exc:
            data = exc.read()
            try:
                self.send_response(exc.code)
                copy_response_headers(exc.headers, self, len(data))
                self.end_headers()
                self.wfile.write(data)
            except Exception as write_exc:
                disconnect = client_disconnect_message("forwarding upstream error", write_exc)
                if not disconnect:
                    raise
                forward_error = disconnect
            return exc.code, data, forward_error, None


def utc_now() -> str:
    return (
        dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    )


def parse_json(raw: bytes) -> dict[str, Any]:
    try:
        parsed = json.loads(raw.decode("utf-8"))
    except Exception:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def content_to_text(content: Any) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            if isinstance(item, dict):
                text = item.get("text") or item.get("content") or ""
                if isinstance(text, str):
                    parts.append(text)
        return "\n".join(parts)
    return ""


def request_texts(body: dict[str, Any]) -> tuple[str, str, str]:
    system_parts: list[str] = []
    user_parts: list[str] = []
    all_parts: list[str] = []
    for msg in body.get("messages") or []:
        if not isinstance(msg, dict):
            continue
        text = content_to_text(msg.get("content"))
        all_parts.append(text)
        role = msg.get("role")
        if role == "system":
            system_parts.append(text)
        elif role == "user":
            user_parts.append(text)
    system = "\n".join(system_parts)
    user = "\n".join(user_parts)
    # Native Google Interactions keeps the stable prefix in
    # system_instruction and the dynamic turn suffix in input.
    if not system:
        system = content_to_text(body.get("system_instruction"))
    if not user:
        user = content_to_text(body.get("input"))
    if not all_parts:
        all_parts = [part for part in (system, user) if part]
    return system, user, "\n".join(all_parts)


def prompt_char_counts(body: dict[str, Any]) -> tuple[int, int]:
    system, user, all_text = request_texts(body)
    prompt_chars = len(all_text)
    system_chars = len(system)
    if prompt_chars == 0 and isinstance(body.get("prompt"), str):
        prompt_chars = len(body["prompt"])
    return prompt_chars or len(user), system_chars


def response_format_label(value: Any) -> str:
    if not isinstance(value, dict):
        return ""
    if value.get("type") == "json_schema":
        return "json_schema"
    if value.get("type") == "json_object":
        return "json_object"
    if value.get("mime_type") == "application/json":
        return "json_schema" if value.get("schema") else "json_object"
    return str(value.get("type") or "")


def classify_request(body: dict[str, Any]) -> str:
    system, user, all_text = request_texts(body)
    haystack = f"{system}\n{user}\n{json.dumps(body, sort_keys=True)}".lower()
    stream = bool(body.get("stream"))
    response_format = response_format_label(body.get("response_format"))

    if "you are playing rundale" in haystack and '"action"' in haystack:
        return "demo-player"
    if "input parser" in haystack or (
        '"intent"' in haystack and '"target"' in haystack and '"dialogue"' in haystack
    ):
        return "intent"
    if (
        "single npc would visibly react" in haystack
        or "choose one emoji or null" in haystack
        or '"emoji":' in haystack
        or "available palette" in haystack
    ):
        return "reaction"
    if (
        "ambient narration for a walking scene" in haystack
        or "write one new line in the same register" in haystack
        or "examples of tone will be provided" in haystack
    ):
        return "travel"
    if (
        "tier 2" in haystack
        or "tier 3" in haystack
        or '"updates"' in haystack
        or "mood_changes" in haystack
        or "relationship_changes" in haystack
        or "simulate a brief interaction" in haystack
    ):
        return "simulation"
    if (
        "write a single brief greeting or reaction" in haystack
        or "a newcomer has just arrived" in haystack
    ):
        return "reaction"
    if stream and response_format in {"json_object", "json_schema"}:
        return "dialogue"
    if "dialogue" in haystack and response_format in {"json_object", "json_schema"}:
        return "dialogue"
    if not all_text and not body:
        return "unknown"
    return "unknown"


def response_metrics(raw: bytes, stream: bool) -> dict[str, Any]:
    if not raw:
        return {
            "response_chars": 0,
            "prompt_tokens_reported": None,
            "completion_tokens_reported": None,
            "cached_tokens_reported": None,
            "thought_tokens_reported": None,
            "total_tokens_reported": None,
            "terminal_status": None,
            "provider_request_id": None,
            "effective_service_tier": None,
            "output_tokens_estimated": 0,
            "error": None,
        }
    if stream:
        text, usage, error = parse_sse_response(raw)
    else:
        text, usage, error = parse_json_response(raw)
    response_chars = len(text)
    prompt_tokens = int_or_none(usage.get("prompt_tokens") or usage.get("total_input_tokens"))
    completion_tokens = int_or_none(
        usage.get("completion_tokens") or usage.get("total_output_tokens")
    )
    return {
        "response_chars": response_chars,
        "prompt_tokens_reported": prompt_tokens,
        "completion_tokens_reported": completion_tokens,
        "cached_tokens_reported": int_or_none(usage.get("total_cached_tokens")),
        "thought_tokens_reported": int_or_none(usage.get("total_thought_tokens")),
        "total_tokens_reported": int_or_none(usage.get("total_tokens")),
        "terminal_status": usage.get("_terminal_status"),
        "provider_request_id": usage.get("_provider_request_id"),
        "effective_service_tier": usage.get("_effective_service_tier"),
        "output_tokens_estimated": completion_tokens or estimate_tokens(response_chars),
        "error": error,
    }


def parse_json_response(raw: bytes) -> tuple[str, dict[str, Any], str | None]:
    try:
        data = json.loads(raw.decode("utf-8", errors="replace"))
    except Exception as exc:
        return raw.decode("utf-8", errors="replace"), {}, f"response JSON parse: {exc}"
    if isinstance(data, dict) and data.get("error"):
        return "", data.get("usage") or {}, json.dumps(data["error"], sort_keys=True)
    text = ""
    if isinstance(data, dict) and isinstance(data.get("steps"), list):
        parts: list[str] = []
        for step in data["steps"]:
            if not isinstance(step, dict) or step.get("type") != "model_output":
                continue
            parts.append(content_to_text(step.get("content")))
        usage = dict(data.get("usage") or {})
        usage["_terminal_status"] = data.get("status")
        usage["_provider_request_id"] = data.get("interaction_id") or data.get("id")
        usage["_effective_service_tier"] = data.get("service_tier")
        return "".join(parts), usage, None
    try:
        choice = (data.get("choices") or [{}])[0]
        message = choice.get("message") or {}
        text = str(message.get("content") or message.get("reasoning") or "")
    except Exception:
        text = ""
    return text, data.get("usage") or {}, None


def parse_sse_response(raw: bytes) -> tuple[str, dict[str, Any], str | None]:
    parts: list[str] = []
    usage: dict[str, Any] = {}
    error: str | None = None
    active_model_output = False
    blocks = raw.decode("utf-8", errors="replace").replace("\r\n", "\n").split("\n\n")
    for block in blocks:
        payload = "\n".join(
            line[5:].lstrip() for line in block.splitlines() if line.startswith("data:")
        )
        if not payload:
            continue
        if payload == "[DONE]":
            break
        try:
            evt = json.loads(payload)
        except json.JSONDecodeError:
            continue
        if isinstance(evt, dict) and evt.get("error"):
            error = json.dumps(evt["error"], sort_keys=True)
        event_type = evt.get("event_type") or evt.get("type")
        event_usage = evt.get("usage")
        if not isinstance(event_usage, dict):
            event_usage = (evt.get("metadata") or {}).get("total_usage")
        if isinstance(event_usage, dict):
            usage.update(event_usage)
        if event_type == "interaction.completed":
            usage["_terminal_status"] = evt.get("status")
            usage["_provider_request_id"] = evt.get("interaction_id") or evt.get("id")
            usage["_effective_service_tier"] = evt.get("service_tier")
        if event_type == "step.start":
            step = evt.get("step") or {}
            active_model_output = step.get("type") == "model_output"
            if active_model_output:
                parts.append(content_to_text(step.get("content")))
            continue
        if event_type in {"step.stop", "step.completed"}:
            active_model_output = False
            continue
        if event_type == "step.delta":
            delta = evt.get("delta") or {}
            if active_model_output and delta.get("type") in {"text", "text_delta"}:
                parts.append(str(delta.get("text") or delta.get("delta") or ""))
            continue
        choices = evt.get("choices") or []
        if not choices:
            continue
        delta = choices[0].get("delta") or {}
        chunk = delta.get("content") or delta.get("reasoning") or ""
        if chunk:
            parts.append(str(chunk))
    return "".join(parts), usage, error


def int_or_none(value: Any) -> int | None:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def estimate_tokens(chars: int) -> int:
    if chars <= 0:
        return 0
    return max(1, math.ceil(chars / 4))


def build_upstream_url(upstream: str, path: str) -> str:
    if upstream.endswith("/v1") and path.startswith("/v1/"):
        return upstream + path[3:]
    return upstream + path


def upstream_for_request(server: _RoutingConfig, category: str, model: str) -> str:
    if model == server.small_model or category in SMALL_SLOT_CATEGORIES:
        return server.small_upstream
    return server.upstream


def client_disconnect_message(stage: str, exc: Exception) -> str | None:
    if isinstance(exc, (BrokenPipeError, ConnectionResetError, ConnectionAbortedError)):
        return f"client disconnected while proxy was {stage}: {exc.__class__.__name__}"
    if isinstance(exc, OSError) and exc.errno in CLIENT_DISCONNECT_ERRNOS:
        return f"client disconnected while proxy was {stage}: OSError {exc.errno}"
    return None


def copy_request_headers(headers: Any, req: urllib.request.Request) -> None:
    for key, value in headers.items():
        lower = key.lower()
        if lower in HOP_BY_HOP_HEADERS or lower in {"host", "content-length"}:
            continue
        req.add_header(key, value)


def copy_response_headers(
    headers: Any, handler: BaseHTTPRequestHandler, length: int | None
) -> None:
    has_content_length = False
    for key, value in headers.items():
        lower = key.lower()
        if lower in HOP_BY_HOP_HEADERS:
            continue
        if lower == "content-length":
            has_content_length = True
            if length is None:
                continue
        handler.send_header(key, value)
    if length is not None and not has_content_length:
        handler.send_header("Content-Length", str(length))
    handler.send_header("Connection", "close")
    handler.close_connection = True


def start_proxy(
    args: argparse.Namespace, recorder: Recorder
) -> tuple[ProfilingServer, threading.Thread]:
    server = ProfilingServer(
        (args.proxy_host, args.proxy_port),
        recorder,
        args.upstream,
        args.small_upstream,
        args.small_model,
        args.upstream_timeout_secs,
    )
    server.quiet = args.quiet
    thread = threading.Thread(target=server.serve_forever, name="profile-demo-proxy", daemon=True)
    thread.start()
    return server, thread


def build_demo_command(args: argparse.Namespace) -> list[str]:
    max_turns = args.max_turns
    if max_turns is None:
        max_turns = max(1, math.ceil(args.duration_secs / args.pause))
    return ["just", "demo", format_float(args.pause), str(max_turns)]


def format_float(value: float) -> str:
    if value.is_integer():
        return str(int(value))
    return str(value)


def run_demo(
    args: argparse.Namespace,
    proxy_url: str,
    run_dir: Path,
    command: list[str],
) -> tuple[int | None, bool, Path]:
    env = os.environ.copy()
    for key in list(env):
        if key.startswith("PARISH_CLOUD_") or key.startswith("PARISH_DIALOGUE_"):
            env.pop(key, None)
        if key.startswith("PARISH_SIMULATION_") or key.startswith("PARISH_INTENT_"):
            env.pop(key, None)
        if key.startswith("PARISH_REACTION_"):
            env.pop(key, None)

    state_tmp = tempfile.TemporaryDirectory(prefix="parish-demo-profile-state-")
    try:
        state_dir = Path(state_tmp.name)
        env.update(
            {
                "PARISH_PROVIDER": args.provider,
                "PARISH_BASE_URL": proxy_url,
                "PARISH_MODEL": args.model,
                "PARISH_DIALOGUE_BASE_URL": proxy_url,
                "PARISH_DIALOGUE_MODEL": args.model,
                "PARISH_SIMULATION_BASE_URL": proxy_url,
                "PARISH_SIMULATION_MODEL": args.model,
                "PARISH_INTENT_BASE_URL": proxy_url,
                "PARISH_INTENT_MODEL": args.small_model,
                "PARISH_REACTION_BASE_URL": proxy_url,
                "PARISH_REACTION_MODEL": args.small_model,
                "PARISH_USER_CONFIG_DIR": str(state_dir / "config"),
                "PARISH_USER_DATA_DIR": str(state_dir / "data"),
                "PARISH_SAVES_DIR": str(state_dir / "saves"),
                "PARISH_TILE_CACHE_DIR": str(state_dir / "tile-cache"),
            }
        )
        for path in [
            state_dir / "config",
            state_dir / "data",
            state_dir / "saves",
            state_dir / "tile-cache",
        ]:
            path.mkdir(parents=True, exist_ok=True)
        user_config_path = write_demo_user_config(state_dir / "config", args, proxy_url)

        demo_log = run_dir / "demo.log"
        timed_out = False
        with demo_log.open("w", encoding="utf-8") as log:
            log.write(f"$ {' '.join(command)}\n")
            log.write(f"PARISH_PROVIDER={args.provider}\nPARISH_BASE_URL={proxy_url}\n")
            log.write(f"PARISH_MODEL={args.model}\n\n")
            log.write(f"PARISH_INTENT_MODEL={args.small_model}\n")
            log.write(f"PARISH_REACTION_MODEL={args.small_model}\n\n")
            log.write(f"User config: {user_config_path}\n\n")
            proc = subprocess.Popen(
                command,
                cwd=REPO_ROOT,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                start_new_session=True,
            )

            def reader() -> None:
                assert proc.stdout is not None
                for line in proc.stdout:
                    log.write(line)
                    log.flush()
                    if args.verbose:
                        print(line, end="")

            reader_thread = threading.Thread(target=reader, daemon=True)
            reader_thread.start()
            try:
                return_code = proc.wait(timeout=args.duration_secs)
            except subprocess.TimeoutExpired:
                timed_out = True
                terminate_process(proc)
                return_code = proc.wait(timeout=20)
            reader_thread.join(timeout=5)
        return return_code, timed_out, demo_log
    finally:
        state_tmp.cleanup()


def write_demo_user_config(config_dir: Path, args: argparse.Namespace, proxy_url: str) -> Path:
    path = config_dir / "parish.toml"
    lines = [
        f'provider = "{toml_string(args.provider)}"',
        f'base_url = "{proxy_url}"',
        f'model = "{toml_string(args.model)}"',
        "",
    ]
    for category in sorted(SMALL_SLOT_CATEGORIES):
        lines.extend(
            [
                f"[category_overrides.{category}]",
                f'provider = "{toml_string(args.provider)}"',
                f'base_url = "{proxy_url}"',
                f'model = "{toml_string(args.small_model)}"',
                "",
            ]
        )
    path.write_text("\n".join(lines), encoding="utf-8")
    return path


def toml_string(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"')


def terminate_process(proc: subprocess.Popen[str]) -> None:
    if proc.poll() is not None:
        return
    with contextlib.suppress(ProcessLookupError):
        if hasattr(os, "killpg"):
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        else:
            proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        with contextlib.suppress(ProcessLookupError):
            if hasattr(os, "killpg"):
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            else:
                proc.kill()


def summarize(events: list[ApiEvent], observed_seconds: float) -> dict[str, Any]:
    by_category: dict[str, list[ApiEvent]] = {cat: [] for cat in CATEGORY_ORDER}
    for event in events:
        by_category.setdefault(event.category, []).append(event)
    rows = {cat: summarize_events(cat, evs, observed_seconds) for cat, evs in by_category.items()}
    gameplay_events = [e for e in events if e.category in GAMEPLAY_CATEGORIES]
    return {
        "observed_seconds": observed_seconds,
        "observed_window_basis": "api_activity_first_request_start_to_last_request_end",
        "categories": rows,
        "total_gameplay": summarize_events("total_gameplay", gameplay_events, observed_seconds),
        "total_observed": summarize_events("total_observed", events, observed_seconds),
    }


def observed_api_activity_seconds(events: list[ApiEvent], fallback_seconds: float) -> float:
    if not events:
        return max(fallback_seconds, 1.0)

    starts = [max(0.0, event.elapsed_since_run_start_secs) for event in events]
    ends = [
        max(0.0, event.elapsed_since_run_start_secs) + max(0, event.duration_ms) / 1000.0
        for event in events
    ]
    return max(max(ends) - min(starts), 1.0)


def summarize_events(
    category: str, events: list[ApiEvent], observed_seconds: float
) -> dict[str, Any]:
    durations = [event.duration_ms for event in events]
    ttfts = [event.ttft_ms for event in events if event.ttft_ms is not None]
    reported_input = sum(event.prompt_tokens_reported or 0 for event in events)
    reported_cached = sum(event.cached_tokens_reported or 0 for event in events)
    minutes = max(observed_seconds / 60.0, 1e-9)
    return {
        "category": category,
        "requests": len(events),
        "requests_per_minute": len(events) / minutes,
        "p50_ms": percentile(durations, 50),
        "p95_ms": percentile(durations, 95),
        "ttft_p50_ms": percentile(ttfts, 50),
        "ttft_p95_ms": percentile(ttfts, 95),
        "errors": sum(1 for event in events if event.error or event.status >= 400),
        "prompt_chars": sum(event.prompt_chars for event in events),
        "response_chars": sum(event.response_chars for event in events),
        "input_tokens_estimated": sum(event.input_tokens_estimated for event in events),
        "output_tokens_estimated": sum(event.output_tokens_estimated for event in events),
        "input_tokens_reported": reported_input,
        "cached_tokens_reported": reported_cached,
        "thought_tokens_reported": sum(event.thought_tokens_reported or 0 for event in events),
        "total_tokens_reported": sum(event.total_tokens_reported or 0 for event in events),
        "cache_ratio": reported_cached / reported_input if reported_input else 0.0,
    }


def percentile(values: list[int], pct: int) -> int:
    if not values:
        return 0
    if len(values) == 1:
        return values[0]
    sorted_values = sorted(values)
    idx = math.ceil((pct / 100) * len(sorted_values)) - 1
    return sorted_values[max(0, min(idx, len(sorted_values) - 1))]


def write_outputs(
    args: argparse.Namespace,
    run_dir: Path,
    events: list[ApiEvent],
    summary: dict[str, Any],
    command: list[str],
    proxy_url: str,
    demo_log: Path | None,
    return_code: int | None,
    timed_out: bool,
    regressions: list[str],
) -> tuple[Path, Path, Path]:
    report = run_dir / "report.md"
    jsonl = run_dir / "events.jsonl"
    summary_json = run_dir / "summary.json"
    with jsonl.open("w", encoding="utf-8") as f:
        for event in events:
            f.write(json.dumps(asdict(event), sort_keys=True) + "\n")
    with summary_json.open("w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2, sort_keys=True)
        f.write("\n")
    report.write_text(
        render_report(
            args,
            events,
            summary,
            command,
            proxy_url,
            jsonl,
            summary_json,
            demo_log,
            return_code,
            timed_out,
            regressions,
        ),
        encoding="utf-8",
    )
    latest = args.report_dir / "latest.md"
    with contextlib.suppress(OSError):
        if latest.exists() or latest.is_symlink():
            latest.unlink()
        latest.symlink_to(report.relative_to(args.report_dir))
    return report, jsonl, summary_json


def render_report(
    args: argparse.Namespace,
    events: list[ApiEvent],
    summary: dict[str, Any],
    command: list[str],
    proxy_url: str,
    jsonl: Path,
    summary_json: Path,
    demo_log: Path | None,
    return_code: int | None,
    timed_out: bool,
    regressions: list[str],
) -> str:
    observed_seconds = summary["observed_seconds"]
    lines = [
        "# Demo API Request Profile",
        "",
        f"Generated: {utc_now()}",
        "",
        "## Configuration",
        "",
        f"- Command: `{' '.join(command)}`",
        f"- Duration target: {args.duration_secs:.0f}s",
        f"- Observed API activity window: {observed_seconds:.1f}s",
        f"- Human reading pause: {args.pause:g}s between demo turns",
        f"- Provider forced for run: `{args.provider}`",
        f"- Parish base URL: `{proxy_url}`",
        f"- Main upstream (dialogue/simulation/demo-player): `{args.upstream}`",
        f"- Small upstream (intent/reaction): `{args.small_upstream}`",
        f"- Main model requested: `{args.model}`",
        f"- Small model requested: `{args.small_model}`",
        f"- Demo process return code: `{return_code}`",
        f"- Stopped after requested duration: `{timed_out}`",
        f"- Events JSONL: `{jsonl}`",
        f"- Summary JSON: `{summary_json}`",
    ]
    if demo_log:
        lines.append(f"- Demo log: `{demo_log}`")
    lines.extend(
        [
            "",
            "## Requests By Category",
            "",
            "| Category | Requests | Req/min | p50/p95 ms | TTFT p50/p95 ms | Errors | Reported input | Cached | Cache % | Thoughts | Est. output |",
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
        ]
    )
    for category in CATEGORY_ORDER:
        row = summary["categories"].get(category) or summarize_events(
            category, [], observed_seconds
        )
        lines.append(render_category_row(row))
    lines.append(
        render_category_row(
            summary["total_gameplay"], label="total_gameplay (excludes demo-player)"
        )
    )
    lines.append(
        render_category_row(
            summary["total_observed"], label="total_observed (includes demo-player)"
        )
    )

    lines.extend(
        [
            "",
            "## Cost Examples",
            "",
            "These are estimates from observed/estimated text tokens only. They exclude prompt caching, batch discounts, higher-context surcharges, tools, images, audio, retries outside the proxy, and provider taxes. Local inference cost is `$0.00` API spend.",
            f"Static price table last checked: {PRICE_TABLE_CHECKED}. Verify provider pages before budget decisions.",
            "",
            "| Example model | Input $/1M | Output $/1M | Estimated run cost | Estimated per hour |",
            "|---|---:|---:|---:|---:|",
        ]
    )
    totals = summary["total_gameplay"]
    for name, input_rate, output_rate in EXAMPLE_MODEL_PRICES:
        cost = estimate_cost(
            totals["input_tokens_estimated"],
            totals["output_tokens_estimated"],
            input_rate,
            output_rate,
        )
        per_hour = cost * (3600.0 / max(observed_seconds, 1e-9))
        lines.append(
            f"| {name} | ${input_rate:.2f} | ${output_rate:.2f} | ${cost:.6f} | ${per_hour:.4f} |"
        )
    lines.extend(["", "Price source URLs checked:"])
    lines.extend(f"- {provider}: {url}" for provider, url in PRICE_SOURCES)

    lines.extend(["", "## Regression Check", ""])
    if regressions:
        lines.append("Regression threshold exceeded:")
        lines.extend(f"- {item}" for item in regressions)
    elif args.baseline:
        lines.append("No request-rate regression detected against the supplied baseline.")
    else:
        lines.append(
            "No baseline supplied. Use `--write-baseline <path>` after a trusted run, then pass `--baseline <path>` in later runs."
        )

    if events:
        lines.extend(["", "## Request Events", ""])
        lines.append(
            "| # | +s | Category | API | Model | Stream | Status | TTFT/ms | Input | Cached | Thought | Output | Error |"
        )
        lines.append("|---:|---:|---|---|---|---:|---:|---:|---:|---:|---:|---:|---|")
        for event in events:
            err = (event.error or "").replace("|", "\\|")
            lines.append(
                f"| {event.request_id} | {event.elapsed_since_run_start_secs:.1f} | {event.category} | {event.api_mode} | "
                f"`{event.model}` | {str(event.stream).lower()} | {event.status} | {event.ttft_ms or 'n/a'}/{event.duration_ms} | "
                f"{event.prompt_tokens_reported or event.input_tokens_estimated} | {event.cached_tokens_reported or 0} | "
                f"{event.thought_tokens_reported or 0} | {event.completion_tokens_reported or event.output_tokens_estimated} | {err} |"
            )
    return "\n".join(lines) + "\n"


def render_category_row(row: dict[str, Any], label: str | None = None) -> str:
    category = label or row["category"]
    return (
        f"| {category} | {row['requests']} | {row['requests_per_minute']:.2f} | "
        f"{row['p50_ms']}/{row['p95_ms']} | {row['ttft_p50_ms']}/{row['ttft_p95_ms']} | {row['errors']} | "
        f"{row['input_tokens_reported']} | {row['cached_tokens_reported']} | {row['cache_ratio'] * 100:.1f}% | "
        f"{row['thought_tokens_reported']} | {row['output_tokens_estimated']} |"
    )


def estimate_cost(
    input_tokens: int, output_tokens: int, input_rate: float, output_rate: float
) -> float:
    return (input_tokens / 1_000_000.0 * input_rate) + (output_tokens / 1_000_000.0 * output_rate)


def check_regressions(
    summary: dict[str, Any],
    baseline_path: Path | None,
    threshold: float,
) -> list[str]:
    if not baseline_path:
        return []
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    regressions: list[str] = []
    for key in ["total_gameplay", "total_observed"]:
        current = summary[key]["requests_per_minute"]
        previous = baseline.get(key, {}).get("requests_per_minute")
        if previous is not None and current > previous * (1.0 + threshold):
            regressions.append(
                f"{key}: {current:.2f} req/min > baseline {previous:.2f} by more than {threshold:.0%}"
            )
    for category, row in summary["categories"].items():
        current = row["requests_per_minute"]
        previous = baseline.get("categories", {}).get(category, {}).get("requests_per_minute")
        if previous is not None and current > previous * (1.0 + threshold):
            regressions.append(
                f"{category}: {current:.2f} req/min > baseline {previous:.2f} by more than {threshold:.0%}"
            )
    return regressions


def write_baseline(summary: dict[str, Any], baseline_path: Path | None) -> None:
    if not baseline_path:
        return
    baseline_path.parent.mkdir(parents=True, exist_ok=True)
    baseline_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def make_run_dir(report_dir: Path) -> Path:
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = report_dir / stamp
    run_dir.mkdir(parents=True, exist_ok=True)
    return run_dir


def dry_run(args: argparse.Namespace) -> int:
    command = build_demo_command(args)
    proxy = f"http://{args.proxy_host}:{args.proxy_port or '<auto>'}"
    print("Dry run:")
    print(f"  command: {' '.join(command)}")
    print(f"  duration: {args.duration_secs:.0f}s")
    print(f"  pause: {args.pause:g}s")
    print("  environment:")
    print(f"    PARISH_PROVIDER={args.provider}")
    print(f"    PARISH_BASE_URL={proxy}")
    print(f"    PARISH_MODEL={args.model}")
    print(f"    PARISH_INTENT_MODEL={args.small_model}")
    print(f"    PARISH_REACTION_MODEL={args.small_model}")
    print(f"  upstream: {args.upstream}")
    print(f"  small_upstream: {args.small_upstream}")
    print(f"  report_dir: {args.report_dir}")
    return 0


def self_test() -> int:
    examples: dict[str, dict[str, Any]] = {
        "demo-player": {
            "model": "m",
            "messages": [
                {
                    "role": "system",
                    "content": 'You are playing Rundale. Respond with {"action": "..."}.',
                },
                {"role": "user", "content": "Location: Kilteevan\nAction (complete the JSON):"},
            ],
        },
        "intent": {
            "model": "m",
            "messages": [
                {"role": "system", "content": "You are an input parser."},
                {"role": "user", "content": "go to the crossroads"},
            ],
            "response_format": {"type": "json_object"},
        },
        "dialogue": {
            "model": "m",
            "stream": True,
            "response_format": {"type": "json_object"},
            "messages": [
                {"role": "system", "content": "You are Brigid, a farmer."},
                {"role": "user", "content": "Good morning."},
            ],
        },
        "simulation": {
            "model": "m",
            "stream": True,
            "messages": [
                {
                    "role": "user",
                    "content": "Return JSON with mood_changes and relationship_changes.",
                },
            ],
        },
        "reaction": {
            "model": "m",
            "messages": [
                {
                    "role": "system",
                    "content": "You decide whether a single NPC would visibly react.",
                },
                {"role": "user", "content": "Choose one emoji or null."},
            ],
        },
        "travel": {
            "model": "m",
            "messages": [
                {
                    "role": "system",
                    "content": "You are writing one line of ambient narration for a walking scene.",
                },
                {"role": "user", "content": "Write one new line in the same register."},
            ],
        },
    }
    for expected, body in examples.items():
        actual = classify_request(body)
        if actual != expected:
            print(f"classifier failed: expected {expected}, got {actual}", file=sys.stderr)
            return 1
    native_body = {
        "model": "gemini-3.6-flash",
        "system_instruction": "You are an input parser. Return intent and target.",
        "input": "ask Brigid about the harvest",
        "response_format": {"type": "text", "mime_type": "application/json"},
    }
    if classify_request(native_body) != "intent":
        print("native Interactions classification failed", file=sys.stderr)
        return 1
    native_sse = (
        b'data: {"event_type":"step.start","step":{"type":"thought","content":'
        b'[{"type":"text","text":"secret"}]}}\n\n'
        b'data: {"event_type":"step.delta","delta":{"type":"text","text":"secret2"}}\n\n'
        b'data: {"event_type":"step.start","step":{"type":"model_output","content":'
        b'[{"type":"text","text":"ok"}]}}\n\n'
        b'data: {"event_type":"interaction.completed","id":"int_test","status":"completed",'
        b'"metadata":{"total_usage":{"total_input_tokens":9000,"total_cached_tokens":8000,'
        b'"total_output_tokens":1,"total_thought_tokens":2,"total_tokens":9003}}}\n\n'
    )
    native_metrics = response_metrics(native_sse, True)
    if native_metrics["response_chars"] != 2 or native_metrics["cached_tokens_reported"] != 8000:
        print(f"native Interactions metrics failed: {native_metrics}", file=sys.stderr)
        return 1
    routing = argparse.Namespace(
        upstream="http://main",
        small_upstream="http://small",
        small_model="small-model",
    )
    if upstream_for_request(routing, "travel", "small-model") != "http://small":
        print("small-model routing failed", file=sys.stderr)
        return 1
    if upstream_for_request(routing, "intent", "main-model") != "http://small":
        print("small-category routing failed", file=sys.stderr)
        return 1
    if upstream_for_request(routing, "dialogue", "main-model") != "http://main":
        print("main routing failed", file=sys.stderr)
        return 1

    recorder = Recorder()
    for idx, (category, body) in enumerate(examples.items(), start=1):
        prompt_chars, system_chars = prompt_char_counts(body)
        recorder.record(
            ApiEvent(
                request_id=idx,
                started_at=utc_now(),
                elapsed_since_run_start_secs=float(idx),
                category=category,
                method="POST",
                path="/v1/chat/completions",
                model="self-test",
                stream=bool(body.get("stream")),
                response_format=response_format_label(body.get("response_format")),
                api_mode="openai-compat",
                status=200,
                duration_ms=100 + idx,
                ttft_ms=50 + idx,
                prompt_chars=prompt_chars,
                system_chars=system_chars,
                response_chars=80,
                prompt_tokens_reported=None,
                completion_tokens_reported=None,
                cached_tokens_reported=None,
                thought_tokens_reported=None,
                total_tokens_reported=None,
                terminal_status="completed",
                provider_request_id=None,
                effective_service_tier=None,
                input_tokens_estimated=estimate_tokens(prompt_chars),
                output_tokens_estimated=20,
                error=None,
            )
        )
    with tempfile.TemporaryDirectory() as tmp:
        args = parse_args(["--report-dir", tmp, "--duration-secs", "60"])
        if not args.quiet or parse_args(["--verbose-proxy"]).quiet:
            print("proxy quiet flag parsing failed", file=sys.stderr)
            return 1
        args.report_dir.mkdir(parents=True, exist_ok=True)
        run_dir = make_run_dir(args.report_dir)
        events = recorder.events()
        observed_seconds = observed_api_activity_seconds(events, 60.0)
        if round(observed_seconds, 3) != 5.106:
            print(f"activity window failed: {observed_seconds}", file=sys.stderr)
            return 1
        summary = summarize(events, observed_seconds)
        report, jsonl, summary_json = write_outputs(
            args,
            run_dir,
            events,
            summary,
            ["just", "demo", "10", "6"],
            "http://127.0.0.1:18080",
            None,
            0,
            False,
            [],
        )
        text = report.read_text(encoding="utf-8")
        for needle in ["demo-player", "total_gameplay", "OpenAI GPT-5.4 mini"]:
            if needle not in text:
                print(f"self-test report missing {needle}", file=sys.stderr)
                return 1
        if not jsonl.read_text(encoding="utf-8").strip():
            print("self-test JSONL was empty", file=sys.stderr)
            return 1
        if not json.loads(summary_json.read_text(encoding="utf-8")):
            print("self-test summary JSON was empty", file=sys.stderr)
            return 1
    print("self-test passed")
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration-secs", type=float, default=DEFAULT_DURATION_SECS)
    parser.add_argument("--pause", type=float, default=DEFAULT_PAUSE_SECS)
    parser.add_argument("--max-turns", type=int, default=None)
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--provider", choices=("custom", "google"), default="custom")
    parser.add_argument("--upstream", default=DEFAULT_UPSTREAM)
    parser.add_argument("--small-model")
    parser.add_argument("--small-upstream")
    parser.add_argument("--upstream-timeout-secs", type=float, default=600.0)
    parser.add_argument("--proxy-host", default="127.0.0.1")
    parser.add_argument("--proxy-port", type=int, default=0)
    parser.add_argument("--report-dir", type=Path, default=DEFAULT_REPORT_DIR)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--write-baseline", type=Path)
    parser.add_argument("--regression-threshold", type=float, default=0.25)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--quiet", action="store_true", dest="quiet", help="Suppress proxy request logging"
    )
    parser.add_argument(
        "--verbose-proxy", action="store_false", dest="quiet", help="Print proxy request logging"
    )
    parser.add_argument("--verbose", action="store_true")
    parser.set_defaults(quiet=True)
    args = parser.parse_args(argv)
    args.report_dir = args.report_dir.resolve()
    if args.duration_secs <= 0:
        parser.error("--duration-secs must be positive")
    if args.pause <= 0:
        parser.error("--pause must be positive")
    if args.small_model is None:
        args.small_model = DEFAULT_SMALL_MODEL if args.model == DEFAULT_MODEL else args.model
    if args.small_upstream is None:
        args.small_upstream = (
            DEFAULT_SMALL_UPSTREAM if args.upstream == DEFAULT_UPSTREAM else args.upstream
        )
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()
    if args.dry_run:
        return dry_run(args)

    args.report_dir.mkdir(parents=True, exist_ok=True)
    run_dir = make_run_dir(args.report_dir)
    recorder = Recorder()
    server, thread = start_proxy(args, recorder)
    host, port = server.server_address[:2]
    proxy_url = f"http://{host.decode() if isinstance(host, (bytes, bytearray)) else host}:{port}"
    command = build_demo_command(args)
    print(f"Proxy: {proxy_url} -> main {args.upstream}, small {args.small_upstream}")
    print(f"Running: {' '.join(command)} for up to {args.duration_secs:.0f}s")
    started = time.monotonic()
    return_code: int | None = None
    timed_out = False
    demo_log: Path | None = None
    try:
        return_code, timed_out, demo_log = run_demo(args, proxy_url, run_dir, command)
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
    elapsed_seconds = max(time.monotonic() - started, 1.0)
    events = recorder.events()
    observed_seconds = observed_api_activity_seconds(events, elapsed_seconds)
    summary = summarize(events, observed_seconds)
    regressions = check_regressions(summary, args.baseline, args.regression_threshold)
    write_baseline(summary, args.write_baseline)
    report, jsonl, summary_json = write_outputs(
        args,
        run_dir,
        events,
        summary,
        command,
        proxy_url,
        demo_log,
        return_code,
        timed_out,
        regressions,
    )
    print(f"Report: {report}")
    print(f"Events: {jsonl}")
    print(f"Summary: {summary_json}")
    if return_code not in (0, None) and not timed_out:
        print(f"demo exited with {return_code}; see {demo_log}", file=sys.stderr)
        assert return_code is not None  # guard above rules out None
        return return_code
    if regressions:
        for item in regressions:
            print(f"regression: {item}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
