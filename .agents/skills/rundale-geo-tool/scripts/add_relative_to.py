#!/usr/bin/env python3
"""
Add `relative_to` references to a set of locations in mods/rundale/world.json.

Takes an anchor id and a mapping `{ "<location_id>": { "dnorth_m": <f>, "deast_m": <f> } }`
and edits world.json in-place, inserting `relative_to: { anchor, dnorth_m, deast_m }`
before `geo_source` (or appending to the struct) to match the serde field
order that `realign_rundale_coords` writes.

Preserves 4-space indent and trailing newline so the file remains
byte-identical-friendly with the editor's deterministic writer.

Example:

    python3 .agents/skills/rundale-geo-tool/scripts/add_relative_to.py \\
        --anchor-id 1 \\
        --offsets '{"2": {"dnorth_m": 445, "deast_m": 462}, \\
                    "3": {"dnorth_m": 389, "deast_m": -264}}'

Or pipe from compute_historical_offsets.py:

    python3 .agents/skills/rundale-geo-tool/scripts/compute_historical_offsets.py \\
        --anchor-id 1 --cluster 2,3 --baseline-commit 91c996c \\
        | python3 .agents/skills/rundale-geo-tool/scripts/add_relative_to.py \\
            --anchor-id 1

After running, execute `just realign-coords` so the resolver materialises
the new `relative_to` refs back into absolute `lat`/`lon`.
"""
import argparse
import json
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--anchor-id",
        type=int,
        required=True,
        help="Location id that cluster members will be relative to",
    )
    parser.add_argument(
        "--offsets",
        help="JSON object mapping location id (as string) to "
        '{"dnorth_m": <f>, "deast_m": <f>}. If omitted, reads from stdin.',
    )
    parser.add_argument(
        "--world-path",
        default="mods/rundale/world.json",
        help="Path to world.json to edit",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would change without writing",
    )
    args = parser.parse_args()

    if args.offsets is None:
        if sys.stdin.isatty():
            print(
                "error: --offsets is required when stdin is a terminal",
                file=sys.stderr,
            )
            sys.exit(1)
        offsets_raw = sys.stdin.read()
    else:
        offsets_raw = args.offsets
    offsets = {int(k): v for k, v in json.loads(offsets_raw).items()}

    with open(args.world_path) as f:
        world = json.load(f)

    updated = 0
    for loc in world["locations"]:
        if loc["id"] not in offsets:
            continue
        off = offsets[loc["id"]]
        rel = {
            "anchor": args.anchor_id,
            "dnorth_m": float(off["dnorth_m"]),
            "deast_m": float(off["deast_m"]),
        }
        # Insert before geo_source to match serde struct field order
        # (id, ..., geo_kind, relative_to, geo_source). Not load-bearing —
        # realign_rundale_coords will canonicalise on the next run anyway —
        # but nice for humans reading the diff.
        new_loc = {}
        inserted = False
        for k, v in loc.items():
            if k == "geo_source" and not inserted:
                new_loc["relative_to"] = rel
                inserted = True
            new_loc[k] = v
        if not inserted:
            new_loc["relative_to"] = rel
        loc.clear()
        loc.update(new_loc)
        updated += 1
        print(
            f"  id={loc['id']:2d} {loc['name']!r:30s} "
            f"relative_to={{anchor: {args.anchor_id}, "
            f"dnorth_m: {rel['dnorth_m']:+.1f}, deast_m: {rel['deast_m']:+.1f}}}",
            file=sys.stderr,
        )

    if args.dry_run:
        print(f"\ndry-run: would update {updated} location(s)", file=sys.stderr)
        return

    with open(args.world_path, "w") as f:
        json.dump(world, f, indent=4)
        f.write("\n")

    print(
        f"\nupdated {updated} location(s) in {args.world_path}\n"
        f"run `just realign-coords` to resolve the new relative_to refs",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
