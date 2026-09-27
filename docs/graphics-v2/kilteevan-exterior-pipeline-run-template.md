# Kilteevan Exterior Pipeline Run Template

Use this template for each Graphics V2 Kilteevan parish exterior attempt. Replace
`<location-slug>` with a stable lowercase slug such as `murphy-farm`,
`grove`, or `beechwood`.

The governing process is `map-to-bu-style-reproducible-pipeline.md`. This file
is only the per-location checklist.

## Scope

- Location:
- World/source coordinate or map anchor:
- Exterior type:
- Intended playable crop scale:
- Historical map source:
- Date started:
- Coordinator:

## Artifact Manifest

PNG entries below are ignored local working paths and become archive-relative
keys when a run is retained. Prompt, report, and audit Markdown sidecars remain
tracked. Promote only reviewed inputs required by a clean checkout into
`map-sources/` or `authorities/`.

- Source crop: `pipeline-experiments/idea-<cycle>-<location-slug>-source-map-crop.png`
- Source/crop report: `pipeline-experiments/idea-<cycle>-<location-slug>-source-map-crop.report.md`
- Map-reader notes: `pipeline-experiments/idea-<cycle>-<location-slug>-map-reader-notes.md`
- Topology/control: `pipeline-experiments/idea-<cycle>-<location-slug>-topology-control.png`
- Control report: `pipeline-experiments/idea-<cycle>-<location-slug>-topology-control.report.md`
- Oblique cue: `pipeline-experiments/idea-<cycle>-<location-slug>-oblique-cue.png`
- Prompt sidecar: `pipeline-experiments/idea-<cycle>-<location-slug>-bu-style.prompt.md`
- Direct render: `pipeline-experiments/idea-<cycle>-<location-slug>-bu-style.png`
- Direct render report: `pipeline-experiments/idea-<cycle>-<location-slug>-bu-style.report.md`
- Bounded correction, if used: `pipeline-experiments/idea-<cycle>-<location-slug>-bounded-<failure>.png`
- Comparison plate: `pipeline-experiments/idea-<cycle>-<location-slug>-pipeline-comparison.png`

## Subagent Checklist

### 1. Map-Reader Subagent

Inputs:

- source crop,
- `map-reader-stage-template.md`.

Output:

- saved map-reader notes.

Pass criteria:

- building inventory is confidence-graded,
- roads/lanes and single boundaries are separated,
- dotted/pecked/admin/survey linework is marked nonphysical unless
  corroborated,
- explicit negative evidence is recorded for church, graveyard, water, bridge,
  shop, smoke, UI, labels, people, animals, and extra landmarks.

### 2. Control-Builder Subagent

Inputs:

- source crop,
- map-reader notes,
- deterministic control script or documented control method.

Output:

- topology/control artifact,
- oblique/perspective cue,
- control report.

Pass criteria:

- north remains up,
- crop and camera scale are explicit,
- symbols/marks are documented,
- nonphysical survey/admin linework is suppressed or clearly marked as ignore,
- no topology is invented from uncertain notes.

### 3. Prompt-Builder Subagent

Inputs:

- source crop,
- map-reader notes,
- topology/control artifact,
- oblique cue,
- BU E2 style target,
- approved door-fixed style crops,
- regional boundary/wall rules.

Output:

- prompt sidecar.

Pass criteria:

- image roles are explicit,
- geometry and perspective precede style,
- BU E2 is style/material only,
- door-fixed crops are door/threshold material only,
- no Beechwood/Grove/Murphy prior layout is copied,
- no full-scene semantic leakage is permitted.

### 4. Render Subagent

Inputs:

- prompt sidecar,
- declared reference images only.

Output:

- direct render saved under the ignored working path and ingested into the
  external archive if retained.

Pass criteria:

- render subagent calls imagegen where available,
- no prior failed/generated renders for the same location are visible,
- output is archived by path/hash while prompt/report sidecars remain tracked,
- coordinator-called imagegen, if unavoidable, is reported as a pipeline
  exception.

### 5. Independent Audit Subagent

Inputs:

- source crop,
- map-reader notes,
- topology/control artifact,
- oblique cue,
- direct render,
- BU E2 style target,
- door audit crops as needed.

Output:

- audit report and comparison plate.

Pass criteria:

- geometry, perspective, style, doors, historical semantics, boundary
  handling, wall material, and reproducibility are each marked pass/fail/caveat,
- continuous stone wall grids on source-ambiguous field/enclosure boundaries
  are called out as a blocking batch-readiness caveat even if the image is
  otherwise attractive,
- any proposed correction is one concrete bounded edit,
- broad topology failure sends the work back upstream instead of into polish.

## Acceptance Summary

Geometry:

- Pass / Fail / Caveat:

Perspective:

- Pass / Fail / Caveat:

Style:

- Pass / Fail / Caveat:

Doors:

- Pass / Fail / Caveat:

Historical semantics:

- Pass / Fail / Caveat:

Boundary and wall material:

- Pass / Fail / Caveat:

Reproducibility:

- Pass / Fail / Caveat:

## Disposition

Choose one:

- Accepted direct recipe evidence.
- Accepted visual target after one bounded correction.
- Candidate only; needs clean rerun.
- Rejected; revise map-reader/control inputs.

Notes:
