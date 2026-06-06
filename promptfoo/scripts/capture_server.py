"""Recording OpenAI-compatible stub server for runtime-faithful prompt capture (REQ 2).

The Rundale engine builds its real prompts (dialogue with PEOPLE YOU KNOW /
WHAT'S ON YOUR MIND / anchors, intent, reaction, tier2/tier3 sim) and POSTs them
to an OpenAI-compatible `/v1/chat/completions` endpoint. Point the engine at this
server (`--provider custom --base-url http://127.0.0.1:PORT/v1 --model capture`)
and it records every request — system + user messages, response_format/schema,
max_tokens, temperature, model — to a JSONL file, then returns a minimal valid
response so the game loop keeps advancing and exercises every category.

This captures the BYTE-EXACT wire request the live game sends — the strongest
possible fidelity (no reconstruction). Run:

    CAPTURE_OUT=/tmp/cap.jsonl python3 promptfoo/scripts/capture_server.py 8730
"""

from __future__ import annotations

import json
import os
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

OUT_PATH = os.environ.get("CAPTURE_OUT", "/tmp/parish_capture.jsonl")
_LOCK = threading.Lock()
_COUNTER = {"n": 0}


def _fill_schema(schema: dict):
    """Synthesize a minimal value satisfying a JSON schema so the engine accepts
    the canned reply and the loop advances. Honours type, enum, required."""
    if not isinstance(schema, dict):
        return "x"
    if "enum" in schema and schema["enum"]:
        return schema["enum"][0]
    t = schema.get("type")
    if isinstance(t, list):
        t = next((x for x in t if x != "null"), t[0])
    if t == "object" or "properties" in schema:
        obj = {}
        props = schema.get("properties", {})
        for k, sub in props.items():
            obj[k] = _fill_schema(sub)
        return obj
    if t == "array":
        return []
    if t in ("integer", "number"):
        return 0
    if t == "boolean":
        return False
    if t == "null":
        return None
    return "x"


def _canned_reply(body: dict) -> str:
    """A short, valid reply. If a JSON schema/object is requested, emit valid JSON;
    otherwise a brief in-character line. Content only matters enough to keep the
    loop alive — the captured *request* is the artifact."""
    rf = body.get("response_format") or {}
    rtype = rf.get("type")
    if rtype == "json_schema":
        schema = (rf.get("json_schema") or {}).get("schema") or {}
        return json.dumps(_fill_schema(schema), ensure_ascii=False)
    if rtype == "json_object":
        # Best-effort: dialogue/sim free-JSON paths. A bare valid object.
        return json.dumps({"dialogue": "Aye, well enough.", "action": "nods", "mood": "calm"})
    return "Aye, well enough, thank ye."


def _record(body: dict) -> None:
    msgs = body.get("messages") or []
    system = next((m.get("content", "") for m in msgs if m.get("role") == "system"), None)
    # Non-system messages, preserved in order (multi-turn aware).
    convo = [
        {"role": m.get("role"), "content": m.get("content", "")}
        for m in msgs
        if m.get("role") != "system"
    ]
    user = convo[-1]["content"] if convo else ""
    rf = body.get("response_format") or {}
    rec = {
        "model": body.get("model"),
        "system": system,
        "user": user,
        "messages": convo,
        "response_format": rf or None,
        "schema": (rf.get("json_schema") or {}).get("schema")
        if rf.get("type") == "json_schema"
        else None,
        "max_tokens": body.get("max_tokens"),
        "temperature": body.get("temperature"),
        "frequency_penalty": body.get("frequency_penalty"),
        "stream": bool(body.get("stream")),
    }
    with _LOCK:
        _COUNTER["n"] += 1
        with open(OUT_PATH, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):  # silence
        pass

    def _read_body(self) -> dict:
        n = int(self.headers.get("Content-Length", 0) or 0)
        raw = self.rfile.read(n) if n else b"{}"
        try:
            return json.loads(raw.decode("utf-8"))
        except (ValueError, UnicodeDecodeError):
            return {}

    def do_GET(self):  # /v1/models health probes
        self._json(200, {"object": "list", "data": [{"id": "capture", "object": "model"}]})

    def do_POST(self):
        body = self._read_body()
        try:
            _record(body)
        except Exception as e:  # noqa: BLE001
            sys.stderr.write(f"[capture] record error: {e}\n")
        content = _canned_reply(body)
        if body.get("stream"):
            self._stream(content)
        else:
            self._json(
                200,
                {
                    "id": "cap",
                    "object": "chat.completion",
                    "created": int(time.time()),
                    "model": body.get("model", "capture"),
                    "choices": [
                        {
                            "index": 0,
                            "message": {"role": "assistant", "content": content},
                            "finish_reason": "stop",
                        }
                    ],
                    "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
                },
            )

    def _json(self, code: int, obj: dict):
        data = json.dumps(obj).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _stream(self, content: str):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()

        def sse(obj):
            self.wfile.write(f"data: {json.dumps(obj)}\n\n".encode())
            self.wfile.flush()

        sse({"choices": [{"index": 0, "delta": {"role": "assistant"}}]})
        sse({"choices": [{"index": 0, "delta": {"content": content}}]})
        sse(
            {
                "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
            }
        )
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


def main(argv):
    port = int(argv[1]) if len(argv) > 1 else 8730
    # truncate output
    open(OUT_PATH, "w").close()
    srv = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    sys.stderr.write(f"[capture] listening :{port} → {OUT_PATH}\n")
    sys.stderr.flush()
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main(sys.argv)
