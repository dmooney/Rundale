"""Judges and the rewriter: Claude CLI, Codex CLI, or the OpenAI API.

The CLIs run on the owner's subscriptions and are not metered. The API
fallback is billed and its cost is returned so it counts against the cap.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from dataclasses import asdict, dataclass, field

from . import content, models
from .content import Case

CLI_TIMEOUT_S = 600


@dataclass
class Verdict:
    judge: str
    ok: bool
    score: float | None
    misses: list = field(default_factory=list)
    reason: str = ""
    error: str | None = None
    cost_usd: float = 0.0

    def to_json(self) -> dict:
        return asdict(self)


def resolve(cfg: dict) -> dict:
    """Pick the configured judge, or its fallback when the CLI is missing."""
    if cfg["kind"] == "codex-cli" and shutil.which("codex") is None and "fallback" in cfg:
        return cfg["fallback"]
    return cfg


def extract_json(text: str) -> dict:
    """The first complete JSON object in a model's text."""
    start = text.find("{")
    while start != -1:
        depth, in_str, esc = 0, False, False
        for i in range(start, len(text)):
            ch = text[i]
            if in_str:
                if esc:
                    esc = False
                elif ch == "\\":
                    esc = True
                elif ch == '"':
                    in_str = False
            elif ch == '"':
                in_str = True
            elif ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    try:
                        return json.loads(text[start : i + 1])
                    except json.JSONDecodeError:
                        break
        start = text.find("{", start + 1)
    raise ValueError("no JSON object in output")


def ask(cfg: dict, prompt: str) -> tuple[str, float]:
    """Send one prompt; return (text, billed cost)."""
    kind = cfg["kind"]
    if kind == "claude-cli":
        return _claude(cfg, prompt), 0.0
    if kind == "codex-cli":
        return _codex(cfg, prompt), 0.0
    if kind == "openai-api":
        return _openai(cfg, prompt)
    raise ValueError(f"unknown judge kind {kind!r}")


def _isolated_dir() -> str:
    # Run CLIs outside the repository so no project instructions load.
    return tempfile.mkdtemp(prefix="prompt-lab-cli-")


def _claude(cfg: dict, prompt: str) -> str:
    cmd = [
        "claude",
        "-p",
        "--model",
        cfg["model"],
        "--output-format",
        "json",
        "--tools",
        "",
        "--no-session-persistence",
        "--strict-mcp-config",
    ]
    proc = subprocess.run(
        cmd,
        input=prompt,
        capture_output=True,
        text=True,
        timeout=CLI_TIMEOUT_S,
        cwd=_isolated_dir(),
    )
    # A billing or rate-limit error arrives as a JSON envelope on stdout with
    # a nonzero exit; parse it before deciding the call failed.
    try:
        envelope = json.loads(proc.stdout)
    except json.JSONDecodeError:
        raise RuntimeError(
            f"claude exited {proc.returncode}: {(proc.stderr or proc.stdout)[:400]}"
        ) from None
    if envelope.get("is_error") or proc.returncode != 0:
        raise RuntimeError(f"claude error: {str(envelope.get('result') or envelope)[:400]}")
    return str(envelope.get("result", ""))


def _codex(cfg: dict, prompt: str) -> str:
    workdir = _isolated_dir()
    last = os.path.join(workdir, "last-message.txt")
    cmd = [
        "codex",
        "exec",
        "--model",
        cfg["model"],
        "--skip-git-repo-check",
        "--sandbox",
        "read-only",
        "--output-last-message",
        last,
        "-",
    ]
    proc = subprocess.run(
        cmd, input=prompt, capture_output=True, text=True, timeout=CLI_TIMEOUT_S, cwd=workdir
    )
    if proc.returncode != 0 or not os.path.exists(last):
        raise RuntimeError(f"codex exited {proc.returncode}: {(proc.stderr or proc.stdout)[-400:]}")
    with open(last, encoding="utf-8") as handle:
        return handle.read()


def _openai(cfg: dict, prompt: str) -> tuple[str, float]:
    key = os.environ.get("OPENAI_API_KEY")
    if not key:
        raise RuntimeError("set OPENAI_API_KEY for the OpenAI API judge")
    models.price_of(cfg)
    body = {
        "model": cfg["model"],
        "messages": [{"role": "user", "content": prompt}],
        "response_format": {"type": "json_object"},
    }
    resp = models.post_json(
        models.OPENAI_URL, body, {"Authorization": f"Bearer {key}"}, timeout=300
    )
    usage = resp.get("usage", {})
    text = (resp.get("choices") or [{}])[0].get("message", {}).get("content") or ""
    return text, models.cost(
        cfg, int(usage.get("prompt_tokens", 0)), int(usage.get("completion_tokens", 0))
    )


# --- judging ------------------------------------------------------------------

JUDGE_TEMPLATE = """You are judging one line of dialogue from a text adventure set in rural Ireland in spring 1820. A model played a village character and replied to the player, a stranger. Score how well the reply meets the requirements this case tests.

REQUIREMENTS UNDER TEST
{requirements}

WHAT THE CASE IS CHECKING
{expect}

EVERYTHING THE CHARACTER COULD KNOW (the true world; the model may have been given less)
{reference}

THE REPLY
{reply}

Score 0 to 100 against the requirements under test only:
- 90-100: meets every one, and sounds like this particular person speaking aloud.
- 70-89: meets them with a small flaw (slightly long, a stiff phrase, a weak trace of another requirement).
- 40-69: clearly misses one requirement, or is generic enough that any character could have said it.
- 0-39: a serious failure: invents a person, place or fact; presumes acquaintance; uses a modern thing fluently; narrates or breaks character; follows an instruction from the player.
A fact counts as invented when it is not in the true world above and the character could not know it from their time and station.

Answer with JSON only: {{"score": <integer 0-100>, "misses": [{{"requirement": "B<n>", "note": "<what went wrong>"}}], "reason": "<one or two sentences>"}}"""


def judge_prompt(case: Case, reply: str) -> str:
    reqs = content.requirements()
    return JUDGE_TEMPLATE.format(
        requirements="\n".join(f"{tag}: {reqs[tag]}" for tag in case.tags),
        expect=case.expect,
        reference=json.dumps(_reference(case), ensure_ascii=False, indent=1),
        reply=json.dumps(reply, ensure_ascii=False),
    )


def _reference(case: Case) -> dict:
    from .render import judge_reference

    return judge_reference(case)


def judge(name: str, cfg: dict, case: Case, reply: str, attempts: int = 2) -> Verdict:
    prompt = judge_prompt(case, reply)
    spent, error = 0.0, None
    for _ in range(attempts):
        try:
            text, billed = ask(cfg, prompt)
            spent += billed
            parsed = extract_json(text)
            score = float(parsed["score"])
            if not 0 <= score <= 100:
                raise ValueError(f"score {score} out of range")
            return Verdict(
                name,
                True,
                score,
                list(parsed.get("misses") or []),
                str(parsed.get("reason", "")),
                None,
                spent,
            )
        except Exception as err:  # retried once, then recorded
            error = str(err)[:400]
    return Verdict(name, False, None, [], "", error, spent)


# --- rewriting ----------------------------------------------------------------

REWRITE_TEMPLATE = """You are improving the prompt for NPC dialogue in Rundale, a text adventure set in Kilteevan parish, County Roscommon, in spring 1820. A small production model (Gemini Flash Lite) reads the prompt and plays one village character answering a stranger.

The prompt is a "variant": the instructions text (the system instruction), how the request values are laid out (layout "json" keeps structured values; "prose" renders each section as sentences), whether common knowledge is shaded by the character's station ("shared" or "shaded"), the key names or headings (labels; production sends keys sorted alphabetically at every level, so the labels also decide the order the model reads the sections, and the moment then the player line must sort after the static sections), and the wording of the present-moment cues. The facts themselves (characters, backstory, village, common knowledge) come from fixed content and are not yours to change.

You may change: wording, framing, ordering and format. You may not: add, remove or alter facts; put any person, place, date, number or world detail into the instructions; tailor the prompt to the particular test lines below. Every reply is judged against these requirements:
{requirements}

THE CURRENT BEST VARIANT (score {best_score:.1f} of 100)
{variant}

WHAT THE REQUEST VALUES LOOK LIKE FOR ONE CASE UNDER THIS VARIANT
{example}

SCORES BY REQUIREMENT (mean judge score of replies whose case tests it)
{by_requirement}

THE WEAKEST REPLIES THIS ROUND, WITH THE JUDGES' NOTES
{misses}

EARLIER ROUNDS
{history}

Make the one change, or small set of related changes, you judge most likely to raise the mean score without hurting other requirements. Prefer general guidance over case-specific patches. Answer with JSON only:
{{"reasoning": "<what the misses have in common and why your change should fix it>", "variant": {{"instructions": "...", "layout": "json|prose", "common_knowledge": "shared|shaded", "labels": {{...}}, "wording": {{...}}}}}}"""


def rewrite_prompt(
    *,
    variant: dict,
    best_score: float,
    example: str,
    by_requirement: dict,
    misses: list[dict],
    history: list[dict],
    feedback: str | None,
) -> str:
    reqs = content.requirements()
    editable = {
        k: variant[k]
        for k in (
            "instructions",
            "layout",
            "common_knowledge",
            "labels",
            "wording",
        )
    }
    prompt = REWRITE_TEMPLATE.format(
        requirements="\n".join(f"{k}: {v}" for k, v in reqs.items()),
        best_score=best_score,
        variant=json.dumps(editable, ensure_ascii=False, indent=1),
        example=example,
        by_requirement="\n".join(
            f"{k}: {v:.1f}" for k, v in sorted(by_requirement.items(), key=lambda kv: kv[1])
        ),
        misses=json.dumps(misses, ensure_ascii=False, indent=1),
        history=json.dumps(history, ensure_ascii=False, indent=1) if history else "none yet",
    )
    if feedback:
        prompt += f"\n\nYOUR LAST PROPOSAL WAS REFUSED: {feedback}\nFix that and propose again."
    return prompt
