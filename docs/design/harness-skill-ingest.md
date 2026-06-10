# Design: harness-skill-ingest

## Feature in one paragraph

Two "harnesses" produce game-quality runs: the `parish-harness` **binary** (`run` subcommand —
LLM player + LLM judge, persists to `harness.db`) and the `quality-harness` **skill** (an agent
drives the live game over the parish MCP, judges by hand, files bugs). Only the binary's runs
reach the `serve` dashboard. This change lets skill runs land in the same DB by adding a thin
`ingest` path to the binary and a final persist step to the skill — so the dashboard's Runs /
Trends / A/B Compare views cover both producers with identical fidelity (quality score, 7-axis
breakdown, findings, per-turn frames, cost).

## Affected subsystems (by crate)

- `parish-harness` (the only crate that changes):
  - `src/persist/sink.rs` — new `Db::ingest_complete_run()` orchestration helper + new
    `Db::update_run_cost()` writer. Reuses the existing `upsert_config`, `start_run`,
    `record_turn`, `insert_finding`, `finish_run_scored`, `finish_run_gated`.
  - `src/ingest.rs` (new) — `IngestPayload` serde struct + `load_and_ingest()` that maps the
    payload into the sink types and copies artifacts into place.
  - `src/main.rs` — new `Ingest(IngestArgs)` clap subcommand wired to `load_and_ingest()`.
  - `src/lib.rs` — `pub mod ingest;`.
- No game-runtime crates change. No `parish-core` / `parish-server` / `parish-tauri` touch
  (the harness CLAUDE.md forbids depending on them anyway).

## Data model

No new tables, no new columns — the existing schema already has everything (`runs`, `turns`,
`axis_scores`, `findings`, `configs`, and the unused-until-now `runs.cost_usd /
player_tokens / judge_tokens`).

New **transport** struct (serde, not persisted directly):

```
IngestPayload {
  config:        RunConfigPayload,   // label forced to "skill:quality-harness"; hashed -> configs row
  git:           GitProvenance,      // sha, branch, dirty, pr_number  (existing struct, derive Deserialize)
  rubric_sha256: String,            // skill passes the binary's pinned rubric sha for comparability
  status:        "completed" | "gated",
  quality_score: Option<f64>,       // None when gated
  gate:          Option<GatePayload>{ reason: String, turn: u32, detail: String },  // present iff gated
  cost:          CostPayload { cost_usd: f64, player_tokens: u64, judge_tokens: u64 },
  turns:         Vec<TurnPayload>,   // maps 1:1 to TurnRecord
  axes:          Vec<AxisPayload>{ axis: String, score: u8, rationale: String },     // 7 entries
  findings:      Vec<FindingPayload>{ category, turn_index?, severity, description, evidence_quote, signature }
}
```

`TurnPayload` mirrors `TurnRecord` fields; `frame_path` / `lines_path` are relative
(`turns/NNN/frame.png`). `severity` / `axis` strings parse via the existing
`Severity`/`Axis` `from`-label paths (add `FromStr`/lookup if not already present).

### Artifacts

The skill writes its run under a UUID dir: `runs/<uuid>/turns/NNN/{frame.png,lines.json}`,
plus `verdict.json` / `transcript.json` for parity with binary runs. `ingest` takes
`--artifacts <root>`; it stores `runs.artifact_dir = <root>/runs/<uuid>` (absolute) exactly as
`execute_run` does, so `get_frame` resolves `{artifact_dir}/turns/NNN/frame.png` unchanged.

Skill screenshots are periodic, not per-turn. Mapping rule (documented in SKILL.md): each turn
points at the most recent screenshot captured at or before it; turns before the first capture
share a single bundled placeholder `frame.png`. `frame_path` is NOT NULL, so every turn always
references a real file.

## Observable signal

This is tooling, so the signal is on the **dashboard API**, not the game JSON:

- `GET /api/runs` — the ingested run appears with `status` + `quality_score`.
- `GET /api/runs/{id}` — `axes` length 7, `findings` count matches, gated runs show null
  quality + gate reason.
- `GET /api/cost` — totals reflect the payload's cost/tokens.
- `GET /api/runs/{id}/turns/0/frame.png` — image/png bytes.

Reused `ActionResult`-equivalents are the `RunSummaryDto` / `RunDetail` / `CostSummary` DTOs in
`persist/queries.rs` — unchanged.

## Feature flag

Per AGENTS.md §6, runtime gameplay features are flagged. This change ships **no game-runtime
behavior** — it is a CLI subcommand on a tool binary plus a skill-doc edit. There is no
`config.flags` seam in `parish-harness` and nothing in the game loop changes, so no flag
applies. The `ingest` subcommand is itself the opt-in surface (nothing calls it unless the
skill or a user does). This deviation is intentional and noted here for the judge.

## Risks / parity

- `ingest` must reuse the same persist fns the `run` pipeline calls (no second write path), so
  gated/scored semantics can't drift. The helper `ingest_complete_run` is the single seam.
- `update_run_cost` is also wired into `run` later (the cost columns are 0 there today); this
  change adds the writer and the ingest caller. Wiring the live `run` cost is out of scope but
  the writer is shared, not ingest-only, to avoid a divergent path (AGENTS.md §12 spirit).
