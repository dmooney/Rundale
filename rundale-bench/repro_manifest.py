#!/usr/bin/env python3
"""Per-sweep reproducibility manifest.

Captures the provenance information needed to reproduce or audit a
rundale-bench sweep:

- ``harness_sha``    — git commit of the harness (``git rev-parse HEAD``).
- ``mlx_lm``        — installed mlx-lm package version.
- ``vllm_mlx``      — installed vllm-mlx package version (if present).
- ``mlx``           — MLX runtime version (``mlx`` package).
- ``models``        — per-candidate mapping of model_id → SHA (from HuggingFace
                      metadata or the local model directory).

Every field degrades gracefully: if a tool or package is absent the value is
``"unknown"`` (strings) or ``None`` (model SHA), so a missing MLX install or
an air-gapped environment never crashes the sweep.

Usage::

    from repro_manifest import collect_manifest, write_manifest

    manifest = collect_manifest(
        model_ids=["mlx-community/Qwen3-1.7B-4bit"],
        hf_repos={"mlx-community/Qwen3-1.7B-4bit": "mlx-community/Qwen3-1.7B-4bit"},
    )
    path = write_manifest(manifest, out_dir=artifacts_dir, stamp="20260101T120000Z")
"""

from __future__ import annotations

import importlib.metadata
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent


# ---------------------------------------------------------------------------
# Version probes — each returns a string, never raises
# ---------------------------------------------------------------------------


def _pkg_version(pkg: str) -> str:
    """Return the installed version of *pkg* or ``"unknown"``."""
    try:
        return importlib.metadata.version(pkg)
    except Exception:
        return "unknown"


def _git_sha(repo_root: Path = _REPO_ROOT) -> str:
    """Return ``git rev-parse HEAD`` or ``"unknown"``."""
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_root,
            stderr=subprocess.DEVNULL,
        )
        return out.decode().strip()
    except Exception:
        return "unknown"


# ---------------------------------------------------------------------------
# Model SHA probes — HF API then local fallback
# ---------------------------------------------------------------------------


def _model_sha_from_hf_api(hf_repo: str) -> str | None:
    """Query the HuggingFace Hub API for the repo's HEAD commit SHA.

    Returns the SHA string on success, or ``None`` if the request fails
    (network absent, private repo without token, rate-limited, etc.).
    """
    try:
        import urllib.error
        import urllib.request

        url = f"https://huggingface.co/api/models/{hf_repo}"
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            data: dict[str, Any] = json.loads(resp.read().decode("utf-8"))
        sha = data.get("sha") or data.get("modelId")
        return str(sha) if sha else None
    except Exception:
        return None


def _model_sha_from_local(local_path: Path) -> str | None:
    """Read the HEAD commit SHA from a local HuggingFace model clone.

    Checks ``.git/refs/heads/main``, then ``.git/HEAD`` for bare commits, then
    any ``*.json`` manifest that carries a ``"sha"`` or ``"model_sha"`` key.
    Returns ``None`` if none of these sources yield a non-empty string.
    """
    if not local_path.is_dir():
        return None
    # Try git refs
    for candidate in [
        local_path / ".git" / "refs" / "heads" / "main",
        local_path / ".git" / "refs" / "heads" / "master",
    ]:
        if candidate.is_file():
            sha = candidate.read_text(encoding="utf-8").strip()
            if sha:
                return sha
    head = local_path / ".git" / "HEAD"
    if head.is_file():
        raw = head.read_text(encoding="utf-8").strip()
        # Detached HEAD: 40-hex chars
        if len(raw) == 40 and all(c in "0123456789abcdefABCDEF" for c in raw):
            return raw
    # Fallback: HuggingFace model.safetensors.index.json or model_index.json
    for name in ("model.safetensors.index.json", "model_index.json", "config.json"):
        p = local_path / name
        if p.is_file():
            try:
                d = json.loads(p.read_text(encoding="utf-8"))
                sha = d.get("sha") or d.get("model_sha")
                if sha:
                    return str(sha)
            except Exception:
                pass
    return None


def resolve_model_sha(
    model_id: str,
    *,
    hf_repo: str | None = None,
    local_path: Path | None = None,
) -> str | None:
    """Best-effort SHA for *model_id*.

    Priority:
    1. HF Hub API (``hf_repo`` or ``model_id`` used as the repo slug).
    2. Local model directory (``local_path``).
    3. ``None``.
    """
    repo = hf_repo or model_id
    sha = _model_sha_from_hf_api(repo)
    if sha:
        return sha
    if local_path:
        return _model_sha_from_local(local_path)
    # Try common local cache locations without network
    cache_candidates = [
        Path.home() / ".cache" / "huggingface" / "hub" / f"models--{repo.replace('/', '--')}",
    ]
    for p in cache_candidates:
        found = _model_sha_from_local(p)
        if found:
            return found
    return None


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def collect_manifest(
    *,
    model_ids: list[str] | None = None,
    hf_repos: dict[str, str] | None = None,
    local_paths: dict[str, Path] | None = None,
    repo_root: Path = _REPO_ROOT,
) -> dict[str, Any]:
    """Build a reproducibility manifest dict for a sweep.

    Parameters
    ----------
    model_ids:
        Canonical model identifiers to probe for SHAs.  May be empty.
    hf_repos:
        ``{model_id: hf_repo_slug}`` overrides — when the catalog model_id
        differs from the HF repo slug used for download.
    local_paths:
        ``{model_id: Path}`` pointing at a local model clone or cache dir.
    repo_root:
        Root of the git repository (default: detected from ``__file__``).

    Returns
    -------
    A JSON-serialisable dict with keys:
    ``harness_sha``, ``mlx_lm``, ``vllm_mlx``, ``mlx``, ``models``,
    ``captured_utc``.
    """
    hf_repos = hf_repos or {}
    local_paths = local_paths or {}
    model_ids = model_ids or []

    models: dict[str, str | None] = {}
    for mid in model_ids:
        models[mid] = resolve_model_sha(
            mid,
            hf_repo=hf_repos.get(mid),
            local_path=local_paths.get(mid),
        )

    return {
        "harness_sha": _git_sha(repo_root),
        "mlx_lm": _pkg_version("mlx-lm"),
        "vllm_mlx": _pkg_version("vllm-mlx"),
        "mlx": _pkg_version("mlx"),
        "models": models,
        "captured_utc": datetime.now(timezone.utc).isoformat(),
    }


def write_manifest(
    manifest: dict[str, Any],
    *,
    out_dir: Path,
    stamp: str | None = None,
    filename: str | None = None,
) -> Path:
    """Write *manifest* as a JSON file inside *out_dir*.

    Parameters
    ----------
    manifest:
        Dict returned by :func:`collect_manifest`.
    out_dir:
        Directory to write into (created if absent).
    stamp:
        UTC timestamp string embedded in the default filename
        (e.g. ``"20260101T120000Z"``).  Ignored when *filename* is given.
    filename:
        Override the auto-generated filename.

    Returns
    -------
    The :class:`~pathlib.Path` of the written file.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    if filename:
        path = out_dir / filename
    else:
        ts = stamp or datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        path = out_dir / f"repro_manifest_{ts}.json"
    path.write_text(json.dumps(manifest, indent=2, default=str) + "\n", encoding="utf-8")
    return path
