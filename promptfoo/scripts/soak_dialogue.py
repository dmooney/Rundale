#!/usr/bin/env python3
"""Measure local dialogue reliability and guard use through the live runtime.

The target model must already be configured on a running Parish backend. Every
attempt goes through `/api/command`; the response carries parser and guard
telemetry emitted by the canonical NPC turn path. This deliberately does not
reimplement the parser or guards in Python.
"""

from __future__ import annotations

import argparse
import hashlib
import http.cookiejar
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402

QUESTIONS = (
    "God save you. What news of the parish?",
    "How does the harvest look this year?",
    "Have you seen Father Declan today?",
    "And how is your own family keeping?",
    "Is old Festus the cooper still at his shop by the bridge?",
    "How do I get to the abbey in town?",
    "Tell me about your work, but keep it brief.",
    "I cannot sleep for worrying. What would you advise?",
    "Do you know Padraig Darcy?",
    "What can you tell me of Ballyglass Castle?",
    "I heard Cormac Duffy was at the mill. Is that true?",
    "Thank you. I should let you return to your work.",
)

LOCATIONS = (
    "the forge",
    "the mill",
    "the holy well",
    "the crossroads",
    "st. brigid's church",
    "darcy's pub",
    "murphy's farm",
    "the weaver's cottage",
    "the letter office",
    "knockcroghery village",
)
DEFAULT_TURNS_PER_LOCATION = len(QUESTIONS)


class RuntimeClient:
    def __init__(self, base_url: str, timeout_s: float):
        self.base_url = base_url.rstrip("/")
        self.timeout_s = timeout_s
        jar = http.cookiejar.CookieJar(policy=LoopbackSecureCookiePolicy())
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))

    def request(self, method: str, path: str, body: dict | None = None) -> Any:
        data = None if body is None else json.dumps(body).encode("utf-8")
        req = urllib.request.Request(
            f"{self.base_url}{path}",
            data=data,
            method=method,
            headers={"Content-Type": "application/json"} if data else {},
        )
        with self.opener.open(req, timeout=self.timeout_s) as response:
            raw = response.read().decode("utf-8")
            return json.loads(raw) if raw.strip() else None

    def health(self) -> None:
        req = urllib.request.Request(f"{self.base_url}/api/health", method="GET")
        with self.opener.open(req, timeout=self.timeout_s) as response:
            response.read()

    def command(self, text: str, addressed_to: list[str] | None = None) -> dict:
        return self.request(
            "POST",
            "/api/command",
            {
                "text": text,
                "addressedTo": addressed_to or [],
                "timeoutMs": int(self.timeout_s * 1000),
                "includeState": False,
            },
        )


class LoopbackSecureCookiePolicy(http.cookiejar.DefaultCookiePolicy):
    """Return Secure session cookies to an HTTP loopback test server only.

    Parish intentionally marks its production session cookie Secure. Browsers
    treat localhost as a trustworthy context, but Python's cookie jar otherwise
    suppresses the cookie on ``http://127.0.0.1``. A long soak would then create
    one server session per request and hit admission control at 50 calls.
    """

    def return_ok_secure(self, cookie, request) -> bool:
        hostname = urllib.parse.urlsplit(request.get_full_url()).hostname
        if cookie.secure and hostname in {"127.0.0.1", "::1", "localhost"}:
            return True
        return super().return_ok_secure(cookie, request)


def _manifest_merkle() -> str:
    manifest = json.loads((rb.V2_DIR / "MANIFEST.json").read_text(encoding="utf-8"))
    return str(manifest["merkle_root_sha256"])


def _npc_name(value: dict) -> str | None:
    for key in ("display_name", "displayName", "name"):
        name = value.get(key)
        if isinstance(name, str) and name.strip():
            return name.strip()
    return None


def _reached_dialogue_inference(quality: dict[str, Any]) -> bool:
    """Whether a command produced a model request eligible for the soak."""

    return bool(quality.get("request_profiles"))


def _find_npc(client: RuntimeClient, location_index: int) -> tuple[str, int]:
    for offset in range(len(LOCATIONS)):
        idx = (location_index + offset) % len(LOCATIONS)
        client.command(f"go to {LOCATIONS[idx]}")
        npcs = client.request("GET", "/api/npcs-here")
        if isinstance(npcs, list):
            names = [_npc_name(item) for item in npcs if isinstance(item, dict)]
            if any(names):
                return next(name for name in names if name), idx
    raise RuntimeError("no NPC found after visiting every configured soak location")


def _atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(content, encoding="utf-8")
    os.replace(temporary, path)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args: argparse.Namespace) -> dict:
    output = args.output.resolve()
    turns_path = output.with_suffix(".turns.jsonl")
    partial_path = output.with_suffix(".partial.jsonl")
    if output.exists() or turns_path.exists():
        raise RuntimeError(
            f"refusing to overwrite completed soak artifact: {output} / {turns_path}"
        )

    client = RuntimeClient(args.base_url, args.timeout_seconds)
    client.health()
    client.request("POST", "/api/new-game", {})
    # Qualification measures the player-facing dialogue path, not contention
    # from autonomous world ticks. Category routing is still exercised by
    # movement and intent parsing, but pausing removes nondeterministic
    # background simulation from the reliability denominator.
    pause = client.command("/pause")
    if pause.get("outcome") not in {"ok", "success", "paused", None}:
        raise RuntimeError(f"failed to pause background simulation: {pause!r}")

    existing: list[dict] = []
    if partial_path.exists():
        existing = [
            json.loads(line)
            for line in partial_path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
        if any(item.get("candidate") != args.candidate for item in existing):
            raise RuntimeError("partial soak belongs to a different candidate")
        if any(item.get("dataset_merkle") != _manifest_merkle() for item in existing):
            raise RuntimeError("partial soak belongs to a different frozen dataset")

    location_index = 0
    npc, location_index = _find_npc(client, location_index)
    started = time.time()
    with partial_path.open("a", encoding="utf-8") as partial:
        for attempt in range(len(existing), args.calls):
            if attempt and attempt % args.turns_per_location == 0:
                location_index = (location_index + 1) % len(LOCATIONS)
                npc, location_index = _find_npc(client, location_index)

            question = QUESTIONS[attempt % len(QUESTIONS)]
            record: dict[str, Any] = {
                "version": 1,
                "candidate": args.candidate,
                "dataset_merkle": _manifest_merkle(),
                "attempt": attempt,
                "npc": npc,
                "question_id": attempt % len(QUESTIONS),
                "question": question,
                "contract_valid": 0,
                "turns": 0,
                "guard_interventions": 0,
                "guard_reasons": [],
                "parse_dispositions": [],
                "request_profiles": [],
                "continuity_retries": 0,
            }
            try:
                for continuity_retry in range(len(LOCATIONS) + 1):
                    response = client.command(question, [npc])
                    quality = (response.get("kind_detail") or {}).get("dialogue_quality", {})
                    # A successful command without a dialogue request means the
                    # prior conversation has closed (for example after a
                    # model-generated farewell). Reacquire an NPC and retry the
                    # same sample; no-inference commands are not reliability
                    # observations and must never enter the denominator.
                    if _reached_dialogue_inference(quality):
                        record["continuity_retries"] = continuity_retry
                        break
                    location_index = (location_index + 1) % len(LOCATIONS)
                    npc, location_index = _find_npc(client, location_index)
                    record["npc"] = npc
                else:
                    raise RuntimeError(
                        "dialogue did not reach inference after visiting every "
                        "configured soak location"
                    )
                record.update(
                    {
                        "outcome": response.get("outcome"),
                        "elapsed_ms": response.get("elapsed_ms"),
                        "contract_valid": int(quality.get("contract_valid", 0)),
                        "turns": int(quality.get("turns", 0)),
                        "guard_interventions": int(quality.get("guard_interventions", 0)),
                        "guard_reasons": list(quality.get("guard_reasons", [])),
                        "parse_dispositions": list(quality.get("parse_dispositions", [])),
                        "request_profiles": list(quality.get("request_profiles", [])),
                        "response_lines": list(response.get("lines", [])),
                    }
                )
            except (urllib.error.URLError, TimeoutError, ValueError) as exc:
                record["transport_error"] = f"{type(exc).__name__}: {exc}"

            partial.write(json.dumps(record, sort_keys=True) + "\n")
            partial.flush()
            os.fsync(partial.fileno())

    os.replace(partial_path, turns_path)
    rows = [
        json.loads(line)
        for line in turns_path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    total_turns = sum(int(row["turns"]) for row in rows)
    request_profiles = {
        json.dumps(profile, sort_keys=True) for row in rows for profile in row["request_profiles"]
    }
    if len(request_profiles) != 1:
        raise RuntimeError(
            "soak observed zero or multiple dialogue request profiles; "
            "qualification requires one exact model/sampling configuration"
        )
    result = {
        "version": 1,
        "candidate": args.candidate,
        "dataset_merkle": _manifest_merkle(),
        "runtime_base_url": args.base_url,
        "elapsed_seconds": time.time() - started,
        "reliability_soak": {
            "calls": len(rows),
            "valid_responses": sum(
                1 for row in rows if int(row["turns"]) == 1 and int(row["contract_valid"]) == 1
            ),
        },
        "guard_observation": {
            "turns": total_turns,
            "interventions": sum(int(row["guard_interventions"]) for row in rows),
            "reasons": {
                reason: sum(row.get("guard_reasons", []).count(reason) for row in rows)
                for reason in sorted(
                    {reason for row in rows for reason in row.get("guard_reasons", [])}
                )
            },
        },
        "parse_dispositions": {
            disposition: sum(row["parse_dispositions"].count(disposition) for row in rows)
            for disposition in ("full_json", "recovered_dialogue", "raw_text")
        },
        "request_profile": json.loads(next(iter(request_profiles))),
        "turns_artifact": {
            "path": turns_path.name,
            "sha256": _sha256(turns_path),
            "records": len(rows),
        },
    }
    _atomic_write(output, json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:3030")
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--calls", type=int, default=500)
    # The final scripted question is a farewell. Move to another NPC before
    # the next cycle so subsequent commands continue to exercise inference
    # rather than correctly returning no dialogue for a closed conversation.
    parser.add_argument("--turns-per-location", type=int, default=DEFAULT_TURNS_PER_LOCATION)
    parser.add_argument("--timeout-seconds", type=float, default=120.0)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.calls <= 0 or args.turns_per_location <= 0:
        parser.error("--calls and --turns-per-location must be positive")
    try:
        result = run(args)
    except (RuntimeError, OSError, ValueError, urllib.error.URLError) as exc:
        print(f"soak failed: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
