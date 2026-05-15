"""Graders for rundale-bench v1 slices.

- `grade_intent` — deterministic exact-match + Jaccard on optional fields.
- `grade_schema` — schema-validate (hand-rolled, no jsonschema dep).
- `grade_dialogue` — invokes the pinned judge_v1 model.
- `grade_reaction` — non-Latin + length + pinned-judge in-character score.
- `grade_simulation` — schema-valid + pinned-judge plausibility score.

The judge graders defer their LLM call to `eval_lib.call_chat`, which makes
`grade.py` itself purely a structuring layer over deterministic checks plus
caller-supplied judge invocations. Reproducibility hinges on rubric pinning
via `<judge-id>.json` files alongside the slices; every judge grader
verifies `rubric_sha256` against `hashlib.sha256(judge["rubric"].encode())`
and raises `RuntimeError` on drift.
"""
from __future__ import annotations

import hashlib
import json
import re
from typing import Any, Callable, Optional

_WORD_RE = re.compile(r"[^\w\s]", flags=re.UNICODE)


def _tokens(s: Optional[str]) -> set[str]:
    if not s:
        return set()
    cleaned = _WORD_RE.sub(" ", s.lower())
    return {t for t in cleaned.split() if t}


def _jaccard(a: Optional[str], b: Optional[str]) -> float:
    """None vs None → 1.0; None vs string → 0.0; else token-set Jaccard."""
    if a is None and b is None:
        return 1.0
    if a is None or b is None:
        return 0.0
    ta, tb = _tokens(a), _tokens(b)
    if not ta and not tb:
        return 1.0
    union = ta | tb
    return len(ta & tb) / len(union) if union else 0.0


# ---------------------------------------------------------------------------
# intent slice — deterministic
# ---------------------------------------------------------------------------

def grade_intent(pred: dict, gold: dict) -> dict:
    """Score one intent prediction against gold.

    score = label_match * (0.5 + 0.25*target_jaccard + 0.25*dialogue_jaccard)
    label_match is hard 0/1. If the label is wrong, score is 0 regardless of
    field overlap — getting "move" vs "talk" wrong is a game-breaking parse
    error, not a partial credit case.
    """
    label_match = 1 if str(pred.get("intent")) == str(gold.get("intent")) else 0
    tj = _jaccard(pred.get("target"), gold.get("target"))
    dj = _jaccard(pred.get("dialogue"), gold.get("dialogue"))
    score = label_match * (0.5 + 0.25 * tj + 0.25 * dj)
    return {
        "label_match": label_match,
        "target_jaccard": tj,
        "dialogue_jaccard": dj,
        "score": score,
    }


# ---------------------------------------------------------------------------
# schema validation — hand-rolled, supports the subset rundale-bench uses
# ---------------------------------------------------------------------------

def _validate(value: Any, schema: dict, path: str = "$") -> list[str]:
    errors: list[str] = []
    t = schema.get("type")
    if isinstance(t, list):
        if not any(_type_matches(value, tt) for tt in t):
            errors.append(f"{path}: type {t} required, got {type(value).__name__}")
            return errors
    elif t and not _type_matches(value, t):
        errors.append(f"{path}: type {t!r} required, got {type(value).__name__}")
        return errors

    if "enum" in schema and value not in schema["enum"]:
        errors.append(f"{path}: value {value!r} not in enum {schema['enum']}")

    if t == "object" or (isinstance(t, list) and "object" in t and isinstance(value, dict)):
        props = schema.get("properties", {})
        for key in schema.get("required", []):
            if key not in value:
                errors.append(f"{path}.{key}: required key missing")
        if schema.get("additionalProperties") is False:
            extras = set(value.keys()) - set(props.keys())
            if extras:
                errors.append(f"{path}: unexpected keys {sorted(extras)}")
        for key, val in value.items():
            if key in props:
                errors.extend(_validate(val, props[key], f"{path}.{key}"))

    if (t == "array" or (isinstance(t, list) and "array" in t and isinstance(value, list))):
        if isinstance(value, list):
            item_schema = schema.get("items")
            if item_schema:
                for i, item in enumerate(value):
                    errors.extend(_validate(item, item_schema, f"{path}[{i}]"))

    return errors


def _type_matches(value: Any, t: str) -> bool:
    if t == "null":
        return value is None
    if t == "string":
        return isinstance(value, str)
    if t == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if t == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if t == "boolean":
        return isinstance(value, bool)
    if t == "object":
        return isinstance(value, dict)
    if t == "array":
        return isinstance(value, list)
    return False


def grade_schema(pred: Any, schema: dict) -> dict:
    """Validate `pred` against `schema`. If pred is a string, JSON-parse first.

    `schema` follows the production OpenAI-style envelope:
    {"name": "...", "strict": true, "schema": {...inner...}}. Inner schema is
    the JSON Schema we actually validate against. If `schema` itself looks
    like the inner schema (has a top-level `type`), we accept that too.
    """
    if isinstance(pred, str):
        try:
            pred = json.loads(pred)
        except (ValueError, TypeError) as e:
            return {"schema_valid": False, "errors": [f"JSON parse: {e}"]}
    inner = schema.get("schema", schema) if isinstance(schema, dict) else schema
    errors = _validate(pred, inner)
    return {"schema_valid": not errors, "errors": errors}


# ---------------------------------------------------------------------------
# judge-backed graders (Phase 3+)
# ---------------------------------------------------------------------------

def verify_judge_rubric(judge: dict) -> None:
    """Raise if rubric_sha256 doesn't match hash of the rubric text.

    Called by every judge-backed grader before invoking the LLM, so silent
    rubric edits surface as a hard failure rather than a quiet score drift.
    """
    rubric = judge.get("rubric", "")
    expected = judge.get("rubric_sha256", "")
    actual = hashlib.sha256(rubric.encode("utf-8")).hexdigest()
    if actual != expected:
        raise RuntimeError(
            f"judge '{judge.get('judge_id')}' rubric sha256 drift: "
            f"manifest={expected} actual={actual}. Edit rubric and "
            f"rubric_sha256 together, never one without the other."
        )


# Lazy-import the script-side flaw scanner so grade.py stays importable
# without the parish/scripts/local-eval path in sys.path.
def _non_latin(text: str) -> dict:
    bad: dict[str, list[str]] = {}
    for c in text:
        if c.isspace() or not c.isprintable():
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
            bad.setdefault(script, [])
            if c not in bad[script]:
                bad[script].append(c)
    return bad


def grade_dialogue(reply: str, judge: dict, invoke: Callable[..., dict]) -> dict:
    """Score a dialogue reply on the pinned 5-axis rubric.

    `invoke` is a thin shim a caller passes in, e.g.
        `lambda system, user, schema: call_chat(judge_target, system, user, schema=schema, temperature=0)[0]`
    It returns the parsed JSON dict the judge emitted. Keeping the LLM call
    behind a callable lets tests substitute deterministic mocks.
    """
    verify_judge_rubric(judge)
    schema = {
        "name": "dialogue_score",
        "strict": True,
        "schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "character": {"type": "integer"},
                "authenticity": {"type": "integer"},
                "language": {"type": "integer"},
                "responsiveness": {"type": "integer"},
                "craft": {"type": "integer"},
                "overall": {"type": "number"},
            },
            "required": ["character", "authenticity", "language", "responsiveness", "craft", "overall"],
        },
    }
    user = (
        "Reply to judge — score the candidate label `Model X` only:\n\n"
        f"Model X: {reply}\n"
    )
    nl = _non_latin(reply)
    try:
        scores = invoke(judge["rubric"], user, schema)
        if not isinstance(scores, dict):
            raise ValueError(f"judge returned non-dict: {type(scores).__name__}")
        return {
            "character": int(scores.get("character") or 0),
            "authenticity": int(scores.get("authenticity") or 0),
            "language": int(scores.get("language") or 0),
            "responsiveness": int(scores.get("responsiveness") or 0),
            "craft": int(scores.get("craft") or 0),
            "overall": float(scores.get("overall") or 0.0),
            "non_latin_chars": nl,
        }
    except Exception as e:
        return {
            "character": 0, "authenticity": 0, "language": 0,
            "responsiveness": 0, "craft": 0, "overall": 0.0,
            "non_latin_chars": nl,
            "error": str(e),
        }


def grade_reaction(reply: str, persona: str, judge: dict, invoke: Callable[..., dict]) -> dict:
    """Non-Latin + length + LLM in-character score for reaction slice."""
    verify_judge_rubric(judge)
    nl = _non_latin(reply)
    length_ok = 5 <= len(reply.strip()) <= 400
    if nl or not length_ok:
        return {
            "schema_valid": True, "in_character": 0,
            "length_ok": length_ok, "non_latin_chars": nl, "score": 0.0,
        }
    schema = {
        "name": "reaction_score",
        "strict": True,
        "schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {"in_character": {"type": "integer"}},
            "required": ["in_character"],
        },
    }
    user = f"Persona: {persona}\n\nReply: {reply}\n"
    try:
        scores = invoke(judge["rubric"], user, schema)
        if not isinstance(scores, dict):
            raise ValueError(f"judge returned non-dict: {type(scores).__name__}")
        ic = int(scores.get("in_character") or 0)
        return {
            "schema_valid": True, "in_character": ic,
            "length_ok": length_ok, "non_latin_chars": nl,
            "score": ic / 5.0,
        }
    except Exception as e:
        return {
            "schema_valid": True, "in_character": 0,
            "length_ok": length_ok, "non_latin_chars": nl,
            "score": 0.0, "error": str(e),
        }


def grade_pairwise(
    reply_a: str,
    reply_b: str,
    prompt: str,
    judge: dict,
    invoke: Callable[..., dict],
) -> dict:
    """Pairwise judge between two candidate replies. Returns {winner, reason}.

    `winner` is `"A"`, `"B"`, or `"tie"`. Caller is responsible for position
    randomization — swap A and B labels at random and back out the swap when
    tallying wins, so the judge's first-position bias doesn't accumulate.

    Non-Latin script in either reply auto-flags it as a loss against any
    Latin-only reply (judge can't be trusted to enforce this consistently).
    """
    verify_judge_rubric(judge)
    nl_a = _non_latin(reply_a)
    nl_b = _non_latin(reply_b)
    if nl_a and not nl_b:
        return {"winner": "B", "reason": f"reply A contains non-Latin: {list(nl_a)}", "auto_disqualified": "A"}
    if nl_b and not nl_a:
        return {"winner": "A", "reason": f"reply B contains non-Latin: {list(nl_b)}", "auto_disqualified": "B"}

    schema = {
        "name": "pairwise_judgment",
        "strict": True,
        "schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "winner": {"type": "string", "enum": ["A", "B", "tie"]},
                "reason": {"type": "string"},
            },
            "required": ["winner", "reason"],
        },
    }
    user = (
        f"Player prompt: {prompt}\n\n"
        f"=== Reply A ===\n{reply_a}\n\n"
        f"=== Reply B ===\n{reply_b}\n"
    )
    try:
        out = invoke(judge["rubric"], user, schema)
        if not isinstance(out, dict):
            raise ValueError(f"judge returned non-dict: {type(out).__name__}")
        winner = out.get("winner")
        if winner not in ("A", "B", "tie"):
            raise ValueError(f"judge returned invalid winner: {winner!r}")
        return {"winner": winner, "reason": str(out.get("reason") or "")[:200]}
    except Exception as e:
        return {"winner": "tie", "reason": "", "error": str(e)}


def grade_simulation(reply: Any, schema: dict, judge: dict, invoke: Callable[..., dict]) -> dict:
    """Schema-valid + LLM plausibility for tier2-sim / tier3-sim slices."""
    verify_judge_rubric(judge)
    s = grade_schema(reply, schema)
    if not s["schema_valid"]:
        return {"schema_valid": False, "plausibility": 0, "score": 0.0, "errors": s["errors"]}
    judge_schema = {
        "name": "sim_plausibility",
        "strict": True,
        "schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {"plausibility": {"type": "integer"}},
            "required": ["plausibility"],
        },
    }
    parsed = json.loads(reply) if isinstance(reply, str) else reply
    user = f"Output to judge: {json.dumps(parsed)}"
    try:
        scores = invoke(judge["rubric"], user, judge_schema)
        if not isinstance(scores, dict):
            raise ValueError(f"judge returned non-dict: {type(scores).__name__}")
        pl = int(scores.get("plausibility") or 0)
        return {"schema_valid": True, "plausibility": pl, "score": pl / 5.0}
    except Exception as e:
        return {"schema_valid": True, "plausibility": 0, "score": 0.0, "error": str(e)}
