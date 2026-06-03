#!/usr/bin/env python3
"""Hermetic tests for Phase 2: tier funnel + differential re-judge.

No network, no model calls. Run: python3 rundale-bench/test_phase2.py
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import cache  # noqa: E402
import judge_bundle as jb  # noqa: E402
import promote as promo  # noqa: E402
import rundale_bench as rb  # noqa: E402
from catalog import load_catalog  # noqa: E402


@contextlib.contextmanager
def sandbox():
    tmp = Path(tempfile.mkdtemp(prefix="bench-phase2-"))
    saved = {
        (cache, "JUDGMENTS_DIR"): cache.JUDGMENTS_DIR,
        (jb, "QUEUE_DIR"): jb.QUEUE_DIR,
        (jb, "PENDING_DIR"): jb.PENDING_DIR,
        (jb, "DONE_DIR"): jb.DONE_DIR,
        (rb, "_ARTIFACTS_DIR"): rb._ARTIFACTS_DIR,
    }
    cache.JUDGMENTS_DIR = tmp / "judgments"
    jb.QUEUE_DIR = tmp / "q"
    jb.PENDING_DIR = tmp / "q" / "pending"
    jb.DONE_DIR = tmp / "q" / "done"
    rb._ARTIFACTS_DIR = tmp / "artifacts"
    (tmp / "artifacts").mkdir()
    try:
        yield tmp
    finally:
        for (mod, name), val in saved.items():
            setattr(mod, name, val)


@contextlib.contextmanager
def stub_call_chat(reply="Aye, a chroí."):
    saved = rb.call_chat
    rb.call_chat = lambda *a, **k: (reply, {})
    try:
        yield
    finally:
        rb.call_chat = saved


def _assert_raises(exc, fn):
    try:
        fn()
    except exc:
        return
    raise AssertionError(f"expected {exc.__name__}")


def _capture(fn):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        fn()
    return buf.getvalue()


def _write_run(tmp, model_id, tier, summary, results=None):
    out = {
        "tier": tier,
        "slice": "dialogue",
        "candidate": {"model_id": model_id, "provider_id": "x", "resolved_target": "t"},
        "slices": {"dialogue": {"summary": summary, "results": results or []}},
    }
    (tmp / "artifacts" / f"run_{model_id}_dialogue_{tier}_T.json").write_text(json.dumps(out))


# ── criterion 1: tier prompt sets resolve ────────────────────────────────────
def test_tiers_resolve():
    assert len(rb.tier_prompt_ids("screen", "dialogue", "v1")) == 5
    assert len(rb.tier_prompt_ids("contender", "dialogue", "v1")) == 25
    assert rb.tier_prompt_ids("finalist", "dialogue", "v1") is None


def test_tier_ids_are_valid_dialogue_ids():
    from eval_lib import load_slice

    valid = {r["id"] for r in load_slice("dialogue", version="v1", split="dev")}
    for tier in ("screen", "contender"):
        for i in rb.tier_prompt_ids(tier, "dialogue", "v1"):
            assert i in valid, f"{tier} id {i} not in dialogue dev"


# ── criterion 3: --models selection forms ────────────────────────────────────
def test_resolve_models_forms():
    cat = load_catalog()
    assert {m.id for m in rb.resolve_models(cat, "all", set())} == set(cat.ids())
    two = rb.resolve_models(cat, "llama-3.3-70b,grok-4-fast", set())
    assert {m.id for m in two} == {"llama-3.3-70b", "grok-4-fast"}
    add = rb.resolve_models(cat, "+claude-haiku-4-5", set())
    assert [m.id for m in add] == ["claude-haiku-4-5"]
    excl = rb.resolve_models(cat, "all", {"llama-3.3-70b"})
    assert "llama-3.3-70b" not in {m.id for m in excl}
    _assert_raises(SystemExit, lambda: rb.resolve_models(cat, "nope-9000", set()))


# ── criterion 2: catalog-driven multi-model run ──────────────────────────────
def test_run_writes_per_model():
    with sandbox() as tmp, stub_call_chat():
        rb.cmd_run(
            [
                "--tier",
                "screen",
                "--models",
                "qwen2.5-7b-mlx,claude-haiku-4-5",
                "--slice",
                "dialogue",
            ]
        )
        runs = sorted((tmp / "artifacts").glob("run_*.json"))
        assert len(runs) == 2
        for p in runs:
            out = json.loads(p.read_text())
            assert out["tier"] == "screen"
            assert out["slices"]["dialogue"]["summary"]["records"] == 5  # screen set
        assert len(jb.list_pending()) == 2  # one bundle per model


# ── criterion 3b: additive run keeps other models' cache warm ────────────────
def test_additive_run_keeps_cache_warm():
    # Cache is content-addressed: a judgment is keyed on the response text, not
    # the model. Give A and B distinct replies so B genuinely needs judging.
    reply_a = "Aye, a chroí, model A speaks."
    with sandbox():
        judge = rb.load_judge("judge_sonnet_v1", "v1")
        from eval_lib import load_slice

        screen_ids = set(rb.tier_prompt_ids("screen", "dialogue", "v1"))
        for rec in load_slice("dialogue", version="v1", split="dev"):
            if rec["id"] in screen_ids:
                key = cache.cache_key(rec["id"], reply_a, judge["rubric_sha256"], judge["model"])
                cache.put(
                    key,
                    {
                        "axes": {a: 4 for a in jb.AXES},
                        "overall": 4.0,
                        "flags": {
                            "non_latin_detected": False,
                            "refused": False,
                            "judge_retry": False,
                        },
                    },
                )
        a_keys_before = sorted(p.name for p in cache.JUDGMENTS_DIR.glob("*.json"))
        with stub_call_chat("Distinct reply from model B."):
            rb.cmd_run(["--tier", "screen", "--models", "+grok-4-fast", "--slice", "dialogue"])
        a_keys_after = sorted(p.name for p in cache.JUDGMENTS_DIR.glob("*.json"))
        assert a_keys_before == a_keys_after  # A's cache untouched (run writes no judgments)
        assert len(jb.list_pending()) == 1  # only B queued
        b = jb.read_json(jb.list_pending()[0])
        assert b["candidate"]["model_id"] == "grok-4-fast"


# ── criterion 4: promotion logic ─────────────────────────────────────────────
def test_promotion_logic_table():
    aggs = {
        "pass-model": {
            "overall": 4.6,
            "character": 5,
            "authenticity": 4,
            "language": 5,
            "responsiveness": 5,
            "craft": 4,
        },
        "low-overall": {
            "overall": 3.2,
            "character": 4,
            "authenticity": 3,
            "language": 4,
            "responsiveness": 3,
            "craft": 3,
        },
        "weak-axis": {
            "overall": 4.5,
            "character": 5,
            "authenticity": 2,
            "language": 5,
            "responsiveness": 5,
            "craft": 5,
        },
    }
    decisions = {d.model_id: d for d in promo.promote(aggs, "screen")}
    assert decisions["pass-model"].promoted and decisions["pass-model"].to_tier == "contender"
    assert not decisions["low-overall"].promoted  # overall 3.2 < 3.5
    assert not decisions["weak-axis"].promoted  # min axis 2 <= 2.0
    # overrides
    forced = {d.model_id: d for d in promo.promote(aggs, "screen", include={"low-overall"})}
    assert forced["low-overall"].promoted
    blocked = {d.model_id: d for d in promo.promote(aggs, "screen", exclude={"pass-model"})}
    assert not blocked["pass-model"].promoted
    # contender->finalist floor is stricter (axis_floor 3.0)
    cf = promo.decide(
        "x",
        {
            "overall": 4.1,
            "character": 3,
            "authenticity": 4,
            "language": 4,
            "responsiveness": 4,
            "craft": 4,
        },
        "contender",
    )
    assert not cf.promoted  # min axis 3 <= 3.0


# ── criterion 5: promote reads real run aggregates ───────────────────────────
def test_promote_reads_runs():
    with sandbox() as tmp:
        _write_run(
            tmp,
            "good",
            "screen",
            {
                "overall": 4.2,
                "character": 4,
                "authenticity": 4,
                "language": 5,
                "responsiveness": 4,
                "craft": 4,
            },
        )
        _write_run(
            tmp,
            "meh",
            "screen",
            {
                "overall": 3.0,
                "character": 3,
                "authenticity": 3,
                "language": 3,
                "responsiveness": 3,
                "craft": 3,
            },
        )
        out = _capture(lambda: rb.cmd_promote(["--from", "screen"]))
        assert "PROMOTE  good" in out
        assert "promoted: good" in out
        assert "PROMOTE  meh" not in out


# ── criterion 6: rejudge dry-run detects staleness ───────────────────────────
def test_rejudge_dry_run_detects_stale():
    with sandbox() as tmp:
        judge = rb.load_judge("judge_sonnet_v1", "v1")
        # A run with two samples; cache them under an OLD rubric sha.
        results = [
            {"id": "dialogue-0001", "reply": "Aye one."},
            {"id": "dialogue-0002", "reply": "Aye two."},
        ]
        _write_run(tmp, "m", "screen", {"overall": 4.0}, results)
        old_sha = "0" * 64
        for r in results:
            cache.put(
                cache.cache_key(r["id"], r["reply"], old_sha, judge["model"]),
                {"axes": {a: 4 for a in jb.AXES}, "overall": 4.0, "flags": {}},
            )
        samples = rb.collect_samples("dialogue")
        assert len(samples) == 2
        stale = rb.stale_samples(samples, judge)  # current sha != old_sha -> all stale
        assert len(stale) == 2
        out = _capture(lambda: rb.cmd_rejudge(["--dry-run"]))
        assert "stale=2" in out
        assert len(jb.list_pending()) == 0  # dry-run queues nothing


def test_rejudge_no_stale_when_cached_current():
    with sandbox() as tmp:
        judge = rb.load_judge("judge_sonnet_v1", "v1")
        results = [{"id": "dialogue-0001", "reply": "Aye one."}]
        _write_run(tmp, "m", "screen", {"overall": 4.0}, results)
        cache.put(
            cache.cache_key("dialogue-0001", "Aye one.", judge["rubric_sha256"], judge["model"]),
            {"axes": {a: 4 for a in jb.AXES}, "overall": 4.0, "flags": {}},
        )
        assert rb.stale_samples(rb.collect_samples("dialogue"), judge) == []


# ── criterion 7: rejudge --since classify + requeue ──────────────────────────
def test_classify_since_reasons():
    judge = rb.load_judge("judge_sonnet_v1", "v1")
    # model swap
    swap = json.dumps({"model": "old-model", "rubric_sha256": judge["rubric_sha256"]})
    assert (
        rb.classify_since("REF", "judge_sonnet_v1", "v1", show=lambda r, p: swap)
        == "judge model swap"
    )
    # rubric drift
    drift = json.dumps({"model": judge["model"], "rubric_sha256": "deadbeef"})
    assert (
        rb.classify_since("REF", "judge_sonnet_v1", "v1", show=lambda r, p: drift) == "rubric drift"
    )
    # unchanged
    same = json.dumps({"model": judge["model"], "rubric_sha256": judge["rubric_sha256"]})
    assert (
        rb.classify_since("REF", "judge_sonnet_v1", "v1", show=lambda r, p: same) == "new responses"
    )
    # missing at ref
    assert (
        rb.classify_since("REF", "judge_sonnet_v1", "v1", show=lambda r, p: None)
        == "new judge config"
    )


def test_rejudge_requeues_stale():
    with sandbox() as tmp:
        results = [
            {"id": "dialogue-0001", "reply": "Aye one."},
            {"id": "dialogue-0002", "reply": "Aye two."},
        ]
        _write_run(tmp, "m", "screen", {"overall": 4.0}, results)
        # No cache seeded -> all stale -> non-dry rejudge queues one bundle for model m.
        rb.cmd_rejudge([])
        pend = jb.list_pending()
        assert len(pend) == 1
        assert len(jb.read_json(pend[0])["items"]) == 2


# ── criterion 8: skip-promotion direct finalist run ──────────────────────────
def test_skip_promotion_finalist():
    with sandbox() as tmp, stub_call_chat():
        rb.cmd_run(
            [
                "--tier",
                "finalist",
                "--models",
                "+qwen2.5-7b-mlx",
                "--slice",
                "dialogue",
                "--skip-promotion",
            ]
        )
        out = json.loads(next((tmp / "artifacts").glob("run_*.json")).read_text())
        assert out["tier"] == "finalist"
        assert out["skip_promotion"] is True
        assert out["promoted_from"] is None
        from eval_lib import load_slice

        assert out["slices"]["dialogue"]["summary"]["records"] == len(
            load_slice("dialogue", version="v1", split="dev")
        )


def main() -> int:
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_") and callable(v)]
    failed = 0
    for t in tests:
        try:
            t()
        except AssertionError as e:
            print(f"FAIL {t.__name__}: {e}")
            failed += 1
        except Exception as e:
            print(f"ERROR {t.__name__}: {type(e).__name__}: {e}")
            failed += 1
        else:
            print(f"OK   {t.__name__}")
    print(f"\n{len(tests) - failed}/{len(tests)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
