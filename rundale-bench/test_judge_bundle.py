#!/usr/bin/env python3
"""Tests for the subagent post-validate wrapper (AC5).

``recover_result`` rescues a dispatch where the Agent printed JSON in its reply
text instead of calling Write, so the orchestrator no longer needs a manual
recovery step.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import pytest  # noqa: E402
from judge_bundle import recover_result  # noqa: E402

_RESULT = {"rubric_sha256": "a" * 64, "items": [{"prompt_id": "p1", "overall": 4.0}]}


def test_recover_result_passthrough_when_file_present(tmp_path: Path) -> None:
    done = tmp_path / "bundle.json"
    done.write_text(json.dumps(_RESULT), encoding="utf-8")
    got = recover_result(done, reply_text=None)
    assert got == _RESULT


def test_recover_result_extracts_from_reply_text(tmp_path: Path) -> None:
    done = tmp_path / "bundle.json"
    assert not done.exists()
    reply = (
        "Here is my judgment for the bundle:\n\n"
        "```json\n" + json.dumps(_RESULT) + "\n```\n"
        "Let me know if you need anything else."
    )
    got = recover_result(done, reply_text=reply)
    assert got == _RESULT
    # The file is now written so the queue is consistent for ingest.
    assert done.exists()
    assert json.loads(done.read_text(encoding="utf-8")) == _RESULT


def test_recover_result_extracts_from_bare_object(tmp_path: Path) -> None:
    done = tmp_path / "bundle.json"
    reply = "prose before " + json.dumps(_RESULT) + " prose after"
    got = recover_result(done, reply_text=reply)
    assert got == _RESULT
    assert done.exists()


def test_recover_result_repairs_unparseable_file_from_reply(tmp_path: Path) -> None:
    done = tmp_path / "bundle.json"
    done.write_text("this is not json", encoding="utf-8")
    got = recover_result(done, reply_text=json.dumps(_RESULT))
    assert got == _RESULT
    # File overwritten with the recovered JSON.
    assert json.loads(done.read_text(encoding="utf-8")) == _RESULT


def test_recover_result_raises_when_unrecoverable(tmp_path: Path) -> None:
    done = tmp_path / "bundle.json"
    with pytest.raises(ValueError):
        recover_result(done, reply_text="no json object anywhere here")


def test_recover_result_raises_when_no_file_and_no_reply(tmp_path: Path) -> None:
    done = tmp_path / "missing.json"
    with pytest.raises(ValueError):
        recover_result(done, reply_text=None)
