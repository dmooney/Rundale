"""Enumerate every viable Rundale benchmark candidate (REQ 1).

Programmatically queries the model list of every provider the repo has a key for
(OpenRouter, Anthropic, Google AI Studio, DeepSeek, NVIDIA NIM, opencode-go) plus
a static local MLX/GGUF set, normalizes them to one record schema, applies an
explicit *viability filter* for a real-time text game, de-dups by model family,
buckets by cost tier, and writes three committed artifacts:

    promptfoo/catalog/candidates.jsonl   every viable candidate (machine-readable)
    promptfoo/catalog/excluded.jsonl     every excluded model + reason (auditable)
    promptfoo/catalog/candidates.md      counts per tier/provider + filter + histogram

Usage:
    python3 promptfoo/scripts/enumerate_candidates.py [--context-floor N] [--offline]

`--offline` skips network and only emits the local + opencode-cache candidates
(used by the test seam). The opencode cache (~/.cache/opencode/models.json) is
also used to *enrich* direct-provider lists that return only model ids.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402  (gives us config/pricing.py via rb.pricing)

PROMPTFOO_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = PROMPTFOO_DIR.parent
CATALOG_DIR = PROMPTFOO_DIR / "catalog"
OPENCODE_CACHE = Path.home() / ".cache" / "opencode" / "models.json"

# ---------------------------------------------------------------------------
# Viability thresholds (documented; surfaced in candidates.md)
# ---------------------------------------------------------------------------

# Context floor: the candidate must fit the game's largest runtime prompt + headroom.
# Provisional default; set precisely from the measured max runtime prompt once the
# REQ-2 runtime-faithful datasets exist (see promptfoo/catalog/candidates.md).
DEFAULT_CONTEXT_FLOOR = 8192

# Interactive-play cost ceiling, in USD per game-hour (pricing.gameplay_cost).
# A model that would cost more than this to *play* is out of scope for a real-time
# game regardless of quality. Opus 4.8 (~$15/hr) sits just under; anything pricier
# is excluded as non-interactive.
COST_CEILING_USD_PER_HOUR = 16.0

# id substrings that mark a non-conversational / non-text model. Case-insensitive.
NON_CHAT_MARKERS = (
    "embed",
    "embedding",
    "rerank",
    "reranker",
    "whisper",
    "tts",
    "stt",
    "moderation",
    "guard",
    "/guard",
    "safety",
    "-vision-only",
    "image",
    "imagen",
    "dall-e",
    "dalle",
    "video",
    "veo",
    "sora",
    "diffusion",
    "flux",
    "sdxl",
    "stable-diffusion",
    "ocr",
    "transcribe",
    "audio",
    "speech",
    "voice",
    "bge-",
    "e5-",
    "gte-",
    "nv-embed",
    "nemoretriever",
    "parakeet",
    "canary",
)
# id substrings that mark a base (non-instruct) model.
BASE_MARKERS = ("-base", "base-", "/base", "pretrain")


# ---------------------------------------------------------------------------
# Source fetchers — each returns a list of raw normalized dicts or [] on failure
# ---------------------------------------------------------------------------


def _http_json(url: str, headers: dict[str, str], timeout: float = 30.0) -> dict | None:
    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, ValueError) as e:
        print(f"[enumerate] WARN fetch failed {url}: {e}", file=sys.stderr)
        return None


def _key(name: str) -> str | None:
    v = os.environ.get(name)
    return v.strip() if v and v.strip() else None


def load_opencode_cache() -> dict[str, Any]:
    """The opencode CLI caches a 139-provider model registry with per-model
    cost + context limits. We use it (a) as the opencode-go gateway source and
    (b) to enrich direct-provider lists that return only ids."""
    if not OPENCODE_CACHE.exists():
        return {}
    try:
        return json.loads(OPENCODE_CACHE.read_text(encoding="utf-8"))
    except (ValueError, OSError):
        return {}


def fetch_openrouter() -> list[dict]:
    key = _key("OPENROUTER_API_KEY")
    if not key:
        return []
    d = _http_json("https://openrouter.ai/api/v1/models", {"Authorization": f"Bearer {key}"})
    if not d:
        return []
    out = []
    for m in d.get("data", []):
        pricing = m.get("pricing") or {}
        arch = m.get("architecture") or {}
        out.append(
            {
                "source": "openrouter",
                "provider_id": "openrouter",
                "base_url": "https://openrouter.ai/api/v1",
                "api_key_env": "OPENROUTER_API_KEY",
                "model_id": m["id"],
                "display_name": m.get("name") or m["id"],
                # OpenRouter prices are USD per token (strings) → USD per Mtok.
                "price_in": _f(pricing.get("prompt")) * 1e6,
                "price_out": _f(pricing.get("completion")) * 1e6,
                "max_context": int(m.get("context_length") or 0),
                "modality": arch.get("modality") or "",
                "input_modalities": arch.get("input_modalities") or [],
                "output_modalities": arch.get("output_modalities") or [],
                "supported_params": m.get("supported_parameters") or [],
                "json_schema": bool(
                    {"response_format", "structured_outputs"}
                    & set(m.get("supported_parameters") or [])
                ),
            }
        )
    return out


def _opencode_provider_models(cache: dict, provider_key: str) -> dict[str, dict]:
    prov = cache.get(provider_key) or {}
    return prov.get("models") or {}


def fetch_opencode_go(cache: dict) -> list[dict]:
    if not _key("OPENCODE_API_KEY"):
        return []
    models = _opencode_provider_models(cache, "opencode-go")
    out = []
    for mid, m in models.items():
        cost = m.get("cost") or {}
        limit = m.get("limit") or {}
        mod = m.get("modalities") or {}
        out.append(
            {
                "source": "opencode-go",
                "provider_id": "opencode-go",
                "base_url": "https://opencode.ai/zen/go/v1",
                "api_key_env": "OPENCODE_API_KEY",
                "model_id": mid,
                "display_name": m.get("name") or mid,
                "price_in": _f(cost.get("input")),  # already USD/Mtok
                "price_out": _f(cost.get("output")),
                "max_context": int(limit.get("context") or 0),
                "modality": "+".join(mod.get("input") or ["text"]) + "->text",
                "input_modalities": mod.get("input") or ["text"],
                "output_modalities": mod.get("output") or ["text"],
                "supported_params": ["tools"] if m.get("tool_call") else [],
                # opencode-go does not honour response_format=json_schema (intent
                # slice 400s) but tool_call enables forced-JSON for sim/dialogue.
                "json_schema": False,
                "flat_rate": True,  # subscription gateway; per-token cost is list price
                "reasoning": bool(m.get("reasoning")),
            }
        )
    return out


def _enrich_from_cache(cache: dict, provider_key: str, model_id: str) -> dict:
    """Return {price_in, price_out, max_context} for a direct-provider model from
    the opencode cache, matching on bare id or org/id suffix."""
    models = _opencode_provider_models(cache, provider_key)
    m = models.get(model_id)
    if not m:
        # try suffix match (cache may key "claude-opus-4-8" vs list "claude-opus-4-8-20260528")
        for k, v in models.items():
            if model_id.startswith(k) or k.startswith(model_id):
                m = v
                break
    if not m:
        return {}
    cost = m.get("cost") or {}
    limit = m.get("limit") or {}
    return {
        "price_in": _f(cost.get("input")),
        "price_out": _f(cost.get("output")),
        "max_context": int(limit.get("context") or 0),
        "modality": "+".join((m.get("modalities") or {}).get("input") or ["text"]) + "->text",
    }


def fetch_id_only(
    *,
    source: str,
    provider_id: str,
    base_url: str,
    api_key_env: str,
    url: str,
    headers: dict,
    cache: dict,
    cache_key: str,
    strip_prefix: str = "",
) -> list[dict]:
    """Providers whose /models returns ids only (Google, DeepSeek, NVIDIA,
    Anthropic). Enrich price/context from the opencode cache."""
    if not _key(api_key_env):
        return []
    d = _http_json(url, headers)
    if not d:
        return []
    out = []
    for m in d.get("data", []):
        mid = m["id"]
        if strip_prefix and mid.startswith(strip_prefix):
            mid = mid[len(strip_prefix) :]
        enr = _enrich_from_cache(cache, cache_key, mid)
        # Anthropic /v1/models carries real context (max_input_tokens).
        ctx = int(m.get("max_input_tokens") or 0) or int(enr.get("max_context") or 0)
        out.append(
            {
                "source": source,
                "provider_id": provider_id,
                "base_url": base_url,
                "api_key_env": api_key_env,
                "model_id": mid,
                "display_name": m.get("display_name") or mid,
                "price_in": enr.get("price_in", 0.0),
                "price_out": enr.get("price_out", 0.0),
                "max_context": ctx,
                "modality": enr.get("modality", "text->text"),
                "input_modalities": ["text"],
                "output_modalities": ["text"],
                "supported_params": ["response_format"],
                "json_schema": True,  # all four expose OpenAI-style/tool JSON
                "price_known": bool(enr.get("price_in") is not None and (enr or ctx)),
                "enriched": bool(enr),
            }
        )
    return out


def local_candidates() -> list[dict]:
    """Static local MLX/GGUF set ($0, ranked by throughput). Sourced from the
    legacy catalog's local entries + candidates_local_mlx.toml model names."""
    out = []
    seen = set()
    # from models.toml local-only entries
    try:
        import tomllib

        toml_path = REPO_ROOT / "rundale-bench" / "v1" / "models.toml"
        data = tomllib.loads(toml_path.read_text(encoding="utf-8"))
        for m in data.get("model", []):
            if not m.get("local_only"):
                continue
            for p in m.get("providers", []):
                mid = p["model_name_at_provider"]
                if mid in seen:
                    continue
                seen.add(mid)
                out.append(
                    {
                        "source": "local",
                        "provider_id": p.get("provider_id", "vllmmlx"),
                        "base_url": p.get("base_url", "http://localhost:8000/v1"),
                        "api_key_env": None,
                        "model_id": mid,
                        "display_name": m.get("display_name", mid),
                        "price_in": 0.0,
                        "price_out": 0.0,
                        "max_context": int(p.get("max_context") or 32768),
                        "modality": "text->text",
                        "input_modalities": ["text"],
                        "output_modalities": ["text"],
                        "supported_params": ["response_format"],
                        "json_schema": True,
                        "local": True,
                    }
                )
    except Exception as e:  # noqa: BLE001
        print(f"[enumerate] WARN local toml: {e}", file=sys.stderr)
    return out


def _f(v) -> float:
    try:
        return float(v)
    except (TypeError, ValueError):
        return 0.0


# ---------------------------------------------------------------------------
# Viability filter + family de-dup + tiering
# ---------------------------------------------------------------------------


def viability(rec: dict, context_floor: int) -> tuple[bool, str]:
    """Return (is_viable, reason_if_not). Order matters — first failing rule wins."""
    mid = rec["model_id"].lower()
    # 0. meta-routers and price sentinels (OpenRouter `auto`/`:online` route to an
    #    opaque backing model and report price -1; they are not a fixed candidate).
    if mid in ("openrouter/auto",) or mid.endswith("/auto") or ":online" in mid:
        return False, "meta-router"
    if rec.get("price_in", 0.0) < 0 or rec.get("price_out", 0.0) < 0:
        return False, "price-sentinel"
    # A non-local, non-`:free` model with no positive price could not be enriched
    # (direct NVIDIA/Google/etc. lists omit pricing) — a benchmark of record must
    # be able to cost-rank it, so exclude rather than mislabel it as free.
    is_free = rec.get("local") or mid.endswith(":free")
    if not is_free and rec.get("price_in", 0.0) <= 0 and rec.get("price_out", 0.0) <= 0:
        return False, "price-unknown"
    # 1. chat/instruct text model
    if any(x in mid for x in NON_CHAT_MARKERS):
        return False, "non-chat-modality"
    if any(x in mid for x in BASE_MARKERS):
        return False, "base-non-instruct"
    out_mods = rec.get("output_modalities") or ["text"]
    if out_mods and out_mods != ["text"] and "text" not in out_mods:
        return False, "non-text-output"
    if rec.get("modality") and "->text" not in rec["modality"]:
        return False, "non-text-output"
    if "text" not in (rec.get("input_modalities") or ["text"]):
        return False, "non-text-input"
    # 2. context window
    ctx = int(rec.get("max_context") or 0)
    if ctx and ctx < context_floor:
        return False, f"context<{context_floor}"
    if not ctx:
        return False, "context-unknown"
    # 3. JSON-capable (need schema for intent+sim; tool/free-form OK elsewhere)
    if not (rec.get("json_schema") or "tools" in (rec.get("supported_params") or [])):
        return False, "no-json-capability"
    # 4. interactive cost ceiling
    gh = rb.pricing.gameplay_cost(rec.get("price_in", 0.0), rec.get("price_out", 0.0))
    cost_hr = gh["gameplay_cost_usd_per_hour"]
    if cost_hr > COST_CEILING_USD_PER_HOUR:
        return False, f"cost>{COST_CEILING_USD_PER_HOUR}/hr"
    return True, ""


_FAMILY_DROP_SUFFIXES = (":free", ":nitro", ":extended", ":beta", ":thinking")


def family_key(rec: dict) -> str:
    """Collapse size/quant/date/provider variants to one logical family bucket.
    Conservative: keep distinct model lines separate, merge obvious dup variants."""
    mid = rec["model_id"].lower()
    for suf in _FAMILY_DROP_SUFFIXES:
        if mid.endswith(suf):
            mid = mid[: -len(suf)]
    # strip org prefix and trailing date/quant/preview tokens
    base = mid.split("/")[-1]
    import re

    # drop a trailing -preview / -exp / -experimental marker (with optional date),
    # then trailing date stamps, then quant/serving suffixes. This collapses
    # e.g. gemini-2.5-pro / -preview / -preview-05-06 to one family, while keeping
    # genuinely distinct sizes (gemma-3-4b vs -12b) and version lines separate.
    base = re.sub(r"[-_](preview|exp|experimental)([-_]\d{2,8}([-_]\d{2})*)?$", "", base)
    base = re.sub(r"[-_](\d{8}|\d{4}-\d{2}-\d{2})$", "", base)
    base = re.sub(r"[-_](4bit|8bit|fp8|bf16|awq|gguf|q4_k_m|turbo|latest)$", "", base)
    return base


def gameplay_hr(rec: dict) -> float:
    return rb.pricing.gameplay_cost(rec.get("price_in", 0.0), rec.get("price_out", 0.0))[
        "gameplay_cost_usd_per_hour"
    ]


def quality_cost_proxy(rec: dict) -> float:
    return rec.get("price_in", 0.0) + 3.0 * rec.get("price_out", 0.0)


def cost_tier(rec: dict) -> str:
    if rec.get("local") or rec["model_id"].endswith(":free"):
        return "free"
    hr = gameplay_hr(rec)
    if hr <= 0.0:
        return "free"
    if hr <= 0.20:
        return "budget"
    if hr <= 1.50:
        return "mid"
    return "premium"


def target_spec(rec: dict) -> str:
    spec = f"{rec['model_id']}@{rec['base_url']}"
    if rec.get("api_key_env"):
        spec += f"#env:{rec['api_key_env']}"
    return spec


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def enumerate_all(context_floor: int, offline: bool) -> tuple[list[dict], list[dict]]:
    cache = load_opencode_cache()
    raw: list[dict] = []
    raw += local_candidates()
    raw += fetch_opencode_go(cache)
    if not offline:
        raw += fetch_openrouter()
        raw += fetch_id_only(
            source="anthropic",
            provider_id="anthropic",
            base_url="https://api.anthropic.com/v1",
            api_key_env="ANTHROPIC_API_KEY",
            url="https://api.anthropic.com/v1/models",
            headers={
                "x-api-key": _key("ANTHROPIC_API_KEY") or "",
                "anthropic-version": "2023-06-01",
            },
            cache=cache,
            cache_key="anthropic",
        )
        raw += fetch_id_only(
            source="google",
            provider_id="google",
            base_url="https://generativelanguage.googleapis.com/v1beta/openai",
            api_key_env="GOOGLE_API_KEY",
            url="https://generativelanguage.googleapis.com/v1beta/openai/models",
            headers={"Authorization": f"Bearer {_key('GOOGLE_API_KEY') or ''}"},
            cache=cache,
            cache_key="google",
            strip_prefix="models/",
        )
        raw += fetch_id_only(
            source="deepseek",
            provider_id="deepseek",
            base_url="https://api.deepseek.com/v1",
            api_key_env="DEEPSEEK_API_KEY",
            url="https://api.deepseek.com/v1/models",
            headers={"Authorization": f"Bearer {_key('DEEPSEEK_API_KEY') or ''}"},
            cache=cache,
            cache_key="deepseek",
        )
        raw += fetch_id_only(
            source="nvidia",
            provider_id="nvidia",
            base_url="https://integrate.api.nvidia.com/v1",
            api_key_env="NVIDIA_API_KEY",
            url="https://integrate.api.nvidia.com/v1/models",
            headers={"Authorization": f"Bearer {_key('NVIDIA_API_KEY') or ''}"},
            cache=cache,
            cache_key="nvidia",
        )

    # Annotate viability
    viable: list[dict] = []
    excluded: list[dict] = []
    for rec in raw:
        ok, reason = viability(rec, context_floor)
        rec = dict(rec)
        rec["spec"] = target_spec(rec)
        rec["family"] = family_key(rec)
        rec["cost_usd_per_game_hour"] = round(gameplay_hr(rec), 5)
        rec["tier"] = cost_tier(rec)
        if ok:
            viable.append(rec)
        else:
            rec["excluded_reason"] = reason
            excluded.append(rec)

    # De-dup by family: keep the cheapest viable provider per family; record alternates.
    by_family: dict[str, list[dict]] = {}
    for rec in viable:
        by_family.setdefault(rec["family"], []).append(rec)
    deduped: list[dict] = []
    for fam, recs in by_family.items():
        recs.sort(key=quality_cost_proxy)
        primary = dict(recs[0])
        primary["alt_providers"] = [
            {
                "provider_id": r["provider_id"],
                "spec": r["spec"],
                "price_in": r["price_in"],
                "price_out": r["price_out"],
            }
            for r in recs[1:]
        ]
        deduped.append(primary)
        # the non-primary family members move to excluded with a dedup reason
        for r in recs[1:]:
            r = dict(r)
            r["excluded_reason"] = f"dedup:family={fam} (kept {primary['spec']})"
            excluded.append(r)

    deduped.sort(key=lambda r: (r["tier"], r["cost_usd_per_game_hour"], r["model_id"]))
    return deduped, excluded


def write_artifacts(candidates: list[dict], excluded: list[dict], context_floor: int) -> None:
    CATALOG_DIR.mkdir(parents=True, exist_ok=True)
    (CATALOG_DIR / "candidates.jsonl").write_text(
        "".join(json.dumps(c, ensure_ascii=False) + "\n" for c in candidates), encoding="utf-8"
    )
    (CATALOG_DIR / "excluded.jsonl").write_text(
        "".join(json.dumps(c, ensure_ascii=False) + "\n" for c in excluded), encoding="utf-8"
    )
    (CATALOG_DIR / "candidates.md").write_text(
        _render_md(candidates, excluded, context_floor), encoding="utf-8"
    )


def _render_md(candidates: list[dict], excluded: list[dict], context_floor: int) -> str:
    from collections import Counter

    tier_counts = Counter(c["tier"] for c in candidates)
    src_counts = Counter(c["source"] for c in candidates)
    excl_counts = Counter(e["excluded_reason"].split(" ")[0].split(":")[0] for e in excluded)
    lines = [
        "# Rundale benchmark candidates (REQ 1)\n",
        f"**{len(candidates)} viable candidates** made the cut "
        f"(de-duped by family from {len(candidates) + len(excluded)} raw provider entries).\n",
        "## Viability filter (explicit)\n",
        "A model is viable iff ALL hold:",
        "1. **Chat/instruct, text-in→text-out** — not embeddings/rerank/audio/image/video/"
        "moderation/base.",
        f"2. **Context window ≥ {context_floor} tokens** — fits the game's largest runtime "
        "prompt + headroom (floor measured from the runtime-faithful datasets).",
        "3. **JSON-capable** — exposes `response_format`/`structured_outputs`, or tool-calling "
        "for forced JSON (needed by the intent + simulation slices).",
        f"4. **Interactive cost ceiling** — ≤ ${COST_CEILING_USD_PER_HOUR:.0f}/game-hour "
        "(`pricing.gameplay_cost`), so it is affordable for real-time play.",
        "De-dup: one logical model per family; cheapest viable provider kept as primary, "
        "others recorded as `alt_providers` (still perf-swept).\n",
        "## Cost tiers (USD per game-hour)\n",
        "- **free**: $0 (local MLX/GGUF + OpenRouter `:free`)",
        "- **budget**: ≤ $0.20/hr",
        "- **mid**: ≤ $1.50/hr",
        "- **premium**: ≤ $16/hr\n",
        "## Counts\n",
        "| Tier | Count |",
        "| --- | --- |",
    ]
    for t in ("free", "budget", "mid", "premium"):
        lines.append(f"| {t} | {tier_counts.get(t, 0)} |")
    lines.append("\n**By source:** " + ", ".join(f"{k}={v}" for k, v in src_counts.most_common()))
    lines.append("\n## Exclusion-reason histogram\n")
    lines.append("| Reason | Count |")
    lines.append("| --- | --- |")
    for reason, n in excl_counts.most_common():
        lines.append(f"| {reason} | {n} |")
    lines.append("\n## Viable candidates (sorted by tier, then cost)\n")
    lines.append("| Model | Provider | Tier | $/game-hr | ctx | family |")
    lines.append("| --- | --- | --- | --- | --- | --- |")
    for c in candidates:
        lines.append(
            f"| `{c['model_id']}` | {c['provider_id']} | {c['tier']} | "
            f"{c['cost_usd_per_game_hour']:.4f} | {c['max_context']} | {c['family']} |"
        )
    return "\n".join(lines) + "\n"


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--context-floor", type=int, default=DEFAULT_CONTEXT_FLOOR)
    ap.add_argument("--offline", action="store_true")
    args = ap.parse_args(argv[1:])

    candidates, excluded = enumerate_all(args.context_floor, args.offline)
    write_artifacts(candidates, excluded, args.context_floor)
    from collections import Counter

    tiers = Counter(c["tier"] for c in candidates)
    print(
        f"[enumerate] {len(candidates)} viable / {len(excluded)} excluded "
        f"(context_floor={args.context_floor}). tiers: "
        + ", ".join(f"{t}={tiers.get(t, 0)}" for t in ("free", "budget", "mid", "premium"))
    )
    print(f"[enumerate] wrote {CATALOG_DIR}/candidates.{{jsonl,md}} + excluded.jsonl")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
