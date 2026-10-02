# promptfoo — agent scope

rundale-bench v2: a promptfoo port of the v1 benchmark with an HTTP-API judge,
runtime-faithful prompts captured from the real engine, and the committed,
append-only leaderboard of record. [README.md](README.md) covers the funnel,
datasets, and every recipe; [RESUME.md](RESUME.md) records in-flight work. The
frozen v1 corpus lives in [`rundale-bench/`](../rundale-bench/AGENTS.md); v2
copies its datasets byte-identically and imports its HTTP layer and graders.

## Commands

```sh
just -f promptfoo/justfile test                       # v2 unit tests (no API calls)
npm --prefix promptfoo run test:keyless               # mock-judge smoke; run after rubric edits
just -f promptfoo/justfile dialogue target='<spec>'   # one slice; `bench` runs every slice + report
just -f promptfoo/justfile capture-prompts            # re-capture runtime prompts, re-pin MANIFEST
pnpm --dir promptfoo/bench-site install && pnpm --dir promptfoo/bench-site run build
```

Model calls cost money. Use the funnel's estimate-only mode before `--yes`.

## Traps

- **Drive evals through the justfile.** A config's `env:` block is applied
  after promptfoo loads the dataset, so a bare `npx promptfoo eval` fails with
  "RB_SLICE env var required". Otherwise export `RB_SLICE`, `RB_TARGET`, and
  `RB_LIMIT` yourself.
- **Every slice belongs in `pin_manifest.py::SLICES`.** The loader verifies
  hash and record count; a slice missing from the manifest lets changed prompts
  keep an old leaderboard merkle and look comparable when they are not.
- **Keep the mock judge in lockstep with every rubric axis and flag**, and run
  `test:keyless` after rubric edits.
- **Prompt capture needs a worktree-keyed Cargo target.** A shared target can
  be replaced by another worktree's build, silently capturing stale request
  parameters; keep the isolation in `capture_prompts.sh`.
- **Build the bench site from a clean dependency tree.** The site uses pnpm
  inside the npm-managed `promptfoo/` parent, and Astro's prerenderer can
  resolve the parent's `cookie@0.7` instead of the site's `cookie@2`. Keep the
  site's `cookie` major declared directly, and reproduce Pages failures in a
  fresh worktree, as `publish-bench-site.yml` installs only the site.
- **`package-lock.json` must pass `npm ci` under npm 10 too.** npm 11 can omit
  an optional peer's nested package that npm 10 then rejects. Add the missing
  entry rather than regenerating the lock, and keep Dependabot for
  `/promptfoo` limited to the direct `promptfoo` package.
- **The leaderboard is append-only.** `leaderboard/leaderboard.jsonl` is the
  site's data source; pushes that change it, `bench-site/`, `catalog/`,
  `v2/MANIFEST.json`, or `config/judge.yaml` republish the site.
