"""Shared bridge for rundale-bench v2 (promptfoo).

Imports the shared HTTP layer (`eval_lib`) and deterministic graders (`grade`)
from the existing tree so candidate-call behaviour stays byte-identical to v1,
and holds the per-slice request builders + the configurable-judge plumbing that
the promptfoo provider and assertions call into.

Only datasets / rubrics / pricing are *copied* into promptfoo/ (so the suite is
self-contained); the call mechanics are imported to avoid drift.
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path
from typing import Any

PROMPTFOO_DIR = Path(__file__).resolve().parent
REPO_ROOT = PROMPTFOO_DIR.parent
V2_DIR = PROMPTFOO_DIR / "v2"
DATASETS_DIR = V2_DIR / "datasets"
RUBRICS_DIR = V2_DIR / "rubrics"
CONFIG_DIR = PROMPTFOO_DIR / "config"

# eval_lib lives with the local-eval scripts; grade.py lives in rundale-bench.
sys.path.insert(0, str(REPO_ROOT / "parish" / "scripts" / "local-eval"))
sys.path.insert(0, str(REPO_ROOT / "rundale-bench"))
sys.path.insert(0, str(CONFIG_DIR))

import eval_lib  # noqa: E402
import grade as rb_grade  # noqa: E402
import pricing  # noqa: E402  (promptfoo/config/pricing.py)

# Bind via the module object (not `from eval_lib import …`) so the linter can't
# strip the names that are only referenced by the provider / assertions / tests.
# Patching `rb_common.call_chat` in tests reassigns the name the code below uses.
Target = eval_lib.Target
parse_target = eval_lib.parse_target
call_chat = eval_lib.call_chat
call_chat_streaming = eval_lib.call_chat_streaming
build_dialogue_system_prompt = eval_lib.build_dialogue_system_prompt

# ---------------------------------------------------------------------------
# Candidate-side system prompts / schemas (mirrors rundale_bench.py constants)
# ---------------------------------------------------------------------------

INTENT_SYS = (
    "You are a text adventure input parser. Given the player's natural language input, "
    "determine their intent. Respond with valid JSON containing:\n"
    '- "intent": one of "move", "talk", "look", "interact", "examine", "unknown"\n'
    '- "target": what the action is directed at (string or null)\n'
    '- "dialogue": what the player is saying, if talking (string or null)\n\n'
    'IMPORTANT: "move" is ONLY for when the player expresses a present desire to '
    "navigate somewhere (imperative or future intent). Narrative, past-tense, or "
    'reflective statements that merely mention a place name are "talk", not "move".\n\n'
    "Examples:\n"
    'Input: "go to the pub" → {"intent": "move", "target": "the pub", "dialogue": null}\n'
    'Input: "talk to Mary" → {"intent": "talk", "target": "Mary", "dialogue": null}\n'
    'Input: "tell Padraig I saw his cow" → {"intent": "talk", "target": "Padraig", "dialogue": "I saw his cow"}\n'
    'Input: "look around" → {"intent": "look", "target": null, "dialogue": null}\n'
    'Input: "pick up the stone" → {"intent": "interact", "target": "the stone", "dialogue": null}\n'
    'Input: "I came from the coast" → {"intent": "talk", "target": null, "dialogue": "I came from the coast"}\n'
    'Input: "I was at the shore yesterday" → {"intent": "talk", "target": null, "dialogue": "I was at the shore yesterday"}\n\n'
    "Respond ONLY with valid JSON. No explanation."
)

INTENT_SCHEMA = {
    "name": "intent",
    "strict": True,
    "schema": {
        "type": "object",
        "additionalProperties": False,
        "properties": {
            "intent": {
                "type": "string",
                "enum": ["move", "talk", "look", "interact", "examine", "unknown"],
            },
            "target": {"type": ["string", "null"]},
            "dialogue": {"type": ["string", "null"]},
        },
        "required": ["intent", "target", "dialogue"],
    },
}

DIALOGUE_SYS = build_dialogue_system_prompt()

GAEILGE_SYS = (
    "You are being evaluated for fluency in Irish Gaeilge.\n\n"
    "Follow the task exactly. Unless the task explicitly says otherwise, answer only "
    "in Irish Gaeilge, not English. Do not explain your choices. Prefer natural Irish "
    "syntax and idiom over word-for-word translation from English."
)


def _gaeilge_candidate_prompt(rec: dict) -> str:
    constraints = "\n".join(f"- {c}" for c in rec.get("constraints", []))
    return (
        f"Task type: {rec['task_type']}\n\n"
        f"Prompt:\n{rec['prompt']}\n\n"
        f"Constraints:\n{constraints}\n\n"
        "Respond now."
    )


# ---------------------------------------------------------------------------
# Candidate generation — one record → (text, usage, perf)
# ---------------------------------------------------------------------------

# Per-slice metadata: which judge rubric + axes, and the candidate max_tokens.
SLICE_META: dict[str, dict[str, Any]] = {
    "dialogue": {
        "rubric": "judge_sonnet_v2",
        "system": "dialogue.system.md",
        "axes": [
            "character",
            "authenticity",
            "language",
            "responsiveness",
            "craft",
            "brevity",
            "repetition",
            "mood_fidelity",
            "grounding",
        ],
    },
    "reaction": {
        "rubric": "judge_reaction_v1",
        "system": "reaction.system.md",
        "axes": ["in_character"],
    },
    "tier2-sim": {"rubric": "judge_sim_v1", "system": "sim.system.md", "axes": ["plausibility"]},
    "tier3-sim": {"rubric": "judge_sim_v1", "system": "sim.system.md", "axes": ["plausibility"]},
    "gaeilge": {
        "rubric": "judge_gaeilge_v1",
        "system": "gaeilge.system.md",
        "axes": ["fluency", "grammar", "idiom", "task_fulfillment", "english_leakage"],
    },
    "multiturn": {
        "rubric": "judge_multiturn_v2",
        "system": "multiturn.system.md",
        "axes": [
            "continuity",
            "name_fidelity",
            "no_premature_farewell",
            "persona_consistency",
            "memory_retention",
            "freshness",
        ],
    },
    "intent": {"rubric": None, "system": None, "axes": []},
}

# Slices whose dataset records carry a captured, runtime-faithful request
# ({system, user, response_format, max_tokens, temperature, frequency_penalty,
# enable_thinking})
# that the candidate must send VERBATIM — no reconstruction (REQ 2).
VERBATIM_SLICES = {"dialogue", "reaction", "tier2-sim", "tier3-sim", "intent"}


def _parse_bool_override(raw: str) -> bool:
    normalised = raw.strip().lower()
    if normalised in {"1", "true", "yes", "on"}:
        return True
    if normalised in {"0", "false", "no", "off"}:
        return False
    raise ValueError("expected true/false")


_DIALOGUE_OVERRIDE_ENV = {
    "max_tokens": ("RB_DIALOGUE_MAX_TOKENS", int),
    "temperature": ("RB_DIALOGUE_TEMPERATURE", float),
    "frequency_penalty": ("RB_DIALOGUE_FREQUENCY_PENALTY", float),
    "enable_thinking": ("RB_DIALOGUE_ENABLE_THINKING", _parse_bool_override),
    "reasoning_effort": ("RB_DIALOGUE_REASONING_EFFORT", str),
}


def effective_dialogue_record(rec: dict) -> dict:
    """Return the captured request with explicitly declared A/B overrides.

    Frozen prompts stay byte-exact; only generation parameters named in the
    environment may differ. The effective record is also stamped into runtime
    metadata, so an exploratory profile can never masquerade as the frozen
    production request.
    """
    effective = dict(rec)
    for field, (env_name, parser) in _DIALOGUE_OVERRIDE_ENV.items():
        raw = os.environ.get(env_name)
        if raw is None:
            continue
        try:
            value = parser(raw)
        except ValueError as exc:
            raise ValueError(f"{env_name} must be a valid {parser.__name__}") from exc
        if field == "max_tokens" and value <= 0:
            raise ValueError(f"{env_name} must be positive")
        effective[field] = value
    return effective


def generate_candidate(slice_name: str, target, rec: dict, *, streaming: bool = False) -> dict:
    """Run one candidate call for `rec` on `slice_name`. Returns
    {output, prompt_tokens, completion_tokens, cost, ttft_ms, tokens_per_second,
     schema_valid, error}. Mirrors the per-slice request shapes in rundale_bench.py.
    """
    out = {
        "output": "",
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "cost": 0.0,
        "ttft_ms": None,
        "tokens_per_second": None,
        "schema_valid": None,
        "error": None,
    }
    try:
        if slice_name in {"dialogue", "multiturn"}:
            rec = effective_dialogue_record(rec)

        if slice_name == "multiturn":
            return _generate_multiturn(target, rec, out)

        if slice_name in VERBATIM_SLICES:
            # Runtime-faithful: send the captured request exactly as the live
            # engine sends it — verbatim system/user, response_format, max_tokens,
            # temperature, frequency_penalty (REQ 2). No reconstruction.
            system = rec.get("system")
            user = rec.get("user", "")
            schema = None
            response_format = rec.get("response_format")
            max_tokens = rec.get("max_tokens")
            temperature = rec.get("temperature", 0.7)
            frequency_penalty = rec.get("frequency_penalty")
            enable_thinking = rec.get("enable_thinking")
            reasoning_effort = rec.get("reasoning_effort")
        elif slice_name == "gaeilge":
            # gaeilge stays a curated Irish-competence probe (no captured request).
            system, user, schema, response_format, frequency_penalty = (
                GAEILGE_SYS,
                _gaeilge_candidate_prompt(rec),
                None,
                None,
                None,
            )
            max_tokens, temperature = rec.get("max_tokens", 300), 0.2
            enable_thinking = None
            reasoning_effort = None
        else:
            raise ValueError(f"unknown slice: {slice_name}")

        if streaming:
            # Perf path — streaming captures ttft + tok/s.
            res = call_chat_streaming(
                target,
                system,
                user,
                max_tokens=max_tokens,
                temperature=temperature,
                response_format=response_format,
                frequency_penalty=frequency_penalty,
                enable_thinking=enable_thinking,
                reasoning={"effort": reasoning_effort} if reasoning_effort else None,
            )
            text = res["text"]
            pt = res.get("prompt_tokens") or 0
            ct = res.get("completion_tokens") or 0
            out["ttft_ms"] = res.get("ttft_ms")
            out["tokens_per_second"] = res.get("tokens_per_second")
            observed_cost = res.get("cost")
        else:
            text, usage = call_chat(
                target,
                system,
                user,
                schema=schema,
                max_tokens=max_tokens,
                temperature=temperature,
                response_format=response_format,
                frequency_penalty=frequency_penalty,
                enable_thinking=enable_thinking,
                reasoning={"effort": reasoning_effort} if reasoning_effort else None,
                max_retries=int(os.environ.get("RB_MAX_RETRIES", "4")),
            )
            pt = usage.get("prompt_tokens", 0)
            ct = usage.get("completion_tokens", 0)
            observed_cost = None

        out["output"] = text
        out["prompt_tokens"] = pt
        out["completion_tokens"] = ct
        out["cost"] = (
            float(observed_cost)
            if observed_cost is not None
            else pricing.estimate_cost(target.model, pt, ct)
        )
        if not text or not text.strip():
            out["error"] = "empty_output"
            return out
        if slice_name in ("tier2-sim", "tier3-sim"):
            out["schema_valid"] = rb_grade.grade_schema(text, rec.get("grade_schema") or {}).get(
                "schema_valid", False
            )
    except Exception as e:  # noqa: BLE001 — surface as a graded error, never crash the run
        out["error"] = str(e)
    return out


def _generate_multiturn(target, rec: dict, out: dict) -> dict:
    """Run a scripted multi-turn conversation, chaining the candidate's own
    replies as the assistant turns (so memory_retention is real). The candidate
    sees the runtime-faithful persona system prompt; each player line is appended
    as a user turn. Returns a single transcript as `output` for the judge."""
    system = rec.get("system")
    turns = rec.get("turns", [])
    response_format = rec.get("response_format")
    temperature = rec.get("temperature", 0.7)
    frequency_penalty = rec.get("frequency_penalty")
    enable_thinking = rec.get("enable_thinking")
    messages: list[dict] = []
    if system:
        messages.append({"role": "system", "content": system})
    transcript_parts: list[str] = []
    pt_total = ct_total = 0
    player = rec.get("player_name", "Player")
    for turn in turns:
        messages.append({"role": "user", "content": turn})
        text, usage = call_chat(
            target,
            None,
            "",
            messages=messages,
            response_format=response_format,
            temperature=temperature,
            frequency_penalty=frequency_penalty,
            enable_thinking=enable_thinking,
        )
        reply = rb_grade.extract_dialogue_for_judging(text) or text
        messages.append({"role": "assistant", "content": reply})
        transcript_parts.append(f"{player}: {turn}\nNPC: {reply}")
        pt_total += usage.get("prompt_tokens", 0)
        ct_total += usage.get("completion_tokens", 0)
    out["output"] = "\n\n".join(transcript_parts)
    out["prompt_tokens"] = pt_total
    out["completion_tokens"] = ct_total
    out["cost"] = pricing.estimate_cost(target.model, pt_total, ct_total)
    return out


# ---------------------------------------------------------------------------
# Configurable judge
# ---------------------------------------------------------------------------

_JUDGE_CACHE: dict | None = None


def load_judge_config() -> dict:
    """Read config/judge.yaml (env-overridable). Tiny hand parser so PyYAML is
    not a hard dependency for the suite."""
    global _JUDGE_CACHE
    if _JUDGE_CACHE is not None:
        return _JUDGE_CACHE
    cfg: dict[str, str] = {}
    path = CONFIG_DIR / "judge.yaml"
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or ":" not in line:
            continue
        k, v = line.split(":", 1)
        # Drop a trailing inline comment and surrounding quotes so e.g.
        # `model: "x"  # note` parses to `x`.
        if " #" in v:
            v = v.split(" #", 1)[0]
        cfg[k.strip()] = v.strip().strip("'\"")
    judge = {
        "model": os.environ.get("RB_JUDGE_MODEL", cfg.get("model", "claude-sonnet-4-6")),
        "base_url": os.environ.get(
            "RB_JUDGE_BASE_URL", cfg.get("base_url", "https://api.anthropic.com/v1")
        ),
        "api_key_env": os.environ.get("RB_JUDGE_API_KEY_ENV", cfg.get("api_key_env") or None),
        "temperature": float(os.environ.get("RB_JUDGE_TEMPERATURE", cfg.get("temperature", "0.0"))),
    }
    _JUDGE_CACHE = judge
    return judge


def load_rubric(rubric_id: str) -> dict:
    """Load a copied judge JSON (rubric text + rubric_sha256 + axes)."""
    return json.loads((RUBRICS_DIR / f"{rubric_id}.json").read_text(encoding="utf-8"))


def load_rubric_system(system_file: str) -> str:
    return (RUBRICS_DIR / system_file).read_text(encoding="utf-8")


_FENCE_RE = re.compile(r"^```(?:json)?\s*\n?", re.IGNORECASE)


def extract_json(text: str) -> dict:
    """Tolerant parse of a judge reply: strip ``` fences / prose, grab the first
    balanced JSON object. Mirrors judge_bundle.read_result's leniency."""
    t = text.strip()
    m = _FENCE_RE.match(t)
    if m:
        t = t[m.end() :]
        if t.endswith("```"):
            t = t[:-3]
        t = t.strip()
    try:
        return json.loads(t)
    except (ValueError, json.JSONDecodeError):
        pass
    start = t.find("{")
    if start == -1:
        raise ValueError("no JSON object in judge reply")
    depth = 0
    for i in range(start, len(t)):
        if t[i] == "{":
            depth += 1
        elif t[i] == "}":
            depth -= 1
            if depth == 0:
                return json.loads(t[start : i + 1])
    raise ValueError("unbalanced JSON in judge reply")


def judge_item(slice_name: str, prompt_id: str, prompt: str, response: str, rec: dict) -> dict:
    """Score ONE candidate response with the configurable API judge.

    Builds the single-item bundle the rubric's system.md expects, calls the
    judge over HTTP, and returns the parsed per-item result:
    {axes:{...}, overall:float, flags:{...}}. On any failure returns a
    judge_failure marker (axes empty) so the caller can exclude it.
    """
    meta = SLICE_META[slice_name]
    rubric = load_rubric(meta["rubric"])
    rb_grade.verify_judge_rubric(rubric)  # rubric_sha256 integrity gate
    system = load_rubric_system(meta["system"])

    # Judge scores the dialogue the player sees (strip runtime envelope) for the
    # spoken slices; sim/gaeilge judge the raw structured/Irish output.
    judged_response = response
    if slice_name in ("dialogue", "reaction"):
        judged_response = rb_grade.extract_dialogue_for_judging(response)

    item: dict = {"prompt_id": prompt_id, "prompt": prompt, "response": judged_response}
    if slice_name == "reaction":
        item["persona"] = rec.get("persona", "")
    if slice_name == "multiturn":
        # The whole transcript is the response; give the judge the persona +
        # the player's stated name so it can score name fidelity / memory.
        item["persona"] = rec.get("persona", "")
        item["player_name"] = rec.get("player_name", "")
        item["turns"] = rec.get("turns", [])
    if slice_name == "gaeilge":
        for k in ("task_type", "constraints", "expected_features", "reference_irish"):
            if k in rec:
                item[k] = rec[k]

    bundle = {
        "slice": slice_name,
        "rubric_sha256": rubric["rubric_sha256"],
        "score_range": rubric.get("score_range", [1, 5]),
        "items": [item],
    }

    judge = load_judge_config()
    target = Target(
        model=judge["model"], base_url=judge["base_url"], api_key_env=judge["api_key_env"]
    )
    try:
        text, _usage = call_chat(
            target, system, json.dumps(bundle, ensure_ascii=False), temperature=judge["temperature"]
        )
        parsed = extract_json(text)
        # Reject a stale / mismatched judge envelope wholesale — a score keyed
        # to a different rubric or prompt cannot be trusted against this record
        # (mirrors v1 judge_bundle.validate_result).
        echoed_sha = parsed.get("rubric_sha256")
        if echoed_sha is not None and echoed_sha != rubric["rubric_sha256"]:
            raise ValueError(
                f"judge rubric_sha256 mismatch: bundle={rubric['rubric_sha256'][:12]} "
                f"echo={str(echoed_sha)[:12]}"
            )
        items = parsed.get("items") or []
        if not items:
            raise ValueError("judge returned no items")
        scored = items[0]
        echoed_pid = scored.get("prompt_id")
        if echoed_pid is not None and str(echoed_pid) != str(prompt_id):
            raise ValueError(f"judge prompt_id mismatch: sent={prompt_id} echo={echoed_pid}")
        axes = scored.get("axes") or {}
        flags = scored.get("flags") or {}
        overall = scored.get("overall")
        return {
            "axes": {k: int(axes.get(k, 0) or 0) for k in meta["axes"]},
            "overall": float(overall) if overall is not None else 0.0,
            "flags": {
                "bench_bug": bool(flags.get("bench_bug", False)),
                "non_latin_detected": bool(flags.get("non_latin_detected", False)),
                "refused": bool(flags.get("refused", False)),
                "degenerate_loop": bool(flags.get("degenerate_loop", False)),
                "fabricated": bool(flags.get("fabricated", False)),
            },
            "judge_model": judge["model"],
        }
    except Exception as e:  # noqa: BLE001
        return {
            "axes": {},
            "overall": 0.0,
            "flags": {},
            "judge_failure": str(e),
            "judge_model": judge["model"],
        }
