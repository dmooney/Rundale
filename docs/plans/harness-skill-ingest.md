# Plan: harness-skill-ingest

Ordered, one commit per step, conventional-commit prefixes. All code changes are confined to
`parish/crates/parish-harness/` plus the skill doc.

## Step 1 — `feat(harness): add IngestPayload + load_and_ingest` (no CLI yet)

- New `src/ingest.rs`:
  - `IngestPayload` and child payload structs (`serde::Deserialize`) per the design note.
  - Ensure `GitProvenance`, and the `Axis`/`Severity` label parsers, are reachable; add
    `#[derive(Deserialize)]` to `GitProvenance` and a `from_label`/`FromStr` for `Axis` +
    `Severity` if missing (small, in `score/axes.rs` / `score/finding.rs`).
  - `pub fn load_and_ingest(db: &Db, payload_path, artifacts_root) -> Result<i64>`:
    1. read + deserialize payload JSON,
    2. resolve `artifact_dir = artifacts_root/runs/<uuid>` (uuid from payload or generated),
    3. validate `turns/NNN/frame.png` exists for each turn (rule #14 — error on missing/empty),
    4. call `db.ingest_complete_run(&payload, &artifact_dir)`.
- `src/lib.rs`: `pub mod ingest;`.
- Tests: `ingest.rs` unit test that deserializes `sample-payload.json`.

## Step 2 — `feat(harness): Db::ingest_complete_run + update_run_cost`

- In `src/persist/sink.rs`:
  - `pub fn update_run_cost(&self, run_id, cost_usd: f64, player_tokens: u64, judge_tokens: u64)
-> Result<()>` — `UPDATE runs SET cost_usd=?, player_tokens=?, judge_tokens=? WHERE id=?`.
  - `pub fn ingest_complete_run(&self, p: &IngestPayload, artifact_dir: &str) -> Result<i64>`:
    1. `upsert_config(&p.config.into())` (label forced `skill:quality-harness`),
    2. `start_run(config_id, &p.git, &p.rubric_sha256, artifact_dir)`,
    3. `record_turn` for each turn (map `TurnPayload -> TurnRecord`),
    4. `insert_finding` for each finding,
    5. `update_run_cost(run_id, ...)`,
    6. if `p.gate` is Some → `finish_run_gated(run_id, &trip)`; else
       `finish_run_scored(run_id, p.quality_score, &axes)`,
    7. return `run_id`.
  - All inside one `conn` transaction (`BEGIN`/`COMMIT`) so a bad payload leaves no half-run.
- Tests in `sink.rs`: scored ingest sets status/quality/axes/cost; gated ingest nulls quality;
  two ingests reuse one config row.

## Step 3 — feat(harness): wire `ingest` subcommand

- `src/main.rs`: add `Ingest(IngestArgs)` to the `Command` enum + `match`.
  `IngestArgs { payload: PathBuf, artifacts: PathBuf, db: Option<PathBuf> }`.
  Handler opens the DB (default path when `--db` omitted), calls
  `ingest::load_and_ingest`, prints `ingested run <id>`.
- Update `README.md` command cookbook with the `ingest` example.

## Step 4 — `test(harness): end-to-end ingest fixture`

- `parish/crates/parish-harness/tests/ingest.rs`: in-memory + temp-dir e2e covering all AC
  (scored, gated, cost, 7 axes, findings, config dedup).
- `parish/testing/fixtures/ingest_harness_skill/` (already scaffolded): `sample-payload.json` +
  `artifacts/runs/<uuid>/turns/000/frame.png` + `verify.sh` that ingests then curls the serve
  API and asserts each signal.

## Step 5 — `docs(skill): quality-harness persists runs to the dashboard`

- Edit `.agents/skills/quality-harness/SKILL.md` (CLAUDE/`.claude` is a symlink) — add a
  section **"6. Persist to the dashboard"** after "Output + file bugs":
  - Build the `IngestPayload` JSON from the per-turn log + 7 axis scores + findings + git
    provenance + cost (token/cost estimate from the run).
  - Create `runs/<uuid>/turns/NNN/frame.png` from captured screenshots (nearest-prior mapping;
    bundled placeholder for pre-first-capture turns) and `lines.json` per turn.
  - Run `cargo run -p parish-harness -- ingest --payload <json> --artifacts <root>`.
  - Surface the printed run id + `http://localhost:8787` so the user can open it.
- Note the same payload schema in `parish/crates/parish-harness/README.md` so the contract has
  one documented source.

## Tests to add / update

- `sink.rs` unit tests (Step 2), `tests/ingest.rs` integration (Step 4).
- `cargo clippy -p parish-harness --all-targets -- -D warnings` clean.
- No game-runtime tests change. No `architecture_fitness` impact (no new cross-crate dep).

## Proof

Tooling change, so `/parish-engine prove` does not apply. Evidence = `cargo test -p
parish-harness ingest` transcript + `verify.sh` transcript showing the dashboard API returning
the ingested run. `evidence.md` maps each AC criterion to those lines; `judge.md` verifies each.
