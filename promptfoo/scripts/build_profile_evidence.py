#!/usr/bin/env python3
"""Assemble a promotion evidence receipt from measured, hashed artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402


def _read_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _manifest_merkle() -> str:
    return str(
        _read_object(rb.V2_DIR / "MANIFEST.json")["merkle_root_sha256"]
    )


def _platform_id(host: dict[str, Any]) -> tuple[str, str]:
    system = str(host.get("platform", "")).lower()
    machine = str(host.get("machine", "")).lower()
    if system == "darwin" and machine in {"arm64", "aarch64"}:
        return "darwin-arm64", "unified"
    raise ValueError(
        "local runner artifact does not identify a supported measured-memory "
        f"platform (platform={system!r}, machine={machine!r})"
    )


def _validate_turns_artifact(soak_path: Path, soak: dict[str, Any]) -> Path:
    artifact = soak.get("turns_artifact")
    if not isinstance(artifact, dict):
        raise ValueError("soak receipt is missing turns_artifact")
    turns_path = (soak_path.parent / str(artifact.get("path", ""))).resolve()
    if _sha256(turns_path) != artifact.get("sha256"):
        raise ValueError("soak turns artifact hash does not match its receipt")
    records = sum(
        1
        for line in turns_path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    )
    if records != int(artifact.get("records", -1)):
        raise ValueError("soak turns artifact record count does not match its receipt")
    return turns_path


def build(args: argparse.Namespace) -> dict[str, Any]:
    soak_path = args.soak.resolve()
    runner_path = args.local_runner_artifact.resolve()
    soak = _read_object(soak_path)
    runner = _read_object(runner_path)
    profiles = _read_object(rb.CONFIG_DIR / "local_hardware_profiles.json")
    profile = (profiles.get("profiles") or {}).get(args.hardware_profile)
    if not isinstance(profile, dict):
        raise ValueError(f"unknown hardware profile {args.hardware_profile!r}")

    merkle = _manifest_merkle()
    if soak.get("candidate") != args.candidate:
        raise ValueError("soak candidate does not match --candidate")
    if soak.get("dataset_merkle") != merkle:
        raise ValueError("soak dataset merkle does not match the frozen manifest")
    turns_path = _validate_turns_artifact(soak_path, soak)

    target = rb.parse_target(args.candidate)
    request_profile = soak.get("request_profile")
    if not isinstance(request_profile, dict) or request_profile.get("model") != target.model:
        raise ValueError("soak request profile does not match the candidate model")
    rows = [
        row
        for row in runner.get("rows", [])
        if isinstance(row, dict) and row.get("hf_repo") == target.model
    ]
    if not rows:
        raise ValueError(
            f"local runner artifact has no measurements for {target.model!r}"
        )
    peak_memory = max(float(row.get("peak_ram_gb", 0.0)) for row in rows)
    host = runner.get("host")
    if not isinstance(host, dict):
        raise ValueError("local runner artifact is missing host metadata")
    platform_id, memory_kind = _platform_id(host)
    if platform_id != profile.get("platform") or memory_kind != profile.get(
        "memory_kind"
    ):
        raise ValueError("local runner host does not match requested hardware profile")

    evidence = {
        "version": 1,
        "candidate": args.candidate,
        "dataset_merkle": merkle,
        "hardware_profile_id": args.hardware_profile,
        "hardware": {
            "platform": platform_id,
            "memory_kind": memory_kind,
            "total_memory_gb": float(host.get("memory_gb", 0.0)),
            "peak_memory_gb": peak_memory,
            "machine": host.get("machine"),
        },
        "reliability_soak": soak.get("reliability_soak"),
        "guard_observation": soak.get("guard_observation"),
        "request_profile": request_profile,
        "provenance": {
            "created_at": __import__("datetime")
            .datetime.now(__import__("datetime").timezone.utc)
            .isoformat(),
            "created_on": {
                "platform": sys.platform,
                "machine": platform.machine(),
            },
            "soak_receipt": {
                "path": os.path.relpath(soak_path, args.output.resolve().parent),
                "sha256": _sha256(soak_path),
            },
            "soak_turns": {
                "path": os.path.relpath(turns_path, args.output.resolve().parent),
                "sha256": _sha256(turns_path),
            },
            "local_runner_artifact": {
                "path": os.path.relpath(runner_path, args.output.resolve().parent),
                "sha256": _sha256(runner_path),
            },
        },
    }
    if not isinstance(evidence["reliability_soak"], dict) or not isinstance(
        evidence["guard_observation"], dict
    ):
        raise ValueError("soak receipt is missing measurement summaries")
    return evidence


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--hardware-profile", required=True)
    parser.add_argument("--soak", type=Path, required=True)
    parser.add_argument("--local-runner-artifact", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.output.exists():
            raise ValueError(f"refusing to overwrite evidence receipt {args.output}")
        evidence = build(args)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_name(f".{args.output.name}.{os.getpid()}.tmp")
        temporary.write_text(
            json.dumps(evidence, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        os.replace(temporary, args.output)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"evidence build failed: {exc}", file=sys.stderr)
        return 1
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
