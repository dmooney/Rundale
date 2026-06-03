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


def model_is_local(model_id: str, families: dict | None = None) -> bool:
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
    "tier2": ("(deprecated alias for tier2-sim — kept for older artifacts.)"),
    "intent": (
        "Deterministic player-input parser. Maps natural-language input "
        '("go to the pub", "tell Mary I saw her cow") to '
        "{intent: move|talk|look|interact|examine|unknown, target, dialogue}. "
        "Exact-match graded — no LLM judge, no axes; the only slice driven entirely "
        "by deterministic scoring."
    ),
}


def slugify(model_id: str) -> str:
    """Route-safe slug for a model id (ids contain '/' and ':')."""
    return re.sub(r"[^A-Za-z0-9]+", "-", model_id).strip("-").lower()


# Quant suffixes the local runner appends to the HuggingFace repo basename.
# Order matters: longer compound tags (`qat-4bit`, `optiq-4bit`, `dwq-4bit`,
# `mxfp4-q8`) must match before the bare quant fragments.
_LOCAL_QUANT_TAGS = (
    "qat-4bit",
    "optiq-4bit",
    "dwq-4bit",
    "mxfp4-q8",
    "mxfp4",
    "nvfp4",
    "bf16",
    "4bit",
    "5bit",
    "6bit",
    "8bit",
)


def _local_quant_label(quant_token: str) -> str:
    """Human-readable variant of a quant token (e.g. `qat-4bit` → `QAT 4-bit`)."""
    if not quant_token:
        return ""
    pretty = (
        quant_token.replace("optiq-4bit", "OptiQ 4-bit")
        .replace("qat-4bit", "QAT 4-bit")
        .replace("dwq-4bit", "DWQ 4-bit")
        .replace("mxfp4-q8", "MXFP4 Q8")
        .replace("mxfp4", "MXFP4")
        .replace("nvfp4", "NVFP4")
        .replace("bf16", "bf16")
    )
    if pretty == quant_token:
        # Bare `<n>bit` → `<n>-bit`.
        m = re.fullmatch(r"(\d+)bit", quant_token)
        if m:
            pretty = f"{m.group(1)}-bit"
    return pretty


def _strip_quant_suffix(basename: str) -> tuple[str, str]:
    """Return `(stem_without_quant, quant_token)` for an mlx-community repo
    basename. `quant_token` is `""` when no recognised quant suffix is
    present."""
    lower = basename.lower()
    for tag in _LOCAL_QUANT_TAGS:
        if lower.endswith("-" + tag):
            return basename[: -(len(tag) + 1)], tag
    return basename, ""


def enrich_local_row(model_id: str, catalog_family: str) -> tuple[str, str | None]:
    """Compute `(family, display_name)` for any row, layering local
    metadata over the catalog's value when the row is a local target.

    - Cloud targets pass through unchanged with `display_name=None` so
      downstream code keeps falling back to `model_id`.
    - Local targets pick up the heuristic `family` from
      `derive_local_metadata` (only if the catalog returned "unknown"),
      plus a human-readable `display_name`.
    """
    if not model_id or not model_is_local(model_id):
        return catalog_family, None
    meta = derive_local_metadata(model_id)
    family = catalog_family if catalog_family and catalog_family != "unknown" else meta["family"]
    return family, meta["display_name"]


def derive_local_metadata(model_id: str) -> dict:
    """Best-effort `family`, `display_name`, and `vendor_prefix` for a local
    repo id like `mlx-community/Qwen2.5-14B-Instruct-4bit`.

    Heuristic only — no catalog lookup, so unknown lineages return
    `family='unknown'` (which Brand.svelte will render as a colored
    initials chip rather than a logo). Used to label MLX rows in the
    bench-site so they sit alongside cloud rows with a logo + proper
    name instead of the raw HuggingFace slug.

    Returns ``{family, display_name, vendor_prefix}``. `vendor_prefix` is
    the leading repo namespace (`mlx-community`, `lmstudio`, …) the
    Brand component can use as a tertiary badge.
    """
    if "/" not in model_id:
        return {"family": "unknown", "display_name": model_id, "vendor_prefix": ""}
    vendor_prefix, _, basename = model_id.partition("/")
    stem, quant = _strip_quant_suffix(basename)
    quant_label = _local_quant_label(quant)

    # Family inference — keyed on the lower-cased stem so additions to
    # FAMILY_TO_SLUG (in lib/brands.ts) map automatically.
    lower = stem.lower()
    family = "unknown"
    if lower.startswith("qwen3.6"):
        family = "qwen3.6"
    elif lower.startswith("qwen3.5"):
        family = "qwen3.5"
    elif lower.startswith("qwen3-") or lower == "qwen3" or lower.startswith("qwen3-coder"):
        family = "qwen3"
    elif lower.startswith("qwen2.5"):
        family = "qwen2.5"
    elif lower.startswith("gemma-4") or lower.startswith("gemma-3"):
        family = "gemma"
    elif lower.startswith("llama-4") or lower.startswith("llama-3"):
        family = "llama"
    elif (
        lower.startswith("mistral") or lower.startswith("ministral") or lower.startswith("devstral")
    ):
        family = "mistral"
    elif lower.startswith("deepseek"):
        family = "deepseek-flash" if "flash" in lower else "deepseek"
    elif lower.startswith("phi-"):
        family = "phi"
    elif lower.startswith("glm-"):
        family = "glm"
    elif lower.startswith("lfm"):
        family = "liquid"
    elif lower.startswith("minimax"):
        family = "minimax-m2.5"
    elif lower.startswith("eurollm"):
        family = "eurollm"

    # Display name: split on dashes, drop noise tokens like "mlx" /
    # date-code "2512" (the quant tag already announces it's an MLX
    # build, and the dated tag is uploader scaffolding), pretty-print
    # each token (parameter counts like "70b" → "70B", MoE tags like
    # "A3B" stay intact, brand-name tokens like "DeepSeek" keep their
    # casing), then expand glued version numbers (Qwen2.5 → Qwen 2.5)
    # **only** on the first token so MoE tags ("A3B") and DeepSeek
    # versioning ("V4") don't get over-split.
    _NOISE_TOKENS = {"mlx"}

    def _pretty_token(t: str) -> str:
        if not t or t.lower() in _NOISE_TOKENS:
            return ""
        # Parameter-count tag: "70b" / "27B" → "70B".
        m = re.fullmatch(r"(\d+(?:\.\d+)?)([bBmM])", t)
        if m:
            return m.group(1) + m.group(2).upper()
        # MoE active-param tag (A3B, A4B, A22B): keep verbatim.
        if re.fullmatch(r"A\d+B", t, re.IGNORECASE):
            return t.upper()
        # All-caps tags (MXFP4, NVFP4, QAT, MoE letters) stay as-is.
        if t.isupper():
            return t
        # Mixed-case brand tokens like "DeepSeek", "EuroLLM", "Qwen2.5"
        # keep their author casing.
        if any(c.isupper() for c in t) and any(c.islower() for c in t):
            return t
        return t[:1].upper() + t[1:].lower()

    tokens = [_pretty_token(t) for t in stem.split("-")]
    tokens = [t for t in tokens if t]
    if tokens:
        # Only expand glued version numbers on the leading brand token
        # ("Qwen2.5" → "Qwen 2.5", "Llama3" → "Llama 3"). Trailing
        # tokens like "A3B" or "V4" stay glued.
        tokens[0] = re.sub(r"([A-Za-z]+)(\d)", r"\1 \2", tokens[0])
    pretty_stem = " ".join(tokens).strip()
    display_name = f"{pretty_stem} (MLX {quant_label})" if quant_label else pretty_stem
    return {"family": family, "display_name": display_name, "vendor_prefix": vendor_prefix}


def _infer_provider(candidate_id: str, model_to_provider: dict | None = None) -> str:
    """Best-effort provider tag for legacy perf rows (no provider_id field).

    Resolution order:
      1. catalog lookup — `rundale-bench/v1/models.toml` knows which
         provider hosts each id.
      2. vendor-prefixed ids (`anthropic/…`, `google/…`) → openrouter
         (legacy OpenRouter runs used this naming).
      3. bare ids → `legacy` so the row isn't silently dropped.
    """
    if model_to_provider and candidate_id in model_to_provider:
        return model_to_provider[candidate_id]
    if "/" in candidate_id:
        return "openrouter"
    return "legacy"


def _provider_lookup(suite: str) -> dict[str, str]:
    """model_id (and provider model_name) → provider_id from the catalog.

    Prefers the first provider listed for each catalog model. Lets
    `_infer_provider` route bare ids like `kimi-k2.5` to `opencode-go`
    instead of the `legacy` fallback.
    """
    try:
        from catalog import load_catalog

        cat = load_catalog(version=suite)
        out: dict[str, str] = {}
        for m in cat.models:
            if not m.providers:
                continue
            pid = m.providers[0].provider_id
            out[m.id] = pid
            for p in m.providers:
                out.setdefault(p.model_name_at_provider, p.provider_id)
        return out
    except Exception:
        return {}


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


def _latest_demo_profile_summary(profile_dir: Path) -> Path | None:
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
        "input_tokens_per_minute_estimated": (input_tokens / observed_minutes)
        if observed_minutes
        else 0.0,
        "output_tokens_per_minute_estimated": (output_tokens / observed_minutes)
        if observed_minutes
        else 0.0,
    }


def build_normal_play_profile(profile_dir: Path = DEMO_PROFILE_DIR) -> dict | None:
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


def _estimate_gameplay_cost(
    profile: dict | None, input_price: float, output_price: float
) -> dict | None:
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


def attach_gameplay_costs(
    perf: list[dict], profile: dict | None, prices: dict[tuple[str, str], dict]
) -> None:
    """Mutate perf rows with catalog prices and normal-play cost estimates."""
    for row in perf:
        model_id = row.get("model_id")
        provider_id = row.get("provider_id")
        provider_model = row.get("model_name_at_provider")
        # Keys are (str, str); only look up when both halves are present.
        price = None
        if model_id is not None and provider_id is not None:
            price = prices.get((model_id, provider_id))
        if price is None and provider_model is not None and provider_id is not None:
            price = prices.get((provider_model, provider_id))
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


def build_cloud_cost_examples(profile: dict | None, suite: str = "v1") -> list[dict]:
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
                "family": m.family or "unknown",
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


_LEADERBOARD_LOCAL_ROW = re.compile(
    r"^\|\s*(?P<date>\d{8}T\d{6}Z)\s*\|\s*(?P<repo>mlx-community/[^\s|]+)\s*\|"
    r"\s*(?P<slot>tiny|large)\s*\|\s*(?P<quant>[^\s|]+)\s*\|"
    r"\s*(?P<params>[\d.()A-Za-z ]+?)\s*\|\s*(?P<ram>[\d.]+)\s*\|"
    r"\s*(?P<slice>[a-z\d-]+)\s*\|"
)


def _build_peak_ram_index(artifacts_dir: Path) -> dict[str, float]:
    """Map model_id -> highest observed peak_ram_gb.

    Two sources, both consulted (max wins so the worst-case budget is
    surfaced):
      1. `local_*.json` per-sweep summaries (round 4+ only — earlier sweeps
         didn't persist this file shape).
      2. The "Local MLX sweeps" table in `artifacts/leaderboard.md`
         (every round since round 1 wrote rows there via
         `local_runner.append_leaderboard_row`).
    Cloud rows appear in neither source → return None and the UI shows
    a dash.
    """
    by_model: dict[str, float] = {}

    def _record(mid: str, ram_gb: float) -> None:
        cur = by_model.get(mid, 0.0)
        if ram_gb > cur:
            by_model[mid] = ram_gb

    # Source 1: per-sweep JSONs
    for p in sorted(artifacts_dir.glob("local_*.json")):
        try:
            d = json.loads(p.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        for row in d.get("rows", []) or []:
            mid = row.get("hf_repo")
            ram = row.get("peak_ram_gb")
            if mid and ram is not None:
                _record(mid, float(ram))

    # Source 2: `artifacts/local_leaderboard.md` (the per-sweep markdown
    # that local_runner.append_leaderboard_row writes — separate from the
    # main `leaderboard.md` page).
    lb_path = artifacts_dir / "local_leaderboard.md"
    if lb_path.exists():
        for line in lb_path.read_text(encoding="utf-8").splitlines():
            m = _LEADERBOARD_LOCAL_ROW.match(line.strip())
            if m:
                _record(m.group("repo"), float(m.group("ram")))

    return by_model


def _build_peak_ram_est_index() -> dict[str, float]:
    """Map model_id -> declared peak_ram_gb_est from candidates_local_mlx.toml.

    Estimates only — used as a fallback for rows where no live-measured
    peak_ram_gb is available (rounds 1-3 didn't persist that data).
    The UI distinguishes estimated values via the `peak_ram_is_estimate`
    flag attached alongside the value.
    """
    try:
        import tomllib  # py311+
    except ImportError:  # pragma: no cover — runtime is py311
        return {}
    candidates_toml = Path(__file__).parent / "candidates_local_mlx.toml"
    if not candidates_toml.exists():
        return {}
    data = tomllib.loads(candidates_toml.read_text(encoding="utf-8"))
    out: dict[str, float] = {}
    for c in data.get("candidate", []):
        repo = c.get("hf_repo")
        est = c.get("peak_ram_gb_est")
        if repo and est is not None:
            out[repo] = float(est)
    return out


def build_leaderboard(artifacts_dir: Path, families: dict | None = None) -> list[dict]:
    families = families or {}
    peak_ram_by_model = _build_peak_ram_index(artifacts_dir)
    peak_ram_est_by_model = _build_peak_ram_est_index()
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
        model_id = (out.get("candidate", {}) or {}).get("model_id") or out.get("target", {}).get(
            "model"
        )
        if not model_id:
            continue
        ts = _run_ts(out, path)
        fam, display_name = enrich_local_row(model_id, families.get(model_id, "unknown"))
        measured_ram = peak_ram_by_model.get(model_id)
        if measured_ram is not None:
            ram_value, ram_is_est = measured_ram, False
        elif model_id in peak_ram_est_by_model:
            ram_value, ram_is_est = peak_ram_est_by_model[model_id], True
        else:
            ram_value, ram_is_est = None, False
        row = {
            "model_id": model_id,
            "display_name": display_name,
            "slug": slugify(model_id),
            "family": fam,
            "tier": out.get("tier"),
            "overall": s.get("overall"),
            "judged": s.get("judged", s.get("records")),
            "bench_bugs": s.get("bench_bugs", 0),
            "records": s.get("records"),
            "non_latin_rate": s.get("non_latin_rate"),
            # Sonnet-subagent is now the only allowed judge (see
            # rundale_bench.load_judge). The legacy "qwen3-235b" fallback
            # was stripped 2026-05-28 — fall back to "claude-sonnet-4-6"
            # since that is the only judge that can produce a score now.
            "judge_id": s.get("judge") or s.get("judge_id") or "judge_sonnet_v1",
            "judge_model": s.get("judge_model") or "claude-sonnet-4-6",
            "peak_ram_gb": ram_value,
            "peak_ram_is_estimate": ram_is_est,
            **{a: s.get(a) for a in AXES},
            "measured_utc": ts,
        }
        if model_id not in latest or ts > latest[model_id][0]:
            latest[model_id] = (ts, row)
    return [row for _, row in sorted(latest.values(), key=lambda kv: -(kv[1].get("overall") or 0))]


def build_perf(
    perf_dir: Path,
    legacy_dir: Path | None = None,
    families: dict | None = None,
    providers: dict | None = None,
) -> list[dict]:
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
            fam, display_name = enrich_local_row(
                row["model_id"], (families or {}).get(row["model_id"], "unknown")
            )
            latest[key] = (
                ts,
                {
                    **row,
                    "slug": slugify(row["model_id"]),
                    "family": fam,
                    "display_name": display_name,
                    "source": "phase3",
                },
            )

    # Legacy multi-target perf JSONs (one file holds many per_target entries)
    if legacy_dir and legacy_dir.exists():
        for p in sorted(glob.glob(str(legacy_dir / "perf_*.json"))):
            try:
                bundle = json.loads(Path(p).read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                continue
            ts = bundle.get("ran_at_utc", "")
            for cand, stats in (bundle.get("per_target") or {}).items():
                pid = _infer_provider(cand, providers)
                key = (cand, pid)
                n_streamed = stats.get("n_streamed", 0) or 0
                n_ok = stats.get("n_ok", 0) or 0
                fam, display_name = enrich_local_row(cand, (families or {}).get(cand, "unknown"))
                row = {
                    "model_id": cand,
                    "display_name": display_name,
                    "slug": slugify(cand),
                    "family": fam,
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

    return [
        row
        for _, row in sorted(
            latest.values(), key=lambda kv: (kv[1]["model_id"], kv[1]["provider_id"])
        )
    ]


def build_gaeilge(artifacts_dir: Path, families: dict | None = None) -> list[dict]:
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
        model_id = (out.get("candidate", {}) or {}).get("model_id") or out.get("target", {}).get(
            "model"
        )
        if not model_id:
            continue
        ts = _run_ts(out, path)
        fam, display_name = enrich_local_row(model_id, families.get(model_id, "unknown"))
        row = {
            "model_id": model_id,
            "display_name": display_name,
            "slug": slugify(model_id),
            "family": fam,
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
        model_id = (out.get("candidate", {}) or {}).get("model_id") or out.get("target", {}).get(
            "model"
        )
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
            "system_prompt": sys_path.read_text(encoding="utf-8")
            if sys_path.exists()
            else cfg.get("rubric", ""),
            "system_prompt_source": str(sys_path.relative_to(_REPO_ROOT))
            if sys_path.exists()
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


def build_models_index(
    leaderboard: list[dict], gaeilge: list[dict], perf: list[dict], samples: dict
) -> list[dict]:
    """One row per observed model: cloud/local + best dialogue + best gaeilge
    + perf summary (best p50 latency, mean tok/s, cheapest $/Mtok). Drives
    /models and lets /perf rows link to a page that always exists."""
    by_slug: dict[str, dict] = {}

    def _ensure(slug: str, model_id: str) -> dict:
        if slug not in by_slug:
            # Local rows pick up display_name + heuristic family even when
            # the catalog doesn't carry the repo.
            _, display_name = enrich_local_row(model_id, "unknown")
            by_slug[slug] = {
                "slug": slug,
                "model_id": model_id,
                "display_name": display_name,
                "family": "unknown",
                "is_local": model_is_local(model_id),
                "dialogue_overall": None,
                "gaeilge_overall": None,
                "perf_providers": [],
                "perf_best_p50_ms": None,
                "perf_best_usd_per_mtok": None,
                "perf_best_gameplay_usd_per_minute": None,
                "perf_best_gameplay_usd_per_hour": None,
                "perf_mean_tok_s": None,
            }
        return by_slug[slug]

    def _adopt_family(e: dict, candidate: str | None) -> None:
        if candidate and candidate != "unknown" and e["family"] == "unknown":
            e["family"] = candidate

    for r in leaderboard:
        e = _ensure(r["slug"], r["model_id"])
        e["dialogue_overall"] = r.get("overall")
        e["dialogue_judge"] = r.get("judge_id")
        _adopt_family(e, r.get("family"))
    for r in gaeilge:
        e = _ensure(r["slug"], r["model_id"])
        e["gaeilge_overall"] = r.get("overall")
        _adopt_family(e, r.get("family"))
    for s in samples.values():
        _ensure(s["slug"], s["model_id"])
    for r in perf:
        e = _ensure(r["slug"], r["model_id"])
        e["perf_providers"].append(r["provider_id"])
        _adopt_family(e, r.get("family"))
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


def build_data(
    artifacts_dir: Path = ARTIFACTS_DIR,
    perf_dir: Path = PERF_DIR,
    profile_dir: Path = DEMO_PROFILE_DIR,
    *,
    suite: str = "v1",
    judge_model: str = "claude-sonnet-4-6",
) -> dict:
    families = _family_lookup(suite)
    providers = _provider_lookup(suite)
    datasets = build_datasets(suite)
    datasets_with_purpose = {
        name: {**info, "purpose": SLICE_PURPOSE.get(name, "")} for name, info in datasets.items()
    }
    leaderboard = build_leaderboard(artifacts_dir, families)
    gaeilge = build_gaeilge(artifacts_dir, families)
    perf = build_perf(perf_dir, legacy_dir=artifacts_dir, families=families, providers=providers)
    samples = build_samples(artifacts_dir, datasets)
    normal_play_profile = build_normal_play_profile(profile_dir)
    attach_gameplay_costs(perf, normal_play_profile, _price_lookup(suite))
    cloud_cost_examples = build_cloud_cost_examples(normal_play_profile, suite)
    # Stamp is_local on every row so the site doesn't have to recompute it.
    for r in leaderboard:
        r["is_local"] = model_is_local(r["model_id"])
    for r in gaeilge:
        r["is_local"] = model_is_local(r["model_id"])
    for r in perf:
        r["is_local"] = model_is_local(r["model_id"])
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
    print(
        f"wrote {SITE_DATA} — {len(data['leaderboard'])} leaderboard row(s), {len(data['perf'])} perf row(s)"
    )


if __name__ == "__main__":
    main()
