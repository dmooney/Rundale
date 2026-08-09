"""Regression tests for the inference-eval player's HTTP session handling."""

import importlib.util
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("player_agent.py")
SPEC = importlib.util.spec_from_file_location("player_agent", MODULE_PATH)
assert SPEC and SPEC.loader
player_agent = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(player_agent)


class SessionHandler(BaseHTTPRequestHandler):
    location = "Kilteevan"

    def log_message(self, _format, *_args):
        pass

    def _has_session(self):
        return "parish-session=test-session" in self.headers.get("Cookie", "")

    def _reply(self, payload, *, set_cookie=False):
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        if set_cookie:
            self.send_header("Set-Cookie", "parish-session=test-session; Path=/")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", "0"))
        payload = json.loads(self.rfile.read(content_length) or b"{}")
        if self.path == "/api/new-game":
            type(self).location = "Kilteevan"
            self._reply({}, set_cookie=True)
            return
        if self.path == "/api/submit-input" and self._has_session():
            if payload.get("text") == "go to the church":
                type(self).location = "Church"
            self._reply({})
            return
        if self.path == "/api/command" and self._has_session():
            if payload.get("text") == "go to the church":
                type(self).location = "Church"
            self._reply(
                {
                    "outcome": "ok",
                    "kind": "moved",
                    "lines": [{"text": f"You arrive at {type(self).location}."}],
                    "state": {
                        "location_name": type(self).location,
                        "hour": 8,
                        "minute": 0,
                    },
                }
            )
            return
        self.send_error(401)

    def do_GET(self):
        if self.path == "/api/world-snapshot" and self._has_session():
            self._reply(
                {
                    "location_name": type(self).location,
                    "hour": 8,
                    "minute": 0,
                    "time_label": "Morning",
                    "weather": "Clear",
                }
            )
            return
        if self.path == "/api/npcs-here" and self._has_session():
            self._reply([{"name": "Nora"}])
            return
        self.send_error(401)


def test_parish_requests_keep_one_cookie_backed_game_session():
    server = ThreadingHTTPServer(("127.0.0.1", 0), SessionHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    old_port = player_agent.PARISH_PORT
    try:
        player_agent.PARISH_COOKIE_JAR.clear()
        player_agent.PARISH_PORT = server.server_port
        player_agent.parish_post("/api/new-game", {})
        assert player_agent.parish_get("/api/world-snapshot")["location_name"] == "Kilteevan"
        player_agent.parish_post("/api/submit-input", {"text": "go to the church"})
        assert player_agent.parish_get("/api/world-snapshot")["location_name"] == "Church"
    finally:
        player_agent.PARISH_PORT = old_port
        player_agent.PARISH_COOKIE_JAR.clear()
        server.shutdown()
        server.server_close()
        thread.join()


def test_session_log_uses_real_server_wire_fields_and_command_output():
    server = ThreadingHTTPServer(("127.0.0.1", 0), SessionHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    old_port = player_agent.PARISH_PORT
    try:
        player_agent.PARISH_COOKIE_JAR.clear()
        player_agent.PARISH_PORT = server.server_port
        log = player_agent.run_session("wire", ["go to the church"], 1, False)
        assert log[0]["state"]["location"] == "Kilteevan"
        assert log[0]["state"]["npc_count"] == 1
        assert log[0]["result"]["location_after"] == "Church"
        assert log[0]["result"]["narrative"] == "You arrive at Church."
    finally:
        player_agent.PARISH_PORT = old_port
        player_agent.PARISH_COOKIE_JAR.clear()
        server.shutdown()
        server.server_close()
        thread.join()
