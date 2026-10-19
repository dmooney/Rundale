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
- **OpenRouter reasoning on/off is not an effort level.** `enable_thinking=true` accepts the provider/model default; controlled `low`/`high`/`max` experiments must set `[engine.inference.dialogue_generation].reasoning_effort` so the exact effort appears in both the wire request and soak telemetry.
- **Public model benchmarks and Rundale latency answer different questions.** Rundale ranks full production prompts primarily by player-visible TTFT through the exact provider route; benchmark throughput, short prompts, routing tier, and reasoning defaults can reverse apparent speed order. Compare providers only with the same frozen profile.
- **Do not count a same-family model as an independent dialogue judge.** Keep its receipt visible for diagnosis, but promotion needs two other model families; use median consensus and send split verdicts or excessive score spread to adjudication.
- **The published leaderboard site is the v2 Astro app at `promptfoo/bench-site/`** (GitHub Pages, base `/Rundale`). It reads `promptfoo/leaderboard/leaderboard.jsonl` directly at build time (no Python data step) and joins `promptfoo/catalog/candidates.jsonl` for brand/tier. The v1 `rundale-bench/bench-site/` + `build_site_data.py` were retired in #1284.
- **Full 18-item production judge bundles can exhaust a 32K completion allowance through hidden reasoning.** Sonnet 4.6 high repeatedly ended at `finish_reason=length`; low produced complete rubric JSON. Circuit-break judge profiles on non-stop finishes, invalid envelopes, and API failures so one bad profile cannot spend through the queue.
- **Python's cookie jar suppresses Limerick's `Secure` session cookie on loopback HTTP.** A production soak can therefore create a session per request and hit the 50-session admission cap unless its test-only cookie policy explicitly treats `127.0.0.1`, `::1`, and `localhost` as secure origins while continuing to reject external HTTP.
- **A dialogue soak must prove that every denominator row reached inference.** Farewells and other conversation-ending replies can make later addressed commands succeed without an NPC turn; reacquire another live NPC and retry the same sample, while retaining a retry count, instead of recording a false model failure or success.
- **A valid one-chunk SSE completion has latency but no measurable post-first-token throughput.** Do not classify it as an inference error when integer-millisecond `total_ms == ttft_ms`; retain it for latency/error accounting and omit only that row from the throughput distribution.
- **Cloud screening must not import the local dialogue-promotion latency policy.** The local 1s warm-TTFT gate measures prefix-cache viability on owned hardware; routed cloud latency is a comparative ranking signal unless the game has a separately measured player-facing release SLO.
- **Dialogue judging needs both halves of the production prompt.** Passing only dynamic user context hides character identity and PEOPLE/PLACES lists, causing false fabrication findings for authored NPCs such as Roisin. Claude CLI also returns structured billing errors on stdout with a nonzero exit; persist and parse that envelope before deciding how to retry.
- **Cloud dialogue preflight receipts do not retain candidate-provider usage or cost.** `screen_cloud_dialogue.py` preserves outputs and request profiles, while perf and judge receipts retain billed cost; report exact spend as incomplete until the live preflight also records provider usage.
