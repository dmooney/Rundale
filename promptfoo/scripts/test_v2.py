"""Offline unit checks for rundale-bench v2 — no network.

Monkeypatches eval_lib.call_chat / call_chat_streaming so every Python seam
(dataset loader, candidate request shapes, deterministic asserts, judge bundle
assembly + envelope parse, report aggregation, game-time cost) is exercised
without an API key. Run: python3 promptfoo/scripts/test_v2.py
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

PF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PF))
sys.path.insert(0, str(PF / "assertions"))
sys.path.insert(0, str(PF / "scripts"))

import rb_common as rb  # noqa: E402

_fails = []


def check(name, cond, detail=""):
    status = "ok  " if cond else "FAIL"
    if not cond:
        _fails.append(name)
    print(f"  [{status}] {name}{(' — ' + detail) if detail else ''}")


# --- dataset loader ---------------------------------------------------------
import load_dataset  # noqa: E402

for slice_name, expect_keys in [
    ("dialogue", {"prompt"}),
    ("intent", {"gold"}),
    ("reaction", {"system_template"}),
    ("tier2-sim", {"schema"}),
    ("gaeilge", {"task_type"}),
]:
    os.environ["RB_SLICE"] = slice_name
    os.environ.pop("RB_LIMIT", None)
    tests = load_dataset.generate_tests()
    rec0 = json.loads(tests[0]["vars"]["record"])
    check(
        f"loader/{slice_name} nonempty + keys",
        len(tests) > 0 and expect_keys <= set(rec0),
        f"{len(tests)} records",
    )

os.environ["RB_SLICE"] = "dialogue"
os.environ["RB_LIMIT"] = "3"
check("loader respects RB_LIMIT", len(load_dataset.generate_tests()) == 3)
os.environ.pop("RB_LIMIT", None)
check("perf loader uses measure ids", len(load_dataset.generate_perf_tests()) >= 1)


# --- candidate request shapes (capture system/user/schema) ------------------
captured = {}


def fake_call_chat(target, system, user, *, schema=None, max_tokens=None, temperature=0.7, **kw):
    captured.update(
        dict(
            target=target,
            system=system,
            user=user,
            schema=schema,
            max_tokens=max_tokens,
            temperature=temperature,
        )
    )
    if schema is not None:
        return json.dumps({"intent": "move", "target": "the pub", "dialogue": None}), {
            "prompt_tokens": 50,
            "completion_tokens": 10,
        }
    return "Ah now, sleep poorly do ye? Chamomile and a quiet mind, a leanbh.", {
        "prompt_tokens": 120,
        "completion_tokens": 25,
    }


rb.call_chat = fake_call_chat

target = rb.parse_target("test-model@http://localhost:9/v1")

g = rb.generate_candidate("dialogue", target, {"id": "d1", "prompt": "I can't sleep."})
check(
    "dialogue uses DIALOGUE_SYS + max_tokens=200",
    captured["system"] == rb.DIALOGUE_SYS and captured["max_tokens"] == 200,
)
check("dialogue cost computed", g["cost"] == 0.0 and g["completion_tokens"] == 25)

g = rb.generate_candidate("intent", target, {"id": "i1", "prompt": "go to the pub"})
check(
    "intent passes INTENT_SCHEMA",
    captured["schema"] is rb.INTENT_SCHEMA and captured["max_tokens"] == 100,
)

sim_schema = {
    "name": "s",
    "schema": {
        "type": "object",
        "properties": {"a": {"type": "integer"}},
        "required": ["a"],
        "additionalProperties": False,
    },
}


def fake_sim(target, system, user, *, schema=None, max_tokens=None, temperature=0.7, **kw):
    return json.dumps({"a": 1}), {"prompt_tokens": 30, "completion_tokens": 5}


rb.call_chat = fake_sim
g = rb.generate_candidate("tier2-sim", target, {"id": "t1", "prompt": "x", "schema": sim_schema})
check("tier2-sim schema_valid computed deterministically", g["schema_valid"] is True)


# --- deterministic asserts --------------------------------------------------
import intent_assert  # noqa: E402
import schema_assert  # noqa: E402

ctx = {
    "vars": {
        "record": json.dumps(
            {"id": "i1", "gold": {"intent": "move", "target": "the pub", "dialogue": None}}
        )
    }
}
r_ok = intent_assert.get_assert(
    json.dumps({"intent": "move", "target": "the pub", "dialogue": None}), ctx
)
r_bad = intent_assert.get_assert(
    json.dumps({"intent": "talk", "target": None, "dialogue": "hi"}), ctx
)
check("intent_assert pass on match", r_ok["pass"] and r_ok["score"] > 0.9)
check("intent_assert fail on label miss", (not r_bad["pass"]) and r_bad["score"] == 0.0)

sctx = {"vars": {"record": json.dumps({"id": "t1", "schema": sim_schema})}}
check("schema_assert valid", schema_assert.get_assert(json.dumps({"a": 1}), sctx)["pass"])
check("schema_assert invalid", not schema_assert.get_assert(json.dumps({"b": 2}), sctx)["pass"])


# --- judge bundle assembly + envelope parse ---------------------------------
judge_seen = {}


def fake_judge_call(target, system, user, *, schema=None, temperature=0.0, **kw):
    judge_seen.update(dict(system=system, user=user, model=target.model))
    bundle = json.loads(user)
    pid = bundle["items"][0]["prompt_id"]
    return json.dumps(
        {
            "version": 1,
            "slice": bundle["slice"],
            "rubric_sha256": bundle["rubric_sha256"],
            "items": [
                {
                    "prompt_id": pid,
                    "axes": {a: 4 for a in rb.SLICE_META[bundle["slice"]]["axes"]},
                    "overall": 4.0,
                    "flags": {"bench_bug": False, "non_latin_detected": False, "refused": False},
                }
            ],
        }
    ), {"prompt_tokens": 200, "completion_tokens": 40}


rb.call_chat = fake_judge_call

jr = rb.judge_item("dialogue", "d1", "I can't sleep.", "Ah now, chamomile.", {"id": "d1"})
check("judge_item dialogue axes parsed", jr["axes"].get("character") == 4 and jr["overall"] == 4.0)
check("judge bundle carries rubric sha", '"rubric_sha256"' in judge_seen["user"])

jr_r = rb.judge_item(
    "reaction", "r1", "greet", "Welcome stranger.", {"id": "r1", "persona": "publican"}
)
check(
    "judge reaction includes persona",
    '"persona"' in judge_seen["user"] and jr_r["axes"]["in_character"] == 4,
)


def fake_judge_benchbug(target, system, user, *, schema=None, temperature=0.0, **kw):
    bundle = json.loads(user)
    pid = bundle["items"][0]["prompt_id"]
    return json.dumps(
        {
            "items": [
                {
                    "prompt_id": pid,
                    "axes": {a: 0 for a in rb.SLICE_META[bundle["slice"]]["axes"]},
                    "overall": 0.0,
                    "flags": {"bench_bug": True},
                }
            ]
        }
    ), {"prompt_tokens": 1, "completion_tokens": 1}


rb.call_chat = fake_judge_benchbug
jr_bb = rb.judge_item("gaeilge", "g1", "x", "The user wants me to...", {"id": "g1"})
check("judge bench_bug flagged", jr_bb["flags"]["bench_bug"] is True)


# --- rubric_judge assertion (uses judge_item) -------------------------------
import rubric_judge  # noqa: E402

rb.call_chat = fake_judge_call
os.environ["RB_SLICE"] = "dialogue"
ar = rubric_judge.get_assert(
    "Ah now, chamomile.", {"vars": {"record": json.dumps({"id": "d1", "prompt": "x"})}}
)
check(
    "rubric_judge namedScores carry axes", ar["namedScores"].get("character") == 4.0 and ar["pass"]
)


# --- report aggregation + game-time cost ------------------------------------
import report  # noqa: E402


def mkres(named, meta=None, latency=100.0, cost=0.0, usage=None, error=None):
    return {
        "namedScores": named,
        "latencyMs": latency,
        "error": error,
        "response": {
            "metadata": meta or {},
            "cost": cost,
            "tokenUsage": usage or {"prompt": 100, "completion": 20},
        },
        "provider": {"label": "candidate"},
    }


dia = [
    mkres(
        {
            "character": 4,
            "authenticity": 3,
            "language": 5,
            "responsiveness": 4,
            "craft": 3,
            "overall": 3.8,
        }
    ),
    mkres({"bench_bug": 1.0}),
    mkres({"judge_failure": 1.0}),
]
agg = report.aggregate_quality("dialogue", dia)
check(
    "report excludes bench_bug + judge_failure",
    agg["judged"] == 1 and agg["bench_bugs"] == 1 and agg["judge_failures"] == 1,
    json.dumps(agg),
)
check("report overall mean", abs(agg["overall"] - 3.8) < 1e-6)

iagg = report.aggregate_intent(
    [
        mkres({"label_match": 1.0, "intent_score": 1.0}),
        mkres({"label_match": 0.0, "intent_score": 0.0}),
    ]
)
check("intent label_match_rate", abs(iagg["label_match_rate"] - 0.5) < 1e-6)

perf_rows = [
    mkres(
        {},
        meta={"model": "claude-haiku-4-5", "ttft_ms": 120, "tokens_per_second": 55.0},
        latency=200.0,
        usage={"prompt": 1000, "completion": 200},
    )
    for _ in range(4)
]
pagg = report.aggregate_perf(perf_rows)
gt = rb.__dict__  # noqa
import pricing  # noqa: E402

pagg.update(pricing.gameplay_cost(*pricing.COSTS["claude-haiku-4-5"]))
check("perf p50 latency", pagg["latency_p50_ms"] == 200.0)
check("perf tok/s mean", abs(pagg["tokens_per_sec_mean"] - 55.0) < 1e-6)
check(
    "game-time cost > 0 for paid model",
    pagg["gameplay_cost_usd_per_minute"] > 0
    and pagg["gameplay_cost_usd_per_hour"] > pagg["gameplay_cost_usd_per_minute"],
)
check(
    "game-time cost == 0 for local",
    pricing.gameplay_cost(0.0, 0.0)["gameplay_cost_usd_per_minute"] == 0.0,
)


# --- tolerant judge JSON parse ----------------------------------------------
check("extract_json strips fences", rb.extract_json('```json\n{"items":[1]}\n```')["items"] == [1])
check("extract_json finds embedded object", rb.extract_json('noise {"a":1} tail')["a"] == 1)


print()
if _fails:
    print(f"FAILED ({len(_fails)}): {_fails}")
    raise SystemExit(1)
print("all v2 offline checks passed")
