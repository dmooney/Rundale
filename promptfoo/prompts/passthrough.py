"""Trivial promptfoo prompt function.

The candidate provider builds the real system+user prompt per slice from the
dataset record (passed as the `record` var), so the nominal "prompt" promptfoo
sends is just the human-readable player input — useful in the web UI but unused
by the provider.
"""

from __future__ import annotations


def get_prompt(context):
    vars_ = (context or {}).get("vars", {}) or {}
    return vars_.get("display_prompt", vars_.get("record", ""))
