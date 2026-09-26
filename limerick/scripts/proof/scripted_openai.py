#!/usr/bin/env python3
"""Scripted OpenAI-compatible model server for differential proof runs.

Serves `GET /v1/models` and `POST /v1/chat/completions` over real HTTP on
127.0.0.1. Each request is classified into a workload from its system prompt
and answered with a fixed reply, streamed or not as requested, so two builds
driven through the same scenario see the same model. Every request body is
appended to a JSONL log as `{"path", "workload", "body"}`.

Standalone use (e.g. to point a Tauri app at it):

    scripted_openai.py --port 8765 --log requests.jsonl

then launch the app with LIMERICK_PROVIDER=lmstudio,
LIMERICK_BASE_URL=http://127.0.0.1:8765/v1 and LIMERICK_MODEL=scripted.
"""

from __future__ import annotations

import argparse
import json
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

MODEL_ID = "scripted"

# Canned NPC replies. A player line mentioning work gets a task; everything
# else gets small talk. Task text must not name a remote place ("well" would
# match the Holy Well and be rejected by the task validator).
SMALL_TALK: dict[str, Any] = {
    "dialogue": "God bless ye. It's a fine soft day for the time of year.",
    "action": "nods toward the road",
    "mood": "content",
    "language_hints": [],
    "assigned_task": None,
}
WORK: dict[str, Any] = {
    "dialogue": "I need ye to dig over the potato patch here now.",
    "action": "offers over a spade",
    "mood": "busy",
    "language_hints": [],
    "assigned_task": "Dig over the potato patch.",
}
BACKGROUND = "A drover passes with two heifers and lifts his hat."
MOVE_PHRASE = re.compile(r"walking on toward (.+?)\s*$")


def _text(messages: list[dict[str, Any]], role: str) -> str:
    parts = []
    for message in messages:
        if message.get("role") != role:
            continue
        content = message.get("content", "")
        if isinstance(content, list):
            content = " ".join(str(part.get("text", "")) for part in content)
        parts.append(str(content))
    return " ".join(parts)


def classify(body: dict[str, Any]) -> tuple[str, str]:
    """Returns `(workload, reply_text)` for a chat-completions request body."""
    messages = body.get("messages", [])
    system = _text(messages, "system")
    user = _text(messages, "user").strip()
    if "input parser" in system:
        move = MOVE_PHRASE.search(user)
        if move:
            intent = {"intent": "move", "target": move.group(1), "dialogue": None}
        else:
            intent = {"intent": "talk", "target": None, "dialogue": user}
        return "intent", json.dumps(intent)
    if "Respond in character" in system:
        reply = WORK if re.search(r"\bwork\b", user, re.I) else SMALL_TALK
        return "dialogue", json.dumps(reply)
    if "emoji" in (system + user).lower() or "react" in system.lower():
        return "reaction", "none"
    if "simulating" in system + user:
        return "simulation", BACKGROUND
    return "other", BACKGROUND


def _chunk(text: str | None, finish: str | None) -> bytes:
    delta = {"content": text} if text is not None else {}
    chunk = {
        "id": "scripted",
        "object": "chat.completion.chunk",
        "model": MODEL_ID,
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
    }
    return f"data: {json.dumps(chunk)}\n\n".encode()


class _Handler(BaseHTTPRequestHandler):
    server: ScriptedServer

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
        pass

    def _send_json(self, payload: dict[str, Any]) -> None:
        data = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self) -> None:
        self._send_json({"object": "list", "data": [{"id": MODEL_ID, "object": "model"}]})

    def do_POST(self) -> None:
        raw = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        body = json.loads(raw or b"{}")
        workload, text = classify(body)
        self.server.record({"path": self.path, "workload": workload, "body": body})
        if body.get("stream"):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            half = len(text) // 2
            self.wfile.write(_chunk(text[:half], None))
            self.wfile.write(_chunk(text[half:], None))
            self.wfile.write(_chunk(None, "stop"))
            self.wfile.write(b"data: [DONE]\n\n")
            return
        self._send_json(
            {
                "id": "scripted",
                "object": "chat.completion",
                "model": MODEL_ID,
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": text},
                        "finish_reason": "stop",
                    }
                ],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
            }
        )


class ScriptedServer(ThreadingHTTPServer):
    """The scripted model server. Port 0 picks a free port (see `.port`)."""

    daemon_threads = True

    def __init__(self, log_path: Path, port: int = 0) -> None:
        super().__init__(("127.0.0.1", port), _Handler)
        self.log_path = log_path
        self._lock = threading.Lock()
        log_path.write_text("")

    @property
    def port(self) -> int:
        return int(self.server_address[1])

    def record(self, entry: dict[str, Any]) -> None:
        with self._lock, self.log_path.open("a") as log:
            log.write(json.dumps(entry) + "\n")

    def start(self) -> ScriptedServer:
        threading.Thread(target=self.serve_forever, daemon=True).start()
        return self


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=0, help="0 picks a free port")
    parser.add_argument("--log", type=Path, required=True, help="request-body JSONL log")
    args = parser.parse_args()
    server = ScriptedServer(args.log, args.port)
    print(f"scripted model server on http://127.0.0.1:{server.port}/v1", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
