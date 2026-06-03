#!/usr/bin/env python3
"""Summarize a harness-shadow divergence ledger (JSONL) into Markdown.

Reads the ledger written by GameTestHarness shadow mode (#1159) — one JSON
record per divergence, each {case, input, old, new} — and emits a Markdown
report: total count, per-case breakdown, the most frequently diverging inputs,
and a handful of concrete examples. This is the go/no-go measurement for the
harness-on-game_loop consolidation, not a gate.
"""

import collections
import json
import sys


def load(path):
    records = []
    try:
        with open(path, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if line:
                    records.append(json.loads(line))
    except FileNotFoundError:
        pass
    return records


def trunc(value, limit=140):
    text = json.dumps(value, ensure_ascii=False)
    return text if len(text) <= limit else text[: limit - 1] + "…"


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "target/harness-shadow-ledger.jsonl"
    records = load(path)

    print("# Harness shadow — initial divergence ledger")
    print()
    print(
        "Differential run of the `GameTestHarness` corpus against the real "
        "`parish_core::game_loop` (`PARISH_HARNESS_SHADOW=1`). Each record is an "
        "input where the legacy router's player-visible (`text-log`) output and "
        "the real loop's output differ after normalization. This is a "
        "**measurement** (non-gating): the go/no-go signal for draining the "
        "legacy path (#1159)."
    )
    print()

    if not records:
        print(f"_No ledger found at `{path}`, or zero divergences recorded._")
        return

    print(f"**Total divergences: {len(records)}**")
    print()

    by_case = collections.Counter(r.get("case", "?") for r in records)
    print("## By case")
    print()
    print("| Case | Divergences |")
    print("| --- | --- |")
    for case, count in by_case.most_common():
        print(f"| `{case}` | {count} |")
    print()

    by_input = collections.Counter(r.get("input", "?") for r in records)
    print("## Most frequently diverging inputs")
    print()
    print("| Input | Count |")
    print("| --- | --- |")
    for inp, count in by_input.most_common(15):
        safe = inp.replace("|", "\\|")
        print(f"| `{safe}` | {count} |")
    print()

    # One example per distinct input, up to a cap, to keep the report readable.
    print("## Examples (legacy `old` vs real-loop `new`)")
    print()
    seen = set()
    shown = 0
    for r in records:
        inp = r.get("input", "?")
        if inp in seen:
            continue
        seen.add(inp)
        shown += 1
        print(f"### `{inp}`  _(case: {r.get('case', '?')})_")
        print()
        print(f"- **old:** `{trunc(r.get('old'))}`")
        print(f"- **new:** `{trunc(r.get('new'))}`")
        print()
        if shown >= 20:
            break


if __name__ == "__main__":
    main()
