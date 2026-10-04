# Graphics V2 preservation branch

`feat/graphics-v2` is the long-lived home for the graphics V2 research. Its draft
pull request is a tracking record; keep it open and do not merge it into main.

The working tree preserves the complete current corpus from main at
`3c2345a904ba0691046c46b7794d2e9d51407c9a`, including
[the research index](docs/graphics-v2/README.md), the map/plate tools and their
source/provenance records, and the notebook portrait experiments and review packets.

The preservation merge also retains these earlier branch tips as parents, without
replacing current files with older versions:

| Original branch                              | Preserved tip                              | Contents                                                          |
| -------------------------------------------- | ------------------------------------------ | ----------------------------------------------------------------- |
| `codex/graphics-v2-map-pipeline-experiments` | `f0879eb776a0dd804da3c7b3855b4ff00c7a3d8f` | Early notebook/map pipeline experiments                           |
| `codex/graphics-v2-style-crop-doors`         | `2510ebe6e6faf8b847e7bc28329cd16c6f5e17c8` | Earlier graphics corpus, style-crop fixes, and map annotator work |
| `graphic`                                    | `ab0fd1c5bee9af647bb04190df1411656c1400d5` | Visual client and Kilteevan sprite compositor                     |

The follow-up preservation merge also retains local graphical work that had not
reached the remote branch tips:

| Local branch                      | Preserved tip                              | Contents                      |
| --------------------------------- | ------------------------------------------ | ----------------------------- |
| `graphic`                         | `c3873c7002a3919482374ae3d7b1ac224af0e911` | Later graphical-game planning |
| `codex/graphic-compositor-m1`     | `65a9a371b2ccc0bb249acb2dff0d0548c3bc6156` | Compositor milestone work     |
| `codex/graphic-main-ci-sync`      | `c4263f2732e52fc8b9d40571483a125556da1036` | Graphical/main CI integration |
| `agent/durable-graphical-harness` | `19d4c504071e7e74e7d36d0e675dce4c4ee428fc` | Graphical harness work        |

Browse a preserved tip with `git show <tip>:<path>` or create a separate worktree
at that tip to run its historical tools. Those snapshots use the layouts and
requirements of their time; the current mobile product specs govern main.

The companion removal PR moves exploratory research off main. Approved art and
inputs still required by maintained builds remain there. This preserves access
without rewriting history or restoring retired graphical clients on main.
