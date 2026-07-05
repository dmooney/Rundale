# Isomorphic Transform Calibration Cycle BM

## Purpose

Cycle BM implements the bounded experiment plan for the top-down/control to
low-3/4 isomorphic transform. The specific question was whether a more explicit
camera/crop transform could move the current map-accurate plates closer to the
original illustrated parish notebook camera: lower, closer, larger facades and
doors, less survey-board zoom.

This was deliberately capped at six imagegen renders. E7/E8 were not used
because the matrix already isolated the main signal.

## Deterministic Setup

The calibration assets are documented in
`pipeline-experiments/idea-bm-transform-calibration-assets.report.md`.

- z18 NLS tiles for Kilteevan were attempted and were unavailable from the
  configured tile source, so E3/E4 use the native z17 crop as the higher-source
  fallback instead of continuing to treat the 512x288 playable crop as highest
  authority.
- The camera/crop candidates were:
  - 70% crop with `y_squash=0.48`,
  - 55% crop with `y_squash=0.40`,
  - native z17 source with the winning 55% / `0.40` transform.
- The deterministic oblique cues are pitch cues only. They are not content
  authorities and their beige margins/strip composition should not be copied.

## Outputs

| ID  | Image                                                                            | Prompt                                                                                 | Report                                                                                 | Result                                                                     |
| --- | -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| E1  | `pipeline-experiments/idea-bm-e1-kilteevan-70-y048-isomorphic.png`               | `pipeline-experiments/idea-bm-e1-kilteevan-70-y048-isomorphic.prompt.md`               | `pipeline-experiments/idea-bm-e1-kilteevan-70-y048-isomorphic.report.md`               | Partial camera improvement; still high/estate-board-like                   |
| E2  | `pipeline-experiments/idea-bm-e2-kilteevan-55-y040-isomorphic.png`               | `pipeline-experiments/idea-bm-e2-kilteevan-55-y040-isomorphic.prompt.md`               | `pipeline-experiments/idea-bm-e2-kilteevan-55-y040-isomorphic.report.md`               | Strongest pure camera/zoom signal; walls too emphatic                      |
| E3  | `pipeline-experiments/idea-bm-e3-kilteevan-z17-55-y040-isomorphic.png`           | `pipeline-experiments/idea-bm-e3-kilteevan-z17-55-y040-isomorphic.prompt.md`           | `pipeline-experiments/idea-bm-e3-kilteevan-z17-55-y040-isomorphic.report.md`           | Native z17 source helps source authority but not wall semantics by itself  |
| E4  | `pipeline-experiments/idea-bm-e4-kilteevan-z17-55-y040-path-wall-isomorphic.png` | `pipeline-experiments/idea-bm-e4-kilteevan-z17-55-y040-path-wall-isomorphic.prompt.md` | `pipeline-experiments/idea-bm-e4-kilteevan-z17-55-y040-path-wall-isomorphic.report.md` | Best Kilteevan BM candidate; lower/closer with softer wall semantics       |
| E5  | `pipeline-experiments/idea-bm-e5-beechwood-55-y040-isomorphic.png`               | `pipeline-experiments/idea-bm-e5-beechwood-55-y040-isomorphic.prompt.md`               | `pipeline-experiments/idea-bm-e5-beechwood-55-y040-isomorphic.report.md`               | Strong Beechwood generalization; connected compound survives               |
| E6  | `pipeline-experiments/idea-bm-e6-grove-55-y040-isomorphic.png`                   | `pipeline-experiments/idea-bm-e6-grove-55-y040-isomorphic.prompt.md`                   | `pipeline-experiments/idea-bm-e6-grove-55-y040-isomorphic.report.md`                   | Good Grove lower-camera generalization; some garden simplification remains |

Comparison plates live in `docs/graphics-v2/cartographic-comparisons/`, with
`bm-isomorphic-transform-matrix-contact-sheet.png` showing every candidate as:

```text
source map -> top-down/topology control -> lower-pitch cue -> render
```

## Verdict

The plan's main camera hypothesis passed. The next transform baseline should be:

```text
closer source/control crop around the playable building-yard-garden core
  -> deterministic lower-pitch cue at y_squash ~= 0.40
  -> low 3/4 notebook render with rare-wall/path wording
```

The 70% / `0.48` branch was too timid. The 55% / `0.40` branch produces the
first renders in this thread that clearly move toward the original concept art
camera and zoom while remaining recognizable as the source topology.

The unresolved problem is no longer primarily camera wording. It is upstream
feature semantics:

- paired pale/dashed corridors are still fragile and can be lost or over-walled,
- garden rows and internal planted marks still want to become physical walls,
- single linework can still be over-promoted into hard boundaries or paths.

E4 proves stronger prose helps, but not enough to freeze the recipe. The next
real improvement should be a deterministic material/semantic control that marks:

- roads/tracks as open walkable wear,
- garden/orchard interiors as soft planting texture,
- wallable edges as rare and explicit,
- ordinary parcel/admin/no-data linework as no-trace or low-confidence texture.

## Current Recommendation

Use E4 as the current Kilteevan BM candidate, E5 as the strongest evidence that
the transform generalizes to Beechwood's connected-compound topology, and E6 as
the Grove separate-building check. Do not run more prompt-only camera retries;
the camera/zoom setting is good enough to make the next experiment about
path/wall/garden controls.
