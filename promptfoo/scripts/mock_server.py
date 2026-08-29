"""Minimal OpenAI-compatible mock for a LIVE end-to-end rundale-bench v2 run
without API keys.

Serves POST /v1/chat/completions (non-stream + SSE stream). Routes by content:
- a request whose system prompt marks it as the judge ("impartial judge" /
  "expert evaluator") → returns the slice's rubric envelope JSON.
- a request with a json_schema response_format → returns a schema-valid instance
  (intent gold-ish for the intent schema; a synthesized instance otherwise).
- anything else (dialogue / reaction / gaeilge / perf) → a short in-character
  reply.

This is a TEST DOUBLE for wiring/proof only — not a model. Run:
    python3 promptfoo/scripts/mock_server.py 8000
"""

from __future__ import annotations

import json
import socketserver
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

REPLY = "Ah now, a chroí, ye'll want chamomile steeped soft and a quiet hearth; sure the dreams pass with the turf-smoke."


def _synth(schema):
    """Produce a minimal valid instance of a (subset) JSON schema."""
    if not isinstance(schema, dict):
        return "x"
    t = schema.get("type")
    if isinstance(t, list):
        t = next((x for x in t if x != "null"), t[0])
    if "enum" in schema:
        return schema["enum"][0]
    if t == "object":
        props = schema.get("properties", {})
        req = schema.get("required", list(props.keys()))
        return {k: _synth(props.get(k, {})) for k in req}
    if t == "array":
        return [_synth(schema.get("items", {"type": "string"}))]
    if t == "integer":
        return 0
    if t == "number":
        return 0.0
    if t == "boolean":
        return True
    if t == "null":
        return None
    return "x"


_SLICE_AXES = {
    "dialogue": {
        "character": 3,
        "authenticity": 3,
        "language": 4,
        "responsiveness": 4,
        "craft": 3,
        "brevity": 4,
        "repetition": 4,
        "mood_fidelity": 3,
        "grounding": 5,
    },
    "reaction": {"in_character": 3},
    "tier2-sim": {"plausibility": 3},
    "tier3-sim": {"plausibility": 3},
    "gaeilge": {
        "fluency": 3,
        "grammar": 3,
        "idiom": 3,
        "task_fulfillment": 3,
        "english_leakage": 4,
    },
    "multiturn": {
        "continuity": 3,
        "name_fidelity": 3,
        "no_premature_farewell": 4,
        "persona_consistency": 3,
        "memory_retention": 3,
    },
}


def _axes_for(slice_name: str) -> dict:
    return _SLICE_AXES.get(slice_name, _SLICE_AXES["dialogue"])


def _judge_envelope(body) -> str:
    user = body["messages"][-1]["content"]
    bundle = json.loads(user)
    items = []
    for it in bundle["items"]:
        axes = _axes_for(bundle.get("slice", ""))
        items.append(
            {
                "prompt_id": it["prompt_id"],
                "axes": axes,
                "overall": round(sum(axes.values()) / len(axes), 1),
                "flags": {
                    "bench_bug": False,
                    "non_latin_detected": False,
                    "refused": False,
                    "degenerate_loop": False,
                    "fabricated": False,
                },
            }
        )
    return json.dumps(
        {
            "version": 1,
            "slice": bundle["slice"],
            "rubric_sha256": bundle["rubric_sha256"],
            "items": items,
        }
    )


def _content_for(body) -> str:
    msgs = body.get("messages", [])
    system = next((m["content"] for m in msgs if m["role"] == "system"), "")
    user = next((m["content"] for m in msgs if m["role"] == "user"), "")
    if "impartial judge" in system.lower() or "expert evaluator" in system.lower():
        return _judge_envelope(body)
    # Runtime-faithful slices send response_format json_object (or none) with the
    # shape described in the prompt — emit a valid instance so grading isn't floored.
    if "input parser" in system.lower():
        return json.dumps({"intent": "move", "target": "the pub", "dialogue": None})
    if "background interactions" in user:
        return json.dumps(
            {"summary": "All quiet at the mill.", "mood_changes": [], "relationship_changes": []}
        )
    if "background NPC activity" in user:
        return json.dumps({"updates": []})
    rf = body.get("response_format") or {}
    schema_wrap = rf.get("json_schema") or {}
    if schema_wrap:
        if schema_wrap.get("name") == "intent":
            return json.dumps({"intent": "move", "target": "the pub", "dialogue": None})
        return json.dumps(_synth(schema_wrap.get("schema", {})))
    return REPLY


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):  # quiet
        pass

    def do_GET(self):
        if self.path.endswith("/models"):
            self._json({"data": [{"id": "mock"}]})
        else:
            self._json({"ok": True})

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length) or b"{}")
        content = _content_for(body)
        usage = {
            "prompt_tokens": 120,
            "completion_tokens": max(1, len(content) // 4),
            "total_tokens": 120 + max(1, len(content) // 4),
        }
        if body.get("stream"):
            self._sse(content, usage, body.get("model", "mock"))
        else:
            self._json(
                {
                    "id": "cmpl-mock",
                    "object": "chat.completion",
                    "model": body.get("model", "mock"),
                    "choices": [
                        {
                            "index": 0,
                            "message": {"role": "assistant", "content": content},
                            "finish_reason": "stop",
                        }
                    ],
                    "usage": usage,
                }
            )

    def _json(self, obj):
        data = json.dumps(obj).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _sse(self, content, usage, model):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        words = content.split(" ")
        for i, w in enumerate(words):
            chunk = {"choices": [{"delta": {"content": (w if i == 0 else " " + w)}}]}
            self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
            self.wfile.flush()
            time.sleep(0.005)
        final = {"choices": [{"delta": {}, "finish_reason": "stop"}], "usage": usage}
        self.wfile.write(f"data: {json.dumps(final)}\n\n".encode())
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


class LoopbackHTTPServer(ThreadingHTTPServer):
    """Bind without HTTPServer's unnecessary reverse-DNS lookup.

    ``HTTPServer.server_bind`` calls ``socket.getfqdn()`` even when the caller
    supplies a numeric loopback address. A missing or slow reverse-DNS service
    can therefore stall this local-only test double before it starts listening.
    """

    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name = str(self.server_address[0])
        self.server_port = int(self.server_address[1])


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
    print(f"mock OpenAI-compat server on :{port}")
    LoopbackHTTPServer(("127.0.0.1", port), Handler).serve_forever()
