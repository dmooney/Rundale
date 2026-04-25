#!/usr/bin/env python3
"""
Compute historical meters-north / meters-east offsets from an anchor location
to a set of cluster locations, as they existed in a specific git commit.

Use this before setting `relative_to` refs so the cluster restores the
authorial spatial layout — not whatever drifted state is in today's working
directory.

Example (the Crossroads cluster from commit e1f3aa0):

    python3 .agents/skills/rundale-geo-tool/scripts/compute_historical_offsets.py \\
        --anchor-id 1 --cluster 2,3,4,6,9,13 --baseline-commit 91c996c

Output is valid JSON on stdout plus a human-readable summary on stderr, so
you can pipe it into `add_relative_to.py`:

    python3 .agents/skills/rundale-geo-tool/scripts/compute_historical_offsets.py \\
        --anchor-id 1 --cluster 2,3,4,6,9,13 --baseline-commit 91c996c \\
        | python3 .agents/skills/rundale-geo-tool/scripts/add_relative_to.py \\
            --anchor-id 1

Assumes cwd is the repo root and the baseline commit is reachable from HEAD.
"""
import argparse
import json
import math
import subprocess
import sys


def offsets_m(anchor_lat, anchor_lon, point_lat, point_lon):
    """Return (dnorth_m, deast_m) from anchor to point using WGS-84 ENU."""
    R = 6_371_000.0
    dnorth = (point_lat - anchor_lat) * (math.pi / 180) * R
    deast = (
        (point_lon - anchor_lon)
        * (math.pi / 180)
        * R
        * math.cos(math.radians(anchor_lat))
    )
    return dnorth, deast


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--anchor-id",
        type=int,
        required=True,
        help="Location id to measure offsets from (e.g. 1 for The Crossroads)",
    )
    parser.add_argument(
        "--cluster",
        required=True,
        help="Comma-separated list of location ids in the cluster (e.g. 2,3,4,6,9,13)",
    )
    parser.add_argument(
        "--baseline-commit",
        required=True,
        help="Git commit SHA to read world.json from (e.g. 91c996c for hand-authored layout)",
    )
    parser.add_argument(
        "--world-path",
        default="mods/rundale/world.json",
        help="Path to world.json inside the baseline commit",
    )
    args = parser.parse_args()

    cluster_ids = [int(s) for s in args.cluster.split(",")]

    try:
        baseline_text = subprocess.check_output(
            ["git", "show", f"{args.baseline_commit}:{args.world_path}"],
            stderr=subprocess.PIPE,
        ).decode()
    except subprocess.CalledProcessError as e:
        print(
            f"failed to read {args.world_path} at {args.baseline_commit}: "
            f"{e.stderr.decode().strip()}",
            file=sys.stderr,
        )
        sys.exit(1)

    baseline = {l["id"]: l for l in json.loads(baseline_text)["locations"]}

    if args.anchor_id not in baseline:
        print(
            f"anchor id {args.anchor_id} not found at {args.baseline_commit}",
            file=sys.stderr,
        )
        sys.exit(1)

    anchor = baseline[args.anchor_id]
    print(
        f"anchor id={args.anchor_id} {anchor['name']!r} at "
        f"({anchor['lat']:.6f}, {anchor['lon']:.6f}) from {args.baseline_commit}",
        file=sys.stderr,
    )
    print("", file=sys.stderr)

    offsets = {}
    for cid in cluster_ids:
        if cid not in baseline:
            print(
                f"  id={cid:2d} (not present at {args.baseline_commit}, skipped)",
                file=sys.stderr,
            )
            continue
        loc = baseline[cid]
        dn, de = offsets_m(anchor["lat"], anchor["lon"], loc["lat"], loc["lon"])
        offsets[str(cid)] = {"dnorth_m": round(dn, 2), "deast_m": round(de, 2)}
        dist = math.hypot(dn, de)
        print(
            f"  id={cid:2d} {loc['name']!r:30s}  "
            f"offset ({dn:+8.1f} N, {de:+8.1f} E)  [{dist:>6.0f} m]",
            file=sys.stderr,
        )

    print(json.dumps(offsets, indent=2))


if __name__ == "__main__":
    main()
