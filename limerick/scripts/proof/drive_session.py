#!/usr/bin/env python3
"""Drives one live session through a real HTTP game server.

Launch mode (default) starts `limerick-server` against a scripted model
server (`scripted_openai.py`) with isolated user data and config, no cloud
keys, and a working directory without `.env`, then submits each scenario
line to `POST /api/submit-input` on one cookie-jar session.

Attach mode (`--attach URL`) drives a server that is already running, such
as the Tauri bridge. Request bodies are logged only when that process was
started against a scripted model server (run `scripted_openai.py` alone).

Writes to `--out`:
  responses.jsonl    one `{"input", "status", "response"}` per scenario line
  requests.jsonl     every provider request body (launch mode)
  engine-state.json  `GET /api/engine-state` after the last line
  server.log         server stdout and stderr (launch mode)
"""

from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from email.message import Message
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from scripted_openai import MODEL_ID, ScriptedServer  # noqa: E402

# Keys that would route inference away from the scripted server.
SCRUBBED_SUFFIXES = ("_API_KEY", "_TOKEN")


def read_scenario(path: Path) -> list[str]:
    lines = []
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            lines.append(line)
    return lines


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def isolated_env(run_dir: Path) -> dict[str, str]:
    """This environment minus cloud keys and LIMERICK_* settings, with user
    data and config under `run_dir`."""
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("LIMERICK_") and not key.endswith(SCRUBBED_SUFFIXES)
    }
    env["LIMERICK_USER_DATA_DIR"] = str(run_dir / "user-data")
    env["LIMERICK_USER_CONFIG_DIR"] = str(run_dir / "user-config")
    return env


def server_env(model_port: int, mod_dir: Path, run_dir: Path) -> dict[str, str]:
    env = isolated_env(run_dir)
    env.update(
        LIMERICK_PROVIDER="lmstudio",
        LIMERICK_BASE_URL=f"http://127.0.0.1:{model_port}/v1",
        LIMERICK_MODEL=MODEL_ID,
        LIMERICK_DATA_DIR=str(mod_dir),
    )
    return env


class Session:
    """One game session: every request carries the server's session cookie.

    The server marks `limerick_sid` `Secure`, which `http.cookiejar` will not
    send back over plain http, so cookies are kept by hand. Losing the cookie
    silently gives every request a fresh game.
    """

    def __init__(self, base_url: str) -> None:
        self.base_url = base_url.rstrip("/")
        self.cookies: dict[str, str] = {}

    def request(self, path: str, body: dict | None = None) -> tuple[int, str]:
        data = None if body is None else json.dumps(body).encode()
        req = urllib.request.Request(self.base_url + path, data=data)
        if data is not None:
            req.add_header("Content-Type", "application/json")
        if self.cookies:
            req.add_header("Cookie", "; ".join(f"{k}={v}" for k, v in self.cookies.items()))
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                self._keep_cookies(resp.headers)
                return resp.status, resp.read().decode()
        except urllib.error.HTTPError as err:
            self._keep_cookies(err.headers)
            return err.code, err.read().decode()

    def _keep_cookies(self, headers: Message) -> None:
        for header in headers.get_all("Set-Cookie") or []:
            name, _, rest = header.partition("=")
            self.cookies[name.strip()] = rest.split(";", 1)[0]

    def wait_ready(self, timeout: float = 90.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                self.request("/")
                return
            except (urllib.error.URLError, ConnectionError):
                time.sleep(0.25)
        raise SystemExit(f"server at {self.base_url} did not become ready in {timeout}s")


def settle(log: Path | None, quiet: float, limit: float) -> None:
    """Waits until the request log stops growing (or a fixed pause without one)."""
    if log is None:
        time.sleep(quiet)
        return
    deadline = time.monotonic() + limit
    size = -1
    while time.monotonic() < deadline:
        current = log.stat().st_size
        if current == size:
            return
        size = current
        time.sleep(quiet)


def drive(
    session: Session,
    scenario: list[str],
    out: Path,
    model: ScriptedServer | None,
    quiet: float,
) -> None:
    session.request("/")
    log = model.log_path if model else None
    with (out / "responses.jsonl").open("w") as responses:
        for turn, line in enumerate(scenario, 1):
            if model:
                # Turn boundary in the request log, so requests group by turn.
                model.record({"turn": turn, "input": line})
            status, text = session.request("/api/submit-input", {"text": line})
            try:
                parsed: object = json.loads(text)
            except json.JSONDecodeError:
                parsed = text
            responses.write(json.dumps({"input": line, "status": status, "response": parsed}))
            responses.write("\n")
            settle(log, quiet, limit=30.0)
    _, state = session.request("/api/engine-state")
    (out / "engine-state.json").write_text(state)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenario", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--server-bin", type=Path, help="limerick-server binary (launch mode)")
    parser.add_argument("--mod-dir", type=Path, help="mods/rundale of the tree under test")
    parser.add_argument("--attach", help="base URL of a running server (attach mode)")
    parser.add_argument("--quiet", type=float, default=1.0, help="settle interval, seconds")
    args = parser.parse_args()

    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    scenario = read_scenario(args.scenario)

    if args.attach:
        drive(Session(args.attach), scenario, out, None, args.quiet)
        return
    if not (args.server_bin and args.mod_dir):
        parser.error("launch mode needs --server-bin and --mod-dir (or use --attach)")

    model = ScriptedServer(out / "requests.jsonl").start()
    cwd = out / "cwd"
    cwd.mkdir(exist_ok=True)
    port = free_port()
    with (out / "server.log").open("w") as server_log:
        server = subprocess.Popen(
            [str(args.server_bin.resolve()), "--port", str(port)],
            cwd=cwd,
            env=server_env(model.port, args.mod_dir.resolve(), out),
            stdout=server_log,
            stderr=subprocess.STDOUT,
        )
        try:
            session = Session(f"http://127.0.0.1:{port}")
            session.wait_ready()
            drive(session, scenario, out, model, args.quiet)
        finally:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
            model.shutdown()


if __name__ == "__main__":
    main()
