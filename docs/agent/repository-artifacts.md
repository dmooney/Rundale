# Repository Artifact Policy

Git is the source of truth for code, authored content, canonical fixtures,
rubrics, promotion receipts, and assets required by clean-checkout or offline
gameplay. Reproducible output, research packets, and diagnostic evidence should
not make every clone pay their storage cost.

Run `just repository-artifacts` after adding or moving binary or generated
files. The same gate runs in CI.

## Canonical destinations

| Artifact family                                        | Canonical destination                                                                    | Tracking rule                                                                                                                                                                                            |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Graphify indexes, HTML, reports, caches, and snapshots | Local `graphify-out/` beside the scanned corpus                                          | Ignored at every depth; regenerate locally. Publish a content-addressed release archive only when a frozen graph must be shared.                                                                         |
| Playwright visual baselines                            | `parish/apps/ui/e2e/screenshots/baseline/`                                               | Keep in Git because tests consume them. `just screenshots` exercises this path.                                                                                                                          |
| Documentation images                                   | `docs/screenshots/`                                                                      | Keep only current images referenced by tracked docs or their generation contract. Promote deliberately; do not mirror every Playwright capture.                                                          |
| Bug evidence                                           | Stable `bug-evidence` GitHub Release plus a hash-keyed external archive                  | Reporter screenshots are Release assets linked from their issues. Never track `bug-reports/`; dry-run bundles stay under the resolved user-data path.                                                    |
| Promptfoo output and local proof bundles               | `promptfoo/output/`, `docs/proofs/`, and `.proofs/`                                      | Ignored local output. Keep only canonical datasets, rubrics, manifests, promotion receipts, and published leaderboard data in Git.                                                                       |
| Rundale-bench generated runs                           | External archive for retained historical runs                                            | Do not add new generated runs to Git. Existing v1 artifacts remain temporarily until their approved archive wave.                                                                                        |
| Character art                                          | Generate and review outside the runtime tree; promote only approved release transactions | Approved masters, raw provenance, manifests, and receipts stay in Git while clean/offline builds require them. Experiments and review packets await their approved archive wave.                         |
| Graphics research                                      | Content-addressed external archive with an in-Git path/hash/license index                | Selected authorities and durable procedure/source files stay in Git. Pipeline experiment PNGs were archived in Wave 3; new PNG working output remains untracked until archived or deliberately promoted. |

## Mechanical limits

`parish/scripts/check-repository-artifacts.sh` enforces these rules over the
Git index:

- no tracked path may contain a `graphify-out` component;
- no PNG may be tracked under `docs/graphics-v2/pipeline-experiments/`;
- retired screenshot, every `bug-reports/` path, and rejected scene-plate paths cannot
  be reintroduced;
- files larger than 8 MiB fail unless
  `parish/scripts/repository-artifact-exceptions.txt` records the exact path,
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

Wave 2 (base commit `d9aff21b3de86cdf1339647f399336c5392d4fa3`) removed the
following generated/intermediate files from the current tree. Their exact
original bytes and SHA-256 values are retained here so the deletion is
auditable. The CF pipeline can regenerate its outputs from the tracked source
mosaic and scripts; the original blobs remain recoverable from Git history and
the verified bare mirror `Rundale-pre-rewrite-20260823T215406Z.git` in the
documented operator backup location.

| Path                                                                                                                      | Original bytes | SHA-256                                                            |
| ------------------------------------------------------------------------------------------------------------------------- | -------------: | ------------------------------------------------------------------ |
| `docs/graphics-v2/overhead-art/cycle-cf-production-county-pipeline/masked-seam-repair-template/seam-contract-overlay.png` |        9484931 | `6aa96a2dcef26323cb07148ff168b08820d76a05864aa4519661a0f669442363` |
| `docs/graphics-v2/overhead-art/cycle-cf-production-county-pipeline/seam-validation-overlay.png`                           |        9406614 | `9bc16003c3da3a75e0f588fb8ac2ca2e37cd088deaa10211059156568210f9f9` |
| `docs/graphics-v2/overhead-art/cycle-cf-production-county-pipeline/county-base-grid-overlay.png`                          |        9406614 | `9bc16003c3da3a75e0f588fb8ac2ca2e37cd088deaa10211059156568210f9f9` |
| `docs/graphics-v2/overhead-art/cycle-cf-production-county-pipeline/runtime-reassembled.png`                               |        9341111 | `9fbe7c715828928ba2840d784e154e7a2c761b34e24c907c9dfa5a3e46f3368a` |
| `docs/graphics-v2/overhead-art/cycle-cf-production-county-pipeline/county-base-supertile.png`                             |        9341111 | `9fbe7c715828928ba2840d784e154e7a2c761b34e24c907c9dfa5a3e46f3368a` |
| `parish/apps/ui/art/notebook-person-art/experiments/roisin-art-progression.png`                                           |        8492224 | `becdeaec87bebf0063d7611cec764254948b0fcad6820235a4da3173f3828331` |

### Wave 3: Graphics V2 pipeline experiments

Wave 3 (base commit `b467cae661b95b12606e5c64b7649429aafa3dc4`)
archived all 474 PNGs formerly under
`docs/graphics-v2/pipeline-experiments/`. The verified payload contains
657,902,063 bytes and 393 unique Git blobs. Four clean-checkout inputs were
promoted to `docs/graphics-v2/map-sources/` and
`docs/graphics-v2/authorities/`. The other 470 payload paths accounted for
648,887,496 bytes; after adding the archive index and policy documentation, the
net current-tree reduction is 648,743,872 bytes.

The exact original paths, sizes, SHA-256 values, Git blob IDs, provenance
classes, and licensing obligations are recorded in
[`archive-index.tsv`](../graphics-v2/pipeline-experiments/archive-index.tsv).
The verified iCloud Drive archive ID is
`graphics-v2-pipeline-experiments-b467cae6-20260826T020635Z-manifest-078b3883c20c`;
its full manifest SHA-256 is
`078b3883c20c43e8da72b422329d8b99b82ea893d52206735eb1218bf6d8671e`.
All 474 payload checks passed after the archive was copied. The pre-rewrite
rollback mirror independently contains all 393 blobs.

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
`parish/scripts/check-doc-paths.sh`; the screenshot rule is the reverse check
that catches files no document or generator contract consumes.
