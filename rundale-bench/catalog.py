#!/usr/bin/env python3
"""Candidate-model catalog loader for rundale-bench.

The catalog (``v1/models.toml``) binds a logical model id to one or more
providers that serve it. Quality judging happens once per model on the
*cheapest* provider; perf benchmarking (later phase) sweeps every provider.

Provider ``base_url`` / ``api_key_env`` mirror the engine's provider presets
in ``parish/crates/parish-config/providers/*.toml`` — the catalog is the
eval-side view of the same provider set, annotated with prices.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from pathlib import Path

import tomllib

_BENCH_DIR = Path(__file__).resolve().parent


@dataclass(frozen=True)
class Provider:
    provider_id: str
    model_name_at_provider: str
    base_url: str
    api_key_env: str | None
    price_in_per_mtok: float
    price_out_per_mtok: float
    max_context: int | None
    notes: str

    def quality_cost_proxy(self) -> float:
        """Blended $/Mtok used to pick the cheapest provider for judging.

        Output tokens dominate dialogue spend, so weight them 3:1 over input.
        Ties are broken by the caller (lowest historical p50 latency once the
        perf phase lands; provider order in the file until then).
        """
        return self.price_in_per_mtok + 3.0 * self.price_out_per_mtok

    def resolved_target(self) -> str:
        """`model@base_url[#env:VAR]` spec consumed by eval_lib.parse_target."""
        spec = f"{self.model_name_at_provider}@{self.base_url}"
        if self.api_key_env:
            spec += f"#env:{self.api_key_env}"
        return spec


@dataclass(frozen=True)
class Model:
    id: str
    display_name: str
    family: str
    params_b: float | None
    quant: str | None
    local_only: bool
    providers: tuple[Provider, ...]

    def cheapest_provider(self) -> Provider:
        """Provider with the lowest blended cost; file order breaks ties."""
        return min(self.providers, key=lambda p: p.quality_cost_proxy())


@dataclass(frozen=True)
class Catalog:
    models: tuple[Model, ...]
    sha256: str

    def by_id(self, model_id: str) -> Model:
        for m in self.models:
            if m.id == model_id:
                return m
        raise KeyError(f"model id not in catalog: {model_id!r}")

    def ids(self) -> list[str]:
        return [m.id for m in self.models]


def _provider_from_toml(raw: dict) -> Provider:
    return Provider(
        provider_id=raw["provider_id"],
        model_name_at_provider=raw["model_name_at_provider"],
        base_url=raw["base_url"],
        api_key_env=raw.get("api_key_env"),
        price_in_per_mtok=float(raw.get("price_in_per_mtok", 0.0)),
        price_out_per_mtok=float(raw.get("price_out_per_mtok", 0.0)),
        max_context=raw.get("max_context"),
        notes=raw.get("notes", ""),
    )


def _model_from_toml(raw: dict) -> Model:
    providers = tuple(_provider_from_toml(p) for p in raw.get("providers", []))
    if not providers:
        raise ValueError(f"model {raw.get('id')!r} has no providers")
    return Model(
        id=raw["id"],
        display_name=raw.get("display_name", raw["id"]),
        family=raw.get("family", "unknown"),
        params_b=raw.get("params_b"),
        quant=raw.get("quant"),
        local_only=bool(raw.get("local_only", False)),
        providers=providers,
    )


def load_catalog(path: Path | None = None, *, version: str = "v1") -> Catalog:
    """Parse ``models.toml`` into a Catalog. The catalog sha256 is hashed over
    the raw file bytes so a run can record exactly which catalog it used."""
    if path is None:
        path = _BENCH_DIR / version / "models.toml"
    raw_bytes = path.read_bytes()
    data = tomllib.loads(raw_bytes.decode("utf-8"))
    models = tuple(_model_from_toml(m) for m in data.get("model", []))
    ids = [m.id for m in models]
    if len(set(ids)) != len(ids):
        raise ValueError(f"duplicate model ids in {path}: {ids}")
    return Catalog(models=models, sha256=hashlib.sha256(raw_bytes).hexdigest())
