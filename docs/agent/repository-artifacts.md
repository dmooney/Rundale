# Repository Artifact Policy

Git is the source of truth for code, authored content, canonical fixtures,
rubrics, promotion receipts, and assets required by clean-checkout or offline
gameplay. Reproducible output, research packets, and diagnostic evidence should
not make every clone pay their storage cost.

Run `just repository-artifacts` after adding or moving binary or generated
files. The same gate runs in CI.

## Canonical destinations

| Artifact family                                        | Canonical destination                                                                    | Tracking rule                                                                                                                                                                    |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Graphify indexes, HTML, reports, caches, and snapshots | Local `graphify-out/` beside the scanned corpus                                          | Ignored at every depth; regenerate locally. Publish a content-addressed release archive only when a frozen graph must be shared.                                                 |
| Playwright visual baselines                            | `parish/apps/ui/e2e/screenshots/baseline/`                                               | Keep in Git because tests consume them. `just screenshots` exercises this path.                                                                                                  |
| Documentation images                                   | `docs/screenshots/`                                                                      | Keep only current images referenced by tracked docs or their generation contract. Promote deliberately; do not mirror every Playwright capture.                                  |
| Bug evidence                                           | GitHub issue attachment or a hash-keyed external archive                                 | Until the reporter is migrated, only reporter-created, issue-linked files may remain under `bug-reports/`; do not stage diagnostic bundles manually.                             |
| Promptfoo output and local proof bundles               | `promptfoo/output/`, `docs/proofs/`, and `.proofs/`                                      | Ignored local output. Keep only canonical datasets, rubrics, manifests, promotion receipts, and published leaderboard data in Git.                                               |
| Rundale-bench generated runs                           | External archive for retained historical runs                                            | Do not add new generated runs to Git. Existing v1 artifacts remain temporarily until their approved archive wave.                                                                |
| Character art                                          | Generate and review outside the runtime tree; promote only approved release transactions | Approved masters, raw provenance, manifests, and receipts stay in Git while clean/offline builds require them. Experiments and review packets await their approved archive wave. |
| Graphics research                                      | Content-addressed external archive with an in-Git path/hash/license index                | The selected notebook authority and durable procedure/source files stay in Git. Existing bulk binaries await their approved archive wave.                                        |

## Mechanical limits

`parish/scripts/check-repository-artifacts.sh` enforces these rules over the
Git index:

- no tracked path may contain a `graphify-out` component;
- retired screenshot, orphan bug-image, and rejected scene-plate paths cannot
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

Forward Markdown links remain covered by
`parish/scripts/check-doc-paths.sh`; the screenshot rule is the reverse check
that catches files no document or generator contract consumes.
