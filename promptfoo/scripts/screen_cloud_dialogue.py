#!/usr/bin/env python3
"""Run one immutable cloud-dialogue preflight and performance profile.

The live preflight goes through Parish's canonical NPC turn path. Only a
structurally valid, low-guard profile advances to the fixed promptfoo perf
panel. Every run uses isolated config/state/output directories, so retries
cannot overwrite a prior paid response.
"""

from __future__ import annotations

import argparse
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request
from datetime import date
from pathlib import Path

PF = Path(__file__).resolve().parents[1]
REPO = PF.parent
RUNS = REPO / "docs" / "proofs" / "cloud-dialogue-qualification" / "runs"


def write_once(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("xb") as handle:
            handle.write(content)
    except FileExistsError:
        if path.read_bytes() != content:
            raise RuntimeError(f"immutable artifact collision: {path}") from None


def wait_for_health(port: int, process: subprocess.Popen, timeout: float = 90.0) -> None:
    deadline = time.monotonic() + timeout
    url = f"http://127.0.0.1:{port}/api/health"
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"parish-server exited during startup: {process.returncode}")
        try:
            with urllib.request.urlopen(url, timeout=2) as response:
                if response.status == 200:
                    return
        except OSError:
            time.sleep(0.5)
    raise RuntimeError(f"parish-server did not become healthy on port {port}")


def engine_config(args: argparse.Namespace) -> str:
    lines = [
        "[engine.inference]",
        "streaming_timeout_secs = 300",
        "timeout_secs = 300",
        "",
        "[engine.inference.dialogue_generation]",
        f"max_tokens = {args.max_tokens}",
        "temperature = 0.7",
        "frequency_penalty = 0.5",
        "json_mode = true",
        "enable_thinking = true",
    ]
    if args.reasoning_effort:
        lines.append(f'reasoning_effort = "{args.reasoning_effort}"')
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--slug", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--provider", default="openrouter")
    parser.add_argument("--base-url", default="https://openrouter.ai/api/v1")
    parser.add_argument("--model", required=True)
    parser.add_argument(
        "--reasoning-effort", choices=("minimal", "low", "medium", "high", "xhigh", "max")
    )
    parser.add_argument("--max-tokens", type=int, default=4096)
    parser.add_argument("--port", type=int, default=3041)
    parser.add_argument("--tested-on", default=date.today().isoformat())
    args = parser.parse_args(argv)

    date_dir = RUNS / args.tested_on
    preflight = date_dir / f"{args.slug}-preflight.json"
    perf = date_dir / f"{args.slug}-perf.json"
    if preflight.exists() or perf.exists():
        raise SystemExit(f"refusing to overwrite existing profile: {args.slug}")

    date_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"rundale-cloud-{args.slug}-") as tmp_raw:
        tmp = Path(tmp_raw)
        config = tmp / "parish.toml"
        config.write_text(engine_config(args), encoding="utf-8")
        output_dir = PF / "output" / "cloud-profiles" / args.tested_on / args.slug
        output_dir.mkdir(parents=True, exist_ok=True)

        env = os.environ.copy()
        env.update(
            {
                "PARISH_PROVIDER": args.provider,
                "PARISH_BASE_URL": args.base_url,
                "PARISH_MODEL": args.model,
                "PARISH_DIALOGUE_PROVIDER": args.provider,
                "PARISH_DIALOGUE_BASE_URL": args.base_url,
                "PARISH_DIALOGUE_MODEL": args.model,
                "PARISH_INTENT_PROVIDER": "simulator",
                "PARISH_SIMULATION_PROVIDER": "simulator",
                "PARISH_REACTION_PROVIDER": "simulator",
                "PARISH_ENGINE_CONFIG": str(config),
                "PARISH_USER_CONFIG_DIR": str(tmp / "user-config"),
                "PARISH_USER_DATA_DIR": str(tmp / "user-data"),
                "PARISH_SAVES_DIR": str(tmp / "saves"),
                "PARISH_TILE_CACHE_DIR": str(tmp / "tiles"),
            }
        )
        log_path = output_dir / "parish-server.log"
        with log_path.open("ab") as log:
            server = subprocess.Popen(
                [
                    "cargo",
                    "run",
                    "--quiet",
                    "-p",
                    "parish-server",
                    "--",
                    "--port",
                    str(args.port),
                    "--data-dir",
                    str(REPO / "mods" / "rundale"),
                    "--static-dir",
                    str(REPO / "parish" / "apps" / "ui" / "dist"),
                    "--engine-config",
                    str(config),
                ],
                cwd=REPO / "parish",
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            try:
                wait_for_health(args.port, server)
                subprocess.run(
                    [
                        sys.executable,
                        str(PF / "scripts" / "soak_dialogue.py"),
                        "--candidate",
                        args.candidate,
                        "--output",
                        str(preflight),
                        "--calls",
                        "12",
                        "--base-url",
                        f"http://127.0.0.1:{args.port}",
                        "--timeout-seconds",
                        "300",
                    ],
                    cwd=REPO,
                    env=env,
                    check=True,
                )
            finally:
                if server.poll() is None:
                    os.killpg(server.pid, signal.SIGTERM)
                    try:
                        server.wait(timeout=20)
                    except subprocess.TimeoutExpired:
                        os.killpg(server.pid, signal.SIGKILL)
                        server.wait(timeout=5)

        preflight_data = json.loads(preflight.read_text(encoding="utf-8"))
        calls = preflight_data["reliability_soak"]
        guards = preflight_data["guard_observation"]
        if (
            calls["valid_responses"] != calls["calls"]
            or guards["interventions"] / guards["turns"] > 0.10
        ):
            print(f"[cloud-profile] {args.slug}: preflight rejected; performance skipped")
            subprocess.run(
                [sys.executable, str(PF / "scripts" / "qualification_dashboard.py")],
                cwd=REPO,
                check=True,
            )
            return 2

        perf_env = env | {
            "RB_OUTPUT_DIR": str(output_dir),
            "RB_DIALOGUE_MAX_TOKENS": str(args.max_tokens),
            "RB_DIALOGUE_ENABLE_THINKING": "true",
        }
        if args.reasoning_effort:
            perf_env["RB_DIALOGUE_REASONING_EFFORT"] = args.reasoning_effort
        subprocess.run(
            ["just", "-f", str(PF / "justfile"), "perf", args.candidate],
            cwd=REPO,
            env=perf_env,
            check=True,
        )
        generated = output_dir / "perf.json"
        write_once(perf, generated.read_bytes())
        html = output_dir / "perf.html"
        if html.is_file():
            write_once(date_dir / f"{args.slug}-perf.html", html.read_bytes())
        subprocess.run(
            [sys.executable, str(PF / "scripts" / "qualification_dashboard.py")],
            cwd=REPO,
            check=True,
        )
        print(f"[cloud-profile] {args.slug}: preflight and performance complete")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
