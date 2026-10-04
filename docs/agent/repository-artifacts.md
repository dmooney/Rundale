# Repository Artifact Policy

Git is the source of truth for code, authored content, canonical fixtures,
rubrics, promotion receipts, and assets required by clean-checkout or offline
gameplay. Reproducible output, research packets, and diagnostic evidence should
not make every clone pay their storage cost.

Run `just repository-artifacts` after adding or moving binary or generated
files. The same gate runs in CI.

## Canonical destinations

| Artifact family                                        | Canonical destination                                                                         | Tracking rule                                                                                                                                                                                            |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Graphify indexes, HTML, reports, caches, and snapshots | Local `graphify-out/` beside the scanned corpus                                               | Ignored at every depth; regenerate locally. Publish a content-addressed release archive only when a frozen graph must be shared.                                                                         |
| Playwright visual baselines                            | `limerick/apps/ui/e2e/screenshots/baseline/`                                                  | Keep in Git because tests consume them. `just screenshots` exercises this path.                                                                                                                          |
| Documentation images                                   | `docs/screenshots/`                                                                           | Keep only current images referenced by tracked docs or their generation contract. Promote deliberately; do not mirror every Playwright capture.                                                          |
| Bug evidence                                           | Stable `bug-evidence` GitHub Release plus a hash-keyed external archive                       | Reporter screenshots are Release assets linked from their issues. Never track `bug-reports/`; dry-run bundles stay under the resolved user-data path.                                                    |
| PR recordings and evidence pages                       | Public [`rundale-pages`](https://github.com/dmooney/rundale-pages) repository, `pr/<number>/` | Never track videos here. Publish with `limerick/scripts/publish-pr-page.sh`; GitHub Pages serves them, and the PR body links the page.                                                                   |
| Promptfoo output, proof archives, and retired bundles  | `promptfoo/output/`, `docs/proofs/`, and `.proofs/`                                           | Ignored local output. Keep only canonical datasets, rubrics, manifests, promotion receipts, and published leaderboard data in Git.                                                                       |
| Rundale-bench generated runs                           | External archive for retained historical runs                                                 | Do not add new generated runs to Git. Existing v1 artifacts remain temporarily until their approved archive wave.                                                                                        |
| Character art                                          | Generate and review outside the runtime tree; promote only approved release transactions      | Approved masters, raw provenance, manifests, and receipts stay in Git while clean/offline builds require them. Experiments and review packets await their approved archive wave.                         |
| Graphics research                                      | Content-addressed external archive with an in-Git path/hash/license index                     | Selected authorities and durable procedure/source files stay in Git. Pipeline experiment PNGs were archived in Wave 3; new PNG working output remains untracked until archived or deliberately promoted. |

## Mechanical limits

`limerick/scripts/check-repository-artifacts.sh` enforces these rules over the
Git index:

- no tracked path may contain a `graphify-out` component;
- no PNG may be tracked under the retired graphics-v2 pipeline-experiments folder; the checker test guards against reintroduction. Its historical contents remain on the [preserved graphics-v2 branch](https://github.com/dmooney/Rundale/tree/feat/graphics-v2);
- retired screenshot, every `bug-reports/` path, and rejected scene-plate paths cannot
  be reintroduced;
- files larger than 8 MiB fail unless
  `limerick/scripts/repository-artifact-exceptions.txt` records the exact path,
  byte count, SHA-256, owner, and purpose;
- tracked files larger than 2 MiB produce an advisory summary so reviewers can
  catch growth before it reaches the hard ceiling; and
- every PNG under `docs/screenshots/` must be referenced by tracked source or
  documentation, or have an exact hash-bound exception.

Exceptions are frozen compatibility records, not wildcard permission. Updating
one requires intentional review of shipping/offline needs, provenance, license,
and the appropriate external destination. Remove an exception as soon as its
file is optimized or archived.

## Retirement ledger

The Graphics V2 research corpus and the person-art experiment and review-packet
files were removed from the active tree. Their full source history remains on the
preserved [`feat/graphics-v2` branch](https://github.com/dmooney/Rundale/tree/feat/graphics-v2).
See [`graphics-v2-archive.md`](../graphics-v2-archive.md) for the removal scope,
retained generation inputs, and hashes. Approved person-art records remain
unchanged and keep their original historical source paths.

### Wave 4: Bug-report screenshots

Wave 4 (base commit `4e95b3027f54475426d1923dae1f98bd26215ba2`)
archived and retired all 22 tracked root `bug-reports/*.png` files. Their
35,533,311 original bytes comprise 21 unique Git blobs totaling 28,793,485
bytes. The complete path, size, SHA-256, blob, issue, old URL, and Release URL
mapping is recorded in
[`bug-evidence-wave4-ledger.tsv`](bug-evidence-wave4-ledger.tsv).

The verified iCloud Drive archive ID is
`bug-evidence-wave4-20260828T160101Z`; its manifest SHA-256 is
`70f10ce18ac52baadd0e60567e8f38dc10e746ddf1ebb77f6e77a0c33383b9c8`.
It also preserves complete pre-edit JSON snapshots for all 22 linked issues.
The same PNG bytes are available as assets on the stable GitHub Release tagged
[`bug-evidence`](https://github.com/dmooney/Rundale/releases/tag/bug-evidence),
and the issue bodies now use those Release download URLs. Future live reports
upload uniquely named PNG assets to that Release; the GitHub Contents API is no
longer used for reporter evidence. A future history rewrite must retarget the
Release tag during cutover; leaving `refs/tags/bug-evidence` on this base commit
would keep its old object graph reachable even after branch refs were rewritten.

Forward Markdown links remain covered by
`limerick/scripts/check-doc-paths.sh`; the screenshot rule is the reverse check
that catches files no document or generator contract consumes.
