"""Promptfoo custom provider — runs one rundale-bench v2 candidate call.

Config (set per promptfooconfig):
    slice:     dialogue | intent | reaction | tier2-sim | tier3-sim | gaeilge
    streaming: bool (perf path — capture ttft + tok/s)

Target comes from the RB_TARGET env var (a `model@base_url[#env:VAR]` spec) so
the same config can be pointed at any candidate without editing YAML.

Returns a promptfoo ProviderResponse with native `tokenUsage` + `cost`, plus
`metadata` carrying the dataset record (for the assertion) and the perf signals
(ttft_ms, tokens_per_second) the report rolls up.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402

_DEFAULT_TARGET = "mlx-community/Qwen2.5-14B-Instruct-4bit@http://localhost:8000/v1"


def _dialogue_request_profile(target, rec: dict) -> dict:
    return {
        "model": target.model,
        "max_tokens": rec.get("max_tokens"),
        "temperature": rec.get("temperature", 0.7),
        "frequency_penalty": rec.get("frequency_penalty"),
        "json_mode": rec.get("response_format") == {"type": "json_object"},
        "enable_thinking": rec.get("enable_thinking"),
        "reasoning_effort": rec.get("reasoning_effort"),
    }


def call_api(prompt, options, context):
    config = (options or {}).get("config", {}) or {}
    slice_name = config.get("slice")
    streaming = bool(config.get("streaming", False))
    if not slice_name:
        raise ValueError("rundale_candidate provider requires config.slice")

    spec = os.environ.get("RB_TARGET", _DEFAULT_TARGET)
    target = rb.parse_target(spec)

    # The dataset record is passed as a JSON string in the `record` var so the
    # provider rebuilds the exact per-slice request (schema, system_template…).
    rec = json.loads((context or {}).get("vars", {}).get("record", "{}"))
    effective_rec = (
        rb.effective_dialogue_record(rec)
        if slice_name in {"dialogue", "multiturn", "perf"}
        else rec
    )
    request_profile = (
        _dialogue_request_profile(target, effective_rec)
        if slice_name in {"dialogue", "multiturn", "perf"}
        else None
    )

    gen = rb.generate_candidate(slice_name, target, rec, streaming=streaming)

    if gen["error"]:
        # Stamp the error into metadata too: promptfoo reuses the top-level
        # `error` field for assertion-failure reasons, so the report keys
        # candidate/transport failures off metadata.error to tell them apart.
        return {
            "error": gen["error"],
            "metadata": {
                "record": rec,
                "slice": slice_name,
                "target": spec,
                "request_profile": request_profile,
                "error": gen["error"],
            },
        }

    return {
        "output": gen["output"],
        "tokenUsage": {
            "prompt": gen["prompt_tokens"],
            "completion": gen["completion_tokens"],
            "total": gen["prompt_tokens"] + gen["completion_tokens"],
        },
        "cost": gen["cost"],
        "metadata": {
            "record": rec,
            "slice": slice_name,
            "target": spec,
            "model": target.model,
            "dataset_split": os.environ.get("RB_SPLIT", "dev"),
            "ttft_ms": gen["ttft_ms"],
            "tokens_per_second": gen["tokens_per_second"],
            "schema_valid": gen["schema_valid"],
            "request_profile": request_profile,
            "perf_cache_state": (context or {}).get("vars", {}).get("perf_cache_state"),
        },
    }
