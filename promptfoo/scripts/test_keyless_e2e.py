"""Run the documented keyless dialogue path through the real promptfoo CLI.

This is a regression sensor for drift between the production dialogue rubric
and ``scripts/mock_server.py``. It deliberately invokes promptfoo's installed
JavaScript entry point, its Python provider, and its Python rubric assertion;
testing only the mock helper in-process would not prove that the documented
end-to-end command still works.
"""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

PF = Path(__file__).resolve().parents[1]
ROOT = PF.parent
PROMPTFOO_CLI = PF / "node_modules" / "promptfoo" / "dist" / "src" / "entrypoint.js"


def _free_loopback_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def _wait_until_ready(base_url: str, process: subprocess.Popen[str]) -> None:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"mock server exited early with {process.returncode}")
        try:
            with urllib.request.urlopen(f"{base_url}/models", timeout=0.5) as response:
                if response.status == 200:
                    return
        except OSError:
            time.sleep(0.05)
    raise TimeoutError("mock server did not become ready within 10 seconds")


def main() -> None:
    node = shutil.which("node")
    if node is None:
        raise RuntimeError("node is required for the promptfoo keyless smoke")
    if not PROMPTFOO_CLI.is_file():
        raise RuntimeError("promptfoo is not installed; run `npm ci` in promptfoo/")

    port = _free_loopback_port()
    base_url = f"http://127.0.0.1:{port}/v1"
    server = subprocess.Popen(
        [sys.executable, str(PF / "scripts" / "mock_server.py"), str(port)],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    try:
        _wait_until_ready(base_url, server)
        with tempfile.TemporaryDirectory(prefix="rundale-promptfoo-keyless-") as output_dir:
            result_path = Path(output_dir) / "dialogue.json"
            env = os.environ.copy()
            env.update(
                {
                    "MOCK_API_KEY": "test-only",
                    "RB_JUDGE_MODEL": "mock",
                    "RB_JUDGE_BASE_URL": base_url,
                    "RB_JUDGE_API_KEY_ENV": "MOCK_API_KEY",
                    "RB_LIMIT": "1",
                    "RB_SLICE": "dialogue",
                    "RB_TARGET": f"mock@{base_url}",
                }
            )
            completed = subprocess.run(
                [
                    node,
                    str(PROMPTFOO_CLI),
                    "eval",
                    "-c",
                    str(PF / "promptfooconfig.dialogue.yaml"),
                    "-o",
                    str(result_path),
                    "--max-concurrency",
                    "1",
                    "--no-cache",
                ],
                cwd=PF,
                env=env,
                capture_output=True,
                text=True,
                timeout=60,
            )
            if completed.returncode != 0:
                raise RuntimeError(
                    "keyless promptfoo dialogue failed\n"
                    f"stdout:\n{completed.stdout}\n"
                    f"stderr:\n{completed.stderr}"
                )

            payload = json.loads(result_path.read_text(encoding="utf-8"))
            rows = payload.get("results", {}).get("results", [])
            if len(rows) != 1 or rows[0].get("success") is not True:
                raise AssertionError(f"expected one passing dialogue row, got: {rows!r}")
            print("keyless promptfoo dialogue smoke passed (1/1 rows)")
    finally:
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait(timeout=5)


if __name__ == "__main__":
    main()
