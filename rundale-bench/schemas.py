#!/usr/bin/env python3
"""Pydantic models for the three main on-disk data shapes.

``BundleSchema``  — bundle written to ``.bench-queue/pending/`` by
                    ``judge_bundle.assemble_bundle`` / ``write_pending``.
``ResultSchema``  — subagent result read from ``.bench-queue/done/`` during
                    ``judge_bundle.validate_result``.
``SummarySchema`` — per-slice summary dict embedded inside run artifacts
                    written to ``artifacts/run_*.json``.

Each model encodes invariants that the old dict-based code checked at
read-time (or sometimes silently missed). Making the shapes models means
that invalid data raises ``pydantic.ValidationError`` at the boundary
where it is created, not later when it is consumed.

Five failure modes that are now impossible by construction
----------------------------------------------------------
1. **None-in-format crash** — axes must be ``dict[str, int | float]``, never
   None. ``ItemSchema.axes`` is typed as ``dict[str, int]``; the field is
   required (no default). Attempting to pass ``axes=None`` raises immediately.
2. **bench_bug=true with nonzero axis scores** — ``ItemSchema`` carries a
   ``model_validator`` that, when ``flags.bench_bug`` is True, asserts every
   axis value is 0 and ``overall == 0.0``. The validator fires before the
   object is returned.
3. **rubric_sha256 mismatch** — ``ResultSchema`` requires ``rubric_sha256``
   (non-empty string); callers pass it into ``validate_result``, which then
   compares it to the bundle's value. The field can no longer be absent or
   None without raising at construction time.
4. **Missing required field** — Pydantic raises ``ValidationError`` for any
   absent required field (no ``default`` / ``default_factory``), so a dict
   missing ``bundle_id``, ``rubric_sha256``, ``items``, etc. fails loudly.
5. **Malformed shape on ingest** — ``BundleItemSchema`` enforces that each
   bundle item has at least ``prompt_id``, ``prompt``, and ``response``.
   ``ResultItemSchema`` enforces presence of ``axes`` (dict), ``overall``
   (float in 0–5), ``prompt_id``, and ``flags``. Shape mismatches raise
   before data ever reaches aggregate logic.
"""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, field_validator, model_validator

# ---------------------------------------------------------------------------
# Supporting sub-models
# ---------------------------------------------------------------------------


class FlagsSchema(BaseModel):
    """Judgment flags attached to a scored item."""

    non_latin_detected: bool = False
    refused: bool = False
    bench_bug: bool = False
    judge_retry: bool = False

    model_config = {"extra": "allow"}


class ResultItemSchema(BaseModel):
    """One judged item inside a ``ResultSchema``.

    Enforces the invariants that ``judge_bundle.validate_item`` checked
    procedurally; having them on the model means they are checked whenever
    a ``ResultItemSchema`` is constructed — including inside ``ingest``.
    """

    prompt_id: str
    axes: dict[str, int | float]
    overall: float
    rationales: dict[str, Any] = {}
    flags: FlagsSchema = FlagsSchema()

    @field_validator("prompt_id")
    @classmethod
    def prompt_id_nonempty(cls, v: str) -> str:
        if not v:
            raise ValueError("prompt_id must not be empty")
        return v

    @field_validator("overall")
    @classmethod
    def overall_in_range(cls, v: float) -> float:
        if not (0.0 <= v <= 5.0):
            raise ValueError(f"overall must be in [0.0, 5.0], got {v!r}")
        return round(v, 1)

    @field_validator("axes")
    @classmethod
    def axes_values_in_range(cls, v: dict[str, int | float]) -> dict[str, int | float]:
        for axis, score in v.items():
            if not (0 <= score <= 5):
                raise ValueError(f"axis {axis!r} value {score!r} out of range [0, 5]")
        return v

    @model_validator(mode="after")
    def bench_bug_all_zeros(self) -> ResultItemSchema:
        """Failure mode 2: bench_bug=true must have all axes == 0 and overall == 0."""
        if not self.flags.bench_bug:
            return self
        bad_axes = {k: v for k, v in self.axes.items() if v != 0}
        if bad_axes:
            raise ValueError(f"bench_bug=true but axes not all 0: {bad_axes}")
        if self.overall != 0.0:
            raise ValueError(f"bench_bug=true but overall != 0.0: {self.overall!r}")
        return self

    model_config = {"extra": "allow"}


class BundleItemSchema(BaseModel):
    """One candidate-response item inside a ``BundleSchema``.

    Failure mode 5: all three fields are required — missing any raises.
    """

    prompt_id: str
    prompt: str
    response: str

    @field_validator("prompt_id")
    @classmethod
    def prompt_id_nonempty(cls, v: str) -> str:
        if not v:
            raise ValueError("prompt_id must not be empty")
        return v

    model_config = {"extra": "allow"}


# ---------------------------------------------------------------------------
# Top-level schemas
# ---------------------------------------------------------------------------


class BundleSchema(BaseModel):
    """Bundle written to ``.bench-queue/pending/`` by ``assemble_bundle``.

    Failure modes addressed:
      - 4 (missing required field): ``bundle_id``, ``rubric_sha256``, and
        ``items`` are all required; absent fields raise at construction.
      - 5 (malformed shape): ``items`` is typed as a list of
        ``BundleItemSchema``, so each item is validated on construction.
    """

    bundle_id: str
    slice: str
    candidate: dict[str, Any]
    judge_id: str
    judge_model: str
    rubric_sha256: str
    rubric: str
    axes: list[str] = []
    system_prompt_file: str | None = None
    items: list[BundleItemSchema]

    @field_validator("rubric_sha256")
    @classmethod
    def rubric_sha256_nonempty(cls, v: str) -> str:
        # Failure mode 3: rubric_sha256 must be a non-empty string.
        if not v:
            raise ValueError("rubric_sha256 must not be empty")
        return v

    @field_validator("items")
    @classmethod
    def items_nonempty(cls, v: list[BundleItemSchema]) -> list[BundleItemSchema]:
        if not v:
            raise ValueError("bundle items list must not be empty")
        return v

    model_config = {"extra": "allow"}


class ResultSchema(BaseModel):
    """Subagent result read from ``.bench-queue/done/``.

    Failure modes addressed:
      - 3 (rubric_sha256 mismatch): the field is required; a result missing
        it raises here, not silently at compare time.
      - 4 (missing required field): ``rubric_sha256`` and ``items`` required.
      - 5 (malformed shape): ``items`` typed as ``list[ResultItemSchema]``.

    Note: ``validate_result`` still performs the bundle vs. result
    rubric_sha256 comparison (a cross-object constraint that cannot be
    expressed on a single model). This model guarantees the field is
    *present* on the result; the caller compares it to the bundle's value.
    """

    rubric_sha256: str
    items: list[dict[str, Any]]  # raw items; validated per-item by validate_item

    @field_validator("rubric_sha256")
    @classmethod
    def rubric_sha256_nonempty(cls, v: str) -> str:
        if not v:
            raise ValueError("rubric_sha256 must not be empty")
        return v

    model_config = {"extra": "allow"}


class SummarySchema(BaseModel):
    """Per-slice summary embedded in run artifacts.

    Failure mode 1: numeric score fields that should never be None when
    judgments are complete are typed as ``float | None``. Code that formats
    them (e.g. ``f"{v:.3f}"``) without a None-guard crashes; the model makes
    the optionality *explicit* and discoverable rather than a hidden runtime
    surprise.  Callers that need a non-None value can assert or branch on it.

    The model is intentionally permissive about extra fields because
    different slice aggregators produce slice-specific keys (e.g.
    ``schema_valid_rate`` for simulation, ``english_leakage_flag_rate`` for
    Gaeilge). Extra fields are passed through unchanged.
    """

    slice: str
    records: int
    judged: int = 0
    bench_bugs: int = 0
    judge_failures: int = 0
    errors: int = 0
    # All numeric score fields are optional (None when no items were judged).
    overall: float | None = None
    # Dialogue axes
    character: float | None = None
    authenticity: float | None = None
    language: float | None = None
    responsiveness: float | None = None
    craft: float | None = None
    # Reaction / simulation
    mean_score: float | None = None

    @field_validator("records", "judged", "bench_bugs", "judge_failures", "errors")
    @classmethod
    def non_negative_int(cls, v: int) -> int:
        if v < 0:
            raise ValueError(f"count field must be non-negative, got {v!r}")
        return v

    model_config = {"extra": "allow"}
