#!/usr/bin/env python3
"""Pick one available iPhone simulator UDID for CI (stdout only)."""

from __future__ import annotations

import json
import subprocess
import sys


def main() -> int:
    payload = json.loads(
        subprocess.check_output(
            ["xcrun", "simctl", "list", "devices", "available", "-j"],
            text=True,
        )
    )
    preferred: str | None = None
    fallback: str | None = None
    for runtime, devices in sorted(payload.get("devices", {}).items(), reverse=True):
        if "iOS-" not in str(runtime) and "iOS " not in str(runtime):
            continue
        if not isinstance(devices, list):
            continue
        for device in devices:
            if not isinstance(device, dict):
                continue
            name = str(device.get("name") or "")
            udid = device.get("udid")
            if not udid or not name.startswith("iPhone"):
                continue
            if not device.get("isAvailable", True):
                continue
            if "SE" in name and preferred is None:
                preferred = str(udid)
            elif fallback is None:
                fallback = str(udid)
    chosen = preferred or fallback
    if not chosen:
        print("no available iPhone simulator", file=sys.stderr)
        return 1
    print(chosen)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
