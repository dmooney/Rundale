#!/usr/bin/env python3
"""Hermetic tests for the Phase 1 eval-redesign foundations.

No network, no model calls: candidate replies are stubbed and judge results
are hand-authored. Plain-script convention (matches test_grade.py):

    python3 rundale-bench/test_phase1.py
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import sys
import tempfile
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import cache  # noqa: E402
import catalog  # noqa: E402
import judge_bundle as jb  # noqa: E402
import rundale_bench as rb  # noqa: E402


# ── helpers ──────────────────────────────────────────────────────────────────
@contextlib.contextmanager
def sandbox():
    """Redirect queue + cache + artifacts dirs to a throwaway tmp tree."""
    tmp = Path(tempfile.mkdtemp(prefix="bench-phase1-"))
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
def stub_call_chat(reply: str):
    saved = rb.call_chat
    rb.call_chat = lambda *a, **k: (reply, {})
    try:
        yield
    finally:
        rb.call_chat = saved


def _records():
    return [
        {"id": "dialogue-0001", "prompt": "I cannot sleep.", "tier": "core"},
        {"id": "dialogue-0002", "prompt": "Tell me of the Cailleach.", "tier": "core"},
    ]


def _args():
    return argparse.Namespace(
        suite="v1",
        split="dev",
        limit=None,
        tier=None,
        judge="judge_sonnet_v1",
        model_id="stub-model",
        provider_id="simulator",
    )


def _assert_raises(exc, fn):
    try:
        fn()
    except exc:
        return
    raise AssertionError(f"expected {exc.__name__}")


# ── criterion 1: catalog parses; cheapest provider resolves ──────────────────
def test_catalog_loads_and_resolves_cheapest():
    cat = catalog.load_catalog()
    assert "llama-3.3-70b" in cat.ids()
    # groq 0.59+3*0.79=2.96, together 0.88+3*0.88=3.52, openrouter 0.13+3*0.39=1.30
    assert cat.by_id("llama-3.3-70b").cheapest_provider().provider_id == "openrouter"
    raw = (_BENCH_DIR / "v1" / "models.toml").read_bytes()
    assert cat.sha256 == hashlib.sha256(raw).hexdigest()


def test_catalog_local_model_flag():
    assert catalog.load_catalog().by_id("qwen2.5-7b-mlx").local_only is True


# ── criterion 2: Sonnet judge config pinned and consistent ───────────────────
def test_sonnet_judge_pinned():
    judge = rb.load_judge("judge_sonnet_v1", "v1")
    assert judge["model"] == "claude-sonnet-4-6"
    assert rb._judge_is_subagent(judge) is True
    rb.verify_judge_rubric(judge)  # must not raise


def test_corrupt_rubric_sha_aborts():
    judge = rb.load_judge("judge_sonnet_v1", "v1")
    judge["rubric_sha256"] = "0" * 64
    _assert_raises(RuntimeError, lambda: rb.verify_judge_rubric(judge))


# ── criterion 3: cache_key matches the locked formula ────────────────────────
def test_cache_key_formula_and_determinism():
    pid, resp, rubric, model = "dialogue-0001", "Aye, sit ye down.", "abc123", "claude-sonnet-4-6"
    expected = hashlib.sha256(
        "\x00".join([pid, hashlib.sha256(resp.encode()).hexdigest(), rubric, model]).encode()
    ).hexdigest()
    assert cache.cache_key(pid, resp, rubric, model) == expected
    assert cache.cache_key(pid, resp, rubric, model) == cache.cache_key(pid, resp, rubric, model)


def test_cache_key_sensitive_to_each_input():
    base = dict(prompt_id="p", response="r", rubric="rub", model="m")
    k0 = cache.cache_key(base["prompt_id"], base["response"], base["rubric"], base["model"])
    for field in ("prompt_id", "response", "rubric", "model"):
        mut = dict(base)
        mut[field] = base[field] + "X"
        k1 = cache.cache_key(mut["prompt_id"], mut["response"], mut["rubric"], mut["model"])
        assert k0 != k1, f"key unchanged when {field} mutated"


# ── criterion 4: dialogue run writes one bundle, no judge HTTP call ──────────
def test_dialogue_run_writes_bundle():
    with sandbox(), stub_call_chat("Aye, a chroí, rest now."):
        judge = rb.load_judge("judge_sonnet_v1", "v1")
        data = rb.run_dialogue_bundled(
            rb.parse_target("stub@http://x/v1"), _records(), rb.CostTracker(), _args(), judge
        )
        pending = jb.list_pending()
        assert len(pending) == 1
        bundle = jb.read_json(pending[0])
        assert len(bundle["items"]) == 2
        assert bundle["judge_model"] == "claude-sonnet-4-6"
        assert data["summary"]["bundles_queued"] == 1
        assert data["summary"]["judged"] == 0


# ── criterion 5: cache hits skip re-queueing ─────────────────────────────────
def test_cache_hit_skips_requeue():
    reply = "Aye, a chroí, rest now."
    with sandbox(), stub_call_chat(reply):
        judge = rb.load_judge("judge_sonnet_v1", "v1")
        for rec in _records():
            key = cache.cache_key(rec["id"], reply, judge["rubric_sha256"], judge["model"])
            cache.put(
                key,
                {
                    "axes": {a: 4 for a in jb.AXES},
                    "overall": 4.0,
                    "flags": {"non_latin_detected": False, "refused": False, "judge_retry": False},
                },
            )
        data = rb.run_dialogue_bundled(
            rb.parse_target("stub@http://x/v1"), _records(), rb.CostTracker(), _args(), judge
        )
        assert data["summary"]["cache_hits"] == 2
        assert data["summary"]["bundles_queued"] == 0
        assert jb.list_pending() == []
        assert data["summary"]["judged"] == 2


# ── criterion 6: ingest validates done/, stores judgments, refreshes run ─────
def test_ingest_stores_and_refreshes():
    reply = "Aye, a chroí, rest now."
    with sandbox() as tmp, stub_call_chat(reply):
        judge = rb.load_judge("judge_sonnet_v1", "v1")
        data = rb.run_dialogue_bundled(
            rb.parse_target("stub@http://x/v1"), _records(), rb.CostTracker(), _args(), judge
        )
        run_path = tmp / "artifacts" / "run_stub_dialogue_TEST.json"
        run_path.write_text(json.dumps({"slices": {"dialogue": data}}), encoding="utf-8")

        pending = jb.list_pending()[0]
        bundle = jb.read_json(pending)
        result = {
            "version": 1,
            "slice": "dialogue",
            "rubric_sha256": bundle["rubric_sha256"],
            "items": [
                {
                    "prompt_id": it["prompt_id"],
                    "axes": {a: 4 for a in jb.AXES},
                    "overall": 4.0,
                    "rationales": {a: "ok" for a in jb.AXES},
                    "flags": {"non_latin_detected": False, "refused": False},
                }
                for it in bundle["items"]
            ],
        }
        jb.DONE_DIR.mkdir(parents=True, exist_ok=True)
        (jb.DONE_DIR / pending.name).write_text(json.dumps(result), encoding="utf-8")

        rb.cmd_ingest(["--finalize"])  # must not raise

        assert len(list((tmp / "judgments").glob("*.json"))) == 2
        summary = json.loads(run_path.read_text())["slices"]["dialogue"]["summary"]
        assert summary["judged"] == 2
        assert summary["judge_failures"] == 0
        assert abs(summary["overall"] - 4.0) < 1e-9


def test_ingest_finalize_errors_on_pending():
    with sandbox(), stub_call_chat("Aye."):
        judge = rb.load_judge("judge_sonnet_v1", "v1")
        rb.run_dialogue_bundled(
            rb.parse_target("stub@http://x/v1"), _records(), rb.CostTracker(), _args(), judge
        )
        _assert_raises(SystemExit, lambda: rb.cmd_ingest(["--finalize"]))


# ── criterion 7: malformed judge output handled, not crashed ─────────────────
def test_malformed_item_marked_failure():
    bundle = {"rubric_sha256": "x", "items": [{"prompt_id": "p1"}]}
    result = {
        "rubric_sha256": "x",
        "items": [{"prompt_id": "p1", "axes": {"character": 9}, "overall": 4.0}],
    }
    valid, failed = jb.validate_result(result, bundle)
    assert valid == []
    assert len(failed) == 1 and failed[0]["axes"] is None
    assert failed[0]["flags"]["judge_retry"] is True


def test_rubric_mismatch_rejects_whole_result():
    bundle = {"rubric_sha256": "aaa", "items": [{"prompt_id": "p1"}]}
    result = {
        "rubric_sha256": "bbb",
        "items": [{"prompt_id": "p1", "axes": {a: 4 for a in jb.AXES}, "overall": 4.0}],
    }
    valid, failed = jb.validate_result(result, bundle)
    assert valid == [] and len(failed) == 1
    assert "rubric_sha256 mismatch" in failed[0]["error"]


def test_extract_json_tolerates_prose_and_fences():
    # A real Sonnet subagent sometimes prepends analysis and wraps the object
    # in a ```json fence despite the JSON-only instruction; ingest must cope.
    obj = {"version": 1, "slice": "dialogue", "items": []}
    fenced = "Here is my scoring:\n\n```json\n" + json.dumps(obj) + "\n```\n"
    assert jb.extract_json(fenced) == obj
    assert jb.extract_json(json.dumps(obj)) == obj  # clean parse still works
    prosey = "Some thoughts. " + json.dumps(obj) + " done."
    assert jb.extract_json(prosey) == obj
    _assert_raises(ValueError, lambda: jb.extract_json("no json here"))


def test_dropped_prompt_is_failure():
    bundle = {"rubric_sha256": "x", "items": [{"prompt_id": "p1"}, {"prompt_id": "p2"}]}
    result = {
        "rubric_sha256": "x",
        "items": [{"prompt_id": "p1", "axes": {a: 4 for a in jb.AXES}, "overall": 4.0}],
    }
    valid, failed = jb.validate_result(result, bundle)
    assert len(valid) == 1
    assert any(f["prompt_id"] == "p2" for f in failed)


# ── criterion 8: both skills exist with the documented loop ──────────────────
def test_skills_exist_with_headings():
    repo = _BENCH_DIR.parent
    outer = repo / ".agents" / "skills" / "rundale-bench" / "SKILL.md"
    inner = repo / ".agents" / "skills" / "rundale-bench-judge" / "SKILL.md"
    assert outer.exists() and inner.exists()
    outer_txt = outer.read_text()
    assert "drain-queue" in outer_txt and ".bench-queue/done" in outer_txt
    inner_txt = inner.read_text()
    assert "judge_sonnet_v1.system.md" in inner_txt and "JSON" in inner_txt


# ── criterion 9: legacy qwen path untouched (additive refactor) ──────────────
def test_legacy_qwen_judge_not_subagent():
    assert rb._judge_is_subagent(rb.load_judge("judge_v1", "v1")) is False


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
