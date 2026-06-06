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

_fails: list[str] = []


def check(name, cond, detail=""):
    status = "ok  " if cond else "FAIL"
    if not cond:
        _fails.append(name)
    print(f"  [{status}] {name}{(' — ' + detail) if detail else ''}")


# --- dataset loader ---------------------------------------------------------
import load_dataset  # noqa: E402

for slice_name, expect_keys in [
    ("dialogue", {"system", "user"}),
    ("intent", {"gold", "system", "user"}),
    ("reaction", {"system", "user", "persona"}),
    ("tier2-sim", {"user", "grade_schema"}),
    ("gaeilge", {"task_type"}),
    ("multiturn", {"system", "turns"}),
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
captured: dict = {}


def fake_call_chat(
    target,
    system,
    user,
    *,
    schema=None,
    max_tokens=None,
    temperature=0.7,
    messages=None,
    response_format=None,
    frequency_penalty=None,
    **kw,
):
    captured.update(
        dict(
            target=target,
            system=system,
            user=user,
            schema=schema,
            max_tokens=max_tokens,
            temperature=temperature,
            messages=messages,
            response_format=response_format,
            frequency_penalty=frequency_penalty,
        )
    )
    if (
        response_format
        and response_format.get("type") == "json_object"
        and "parser" in (system or "")
    ):
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

# REQ 2: dialogue is sent VERBATIM from the captured record (system carries the
# real runtime blocks; json_object; freq_pen 0.5 — no reconstruction).
dlg_rec = {
    "id": "d1",
    "system": "You are Brigid. STAY IN YOUR LANE.\n\nPEOPLE YOU KNOW:\n- Sean",
    "user": 'Sean says: "I can\'t sleep."',
    "response_format": {"type": "json_object"},
    "max_tokens": None,
    "temperature": 0.7,
    "frequency_penalty": 0.5,
}
g = rb.generate_candidate("dialogue", target, dlg_rec)
check(
    "dialogue sent verbatim (system + json_object + freq_pen)",
    captured["system"] == dlg_rec["system"]
    and captured["response_format"] == {"type": "json_object"}
    and captured["frequency_penalty"] == 0.5
    and captured["max_tokens"] is None,
)
check("dialogue cost computed", g["cost"] == 0.0 and g["completion_tokens"] == 25)

intent_rec = {
    "id": "i1",
    "system": "You are a text adventure input parser.",
    "user": "go to the pub",
    "response_format": {"type": "json_object"},
    "max_tokens": 100,
    "temperature": 0.7,
    "gold": {},
}
g = rb.generate_candidate("intent", target, intent_rec)
check(
    "intent sent runtime-faithful (json_object, not strict schema)",
    captured["response_format"] == {"type": "json_object"}
    and captured["schema"] is None
    and captured["max_tokens"] == 100,
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


def fake_sim(
    target,
    system,
    user,
    *,
    schema=None,
    max_tokens=None,
    temperature=0.7,
    messages=None,
    response_format=None,
    frequency_penalty=None,
    **kw,
):
    return json.dumps({"a": 1}), {"prompt_tokens": 30, "completion_tokens": 5}


rb.call_chat = fake_sim
g = rb.generate_candidate(
    "tier2-sim",
    target,
    {"id": "t1", "user": "simulate...", "grade_schema": sim_schema, "response_format": None},
)
check("tier2-sim schema_valid computed deterministically", g["schema_valid"] is True)

# REQ 3: multiturn chains the candidate's own replies as assistant turns.
mt_calls = []


def fake_mt(
    target,
    system,
    user,
    *,
    schema=None,
    max_tokens=None,
    temperature=0.7,
    messages=None,
    response_format=None,
    frequency_penalty=None,
    **kw,
):
    mt_calls.append(list(messages) if messages else None)
    return json.dumps({"dialogue": f"reply {len(mt_calls)}"}), {
        "prompt_tokens": 10,
        "completion_tokens": 5,
    }


rb.call_chat = fake_mt
g = rb.generate_candidate(
    "multiturn",
    target,
    {
        "id": "m1",
        "system": "You are Brigid.",
        "turns": ["hello", "your name?", "bye"],
        "player_name": "Sean",
        "temperature": 0.7,
    },
)
check(
    "multiturn chains 3 turns with growing message history",
    len(mt_calls) == 3 and len(mt_calls[-1]) == 6 and "reply 1" in g["output"],
)
rb.call_chat = fake_call_chat


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
judge_seen: dict = {}


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


# --- review-fix regressions -------------------------------------------------
# errored rows stay in the intent denominator (transport failure can't inflate)
iagg_err = report.aggregate_intent(
    [
        mkres({"label_match": 1.0, "intent_score": 1.0}),
        mkres({}, meta={"error": "boom"}),
        mkres({}, meta={"error": "boom"}),
    ]
)
check("intent errors counted in denominator", abs(iagg_err["label_match_rate"] - (1 / 3)) < 1e-6)

# errored sim rows stay in the schema_valid_rate denominator
sagg = report.aggregate_quality(
    "tier2-sim",
    [
        mkres({"plausibility": 4, "overall": 4.0, "schema_valid": 1.0}),
        mkres({}, meta={"error": "timeout"}),
    ],
)
check("sim error in schema denominator", abs(sagg["schema_valid_rate"] - 0.5) < 1e-6)

# perf warmup rows are discarded
pagg_w = report.aggregate_perf(
    [
        {
            "vars": {"perf_warmup": True},
            "latencyMs": 9999.0,
            "response": {"metadata": {"model": "m", "ttft_ms": 1, "tokens_per_second": 1.0}},
        },
        mkres({}, meta={"model": "m", "ttft_ms": 100, "tokens_per_second": 50.0}, latency=200.0),
    ]
)
check("perf drops warmup row", pagg_w["latency_p50_ms"] == 200.0 and pagg_w["n_ok"] == 1)

# judge rejects a mismatched rubric_sha256 envelope


def fake_judge_badsha(target, system, user, *, schema=None, temperature=0.0, **kw):
    bundle = json.loads(user)
    return json.dumps(
        {
            "rubric_sha256": "deadbeef",
            "items": [
                {
                    "prompt_id": bundle["items"][0]["prompt_id"],
                    "axes": {a: 5 for a in rb.SLICE_META[bundle["slice"]]["axes"]},
                    "overall": 5.0,
                }
            ],
        }
    ), {"prompt_tokens": 1, "completion_tokens": 1}


rb.call_chat = fake_judge_badsha
jr_bad = rb.judge_item("dialogue", "d1", "p", "a reply", {"id": "d1"})
check("judge rejects mismatched rubric_sha256", "judge_failure" in jr_bad)

# empty candidate output short-circuits to bench_bug without a judge call
rb.call_chat = fake_judge_call
os.environ["RB_SLICE"] = "dialogue"
ar_empty = rubric_judge.get_assert(
    "   ", {"vars": {"record": json.dumps({"id": "d1", "prompt": "x"})}}
)
check("empty output → bench_bug", ar_empty["namedScores"].get("bench_bug") == 1.0)

# YAML parser strips inline comments + quotes
import tempfile  # noqa: E402

with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as tf:
    tf.write('model: "x-model"  # a comment\nbase_url: http://h/v1\ntemperature: 0.0\n')
    tf_path = tf.name
_saved_cfg, _saved_cache = rb.CONFIG_DIR, rb._JUDGE_CACHE
rb.CONFIG_DIR = Path(tf_path).parent
rb._JUDGE_CACHE = None
# point load_judge_config at our temp file by name
import shutil  # noqa: E402

shutil.copy(tf_path, rb.CONFIG_DIR / "judge.yaml")
for k in ("RB_JUDGE_MODEL", "RB_JUDGE_BASE_URL", "RB_JUDGE_API_KEY_ENV", "RB_JUDGE_TEMPERATURE"):
    os.environ.pop(k, None)
jcfg = rb.load_judge_config()
check("YAML parser strips comment+quotes", jcfg["model"] == "x-model")
rb.CONFIG_DIR, rb._JUDGE_CACHE = _saved_cfg, _saved_cache


# --- REQ 1: enumeration viability filter + family de-dup + tiering -----------
import enumerate_candidates as enum  # noqa: E402

_ok_rec = {
    "model_id": "vendor/chatty-12b",
    "output_modalities": ["text"],
    "input_modalities": ["text"],
    "modality": "text->text",
    "max_context": 131072,
    "json_schema": True,
    "supported_params": ["response_format"],
    "price_in": 0.2,
    "price_out": 0.5,
}
check("enum: viable chat model passes", enum.viability(_ok_rec, 8192)[0])
check(
    "enum: embeddings rejected",
    enum.viability({**_ok_rec, "model_id": "vendor/text-embedding-3"}, 8192)
    == (False, "non-chat-modality"),
)
check(
    "enum: short context rejected",
    enum.viability({**_ok_rec, "max_context": 4096}, 8192)[1] == "context<8192",
)
check(
    "enum: price-sentinel rejected",
    enum.viability({**_ok_rec, "price_in": -1, "price_out": -1}, 8192) == (False, "price-sentinel"),
)
check(
    "enum: meta-router rejected",
    enum.viability({**_ok_rec, "model_id": "openrouter/auto"}, 8192) == (False, "meta-router"),
)
check(
    "enum: family collapses preview+date variants",
    (
        enum.family_key({"model_id": "google/gemini-9.9-pro-preview-05-06"})
        == enum.family_key({"model_id": "google/gemini-9.9-pro"})
    ),
)
check(
    "enum: distinct sizes stay separate",
    enum.family_key({"model_id": "x/gemma-3-4b-it"})
    != enum.family_key({"model_id": "x/gemma-3-12b-it"}),
)
check(
    "enum: free tier for :free",
    enum.cost_tier({"model_id": "x/y:free", "price_in": 0, "price_out": 0}) == "free",
)
check(
    "enum: premium tier for pricey",
    enum.cost_tier({"model_id": "x/opus", "price_in": 15, "price_out": 75}) == "premium",
)

# --- REQ 4: leaderboard CI + category weighting + overall --------------------
import leaderboard as lb  # noqa: E402

mean, lo, hi = lb._bootstrap_ci([3, 3, 3, 3], iters=200)
check("lb: zero-variance CI collapses to the mean", mean == 3.0 and lo == 3.0 and hi == 3.0)
mean2, lo2, hi2 = lb._bootstrap_ci([1, 2, 3, 4, 5], iters=500)
check("lb: CI brackets the mean", lo2 <= mean2 <= hi2 and lo2 < hi2)
check("lb: category weights sum to 1", abs(sum(lb.CATEGORY_WEIGHTS.values()) - 1.0) < 1e-9)
check(
    "lb: simulation outweighs reaction (gameplay token volume)",
    lb.CATEGORY_WEIGHTS["simulation"] > lb.CATEGORY_WEIGHTS["reaction"],
)
_cats = {
    "dialogue": {"mean": 4.0, "lo": 3.8, "hi": 4.2, "n": 10},
    "simulation": {"mean": 2.0, "lo": 1.8, "hi": 2.2, "n": 10},
}
om, ol, oh = lb._overall(_cats)
check("lb: overall is weight-renormalised over present categories", 2.0 < om < 4.0 and ol < om < oh)
_collapsed = lb._category_scores(
    {
        "dialogue": {"mean": 4.0, "lo": 4.0, "hi": 4.0, "n": 4},
        "multiturn": {"mean": 2.0, "lo": 2.0, "hi": 2.0, "n": 4},
        "tier2-sim": {"mean": 3.0, "lo": 3.0, "hi": 3.0, "n": 2},
        "tier3-sim": {"mean": 5.0, "lo": 5.0, "hi": 5.0, "n": 2},
    }
)
check(
    "lb: dialogue category folds in multiturn (n-weighted mean = 3.0)",
    abs(_collapsed["dialogue"]["mean"] - 3.0) < 1e-9,
)
check(
    "lb: simulation category folds tier2+tier3 (n-weighted = 4.0)",
    abs(_collapsed["simulation"]["mean"] - 4.0) < 1e-9,
)

# --- REQ 2: structural drift guard — datasets carry verbatim runtime fields --
for slice_name, required in [
    ("dialogue", {"system", "user", "response_format"}),
    ("reaction", {"system", "user", "persona"}),
    ("tier2-sim", {"user", "grade_schema"}),
    ("intent", {"system", "user", "gold"}),
    ("multiturn", {"system", "turns"}),
]:
    recs = [
        json.loads(ln)
        for ln in (rb.DATASETS_DIR / f"{slice_name}.jsonl").read_text().splitlines()
        if ln.strip()
    ]
    check(
        f"drift-guard: {slice_name} records carry {required}",
        all(required <= set(r) for r in recs),
        f"{len(recs)} records",
    )
# dialogue must carry the real runtime blocks (not the old simplified template)
_dlg0 = json.loads((rb.DATASETS_DIR / "dialogue.jsonl").read_text().splitlines()[0])
check(
    "drift-guard: dialogue system has PEOPLE YOU KNOW + WHAT'S ON YOUR MIND",
    "PEOPLE YOU KNOW" in _dlg0["system"] and "WHAT'S ON YOUR MIND" in _dlg0["system"],
)
check(
    "drift-guard: dialogue uses json_object (runtime) not strict schema",
    _dlg0.get("response_format") == {"type": "json_object"},
)


# --- REQ 6: funnel resume — run-state checkpoint helpers --------------------
import funnel as fn  # noqa: E402

_clean = [{"response": {"metadata": {}}, "namedScores": {"overall": 4.0}}]
_errored = [{"response": {"metadata": {"error": "HTTP Error 402"}}, "namedScores": {}}]
_jf = [{"response": {"metadata": {}}, "namedScores": {"judge_failure": 1.0}}]
check("funnel: clean slice is complete", fn.slice_clean(_clean))
check("funnel: errored slice not complete (retried on resume)", not fn.slice_clean(_errored))
check("funnel: judge-failed slice not complete (retried after top-up)", not fn.slice_clean(_jf))

_tmp_state = Path(tempfile.mkdtemp()) / "funnel_state.json"
_saved_state_path = fn.RUN_STATE
fn.RUN_STATE = _tmp_state
try:
    k1 = {
        "phase": "screen",
        "tier": "budget",
        "limit": 4,
        "judge_model": "j",
        "merkle": "m",
        "slices": ["dialogue"],
    }
    fn.save_run_state({"key": k1, "completed": {"spec\x00dialogue": _clean}})
    st = fn.load_run_state(k1, fresh=False)
    check("funnel: matching key resumes completed work", "spec\x00dialogue" in st["completed"])
    check(
        "funnel: --fresh ignores checkpoint", fn.load_run_state(k1, fresh=True)["completed"] == {}
    )
    k2 = {**k1, "limit": 8}  # changed key → not comparable → fresh
    check(
        "funnel: changed run key starts fresh",
        fn.load_run_state(k2, fresh=False)["completed"] == {},
    )
finally:
    fn.RUN_STATE = _saved_state_path


print()
if _fails:
    print(f"FAILED ({len(_fails)}): {_fails}")
    raise SystemExit(1)
print("all v2 offline checks passed")
