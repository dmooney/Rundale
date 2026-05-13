#!/usr/bin/env python3
"""Run N dialogue prompts through one target; flag non-Latin script leakage.

Target is a `model@base_url[#env:VAR]` spec (see `eval_lib.parse_target`).
Default reproduces the macOS vllm-mlx large-slot run.

Examples::

    # default vllm-mlx large slot (Qwen2.5-14B on :8000)
    python3 flaw_scan.py

    # cloud: Claude Sonnet 4.6 via Anthropic's OpenAI-compat endpoint
    python3 flaw_scan.py \\
        --target 'claude-sonnet-4-6@https://api.anthropic.com/v1#env:PARISH_ANTHROPIC_API_KEY' \\
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
from eval_lib import CostTracker, Target, call_chat, parse_target  # noqa: E402

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

PROMPTS = [
    "I have been having trouble sleeping. The dreams keep coming back.",
    "What do you know about the old Cailleach who lives near the fairy fort?",
    "My mother is taken with a bad cough. Is there anything you can give her?",
    "They say a stranger arrived in the village. Have you heard?",
    "I lost a sheep last night. Could it be more than a wolf?",
    "Do you remember when my father broke his leg in the south field?",
    "I saw lights moving on the hill last night. What were they?",
    "The well water tastes strange this week. Should I be worried?",
    "My wife is heavy with child. When should I fetch you?",
    "Father Cathal preached against the old ways on Sunday. What do you think?",
    "Have you any cure for a toothache that will not let me rest?",
    "I cut my hand on a scythe and the wound runs hot. What should I do?",
    "Is it true that you delivered the Maguire twins?",
    "The crows have been gathering at the crossroads. Is it an omen?",
    "I dreamt of my dead grandmother three nights running. What does it mean?",
    "How does one ward off the evil eye?",
    "The cow's milk has gone bloody. The farmer says it's pixies.",
    "Tell me about the herbs that grow on the bog.",
    "My boy will not stop crying through the night.",
    "What plants do you keep for fever?",
    "The blacksmith's wife is barren. Could you help her?",
    "I am afraid of what the priest would say if he knew I came to you.",
    "How long have you been a midwife?",
    "Were you born in this parish?",
    "Did your mother teach you the healing ways?",
    "My grandfather always trusted you. He said you were the only one who told the truth.",
    "Will it be a hard winter, do you think?",
    "What signs do you watch for in the sky?",
    "Tell me how to make a poultice for an inflamed leg.",
    "Do banshees really cry before a death?",
    "I think my sister has been cursed by a neighbor.",
    "What is the right way to bury a stillborn child?",
    "How can I keep the milk from souring in summer?",
    "Should I salt the doorway against bad spirits?",
    "I want to ask Sean Murphy for his daughter's hand. Will it go well?",
    "Old Tommy said the fairies took his shillings. Could that be true?",
    "What hour is best for picking St. John's wort?",
    "I have heard that yarrow stops bleeding. Is it so?",
    "Tell me about the night my father was born.",
    "Is there a charm to keep mice from the grain?",
    "The Father says drinking the holy well is heresy. What do you say?",
    "My horse is gone lame. He will not bear weight on the hoof.",
    "How do you know when a fever has turned dangerous?",
    "I burned my arm at the forge yesterday. The skin has blistered.",
    "Padraig at the pub said you cured his dog of mange.",
    "What do you use for nettle stings?",
    "I cannot keep food down since the funeral.",
    "Is mistletoe really lucky?",
    "Why do they say the fairy fort should never be ploughed?",
    "What was the parish like when you were a girl?",
    "My daughter has hives all down her arms.",
    "The hens have stopped laying. Has something passed over the yard?",
    "I think the old well has gone dry. What now?",
    "Niamh told me she dreamt of fire and water mixed. Is that bad?",
    "How do you prepare a bath for a child with the croup?",
    "Will you teach me the names of the plants?",
    "I do not trust the new doctor in the town. He bleeds people for everything.",
    "What is willow bark good for?",
    "I have aches in every joint when it rains.",
    "My eyes water and my chest is tight. Is it the hay?",
    "Could you read my palm?",
    "Was there ever a wise woman before you here?",
    "The thatcher fell off the roof. He breathes but he will not wake.",
    "I have not bled in two moons.",
    "My old dog will not eat. He just lies in the corner.",
    "Tell me what a wake should look like.",
    "Does honey heal more than just a sore throat?",
    "Why do they say iron keeps fairies away?",
    "What is the proper way to greet a stranger in this parish?",
    "I cut my foot on a stone. There was a black mark on the wound this morning.",
    "Mary's baby came too early. She is full of grief.",
    "Tell me a story from when you were small.",
    "Father Cathal said the bog water cures nothing. Yet my uncle swore by it.",
    "How do you know if a child has the rickets?",
    "The seanchaí passed through last week. He told us tales of the Fianna.",
    "Have the English soldiers come this far before?",
    "I have a stitch in my side that will not leave.",
    "I think I am with child but cannot say for sure.",
    "How can I help my mother bear the loss of my brother?",
    "Show me how to grind herbs the way you do.",
    "What should I plant for a kitchen garden?",
    "The priest will not bury our cousin in the churchyard. What do we do?",
    "I have a wart on my hand that I cannot be rid of.",
    "Does butter from a black cow really cure burns?",
    "What is the leanan sídhe?",
    "Tell me of the night you delivered your first baby.",
    "Do you ever fear the things you have seen?",
    "I want to learn to read the weather like you do.",
    "What hours of the day do you keep?",
    "The river has risen high. Will it spill into the lower fields?",
    "I cannot stop trembling since the storm last week.",
    "Who taught the herb wisdom before your mother?",
    "Has anyone been lost in the bog this year?",
    "My grandmother left me a brooch shaped like a knot. What does it mean?",
    "Are there proper words to bless a new house?",
    "Should I take chamomile or comfrey for a swollen ankle?",
    "Why is the well by the church called St. Bridget's?",
    "I keep hearing footsteps behind me at night when I walk home.",
    "How do you bind a sprained wrist?",
    "Is it true a red string at the wrist keeps the fever away?",
    "My grandfather is dying. How long should I sit with him?",
    "What was the worst winter you ever lived through?",
]
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
