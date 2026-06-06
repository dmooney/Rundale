#!/usr/bin/env python3
"""Tests for repro_manifest.collect_manifest and write_manifest.

Covers:
- Version fields populated from installed packages (mocked).
- Version fields fall back to "unknown" when packages are absent.
- harness_sha populated from git (mocked) and falls back to "unknown".
- Model SHA resolved from the HF API (mocked) or local path or None.
- write_manifest writes valid JSON with the expected filename.
- The full manifest shape for a sweep with multiple candidates.
"""

from __future__ import annotations

import importlib.metadata
import json
import subprocess
import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import repro_manifest as rm  # noqa: E402

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _fake_pkg_version(present: dict[str, str]):
    """Return a side_effect callable that resolves packages from *present*,
    raising PackageNotFoundError for anything else."""

    def _version(pkg: str) -> str:
        if pkg in present:
            return present[pkg]
        raise importlib.metadata.PackageNotFoundError(pkg)

    return _version


# ---------------------------------------------------------------------------
# _pkg_version
# ---------------------------------------------------------------------------


def test_pkg_version_present():
    with patch("importlib.metadata.version", return_value="1.2.3"):
        assert rm._pkg_version("mlx-lm") == "1.2.3"


def test_pkg_version_absent():
    with patch(
        "importlib.metadata.version",
        side_effect=importlib.metadata.PackageNotFoundError("mlx-lm"),
    ):
        assert rm._pkg_version("mlx-lm") == "unknown"


def test_pkg_version_unexpected_error():
    with patch("importlib.metadata.version", side_effect=RuntimeError("boom")):
        assert rm._pkg_version("mlx-lm") == "unknown"


# ---------------------------------------------------------------------------
# _git_sha
# ---------------------------------------------------------------------------


def test_git_sha_success(tmp_path):
    with patch(
        "subprocess.check_output",
        return_value=b"abc1234567890abcdef1234567890abcdef123456\n",
    ):
        sha = rm._git_sha(tmp_path)
    assert sha == "abc1234567890abcdef1234567890abcdef123456"


def test_git_sha_failure(tmp_path):
    with patch(
        "subprocess.check_output",
        side_effect=subprocess.CalledProcessError(128, "git"),
    ):
        sha = rm._git_sha(tmp_path)
    assert sha == "unknown"


# ---------------------------------------------------------------------------
# _model_sha_from_hf_api
# ---------------------------------------------------------------------------


def _fake_hf_response(sha: str):
    """Build a fake urllib response context manager returning *sha* in JSON."""
    body = json.dumps({"sha": sha}).encode("utf-8")
    resp = MagicMock()
    resp.__enter__ = lambda s: s
    resp.__exit__ = MagicMock(return_value=False)
    resp.read.return_value = body
    return resp


def test_model_sha_from_hf_api_success():
    with patch("urllib.request.urlopen", return_value=_fake_hf_response("deadbeef1234")):
        sha = rm._model_sha_from_hf_api("org/model")
    assert sha == "deadbeef1234"


def test_model_sha_from_hf_api_network_error():
    import urllib.error

    with patch("urllib.request.urlopen", side_effect=urllib.error.URLError("timeout")):
        sha = rm._model_sha_from_hf_api("org/model")
    assert sha is None


def test_model_sha_from_hf_api_missing_sha_field():
    body = json.dumps({"other": "data"}).encode("utf-8")
    resp = MagicMock()
    resp.__enter__ = lambda s: s
    resp.__exit__ = MagicMock(return_value=False)
    resp.read.return_value = body
    with patch("urllib.request.urlopen", return_value=resp):
        sha = rm._model_sha_from_hf_api("org/model")
    # "modelId" key also absent, so sha should be None
    assert sha is None


# ---------------------------------------------------------------------------
# _model_sha_from_local
# ---------------------------------------------------------------------------


def test_model_sha_from_local_git_ref(tmp_path):
    git_dir = tmp_path / ".git" / "refs" / "heads"
    git_dir.mkdir(parents=True)
    (git_dir / "main").write_text("a" * 40 + "\n", encoding="utf-8")
    sha = rm._model_sha_from_local(tmp_path)
    assert sha == "a" * 40


def test_model_sha_from_local_detached_head(tmp_path):
    git_dir = tmp_path / ".git"
    git_dir.mkdir()
    (git_dir / "HEAD").write_text("b" * 40, encoding="utf-8")
    sha = rm._model_sha_from_local(tmp_path)
    assert sha == "b" * 40


def test_model_sha_from_local_config_json_fallback(tmp_path):
    config = {"sha": "c" * 40}
    (tmp_path / "config.json").write_text(json.dumps(config), encoding="utf-8")
    sha = rm._model_sha_from_local(tmp_path)
    assert sha == "c" * 40


def test_model_sha_from_local_missing_dir():
    sha = rm._model_sha_from_local(Path("/nonexistent/path/xyz"))
    assert sha is None


def test_model_sha_from_local_no_recognisable_source(tmp_path):
    # Directory exists but has no git info or known JSON
    sha = rm._model_sha_from_local(tmp_path)
    assert sha is None


# ---------------------------------------------------------------------------
# resolve_model_sha — integration between HF API and local fallback
# ---------------------------------------------------------------------------


def test_resolve_model_sha_prefers_hf_api():
    with patch.object(rm, "_model_sha_from_hf_api", return_value="hf_sha_abc"):
        sha = rm.resolve_model_sha("org/model")
    assert sha == "hf_sha_abc"


def test_resolve_model_sha_falls_back_to_local(tmp_path):
    git_dir = tmp_path / ".git" / "refs" / "heads"
    git_dir.mkdir(parents=True)
    (git_dir / "main").write_text("d" * 40 + "\n", encoding="utf-8")
    with patch.object(rm, "_model_sha_from_hf_api", return_value=None):
        sha = rm.resolve_model_sha("org/model", local_path=tmp_path)
    assert sha == "d" * 40


def test_resolve_model_sha_none_when_all_absent():
    with patch.object(rm, "_model_sha_from_hf_api", return_value=None):
        sha = rm.resolve_model_sha("org/nonexistent", local_path=Path("/nonexistent"))
    assert sha is None


# ---------------------------------------------------------------------------
# collect_manifest
# ---------------------------------------------------------------------------


def test_collect_manifest_all_present():
    """All tools present: fields carry real version strings, not 'unknown'."""
    pkg_map = {"mlx-lm": "0.22.0", "vllm-mlx": "1.0.0", "mlx": "0.25.1"}
    with (
        patch("importlib.metadata.version", side_effect=_fake_pkg_version(pkg_map)),
        patch("subprocess.check_output", return_value=b"e" * 40 + b"\n"),
        patch.object(rm, "_model_sha_from_hf_api", return_value=None),
    ):
        manifest = rm.collect_manifest(
            model_ids=["org/model-a"],
            hf_repos={"org/model-a": "org/model-a"},
        )

    assert manifest["harness_sha"] == "e" * 40
    assert manifest["mlx_lm"] == "0.22.0"
    assert manifest["vllm_mlx"] == "1.0.0"
    assert manifest["mlx"] == "0.25.1"
    assert manifest["models"] == {"org/model-a": None}
    assert "captured_utc" in manifest


def test_collect_manifest_all_absent():
    """No tools installed: version fields are 'unknown', model SHA is None."""
    with (
        patch(
            "importlib.metadata.version",
            side_effect=importlib.metadata.PackageNotFoundError("x"),
        ),
        patch(
            "subprocess.check_output",
            side_effect=subprocess.CalledProcessError(128, "git"),
        ),
        patch.object(rm, "_model_sha_from_hf_api", return_value=None),
    ):
        manifest = rm.collect_manifest(model_ids=["missing/model"])

    assert manifest["harness_sha"] == "unknown"
    assert manifest["mlx_lm"] == "unknown"
    assert manifest["vllm_mlx"] == "unknown"
    assert manifest["mlx"] == "unknown"
    assert manifest["models"] == {"missing/model": None}


def test_collect_manifest_partial_packages():
    """mlx-lm present, vllm-mlx and mlx absent."""
    pkg_map = {"mlx-lm": "0.20.0"}
    with (
        patch("importlib.metadata.version", side_effect=_fake_pkg_version(pkg_map)),
        patch(
            "subprocess.check_output",
            side_effect=subprocess.CalledProcessError(128, "git"),
        ),
    ):
        manifest = rm.collect_manifest()

    assert manifest["mlx_lm"] == "0.20.0"
    assert manifest["vllm_mlx"] == "unknown"
    assert manifest["mlx"] == "unknown"
    assert manifest["harness_sha"] == "unknown"
    assert manifest["models"] == {}


def test_collect_manifest_multiple_candidates():
    """SHA resolution happens per model_id, independently."""

    def _sha(repo, **_):
        return {"org/fast": "sha_fast", "org/slow": None}.get(repo)

    with (
        patch("importlib.metadata.version", return_value="1.0"),
        patch("subprocess.check_output", return_value=b"f" * 40 + b"\n"),
        patch.object(rm, "_model_sha_from_hf_api", side_effect=_sha),
    ):
        manifest = rm.collect_manifest(
            model_ids=["org/fast", "org/slow"],
        )

    assert manifest["models"]["org/fast"] == "sha_fast"
    assert manifest["models"]["org/slow"] is None


def test_collect_manifest_no_models():
    """Empty model_ids yields an empty models dict — no crash."""
    with (
        patch("importlib.metadata.version", return_value="1.0"),
        patch("subprocess.check_output", return_value=b"0" * 40 + b"\n"),
    ):
        manifest = rm.collect_manifest(model_ids=[])

    assert manifest["models"] == {}


# ---------------------------------------------------------------------------
# write_manifest
# ---------------------------------------------------------------------------


def test_write_manifest_creates_file(tmp_path):
    manifest = {"harness_sha": "abc", "models": {}, "captured_utc": "2026-06-01T00:00:00+00:00"}
    path = rm.write_manifest(manifest, out_dir=tmp_path, stamp="20260601T000000Z")
    assert path.exists()
    assert path.name == "repro_manifest_20260601T000000Z.json"


def test_write_manifest_valid_json(tmp_path):
    manifest = {
        "harness_sha": "abc123",
        "mlx_lm": "0.22.0",
        "vllm_mlx": "unknown",
        "mlx": "0.25.1",
        "models": {"org/model": "sha1"},
        "captured_utc": "2026-06-01T00:00:00+00:00",
    }
    path = rm.write_manifest(manifest, out_dir=tmp_path, stamp="20260601T120000Z")
    loaded = json.loads(path.read_text(encoding="utf-8"))
    assert loaded["harness_sha"] == "abc123"
    assert loaded["models"]["org/model"] == "sha1"


def test_write_manifest_custom_filename(tmp_path):
    manifest = {"harness_sha": "x"}
    path = rm.write_manifest(manifest, out_dir=tmp_path, filename="custom_manifest.json")
    assert path.name == "custom_manifest.json"
    assert path.exists()


def test_write_manifest_creates_out_dir(tmp_path):
    nested = tmp_path / "a" / "b" / "c"
    manifest = {"harness_sha": "y"}
    path = rm.write_manifest(manifest, out_dir=nested, stamp="20260101T000000Z")
    assert path.exists()


def test_write_manifest_auto_stamp(tmp_path):
    """Without an explicit stamp, the filename still matches the expected pattern."""
    manifest = {"harness_sha": "z"}
    path = rm.write_manifest(manifest, out_dir=tmp_path)
    assert path.name.startswith("repro_manifest_")
    assert path.name.endswith(".json")
    assert path.exists()
