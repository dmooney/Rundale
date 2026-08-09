"""Offline unit checks for rundale-bench v2 — no network.

Monkeypatches eval_lib.call_chat / call_chat_streaming so every Python seam
(dataset loader, candidate request shapes, deterministic asserts, judge bundle
assembly + envelope parse, report aggregation, game-time cost) is exercised
without an API key. Run: python3 promptfoo/scripts/test_v2.py
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
import urllib.request
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


_deepseek_body: dict = {}
rb.eval_lib._apply_reasoning_request(
    _deepseek_body,
    rb.Target("deepseek-v4-flash", "https://api.deepseek.com/v1"),
    reasoning={"effort": "medium"},
    enable_thinking=True,
)
check(
    "direct DeepSeek uses native high reasoning fields",
    _deepseek_body
    == {"thinking": {"type": "enabled"}, "reasoning_effort": "high"},
)
_deepseek_max_body: dict = {}
rb.eval_lib._apply_reasoning_request(
    _deepseek_max_body,
    rb.Target("deepseek-v4-flash", "https://api.deepseek.com/v1"),
    reasoning={"effort": "max"},
    enable_thinking=True,
)
check(
    "direct DeepSeek preserves native max reasoning",
    _deepseek_max_body
    == {"thinking": {"type": "enabled"}, "reasoning_effort": "max"},
)
_google_body: dict = {"temperature": 0.7, "frequency_penalty": 0.5}
_google_target = rb.Target(
    "gemini-3.6-flash",
    "https://generativelanguage.googleapis.com/v1beta/openai",
)
rb.eval_lib._apply_reasoning_request(
    _google_body,
    _google_target,
    reasoning={"effort": "max"},
    enable_thinking=True,
)
if rb.eval_lib._uses_latest_gemini_sampling_contract(_google_target):
    _google_body.pop("temperature", None)
    _google_body.pop("frequency_penalty", None)
check(
    "direct latest Gemini uses native capped effort and no deprecated sampling",
    _google_body == {"reasoning_effort": "high"},
)


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
for _perf_split in ("dev", "holdout"):
    os.environ["RB_SPLIT"] = _perf_split
    _perf_tests = load_dataset.generate_perf_tests()
    check(
        f"perf loader/{_perf_split} has one warmup + fixed panel",
        len(_perf_tests) == 19
        and _perf_tests[0]["vars"]["perf_warmup"] is True
        and all(
            row["vars"]["perf_warmup"] is False for row in _perf_tests[1:]
        )
        and sum(
            row["vars"]["perf_cache_state"] == "cold"
            for row in _perf_tests[1:]
        )
        == 6
        and sum(
            row["vars"]["perf_cache_state"] == "warm"
            for row in _perf_tests[1:]
        )
        == 12,
    )
    _perf_ids = [row["vars"]["rb_id"] for row in _perf_tests]
    check(
        f"perf loader/{_perf_split} keeps warmup out of measured panel",
        _perf_ids[0] not in _perf_ids[1:],
    )
    _cold_perf = [
        json.loads(row["vars"]["record"])
        for row in _perf_tests
        if row["vars"]["perf_cache_state"] == "cold"
    ]
    check(
        f"perf loader/{_perf_split} cold panel uses distinct system prefixes",
        len({row["system"] for row in _cold_perf}) == len(_cold_perf),
    )
os.environ["RB_SPLIT"] = "dev"
os.environ["RB_LIMIT"] = "5"
_limited_perf = load_dataset.generate_perf_tests()
check(
    "perf loader limit means warmup + N measured",
    len(_limited_perf) == 6 and _limited_perf[0]["vars"]["perf_warmup"] is True,
)
os.environ.pop("RB_LIMIT", None)

_drift_root = Path(tempfile.mkdtemp())
(_drift_root / "datasets").mkdir()
(_drift_root / "datasets" / "dialogue.jsonl").write_text(
    '{"id":"changed"}\n',
    encoding="utf-8",
)
(_drift_root / "MANIFEST.json").write_text(
    json.dumps(
        {
            "slices": {
                "dialogue.jsonl": {
                    "sha256": "not-the-content-hash",
                    "records": 1,
                }
            }
        }
    ),
    encoding="utf-8",
)
_saved_v2, _saved_datasets = rb.V2_DIR, rb.DATASETS_DIR
rb.V2_DIR, rb.DATASETS_DIR = _drift_root, _drift_root / "datasets"
try:
    load_dataset._load_records("dialogue", "dev")
except RuntimeError:
    _dataset_drift_rejected = True
else:
    _dataset_drift_rejected = False
finally:
    rb.V2_DIR, rb.DATASETS_DIR = _saved_v2, _saved_datasets
check("loader rejects unpinned dataset drift", _dataset_drift_rejected)


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
    enable_thinking=None,
    reasoning=None,
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
            enable_thinking=enable_thinking,
            reasoning=reasoning,
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


def fake_call_chat_streaming(*args, **kwargs):
    return {
        "text": '{"dialogue":"Measured"}',
        "prompt_tokens": 100,
        "completion_tokens": 20,
        "ttft_ms": 50,
        "tokens_per_second": 40.0,
        "cost": 0.0123,
    }


rb.call_chat_streaming = fake_call_chat_streaming
_streamed = rb.generate_candidate("dialogue", target, dlg_rec, streaming=True)
check(
    "streaming candidate preserves provider-observed cost",
    _streamed["cost"] == 0.0123,
)

_old_temp_override = os.environ.get("RB_DIALOGUE_TEMPERATURE")
_old_max_override = os.environ.get("RB_DIALOGUE_MAX_TOKENS")
_old_thinking_override = os.environ.get("RB_DIALOGUE_ENABLE_THINKING")
_old_reasoning_override = os.environ.get("RB_DIALOGUE_REASONING_EFFORT")
try:
    os.environ["RB_DIALOGUE_TEMPERATURE"] = "0.3"
    os.environ["RB_DIALOGUE_MAX_TOKENS"] = "256"
    os.environ["RB_DIALOGUE_ENABLE_THINKING"] = "false"
    os.environ["RB_DIALOGUE_REASONING_EFFORT"] = "high"
    _effective = rb.effective_dialogue_record(dlg_rec)
    rb.generate_candidate("dialogue", target, dlg_rec)
    check(
        "dialogue A/B overrides preserve frozen prompts and stamp generation",
        _effective["system"] == dlg_rec["system"]
        and _effective["user"] == dlg_rec["user"]
        and _effective["temperature"] == 0.3
        and _effective["max_tokens"] == 256
        and _effective["enable_thinking"] is False
        and _effective["reasoning_effort"] == "high"
        and captured["reasoning"] == {"effort": "high"},
    )
finally:
    if _old_temp_override is None:
        os.environ.pop("RB_DIALOGUE_TEMPERATURE", None)
    else:
        os.environ["RB_DIALOGUE_TEMPERATURE"] = _old_temp_override
    if _old_max_override is None:
        os.environ.pop("RB_DIALOGUE_MAX_TOKENS", None)
    else:
        os.environ["RB_DIALOGUE_MAX_TOKENS"] = _old_max_override
    if _old_thinking_override is None:
        os.environ.pop("RB_DIALOGUE_ENABLE_THINKING", None)
    else:
        os.environ["RB_DIALOGUE_ENABLE_THINKING"] = _old_thinking_override
    if _old_reasoning_override is None:
        os.environ.pop("RB_DIALOGUE_REASONING_EFFORT", None)
    else:
        os.environ["RB_DIALOGUE_REASONING_EFFORT"] = _old_reasoning_override
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
mt_calls: list = []


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
rubric_judge.get_assert(
    "Aye.",
    {"vars": {"record": json.dumps({"id": "d2", "system": "PEOPLE YOU KNOW: Roisin", "user": "Do ye know Roisin?"})}},
)
_judge_prompt = json.loads(judge_seen["user"])["items"][0]["prompt"]
check(
    "dialogue judge receives production system and user context",
    _judge_prompt == "SYSTEM PROMPT:\nPEOPLE YOU KNOW: Roisin\n\nUSER PROMPT:\nDo ye know Roisin?",
)


# --- report aggregation + game-time cost ------------------------------------
import report  # noqa: E402


def mkres(named, meta=None, latency=100.0, cost=0.0, usage=None, error=None):
    return {
        "namedScores": named,
        "latencyMs": latency,
        "error": error,
        "response": {
            "output": "non-empty candidate output",
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
check("perf p95 ttft", pagg["ttft_p95_ms"] == 120.0)
check("perf p50 tok/s", pagg["tokens_per_sec_p50"] == 55.0)
check("perf tok/s mean", abs(pagg["tokens_per_sec_mean"] - 55.0) < 1e-6)
_observed_cost_perf = report.aggregate_perf(
    [
        mkres(
            {},
            meta={"model": "unknown-routed-model", "ttft_ms": 100, "tokens_per_second": 50.0},
            latency=200.0,
            cost=0.012,
            usage={"prompt": 1000, "completion": 200},
        )
    ]
)
check(
    "perf observed cost does not require a static model price",
    _observed_cost_perf["usd_per_mtok_observed"] == 10.0,
)
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

_empty_perf = mkres(
    {},
    meta={"model": "m", "ttft_ms": None, "tokens_per_second": None},
    latency=50_000.0,
)
_empty_perf["response"]["output"] = ""
_empty_perf_agg = report.aggregate_perf([_empty_perf])
check(
    "perf treats empty or incomplete streaming output as an error",
    _empty_perf_agg["n_ok"] == 0
    and _empty_perf_agg["n_error"] == 1
    and _empty_perf_agg["error_rate"] == 1.0,
)

_single_chunk_perf = mkres(
    {},
    meta={"model": "m", "ttft_ms": 50, "tokens_per_second": None},
    latency=50.0,
)
_single_chunk_perf["response"]["output"] = '{"dialogue":"Fast single chunk"}'
_single_chunk_perf_agg = report.aggregate_perf([_single_chunk_perf])
check(
    "perf accepts single-chunk completion with unmeasurable throughput",
    _single_chunk_perf_agg["n_ok"] == 1
    and _single_chunk_perf_agg["n_error"] == 0
    and _single_chunk_perf_agg["error_rate"] == 0.0
    and _single_chunk_perf_agg["ttft_p50_ms"] == 50.0
    and _single_chunk_perf_agg["tokens_per_sec_p50"] == 0.0,
)

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

# --- production local-dialogue promotion gate -------------------------------
import promotion_gate as pg  # noqa: E402
import check_local_dialogue_qualification as qualification  # noqa: E402

check(
    "qualification registry has receipts for every production claim",
    qualification.validate() == [],
    qualification.validate(),
)

_promotion_dir = Path(tempfile.mkdtemp())
_promotion_candidate = "local-test@http://127.0.0.1:8000/v1"
_promotion_request_profile = {
    "model": "local-test",
    "max_tokens": 768,
    "temperature": 0.7,
    "frequency_penalty": 0.5,
    "json_mode": True,
}
_holdout_meta = {
    "target": _promotion_candidate,
    "dataset_split": "holdout",
    "request_profile": _promotion_request_profile,
}
_dialogue_named = {
    **{axis: 4.0 for axis in rb.SLICE_META["dialogue"]["axes"]},
    "overall": 4.0,
}
_multiturn_named = {
    **{axis: 4.0 for axis in rb.SLICE_META["multiturn"]["axes"]},
    "overall": 4.0,
}
_promotion_rows = {
    "dialogue": [mkres(dict(_dialogue_named), meta=dict(_holdout_meta)) for _ in range(100)],
    "multiturn": [mkres(dict(_multiturn_named), meta=dict(_holdout_meta)) for _ in range(30)],
    "perf": [],
}
for _index in range(20):
    _cache_state = "cold" if _index < 6 else "warm"
    _promotion_rows["perf"].append(
        mkres(
            {},
            meta={
                **_holdout_meta,
                "model": "local-test",
                "ttft_ms": 200,
                "tokens_per_second": 30.0,
                "perf_cache_state": _cache_state,
            },
            latency=1500.0,
        )
    )
for _slice, _rows in _promotion_rows.items():
    (_promotion_dir / f"{_slice}.json").write_text(
        json.dumps({"results": {"results": _rows}}),
        encoding="utf-8",
    )
_promotion_config = json.loads((rb.CONFIG_DIR / "dialogue_promotion.json").read_text())
_promotion_profiles = json.loads((rb.CONFIG_DIR / "local_hardware_profiles.json").read_text())
_promotion_manifest = json.loads((rb.V2_DIR / "MANIFEST.json").read_text())
_promotion_evidence = {
    "version": 1,
    "candidate": _promotion_candidate,
    "hardware_profile_id": "apple-silicon-16gb",
    "dataset_merkle": _promotion_manifest["merkle_root_sha256"],
    "hardware": {
        "platform": "darwin-arm64",
        "memory_kind": "unified",
        "total_memory_gb": 16.0,
        "peak_memory_gb": 12.0,
    },
    "reliability_soak": {"calls": 500, "valid_responses": 500},
    "guard_observation": {"turns": 500, "interventions": 25},
    "request_profile": _promotion_request_profile,
}
_promotion = pg.evaluate(
    _promotion_dir,
    candidate=_promotion_candidate,
    evidence=_promotion_evidence,
    config=_promotion_config,
    profiles=_promotion_profiles,
    manifest_merkle=_promotion_manifest["merkle_root_sha256"],
)
check(
    "promotion: complete holdout/profile evidence passes",
    _promotion["passed"],
    json.dumps([c for c in _promotion["checks"] if not c["passed"]]),
)
check(
    "promotion: player-ready metric includes Wilson lower bound",
    _promotion["metrics"]["player_ready"]["wilson_lower_95"] >= 0.90,
)

_fabricated_rows = list(_promotion_rows["dialogue"])
_fabricated_rows[0] = mkres(
    {**_dialogue_named, "fabricated": 1.0},
    meta=dict(_holdout_meta),
)
(_promotion_dir / "dialogue.json").write_text(
    json.dumps({"results": {"results": _fabricated_rows}}),
    encoding="utf-8",
)
_promotion_bad = pg.evaluate(
    _promotion_dir,
    candidate=_promotion_candidate,
    evidence=_promotion_evidence,
    config=_promotion_config,
    profiles=_promotion_profiles,
    manifest_merkle=_promotion_manifest["merkle_root_sha256"],
)
check(
    "promotion: one fabrication hard-fails an otherwise strong profile",
    not _promotion_bad["passed"]
    and _promotion_bad["metrics"]["hard_failures"]["fabricated"] == 1,
)

try:
    pg.evaluate(
        _promotion_dir,
        candidate=_promotion_candidate,
        evidence={**_promotion_evidence, "dataset_merkle": "stale"},
        config=_promotion_config,
        profiles=_promotion_profiles,
        manifest_merkle=_promotion_manifest["merkle_root_sha256"],
    )
except pg.EvidenceError:
    _stale_rejected = True
else:
    _stale_rejected = False
check("promotion: stale dataset evidence is rejected", _stale_rejected)

# Promotion CLI provenance: summaries must be reproducible from immutable raw
# soak turns and the local runner's memory samples.
import build_profile_evidence as bpe  # noqa: E402

_evidence_root = Path(tempfile.mkdtemp())
_turns_path = _evidence_root / "soak.turns.jsonl"
_turn_rows = [
    {
        "version": 1,
        "candidate": _promotion_candidate,
        "dataset_merkle": _promotion_manifest["merkle_root_sha256"],
        "attempt": i,
        "contract_valid": 1,
        "turns": 1,
        "guard_interventions": int(i < 25),
        "parse_dispositions": ["full_json"],
        "request_profiles": [_promotion_request_profile],
    }
    for i in range(500)
]
_turns_path.write_text(
    "".join(json.dumps(row, sort_keys=True) + "\n" for row in _turn_rows),
    encoding="utf-8",
)
_turns_sha = hashlib.sha256(_turns_path.read_bytes()).hexdigest()
_soak_path = _evidence_root / "soak.json"
_soak_path.write_text(
    json.dumps(
        {
            "version": 1,
            "candidate": _promotion_candidate,
            "dataset_merkle": _promotion_manifest["merkle_root_sha256"],
            "reliability_soak": {"calls": 500, "valid_responses": 500},
            "guard_observation": {"turns": 500, "interventions": 25},
            "request_profile": _promotion_request_profile,
            "turns_artifact": {
                "path": _turns_path.name,
                "sha256": _turns_sha,
                "records": 500,
            },
        }
    ),
    encoding="utf-8",
)
_runner_path = _evidence_root / "local_runner.json"
_runner_path.write_text(
    json.dumps(
        {
            "host": {"platform": "darwin", "machine": "arm64", "memory_gb": 16.0},
            "rows": [{"hf_repo": "local-test", "peak_ram_gb": 12.0}],
        }
    ),
    encoding="utf-8",
)
_evidence_path = _evidence_root / "evidence.json"
_built_evidence = bpe.build(
    argparse.Namespace(
        candidate=_promotion_candidate,
        hardware_profile="apple-silicon-16gb",
        soak=_soak_path,
        local_runner_artifact=_runner_path,
        output=_evidence_path,
    )
)
_evidence_path.write_text(json.dumps(_built_evidence), encoding="utf-8")
try:
    pg._validate_provenance(_built_evidence, _evidence_path)
except pg.EvidenceError:
    _provenance_valid = False
else:
    _provenance_valid = True
check("promotion: measured artifact provenance validates", _provenance_valid)

_turns_path.write_text(_turns_path.read_text() + "{}\n", encoding="utf-8")
try:
    pg._validate_provenance(_built_evidence, _evidence_path)
except pg.EvidenceError:
    _tamper_rejected = True
else:
    _tamper_rejected = False
check("promotion: tampered soak artifact is rejected", _tamper_rejected)

# --- REQ 2: structural drift guard — datasets carry verbatim runtime fields --
for slice_name, required in [
    ("dialogue", {"system", "user", "response_format", "max_tokens"}),
    ("reaction", {"system", "user", "persona"}),
    ("tier2-sim", {"user", "grade_schema"}),
    ("intent", {"system", "user", "gold"}),
    ("multiturn", {"system", "turns", "max_tokens"}),
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

# Runtime-corpus builder: promotion holdouts must be deterministic, large
# enough, and use unseen multiturn scripts.
import build_runtime_datasets as brd  # noqa: E402

_split_records = [{"id": f"d-{i}", "system": f"s-{i}", "user": f"u-{i}"} for i in range(200)]
_split_main, _split_hold = brd._split(_split_records, holdout_frac=0.50)
_split_main_2, _split_hold_2 = brd._split(list(reversed(_split_records)), holdout_frac=0.50)
check(
    "corpus: dialogue 50/50 split is capture-order independent",
    len(_split_main) == 100
    and len(_split_hold) == 100
    and [r["id"] for r in _split_hold] == [r["id"] for r in _split_hold_2]
    and [r["id"] for r in _split_main] == [r["id"] for r in _split_main_2],
)
_persona_roles = [
    "Blacksmith",
    "Miller",
    "Midwife",
    "Teacher",
    "Publican",
    "Farmer",
    "Weaver",
    "Fisherman",
]
_persona_caps = [
    {
        "system": f"You are Person {i}, a 40-year-old {_persona_roles[i]} in rural Ireland.",
        "response_format": {"type": "json_object"},
        "max_tokens": 768,
        "temperature": 0.7,
        "frequency_penalty": 0.5,
    }
    for i in range(brd.MULTITURN_PERSONAS)
]
_mt_dev, _mt_hold = brd.build_multiturn(_persona_caps)
check(
    "corpus: multiturn has 24 dev + 30 holdout transcripts",
    len(_mt_dev) == 24 and len(_mt_hold) == 30,
)
check(
    "corpus: multiturn preserves the production token budget",
    all(r["max_tokens"] == 768 for r in _mt_dev + _mt_hold),
)
check(
    "corpus: multiturn holdout scripts and player names are unseen in dev",
    {tuple(r["turns"]) for r in _mt_dev}.isdisjoint({tuple(r["turns"]) for r in _mt_hold})
    and {r["player_name"] for r in _mt_dev}.isdisjoint(
        {r["player_name"] for r in _mt_hold}
    ),
)

_capture_order_a = [
    {"system": "system-z", "user": "user-z"},
    {"system": "system-a", "user": "user-a"},
    {"system": "system-z", "user": "user-z"},
]
_capture_order_b = list(reversed(_capture_order_a))
check(
    "corpus: deduplication is independent of concurrent capture order",
    brd._dedup(_capture_order_a) == brd._dedup(_capture_order_b),
)

# --- REQ 6: funnel resume — run-state checkpoint helpers --------------------
import funnel as fn  # noqa: E402
import soak_dialogue as soak  # noqa: E402

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

# --- production soak session continuity -------------------------------------
_secure_cookie = type("Cookie", (), {"secure": True})()
_loopback_policy = soak.LoopbackSecureCookiePolicy()
check(
    "soak: Secure session cookie is returned to HTTP loopback",
    _loopback_policy.return_ok_secure(
        _secure_cookie, urllib.request.Request("http://127.0.0.1:3040/api/health")
    ),
)
check(
    "soak: Secure session cookie remains blocked on external HTTP",
    not _loopback_policy.return_ok_secure(
        _secure_cookie, urllib.request.Request("http://example.com/api/health")
    ),
)
check(
    "soak: the fixed question cycle ends in a farewell",
    soak.DEFAULT_TURNS_PER_LOCATION == len(soak.QUESTIONS)
    and soak.QUESTIONS[-1].startswith("Thank you"),
)
check(
    "soak: closed conversations are reacquired outside the reliability denominator",
    not soak._reached_dialogue_inference({})
    and not soak._reached_dialogue_inference({"request_profiles": []})
    and soak._reached_dialogue_inference(
        {"request_profiles": [{"model": "candidate"}]}
    ),
)

# --- cloud-dialogue qualification dashboard -------------------------------
import qualification_dashboard as qdash  # noqa: E402

_qualification_feed = json.loads(
    (PF / "leaderboard" / "dialogue-qualification.json").read_text(encoding="utf-8")
)
_qualification_rebuilt = qdash.build(qdash.DEFAULT_RUNS)
check(
    "qualification dashboard is reproducible from immutable receipts",
    _qualification_feed == _qualification_rebuilt,
)
check(
    "qualification dashboard never scores deterministic rejects",
    all("quality" not in row and "score" not in row for row in _qualification_feed["runs"]),
)
check(
    "qualification dashboard exposes evidence hashes for every run",
    all(
        len(row["preflight"]["artifact"]["sha256"]) == 64
        for row in _qualification_feed["runs"]
    ),
)
check(
    "qualification dashboard exposes individual-call evidence for every run",
    all(
        row["calls"]["count"] >= row["preflight"]["calls"]
        and len(row["calls"]["sha256"]) == 64
        for row in _qualification_feed["runs"]
    ),
)
check(
    "qualification call feeds match their published content hashes",
    all(
        (lambda path, expected: path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == expected)(
            PF / "bench-site" / "public" / row["calls"]["path"],
            row["calls"]["sha256"],
        )
        for row in _qualification_feed["runs"]
    ),
)
_speed_ranked = [
    row for row in _qualification_feed["runs"]
    if row["status"] in {
        "needs_judgment", "needs_adjudication", "qualified", "quality_rejected"
    }
    and row.get("performance", {}).get("speed_rank") is not None
]
check(
    "cloud qualification ranks every deterministic survivor by speed",
    sorted(row["performance"]["speed_rank"] for row in _speed_ranked)
    == list(range(1, len(_speed_ranked) + 1))
    and all(
        row["performance"]["speed_cohort_size"] == len(_speed_ranked)
        and row["performance"]["speed_index_ms"] > 0
        for row in _speed_ranked
    ),
)
check(
    "cloud qualification never hard-rejects a profile for relative speed",
    all(
        not (
            row["stage"] == "performance"
            and any(signal in row["reason"] for signal in ("TTFT", "completion", "throughput"))
        )
        for row in _qualification_feed["runs"]
    )
    and all(
        key not in _qualification_feed["policy"]["performance"]
        for key in (
            "maximum_cold_ttft_p95_ms",
            "maximum_warm_ttft_p95_ms",
            "maximum_cold_completion_p95_ms",
            "maximum_warm_completion_p95_ms",
            "minimum_tokens_per_second_p50",
        )
    ),
)
_quality_ranked = [row for row in _qualification_feed["runs"] if row.get("judgment")]
_consensus_ranked = [
    row for row in _quality_ranked
    if row["judgment"].get("quality_rank") is not None
]
check(
    "cloud qualification publishes a contiguous consensus-only quality ranking",
    bool(_quality_ranked)
    and bool(_consensus_ranked)
    and sorted(row["judgment"]["quality_rank"] for row in _consensus_ranked)
    == list(range(1, len(_consensus_ranked) + 1))
    and all(
        row["judgment"]["complete"]
        and not row["judgment"]["needs_adjudication"]
        for row in _consensus_ranked
    ),
)
check(
    "cloud judgments use configured blinded multi-family judges",
    len(_qualification_feed["policy"]["judgment"]["judges"]) == 3
    and len({
        judge["family"]
        for judge in _qualification_feed["policy"]["judgment"]["judges"]
    }) == 3
    and _qualification_feed["policy"]["judgment"]["minimum_independent_judges"] == 2
    and _qualification_feed["policy"]["judgment"]["exclude_same_family"]
    and all(
        all(
            entry["eligible"]
            == (entry["family"] != row["judgment"]["candidate_family"])
            for entry in row["judgment"]["judges"]
        )
        for row in _quality_ranked
    ),
)
check(
    "legacy Sol receipts remain visible but self-family votes are excluded",
    any(
        any(
            entry["judge"]["model"] == "openai/gpt-5.6-sol"
            and entry["judge"]["provider"] == "openrouter"
            and entry["judge"]["reasoning_effort"] == "high"
            for entry in row["judgment"]["judges"]
        )
        for row in _quality_ranked
    )
    and all(
        not entry["eligible"]
        for row in _quality_ranked
        if row["judgment"]["candidate_family"] == "openai"
        for entry in row["judgment"]["judges"]
        if entry["family"] == "openai"
    ),
)
import judge_cloud_dialogue as cloud_judge  # noqa: E402

_judge_source_run = next(
    row for row in _qualification_feed["runs"]
    if row["status"] in {"needs_judgment", "qualified", "quality_rejected"}
)
_cloud_judge_items = cloud_judge._items(_judge_source_run)
check(
    "cloud judge uses a balanced full-production-context panel",
    len(_cloud_judge_items) == 18
    and all(item["prompt"].startswith("SYSTEM PROMPT:") for item in _cloud_judge_items)
    and all("\n\nUSER PROMPT:\n" in item["prompt"] for item in _cloud_judge_items),
)
check(
    "cloud judge profiles pin three independent OpenRouter families",
    {judge["family"] for judge in cloud_judge.JUDGES}
    == {"openai", "anthropic", "google"}
    and all(judge["provider"] == "openrouter" for judge in cloud_judge.JUDGES)
    and {
        judge["family"]: judge["reasoning_effort"] for judge in cloud_judge.JUDGES
    } == {"openai": "high", "anthropic": "low", "google": "high"},
)
check(
    "cloud judge never schedules a same-family candidate judge",
    all(
        judge["family"] != "openai"
        for judge in cloud_judge.eligible_judges({"model": "openai/gpt-5.6-luna"})
    )
    and all(
        judge["family"] != "google"
        for judge in cloud_judge.eligible_judges({"model": "google/gemini-3.6-flash"})
    ),
)
_bench_bug_item = {
    "prompt_id": _cloud_judge_items[0]["prompt_id"],
    "axes": {axis: 0 for axis in cloud_judge.WEIGHTS},
    "overall": 0.0,
    "flags": {"bench_bug": True},
}
_normal_items = [
    {
        "prompt_id": item["prompt_id"],
        "axes": {axis: 4 for axis in cloud_judge.WEIGHTS},
        "overall": 4.0,
        "flags": {"bench_bug": False},
    }
    for item in _cloud_judge_items[1:]
]
_judge_aggregate = cloud_judge._aggregate(
    {
        "rubric_sha256": cloud_judge.RUBRIC["rubric_sha256"],
        "sampling": {"unique_prompts": 6, "samples_per_prompt": 3},
        "items": _cloud_judge_items,
    },
    {
        "rubric_sha256": cloud_judge.RUBRIC["rubric_sha256"],
        "items": [_bench_bug_item, *_normal_items],
    },
    {"usage": {"cost": 1.0}},
)
check(
    "cloud judge hard-fails retained unusable candidate outputs without discarding usable scores",
    not _judge_aggregate["quality"]["pass"]
    and _judge_aggregate["sample"]["unusable_outputs"] == 1
    and _judge_aggregate["quality"]["hard_failures"]["unusable_output"] == 1,
)
_complete_judge_result = {
    "rubric_sha256": cloud_judge.RUBRIC["rubric_sha256"],
    "items": [_bench_bug_item, *_normal_items],
}
_resume_bundle = {
    "rubric_sha256": cloud_judge.RUBRIC["rubric_sha256"],
    "items": _cloud_judge_items,
}
check(
    "cloud judge resumes only schema-complete paid raw responses",
    cloud_judge._raw_is_resumable(
        {
            "choices": [{
                "finish_reason": "stop",
                "message": {"content": json.dumps(_complete_judge_result)},
            }],
        },
        _resume_bundle,
    )
    and not cloud_judge._raw_is_resumable(
        {
            "choices": [{
                "finish_reason": "stop",
                "message": {"content": json.dumps({
                    "rubric_sha256": cloud_judge.RUBRIC["rubric_sha256"],
                    "items": _normal_items[:3],
                })},
            }],
        },
        _resume_bundle,
    ),
)


print()
if _fails:
    print(f"FAILED ({len(_fails)}): {_fails}")
    raise SystemExit(1)
print("all v2 offline checks passed")
