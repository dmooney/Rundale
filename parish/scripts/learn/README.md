# learn — self-improving-agent feedback loop

Distils dev-time agent failures into terse lessons appended to
`LEARNINGS.md`. Inspired by
[*Self-Improving AI Agent Feedback Loops*](https://www.mindstudio.ai/blog/self-improving-ai-agent-feedback-loop/).

## Loop

```
push to main
   │
   ▼
collect signals ───┬─ failed CI runs                (collect_ci.py)
                   ├─ rejected proof bundles        (collect_judges.py)
                   ├─ review-bot comments           (collect_reviews.py)
                   └─ Stop-hook proof blocks        (collect_stop_blocks.py)
   │
   ▼
extract  (LLM) ── candidate lessons in existing bullet style
   │
   ▼
judge          ── format asserts (deterministic) + dedupe judge (LLM)
   │
   ▼
write          ── overwrite LEARNINGS.md, append run footer
   │
   ▼
open PR        ── peter-evans/create-pull-request
```

## Local usage

```
# 1. Install deps
pip install -r parish/scripts/learn/requirements.txt

# 2. Dry-run against a fixture (no API calls if --skip-llm)
just learn-dry

# 3. Real local run — needs ANTHROPIC_API_KEY in env, no GitHub API
ANTHROPIC_API_KEY=... python parish/scripts/learn/driver.py --local --dry-run

# 4. Tests (offline, no network)
pytest parish/scripts/learn/tests/ -q
```

## What runs in CI

`.github/workflows/learn.yml` triggers on push to `main`. It runs the
full collectors (with `GITHUB_TOKEN`), calls the extractor + dedupe
judge using `secrets.ANTHROPIC_API_KEY`, and opens a follow-up PR if
`LEARNINGS.md` changed.

## Files

| File | Role |
|---|---|
| `signals.py` | Common `Signal` dataclass + JSON serialisation. |
| `github_api.py` | Minimal `urllib`-based GitHub REST wrapper. |
| `collect_*.py` | One collector per signal source. Each is invocable as a script and dumps JSON to stdout. |
| `extract.py` | Anthropic API call: signals + LEARNINGS.md → candidates. |
| `judge.py` | Format asserts (no LLM) + per-candidate dedupe LLM call. |
| `write.py` | Rewrites LEARNINGS.md preserving the intro + existing bullets. |
| `driver.py` | End-to-end pipeline; CLI entry point. |
| `prompts/` | System prompts for the extractor and the dedupe judge. |
| `tests/` | Offline unit tests + signal fixture. |
| `requirements.txt` | `anthropic` SDK only. |

## Format rules baked into the extractor prompt

- Bullets are 2–3 lines.
- Lead with `- **<short claim>.**`.
- Cite a real file or test name from the signal.
- No model / agent names (tool-agnostic lessons).

Rejected candidates land in
`docs/proofs/learn-runs/<timestamp>.md` for review.

## Cost

Each run is one extractor call + N dedupe calls (one per surviving
candidate). With Sonnet 4.6 + prompt caching on the system block, a
typical run with ~10 signals runs ~30K input + 1K output tokens.
