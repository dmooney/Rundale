# docs/agent — agent scope

Reference directory for AI coding agents and human contributors. Human-facing entry point is [`README.md`](README.md). Historical proof archives live in the ignored, iCloud-backed local `docs/proofs/` directory; screenshots and design artifacts live in `docs/screenshots/`, `docs/adr/`, `docs/design/`, `docs/plans/`, `docs/research/`, `docs/reviews/`, and `docs/audits/`.

## Scoped commands

```sh
just screenshots                                    # exercise Playwright screenshot baselines
bash parish/scripts/check-doc-paths.sh              # validate backtick-quoted paths in docs
bash parish/scripts/check-repository-artifacts.sh   # enforce generated/binary artifact policy
just agent-check                                    # proof evidence + judge verdict gate
witness-scan                                        # catch AI partial-completion markers
```

## Local gotchas

- **Docs must stay in sync with code.** `check-doc-paths.sh` (part of `just check` / CI `docs-consistency`) rejects broken relative Markdown links across active docs and nonexistent backtick-quoted agent paths. Update the doc before committing any file or module rename it cites.
- **Start orientation at [`codebase-map.md`](codebase-map.md).** Keep its `Parish Crates` table and repository-layout table fresh.
- **[`gotchas.md`](gotchas.md) is the most mutation-prone file** — Tokio, SQLite, Ollama, and mode-parity pitfalls change as tooling evolves.
- **Documentation screenshots live in `docs/screenshots/`** and must stay referenced. `just screenshots` exercises or updates Playwright baselines under `parish/apps/ui/e2e/screenshots/baseline/`; promotion into documentation is an explicit review step.
- **Generated and large artifacts follow [`repository-artifacts.md`](repository-artifacts.md).** The repository gate rejects tracked Graphify output, retired artifact paths, unapproved files over 8 MiB, and unreferenced documentation screenshots.
- **Proof archives in local `docs/proofs/`** — ignored by Git and expected to resolve to the iCloud-backed archive. Per-task bundles go in `.proofs/<task-id>/` (also gitignored); publish concise hashes and summaries in tracked docs or PR bodies.
- **Witness scan blocks merge.** Docs with partial-completion markers (`[...]`, `TODO` in code blocks, unfinished sentences before stop-tokens) fail `witness-scan`, which gates `just check` and `just verify`.
- **Scaling guardrails (rule #11)** are in [scaling-rules.md](scaling-rules.md). Every entry-point crate AGENTS.md links here — edits ripple across the workspace.
- **[`act-local.md`](act-local.md)** is the source of truth for `.actrc` and the `act-*` justfile recipes.

## Documentation routing

Use [README.md](README.md) as the maintained index rather than duplicating its
full inventory here. Current requirements live in [product specs](../product-specs/README.md).
The [engineering rule index](engineering-rules.md) preserves the original rule
numbers and routes to scoped references; root AGENTS.md is only the short map.

When reorganizing docs, preserve requirement wording and provenance, repair
relative links, and distinguish current requirements, proposed architecture,
existing implementation, and historical product directions. Do not turn a
requested future verification command into a claim that it exists or passed.
