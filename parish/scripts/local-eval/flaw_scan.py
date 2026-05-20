#!/usr/bin/env python3
"""Run N dialogue prompts through one target; flag non-Latin script leakage.

Target is a `model@base_url[#env:VAR]` spec (see `eval_lib.parse_target`).
Default reproduces the macOS vllm-mlx large-slot run.

Examples::

    # default vllm-mlx large slot (Qwen2.5-14B on :8000)
    python3 flaw_scan.py

    # cloud: Claude Sonnet 4.6 via Anthropic's OpenAI-compat endpoint
    python3 flaw_scan.py \\
        --target 'claude-sonnet-4-6@https://api.anthropic.com/v1#env:ANTHROPIC_API_KEY' \\
        --output docs/proofs/local-perf/dialogue_flaw_scan_sonnet.md \\
        --prompts 25
"""
from __future__ import annotations

import argparse
import sys
import unicodedata
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from eval_lib import CostTracker, Target, call_chat, load_slice, parse_target  # noqa: E402

DEFAULT_TARGET = "mlx-community/Qwen2.5-14B-Instruct-4bit@http://localhost:8000/v1"
DEFAULT_OUTPUT = (
    Path(__file__).resolve().parents[3]
    / "docs"
    / "proofs"
    / "local-perf"
    / "dialogue_flaw_scan.md"
)

SYSTEM = (
    "You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. "
    "You are kind but direct, with a deep knowledge of local plants and folk medicine. "
    "You have known the player's family for years.\n\n"
    "LANGUAGE: Speak in en-IE (Hiberno-English). "
    "Use spelling, idioms, and conventions appropriate to en-IE. "
    'Never use en-US spellings such as "color", "realize", "favor", '
    '"neighbor", or "-ize" verb endings — use the en-IE form. '
    "Where a native speaker would naturally code-switch, sprinkle words and "
    "short phrases from ga-IE (Irish Gaeilge) into your dialogue. "
    "Use ONLY en-IE and ga-IE. "
    "Do NOT use any other language under any circumstances — no Russian, "
    "Chinese, Japanese, Korean, Arabic, Hebrew, Greek, Hindi, Spanish, "
    "French, German, Italian, or transliterations. "
    "Every character you emit must be either Latin (a-z, A-Z, accented "
    "Latin like á é í ó ú) or standard punctuation. "
    "If you are tempted to use a non-English word, replace it with its "
    "en-IE or ga-IE equivalent or omit it.\n\n"
    "Stay in character. Speak in 1-3 sentences. Do not use modern language."
)

# Prompts are loaded from the frozen rundale-bench v1 dataset so the same
# corpus drives both this flaw-scan probe and the broader benchmark. Edit
# rundale-bench/v1/dialogue.jsonl + rebuild MANIFEST.json to
# add or change prompts; this script auto-loads any new records.
_RECORDS = load_slice('dialogue', version='v1')
PROMPTS = [r['prompt'] for r in _RECORDS]
assert len(PROMPTS) >= 100, len(PROMPTS)


def chars_by_script(text: str) -> dict[str, set[str]]:
    bad: dict[str, set[str]] = {}
    for c in text:
        if c.isspace() or not c.isprintable():
            continue
        try:
            unicodedata.name(c)
        except ValueError:
            continue
        cp = ord(c)
        script = None
        if 0x0400 <= cp <= 0x04FF or 0x0500 <= cp <= 0x052F:
            script = "Cyrillic"
        elif 0x0600 <= cp <= 0x06FF or 0x0750 <= cp <= 0x077F:
            script = "Arabic"
        elif 0x0590 <= cp <= 0x05FF:
            script = "Hebrew"
        elif 0x0370 <= cp <= 0x03FF or 0x1F00 <= cp <= 0x1FFF:
            script = "Greek"
        elif 0x4E00 <= cp <= 0x9FFF or 0x3400 <= cp <= 0x4DBF:
            script = "Han"
        elif 0x3040 <= cp <= 0x309F:
            script = "Hiragana"
        elif 0x30A0 <= cp <= 0x30FF:
            script = "Katakana"
        elif 0xAC00 <= cp <= 0xD7AF:
            script = "Hangul"
        elif 0x0900 <= cp <= 0x097F:
            script = "Devanagari"
        if script:
            bad.setdefault(script, set()).add(c)
    return bad


def flaws(text: str) -> list[str]:
    found: list[str] = []
    by_script = chars_by_script(text)
    for script, chars in by_script.items():
        found.append(f"{script}({''.join(sorted(chars))})")
    if not text.strip():
        found.append("empty")
    if len(text) > 800:
        found.append(f"long({len(text)}c)")
    return found


def run_one(idx: int, prompt: str, target: Target) -> tuple[int, str, str, list[str], dict]:
    try:
        out, usage = call_chat(target, SYSTEM, prompt, max_tokens=200)
    except Exception as e:
        return idx, prompt, "", [f"error:{e}"], {}
    return idx, prompt, out, flaws(out), usage


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--target", default=DEFAULT_TARGET,
                    help=f"Target spec (default: {DEFAULT_TARGET})")
    ap.add_argument("--output", default=str(DEFAULT_OUTPUT),
                    help=f"Markdown report path (default: {DEFAULT_OUTPUT})")
    ap.add_argument("--prompts", type=int, default=100,
                    help="Number of prompts to run (default: 100, max: %d)" % len(PROMPTS))
    ap.add_argument("--workers", type=int, default=4,
                    help="Concurrent requests (default: 4; lower for rate-limited cloud)")
    args = ap.parse_args()

    target = parse_target(args.target)
    n_prompts = min(args.prompts, len(PROMPTS))
    selected = PROMPTS[:n_prompts]

    print(f"target: {target.model} @ {target.base_url}")
    print(f"running {n_prompts} prompts with {args.workers} workers")

    tracker = CostTracker()
    results: list[tuple[int, str, str, list[str], dict]] = []
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        futs = [pool.submit(run_one, i, p, target) for i, p in enumerate(selected, 1)]
        for fut in as_completed(futs):
            idx, prompt, out, fs, usage = fut.result()
            tracker.record(target, usage)
            results.append((idx, prompt, out, fs, usage))
            tag = " ".join(fs) if fs else "ok"
            print(f"[{idx:3d}] {tag}")
    results.sort()
    flawed = [r for r in results if r[3]]
    print()
    print(f"=== {len(flawed)}/{len(results)} ({len(flawed) * 100 / max(1, len(results)):.0f}%) flagged ===")
    for idx, prompt, out, fs, _u in flawed:
        print(f"\n#{idx}  flaws: {fs}")
        print(f"  Q: {prompt}")
        print(f"  A: {out.strip()[:400]}")
    print(f"\ncost: {tracker.summary()}")

    lines = [
        f"# Dialogue flaw scan — {n_prompts} prompts on `{target.label()}`\n",
        f"\nTarget: `{target.model}` at `{target.base_url}`.\n",
        f"\nFlagged {len(flawed)}/{len(results)} ({len(flawed) * 100 / max(1, len(results)):.0f}%) for non-Latin script leakage or empty/over-long output.\n",
        f"\nRun cost: {tracker.summary()}.\n",
        "\n## Flagged samples\n",
    ]
    if flawed:
        for idx, prompt, out, fs, _u in flawed:
            lines.append(f"\n### #{idx} — {', '.join(fs)}\n")
            lines.append(f"**Prompt:** {prompt}\n")
            lines.append(f"\n**Output:**\n> {out.strip().replace(chr(10), chr(10) + '> ')}\n")
    else:
        lines.append("\n_None._\n")
    lines.append(f"\n## All {n_prompts} prompts + responses\n")
    for idx, prompt, out, fs, _u in results:
        tag = " ⚠ " + " ".join(fs) if fs else ""
        lines.append(f"\n### #{idx}{tag}\n")
        lines.append(f"**Q:** {prompt}\n")
        lines.append(f"\n**A:** {out.strip()}\n")
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text("".join(lines))
    print(f"\nwrote {args.output}")


if __name__ == "__main__":
    main()
