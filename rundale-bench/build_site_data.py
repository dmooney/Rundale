#!/usr/bin/env python3
"""Bridge committed eval artifacts into one JSON the static site consumes.

Walks `rundale-bench/artifacts/run_*.json` (dialogue quality) and
`docs/proofs/rundale-bench/perf/*.json` (per-provider perf) and writes
`rundale-bench/bench-site/src/data/bench.json`:

    { generated_utc, judge_model, suite, leaderboard[], perf[] }

Pure aggregation with injectable directories so it tests without a network.
Latest run/measurement wins per model / (model, provider).
"""
from __future__ import annotations

import glob
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent
ARTIFACTS_DIR = _BENCH_DIR / "artifacts"
PROOFS_RUNS_DIR = _REPO_ROOT / "docs" / "proofs" / "rundale-bench"
PERF_DIR = _REPO_ROOT / "docs" / "proofs" / "rundale-bench" / "perf"
DEMO_PROFILE_DIR = _REPO_ROOT / "docs" / "proofs" / "demo-api-profile"
SITE_DATA = _BENCH_DIR / "bench-site" / "src" / "data" / "bench.json"


def _run_paths(artifacts_dir: Path) -> list[str]:
    """Every `run_*.json` we know about — artifacts + the proofs mirror dir
    (some early runs were committed there, not under artifacts)."""
    paths = sorted(glob.glob(str(artifacts_dir / "run_*.json")))
    if PROOFS_RUNS_DIR.exists():
        paths.extend(sorted(glob.glob(str(PROOFS_RUNS_DIR / "run_*.json"))))
    return paths


_LOCAL_PREFIXES = ("mlx-community/", "ollama/", "lmstudio/", "local/")


def model_is_local(model_id: str, families: Optional[dict] = None) -> bool:
    """True for MLX/Ollama/LM Studio targets. Catalog `local_only` wins when
    the id is in the catalog; otherwise guess from common prefixes."""
    families = families or {}
    try:
        from catalog import load_catalog
        cat = load_catalog()
        for m in cat.models:
            if m.id == model_id or any(p.model_name_at_provider == model_id for p in m.providers):
                return m.local_only
    except Exception:
        pass
    return model_id.startswith(_LOCAL_PREFIXES)

AXES = ("character", "authenticity", "language", "responsiveness", "craft")
GAEILGE_AXES = ("fluency", "grammar", "idiom", "task_fulfillment", "english_leakage")
DATASET_SLICES = ("dialogue", "reaction", "tier2-sim", "tier3-sim", "gaeilge", "intent")

SLICE_PURPOSE = {
    "dialogue": (
        "First-person NPC dialogue in the voice of Brigid O'Brien — a 42-year-old "
        "midwife in 1820 rural Ireland. Probes period-accurate vocabulary, in-character "
        "voice, en-IE / ga-IE code-switching, and refusal of non-Latin script. The "
        "headline quality slice — drives the main leaderboard."
    ),
    "reaction": (
        "Short in-character one-liners NPCs emit in response to nearby game events "
        "(weather shifts, arrivals, gossip). Tests whether the model can stay in "
        "voice under tight token budgets without drifting into narration."
    ),
    "tier2-sim": (
        "Structured world-tick outputs. The model emits JSON describing NPC state "
        "updates (mood, goal, current action) given a scene. Tests schema compliance "
        "and plausible micro-simulation — the engine runs hundreds of these per game day."
    ),
    "tier3-sim": (
        "Deeper structured sim: multi-step NPC plans, conditional triggers, longer "
        "JSON. Same schema-validation + plausibility bar as tier2, but the model must "
        "compose several intents coherently."
    ),
    "gaeilge": (
        "Irish-language (Gaeilge) fluency. Eleven prompts in Irish probe natural syntax, "
        "idiom, grammar, task-fulfilment, and resistance to falling back to English. "
        "Decoupled from the dialogue slice so models that fake en-IE can't fake ga-IE."
    ),
    "tier2": (
        "(deprecated alias for tier2-sim — kept for older artifacts.)"
    ),
    "intent": (
        "Deterministic player-input parser. Maps natural-language input "
        "(\"go to the pub\", \"tell Mary I saw her cow\") to "
        "{intent: move|talk|look|interact|examine|unknown, target, dialogue}. "
        "Exact-match graded — no LLM judge, no axes; the only slice driven entirely "
        "by deterministic scoring."
    ),
}


def slugify(model_id: str) -> str:
    """Route-safe slug for a model id (ids contain '/' and ':')."""
    return re.sub(r"[^A-Za-z0-9]+", "-", model_id).strip("-").lower()


def _infer_provider(candidate_id: str) -> str:
    """Best-effort provider tag for legacy perf rows (no provider_id field).

    Legacy runs went through OpenRouter; vendor-prefixed ids (`anthropic/…`,
    `google/…`, etc.) signal OpenRouter routing. Bare local ids fall back to
    `legacy` so the row isn't silently dropped.
    """
    if "/" in candidate_id:
        return "openrouter"
    return "legacy"


def _family_lookup(suite: str) -> dict[str, str]:
    try:
        from catalog import load_catalog
        cat = load_catalog(version=suite)
        out: dict[str, str] = {}
        for m in cat.models:
            out[m.id] = m.family
            for p in m.providers:
                out[p.model_name_at_provider] = m.family  # match legacy target.model ids too
        return out
    except Exception:
        return {}


def _price_lookup(suite: str) -> dict[tuple[str, str], dict]:
    """Catalog input/output token prices keyed by logical and provider model id."""
    try:
        from catalog import load_catalog
        cat = load_catalog(version=suite)
    except Exception:
        return {}

    out: dict[tuple[str, str], dict] = {}
    for m in cat.models:
        for p in m.providers:
            price = {
                "price_input_usd_per_mtok": p.price_in_per_mtok,
                "price_output_usd_per_mtok": p.price_out_per_mtok,
                "price_source": f"rundale-bench/{suite}/models.toml",
            }
            out[(m.id, p.provider_id)] = price
            out[(p.model_name_at_provider, p.provider_id)] = price
    return out


def _latest_demo_profile_summary(profile_dir: Path) -> Optional[Path]:
    if not profile_dir.exists():
        return None
    paths = sorted(p for p in profile_dir.glob("*/summary.json") if p.is_file())
    return paths[-1] if paths else None


def _enrich_profile_bucket(bucket: dict, observed_minutes: float, *, included: bool) -> dict:
    requests = int(bucket.get("requests") or 0)
    input_tokens = int(bucket.get("input_tokens_estimated") or 0)
    output_tokens = int(bucket.get("output_tokens_estimated") or 0)
    return {
        **bucket,
        "included_in_gameplay_cost": included,
        "input_tokens_per_request_estimated": (input_tokens / requests) if requests else 0.0,
        "output_tokens_per_request_estimated": (output_tokens / requests) if requests else 0.0,
        "input_tokens_per_minute_estimated": (input_tokens / observed_minutes) if observed_minutes else 0.0,
        "output_tokens_per_minute_estimated": (output_tokens / observed_minutes) if observed_minutes else 0.0,
    }


def build_normal_play_profile(profile_dir: Path = DEMO_PROFILE_DIR) -> Optional[dict]:
    """Normal-play request profile from the latest committed demo profiling run.

    The profile intentionally uses total_gameplay, excluding the synthetic
    demo-player category, because cloud cost estimates should describe normal
    game inference calls rather than the local bot that supplied demo input.
    """
    summary_path = _latest_demo_profile_summary(profile_dir)
    if summary_path is None:
        return None
    try:
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None

    observed_seconds = float(summary.get("observed_seconds") or 0.0)
    observed_minutes = observed_seconds / 60.0 if observed_seconds else 0.0
    gameplay_categories = {"intent", "dialogue", "simulation", "reaction", "travel"}
    categories = {
        name: _enrich_profile_bucket(bucket, observed_minutes, included=name in gameplay_categories)
        for name, bucket in sorted((summary.get("categories") or {}).items())
    }
    total_gameplay = _enrich_profile_bucket(
        summary.get("total_gameplay") or {},
        observed_minutes,
        included=True,
    )
    total_observed = _enrich_profile_bucket(
        summary.get("total_observed") or {},
        observed_minutes,
        included=False,
    )
    try:
        source = str(summary_path.relative_to(_REPO_ROOT))
    except ValueError:
        source = str(summary_path)
    return {
        "source": source,
        "label": "Five-minute vLLM-MLX demo run with human-readable turn delay",
        "scope": "gameplay_excluding_demo_player",
        "observed_seconds": observed_seconds,
        "observed_minutes": observed_minutes,
        "categories": categories,
        "total_gameplay": total_gameplay,
        "total_observed": total_observed,
    }


def _estimate_gameplay_cost(profile: Optional[dict], input_price: float, output_price: float) -> Optional[dict]:
    if profile is None:
        return None
    total = profile.get("total_gameplay") or {}
    input_per_min = total.get("input_tokens_per_minute_estimated")
    output_per_min = total.get("output_tokens_per_minute_estimated")
    if not isinstance(input_per_min, (int, float)) or not isinstance(output_per_min, (int, float)):
        return None

    per_category: dict[str, float] = {}
    for name, cat in (profile.get("categories") or {}).items():
        if not cat.get("included_in_gameplay_cost"):
            continue
        cat_input = cat.get("input_tokens_per_minute_estimated")
        cat_output = cat.get("output_tokens_per_minute_estimated")
        if not isinstance(cat_input, (int, float)) or not isinstance(cat_output, (int, float)):
            continue
        per_category[name] = (cat_input * input_price + cat_output * output_price) / 1_000_000

    per_minute = (input_per_min * input_price + output_per_min * output_price) / 1_000_000
    return {
        "gameplay_cost_usd_per_minute": per_minute,
        "gameplay_cost_usd_per_hour": per_minute * 60.0,
        "gameplay_cost_by_category_usd_per_minute": per_category,
    }


def attach_gameplay_costs(perf: list[dict], profile: Optional[dict], prices: dict[tuple[str, str], dict]) -> None:
    """Mutate perf rows with catalog prices and normal-play cost estimates."""
    for row in perf:
        model_id = row.get("model_id")
        provider_id = row.get("provider_id")
        provider_model = row.get("model_name_at_provider")
        price = (
            prices.get((model_id, provider_id))
            or prices.get((provider_model, provider_id))
        )
        row["price_input_usd_per_mtok"] = None
        row["price_output_usd_per_mtok"] = None
        row["price_source"] = None
        row["gameplay_cost_usd_per_minute"] = None
        row["gameplay_cost_usd_per_hour"] = None
        row["gameplay_cost_by_category_usd_per_minute"] = None
        if price is None:
            continue
        row.update(price)
        cost = _estimate_gameplay_cost(
            profile,
            price["price_input_usd_per_mtok"],
            price["price_output_usd_per_mtok"],
        )
        if cost is not None:
            row.update(cost)


def build_cloud_cost_examples(profile: Optional[dict], suite: str = "v1") -> list[dict]:
    """Cost/min examples for all non-local catalog providers, independent of perf rows."""
    try:
        from catalog import load_catalog
        cat = load_catalog(version=suite)
    except Exception:
        return []

    rows: list[dict] = []
    for m in cat.models:
        if m.local_only:
            continue
        for p in m.providers:
            row = {
                "model_id": m.id,
                "display_name": m.display_name,
                "provider_id": p.provider_id,
                "model_name_at_provider": p.model_name_at_provider,
                "price_input_usd_per_mtok": p.price_in_per_mtok,
                "price_output_usd_per_mtok": p.price_out_per_mtok,
                "price_source": f"rundale-bench/{suite}/models.toml",
            }
            cost = _estimate_gameplay_cost(profile, p.price_in_per_mtok, p.price_out_per_mtok)
            if cost is not None:
                row.update(cost)
            else:
                row["gameplay_cost_usd_per_minute"] = None
                row["gameplay_cost_usd_per_hour"] = None
                row["gameplay_cost_by_category_usd_per_minute"] = None
            rows.append(row)

    def _sort_key(row: dict) -> tuple[float, str, str]:
        cost = row.get("gameplay_cost_usd_per_minute")
        return (
            cost if isinstance(cost, (int, float)) else float("inf"),
            row["model_id"],
            row["provider_id"],
        )

    return sorted(rows, key=_sort_key)


def _run_ts(out: dict, path: Path) -> str:
    return out.get("run_started_utc") or path.stem


def build_leaderboard(artifacts_dir: Path, families: Optional[dict] = None) -> list[dict]:
    families = families or {}
    latest: dict[str, tuple[str, dict]] = {}  # model_id -> (ts, row)
    for p in _run_paths(artifacts_dir):
        path = Path(p)
        try:
            out = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        dia = out.get("slices", {}).get("dialogue")
        if not dia or "summary" not in dia:
            continue
        s = dia["summary"]
        model_id = (out.get("candidate", {}) or {}).get("model_id") or out.get("target", {}).get("model")
        if not model_id:
            continue
        ts = _run_ts(out, path)
        row = {
            "model_id": model_id,
            "slug": slugify(model_id),
            "family": families.get(model_id, "unknown"),
            "tier": out.get("tier"),
            "overall": s.get("overall"),
            "judged": s.get("judged", s.get("records")),
            "records": s.get("records"),
            "non_latin_rate": s.get("non_latin_rate"),
            "judge_id": s.get("judge") or s.get("judge_id") or "judge_v1 (qwen3-235b)",
            "judge_model": s.get("judge_model") or ("qwen/qwen3-235b-a22b-2507" if not s.get("judge") else None),
            **{a: s.get(a) for a in AXES},
            "measured_utc": ts,
        }
        if model_id not in latest or ts > latest[model_id][0]:
            latest[model_id] = (ts, row)
    return [row for _, row in sorted(latest.values(), key=lambda kv: -(kv[1].get("overall") or 0))]


def build_perf(perf_dir: Path, legacy_dir: Optional[Path] = None) -> list[dict]:
    """Per-(model, provider) perf row, latest per pair wins.

    Reads Phase 3 schema from `perf_dir` and the legacy multi-target schema
    from `legacy_dir` (per_target keyed by candidate, ttft + total_ms +
    tokens_per_second). Legacy rows are inferred to provider `openrouter` for
    vendor-prefixed candidates.
    """
    latest: dict[tuple[str, str], tuple[str, dict]] = {}

    # Phase 3 perf JSONs
    for p in sorted(glob.glob(str(perf_dir / "perf_*.json"))):
        try:
            row = json.loads(Path(p).read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        key = (row.get("model_id"), row.get("provider_id"))
        if None in key:
            continue
        ts = row.get("measured_utc", "")
        if key not in latest or ts > latest[key][0]:
            latest[key] = (ts, {**row, "slug": slugify(row["model_id"]), "source": "phase3"})

    # Legacy multi-target perf JSONs (one file holds many per_target entries)
    if legacy_dir and legacy_dir.exists():
        for p in sorted(glob.glob(str(legacy_dir / "perf_*.json"))):
            try:
                bundle = json.loads(Path(p).read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                continue
            ts = bundle.get("ran_at_utc", "")
            for cand, stats in (bundle.get("per_target") or {}).items():
                pid = _infer_provider(cand)
                key = (cand, pid)
                n_streamed = stats.get("n_streamed", 0) or 0
                n_ok = stats.get("n_ok", 0) or 0
                row = {
                    "model_id": cand,
                    "slug": slugify(cand),
                    "provider_id": pid,
                    "model_name_at_provider": cand,
                    "n_ok": n_ok,
                    "n_error": max(0, n_streamed - n_ok),
                    "error_rate": ((n_streamed - n_ok) / n_streamed) if n_streamed else 0.0,
                    "latency_p50_ms": stats.get("total_ms_median"),
                    "latency_p95_ms": stats.get("total_ms_p90"),  # legacy p90 — best available
                    "tokens_per_sec_mean": stats.get("tokens_per_second_median"),
                    "usd_per_mtok_observed": None,  # legacy lacks usage rollup
                    "measured_utc": ts,
                    "ttft_p50_ms": stats.get("ttft_ms_median"),
                    "ttft_p90_ms": stats.get("ttft_ms_p90"),
                    "source": "legacy",
                }
                if key not in latest or ts > latest[key][0]:
                    latest[key] = (ts, row)

    return [row for _, row in sorted(latest.values(), key=lambda kv: (kv[1]["model_id"], kv[1]["provider_id"]))]


def build_gaeilge(artifacts_dir: Path, families: Optional[dict] = None) -> list[dict]:
    """Gaeilge leaderboard: per model, axes + leakage. Latest wins."""
    families = families or {}
    latest: dict[str, tuple[str, dict]] = {}
    for p in _run_paths(artifacts_dir):
        path = Path(p)
        try:
            out = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        ga = out.get("slices", {}).get("gaeilge")
        if not ga or "summary" not in ga:
            continue
        s = ga["summary"]
        model_id = (out.get("candidate", {}) or {}).get("model_id") or out.get("target", {}).get("model")
        if not model_id:
            continue
        ts = _run_ts(out, path)
        row = {
            "model_id": model_id,
            "slug": slugify(model_id),
            "family": families.get(model_id, "unknown"),
            "overall": s.get("overall_mean"),
            "records": s.get("records"),
            "errors": s.get("errors"),
            "english_leakage_flag_rate": s.get("english_leakage_flag_rate"),
            **{a: s.get(f"{a}_mean") for a in GAEILGE_AXES},
            "measured_utc": ts,
        }
        if model_id not in latest or ts > latest[model_id][0]:
            latest[model_id] = (ts, row)
    return [row for _, row in sorted(latest.values(), key=lambda kv: -(kv[1].get("overall") or 0))]


_SLICE_AXES = {
    "dialogue": AXES,
    "gaeilge": GAEILGE_AXES,
}

# Slice-specific default judge ids when legacy runs don't record one.
_LEGACY_JUDGE = {
    "dialogue": ("judge_v1 (qwen3-235b)", "qwen/qwen3-235b-a22b-2507"),
    "gaeilge": ("gaeilge_fluency_judge_v1", "claude-sonnet-4-6"),
}


def build_samples(artifacts_dir: Path, datasets: dict) -> dict:
    """Per-model per-slice samples: slug -> {model_id, dialogue?, gaeilge?, ...}.

    Each slice block carries `judge_id`, `judge_model`, `measured_utc`, `axes`
    (list of axis names for the slice), and `items` of `{id, prompt, reply,
    axes:{...}, overall, reason?, extras...}`. Latest run per (model, slice)
    wins.
    """
    prompt_lookup: dict[str, dict[str, str]] = {}
    for slice_name in _SLICE_AXES:
        prompt_lookup[slice_name] = {
            rec["id"]: rec.get("prompt", "")
            for rec in (datasets.get(slice_name) or {}).get("records", [])
        }

    by_model: dict[str, dict] = {}
    latest_ts: dict[tuple[str, str], str] = {}

    for p in _run_paths(artifacts_dir):
        path = Path(p)
        try:
            out = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        model_id = (out.get("candidate", {}) or {}).get("model_id") or out.get("target", {}).get("model")
        if not model_id:
            continue
        ts = _run_ts(out, path)
        slug = slugify(model_id)

        for slice_name, axis_names in _SLICE_AXES.items():
            block = out.get("slices", {}).get(slice_name)
            if not block or "results" not in block:
                continue
            key = (slug, slice_name)
            if key in latest_ts and ts <= latest_ts[key]:
                continue
            latest_ts[key] = ts

            items = []
            for r in block.get("results", []):
                if r.get("error") or r.get("reply") is None:
                    continue
                inline = {a: r.get(a) for a in axis_names if isinstance(r.get(a), (int, float))}
                j = r.get("judgment") or {}
                axes_out = j["axes"] if isinstance(j.get("axes"), dict) else inline
                overall = r.get("overall") if "overall" in r else j.get("overall")
                entry = {
                    "id": r["id"],
                    "prompt": prompt_lookup[slice_name].get(r["id"], ""),
                    "reply": r["reply"],
                    "axes": axes_out,
                    "overall": overall,
                }
                if r.get("reason"):
                    entry["reason"] = r["reason"]
                if r.get("english_leakage_examples"):
                    entry["english_leakage_examples"] = r["english_leakage_examples"]
                if r.get("non_latin_chars"):
                    entry["non_latin_chars"] = r["non_latin_chars"]
                items.append(entry)

            summary = block.get("summary", {})
            legacy_id, legacy_model = _LEGACY_JUDGE.get(slice_name, ("unknown", None))
            slice_data = {
                "judge_id": summary.get("judge") or legacy_id,
                "judge_model": summary.get("judge_model") or legacy_model,
                "measured_utc": ts,
                "axes": list(axis_names),
                "items": items,
            }
            by_model.setdefault(slug, {"model_id": model_id, "slug": slug})[slice_name] = slice_data

    return by_model


JUDGE_LABELS = {
    "judge_sonnet_v1": ("dialogue", "Dialogue (Sonnet-judged)"),
    "judge_v1": ("dialogue (legacy)", "Dialogue — legacy OpenRouter judge"),
    "gaeilge_fluency_judge_v1": ("gaeilge", "Irish (Gaeilge) fluency"),
    "judge_reaction_v1": ("reaction", "NPC reactions"),
    "judge_sim_v1": ("tier2-sim / tier3-sim", "Structured world-tick simulation"),
    "judge_pairwise_v1": ("dialogue (ELO)", "Pairwise dialogue (ELO mode)"),
}


def build_judge_prompts(suite: str = "v1") -> dict:
    """Verbatim judge prompts + rubric configs so the site shows exactly what
    each subagent saw. A sibling `<judge_id>.system.md` (if present) is the
    preamble + rubric envelope; otherwise the json's `rubric` field IS the
    full prompt the judge received."""
    out: dict[str, dict] = {}
    for cfg_path in sorted((_BENCH_DIR / suite).glob("judge_*.json")):
        cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
        judge_id = cfg.get("judge_id") or cfg_path.stem
        sys_path = _BENCH_DIR / suite / f"{judge_id}.system.md"
        slice_for, label = JUDGE_LABELS.get(judge_id, ("(unknown)", judge_id))
        out[judge_id] = {
            "judge_id": judge_id,
            "label": label,
            "slice": slice_for,
            "model": cfg.get("model"),
            "base_url": cfg.get("base_url"),
            "rubric_sha256": cfg.get("rubric_sha256"),
            "axes": cfg.get("axes"),
            "system_prompt": sys_path.read_text(encoding="utf-8") if sys_path.exists() else cfg.get("rubric", ""),
            "system_prompt_source": str(sys_path.relative_to(_REPO_ROOT)) if sys_path.exists()
                else f"{cfg_path.relative_to(_REPO_ROOT)} (rubric field)",
            "rubric_text": cfg.get("rubric", ""),
        }
    return out


def build_datasets(suite: str = "v1") -> dict:
    """Browseable dev-split datasets — counts + every record.

    Holdout is sealed (model-pick defense); only dev records are exposed. Each
    slice contributes `{count, records: [{id, prompt, ...}]}` keyed by slice
    name. Total size on the dev set is ~120 KB across all slices — small
    enough to ship inside `bench.json` and render statically.
    """
    out: dict[str, dict] = {}
    for slice_name in DATASET_SLICES:
        path = _BENCH_DIR / suite / f"{slice_name}.jsonl"
        if not path.exists():
            continue
        records = []
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                records.append(json.loads(line))
            except json.JSONDecodeError:
                continue
        out[slice_name] = {"count": len(records), "records": records}
    return out


def build_models_index(leaderboard: list[dict], gaeilge: list[dict], perf: list[dict],
                       samples: dict) -> list[dict]:
    """One row per observed model: cloud/local + best dialogue + best gaeilge
    + perf summary (best p50 latency, mean tok/s, cheapest $/Mtok). Drives
    /models and lets /perf rows link to a page that always exists."""
    by_slug: dict[str, dict] = {}

    def _ensure(slug: str, model_id: str) -> dict:
        if slug not in by_slug:
            by_slug[slug] = {
                "slug": slug, "model_id": model_id,
                "is_local": model_is_local(model_id),
                "dialogue_overall": None, "gaeilge_overall": None,
                "perf_providers": [], "perf_best_p50_ms": None,
                "perf_best_usd_per_mtok": None, "perf_best_gameplay_usd_per_minute": None,
                "perf_best_gameplay_usd_per_hour": None, "perf_mean_tok_s": None,
            }
        return by_slug[slug]

    for r in leaderboard:
        e = _ensure(r["slug"], r["model_id"])
        e["dialogue_overall"] = r.get("overall")
        e["dialogue_judge"] = r.get("judge_id")
    for r in gaeilge:
        e = _ensure(r["slug"], r["model_id"])
        e["gaeilge_overall"] = r.get("overall")
    for s in samples.values():
        _ensure(s["slug"], s["model_id"])
    for r in perf:
        e = _ensure(r["slug"], r["model_id"])
        e["perf_providers"].append(r["provider_id"])
        p50 = r.get("latency_p50_ms")
        if isinstance(p50, (int, float)):
            cur = e["perf_best_p50_ms"]
            e["perf_best_p50_ms"] = p50 if cur is None else min(cur, p50)
        usd = r.get("usd_per_mtok_observed")
        if isinstance(usd, (int, float)):
            cur = e["perf_best_usd_per_mtok"]
            e["perf_best_usd_per_mtok"] = usd if cur is None else min(cur, usd)
        gameplay_per_minute = r.get("gameplay_cost_usd_per_minute")
        if isinstance(gameplay_per_minute, (int, float)):
            cur = e["perf_best_gameplay_usd_per_minute"]
            best = gameplay_per_minute if cur is None else min(cur, gameplay_per_minute)
            e["perf_best_gameplay_usd_per_minute"] = best
            e["perf_best_gameplay_usd_per_hour"] = best * 60.0
        ts = r.get("tokens_per_sec_mean")
        if isinstance(ts, (int, float)):
            cur = e["perf_mean_tok_s"]
            e["perf_mean_tok_s"] = ts if cur is None else max(cur, ts)
    # Score rows so models with more data float to the top.
    def _score(e):
        return (
            -(1 if e["dialogue_overall"] is not None else 0),
            -(e["dialogue_overall"] or 0),
            -(1 if e["gaeilge_overall"] is not None else 0),
            -(e["gaeilge_overall"] or 0),
            -len(e["perf_providers"]),
            e["model_id"],
        )
    return sorted(by_slug.values(), key=_score)


def build_data(artifacts_dir: Path = ARTIFACTS_DIR, perf_dir: Path = PERF_DIR,
               profile_dir: Path = DEMO_PROFILE_DIR,
               *, suite: str = "v1", judge_model: str = "claude-sonnet-4-6") -> dict:
    families = _family_lookup(suite)
    datasets = build_datasets(suite)
    datasets_with_purpose = {
        name: {**info, "purpose": SLICE_PURPOSE.get(name, "")} for name, info in datasets.items()
    }
    leaderboard = build_leaderboard(artifacts_dir, families)
    gaeilge = build_gaeilge(artifacts_dir, families)
    perf = build_perf(perf_dir, legacy_dir=artifacts_dir)
    samples = build_samples(artifacts_dir, datasets)
    normal_play_profile = build_normal_play_profile(profile_dir)
    attach_gameplay_costs(perf, normal_play_profile, _price_lookup(suite))
    cloud_cost_examples = build_cloud_cost_examples(normal_play_profile, suite)
    # Stamp is_local on every row so the site doesn't have to recompute it.
    for r in leaderboard: r["is_local"] = model_is_local(r["model_id"])
    for r in gaeilge: r["is_local"] = model_is_local(r["model_id"])
    for r in perf: r["is_local"] = model_is_local(r["model_id"])
    return {
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "judge_model": judge_model,
        "suite": suite,
        "leaderboard": leaderboard,
        "gaeilge": gaeilge,
        "perf": perf,
        "normal_play_profile": normal_play_profile,
        "cloud_cost_examples": cloud_cost_examples,
        "datasets": datasets_with_purpose,
        "samples": samples,
        "judge_prompts": build_judge_prompts(suite),
        "models_index": build_models_index(leaderboard, gaeilge, perf, samples),
    }


def main() -> None:
    data = build_data()
    SITE_DATA.parent.mkdir(parents=True, exist_ok=True)
    SITE_DATA.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {SITE_DATA} — {len(data['leaderboard'])} leaderboard row(s), {len(data['perf'])} perf row(s)")


if __name__ == "__main__":
    main()
