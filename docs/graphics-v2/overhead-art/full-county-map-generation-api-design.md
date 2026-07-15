# Full County Map Generation And Asset Versioning

Status: planning note  
Date: 2026-07-04  
Scope: Graphics V2 overhead map art for County Roscommon / Rundale

## Summary

The full game map should be generated as a local deterministic county base,
stored as large binary assets outside Git, and versioned with a binary-first
asset VCS such as Lore once a small pilot proves the workflow. The OpenAI API
should be used for reproducible high-value local art overrides and masked seam
repairs, not as the default way to repaint every county tile.

If we deliberately use the Image API to generate the entire clipped county at
3x gameplay scale, current output-only cost is roughly:

| Scope                                      | Panel-equivalent outputs | GPT Image 2 medium output | GPT Image 2 high output |
| ------------------------------------------ | -----------------------: | ------------------------: | ----------------------: |
| Clipped county, no overlap                 |                  ~29,144 |                   ~$1,195 |                 ~$4,809 |
| Clipped county, 1.5x overlap/retry budget  |                  ~43,716 |                   ~$1,792 |                 ~$7,213 |
| Clipped county, 2.0x conservative budget   |                  ~58,288 |                   ~$2,390 |                 ~$9,618 |
| Rectangular bbox, no clipping              |                  ~76,983 |                   ~$3,156 |                ~$12,702 |
| Rectangular bbox, 2.0x conservative budget |                 ~153,966 |                   ~$6,313 |                ~$25,404 |

Those rows are output image cost only. Edit/reference-image workflows also pay
image input token costs. For a full clipped county run, a realistic all-in
budget range is:

- GPT Image 2 medium: about `$3k-$8k`;
- GPT Image 2 high: about `$8k-$18k`;
- rectangular bbox instead of county clipping: multiply by about `2.6x`.

These are planning estimates, not quotes. A 100-panel pilot should measure the
actual input-token count, retry rate, rejected-panel rate, and seam-repair rate
before buying a full-county API run.

## Source Facts And Assumptions

- Current NLS Roscommon source appears to top out at z17 for the Murphy proof
  area. A 2026-07-04 probe returned HTTP 200 at z17 and HTTP 404 at z18/z19.
- The Cycle CF proof measured average compressed tile sizes:
  - source NLS z17 PNG: about `34 KB` per `256x256` tile;
  - generated deterministic runtime PNG: about `94 KB` per `256x256` tile.
- Approximate County Roscommon z17 coverage:
  - clipped county estimate: about `77,715` z17 tiles;
  - rectangular working bbox estimate: about `205,288` z17 tiles.
- 3x gameplay scale multiplies pixels by `9x`.
- A full downsampled pyramid below the max layer adds at most about `33%` over
  the max-resolution layer.
- Cost estimates use `1536x1024` output panel equivalents because GPT Image 2
  officially lists that as a common landscape size, while also supporting many
  other valid sizes.

## Data Volume

| Asset set                                 | Clipped county | Rectangular working bbox |
| ----------------------------------------- | -------------: | -----------------------: |
| z17 source tiles                          |        ~77,715 |                 ~205,288 |
| Source compressed PNG                     |        ~2.7 GB |                  ~7.0 GB |
| Source raw RGB working data               |         ~15 GB |                   ~40 GB |
| 3x generated max layer                    |       ~65.7 GB |                ~173.6 GB |
| 3x generated max layer plus lower pyramid |         ~87 GB |                  ~231 GB |
| 3x raw RGB working data                   |        ~138 GB |                  ~363 GB |

The full county map is feasible, but it should not be one giant raster and it
should not be committed to Git. Process it in chunks, store immutable assets in
Lore/object storage, and ship the game with packed local map bundles.

## Storage And Versioning

Use three layers:

1. **Git** for code, docs, prompt templates, small manifests, validation
   summaries, and release pointers.
2. **Lore** for heavy source and generated assets once the pilot is proven.
   Lore is a good fit because it is binary-first, content-addressed, chunked,
   branchable, and designed for games/entertainment projects with large
   binary assets.
3. **Packed game delivery artifacts** such as PMTiles, MBTiles, or a compressed
   tile bundle installed locally with the game.

Recommended layout:

```text
docs/graphics-v2/overhead-art/
  full-county-map-generation-api-design.md
  county-tile-continuity-plan.md

map-releases/
  roscommon-overhead-v0.1.json

Lore repository:
  maps/roscommon/sources/nls-os6-z17/<revision>/
  maps/roscommon/base/deterministic-v1/<revision>/
  maps/roscommon/local-overrides/api-watercolor-v1/<revision>/
  maps/roscommon/releases/roscommon-overhead-v0.1/
```

Git-tracked release manifest:

```json
{
  "id": "roscommon-overhead-v0.1",
  "status": "candidate",
  "lore_repo": "rundale-map-assets",
  "lore_revision": "REPLACE_WITH_LORE_REVISION",
  "source_revision": "nls-os6-z17@REPLACE_WITH_REVISION",
  "pipeline_git_sha": "REPLACE_WITH_GIT_SHA",
  "max_layer": {
    "scale": 3,
    "format": "pmtiles",
    "sha256": "REPLACE_WITH_HASH"
  },
  "validation": {
    "max_seam_to_control_ratio": 1.15,
    "max_abs_reassembly_error": 0,
    "contact_sheet": "validation/contact-sheet.png"
  }
}
```

Lore caution: Lore is currently pre-stable `0.x`. Its docs say data committed
now is intended to remain readable by future releases, but APIs and protocols
may evolve before `1.0`. Use it first as an asset-store pilot, not as the only
copy of irreplaceable data.

## Zoom Levels For A Local Install

The game does not need web-style aggressive zoom pyramids for bandwidth, but it
still benefits from mip/LOD levels for rendering performance and visual quality.

Recommended:

- Generate one max gameplay layer at the chosen 3x walking scale.
- Generate lower overview layers deterministically from the max/base layers.
- Do not call the Image API separately for lower zoom levels.
- Use lower levels for county map, parish map, minimap, and travel UI.
- Use the max layer for walking and interaction.

The lower pyramid is cheap compared with the max layer: roughly `33%` extra
storage if every level below max is retained.

## API Usage Policy

Use ChatGPT Pro for exploratory visual direction only:

- prompt exploration;
- a few sample plates;
- human visual review;
- one-off local repair experiments.

Use the OpenAI API for any production or reproducible generation:

- scripted runs;
- saved prompts and parameters;
- request IDs and retries;
- cost tracking;
- batch submission;
- content hashing;
- validation reports;
- asset manifests.

Do not automate ChatGPT consumer UI as a bulk renderer. The asset pipeline
should call the API directly.

## Proposed Tool

Add a production tool, either as a new script or as an expansion of
`docs/graphics-v2/scripts/county_tile_pipeline.py`:

```text
docs/graphics-v2/scripts/full_county_map_tool.py
```

### Commands

```text
plan
  Read county boundary, source zoom, target scale, tile size, panel size, and
  overlap policy. Write the planned chunk grid, estimated storage, estimated API
  cost, and expected job count.

ingest-source
  Download/cache NLS z17 tiles, record HTTP status, tile hashes, source URL
  template, attribution, and missing-tile gaps.

build-base
  Render deterministic continuous county-base chunks locally. Export runtime
  tiles, seam contracts, semantic masks, and contact sheets.

build-pyramid
  Generate lower zoom levels from accepted base/max layers. Never use imagegen
  for these lower levels.

prepare-api-jobs
  Create API job records for high-value local art overrides or, if explicitly
  enabled, full-county imagegen. Render source/control/reference inputs and
  write per-job cost estimates.

submit-api-batch
  Submit JSONL batch jobs for image generation or image edits when the request
  shape is compatible with Batch. Otherwise submit standard API calls with
  throttling and checkpointing.

poll-api-batch
  Resume-safe status polling. Store OpenAI request IDs, batch IDs, timestamps,
  costs, and errors.

collect-api-results
  Decode generated images, hash outputs, normalize dimensions, crop safe
  centers, split runtime tiles, and write provenance.

validate
  Reassemble tiles, compute seam ratios, compare against seam contracts, detect
  missing/blank/degenerate tiles, and build contact sheets.

repair-seams
  Run deterministic local repairs first. Use API masked edits only for
  color/style discontinuity or explicitly reviewed seam patches.

promote
  Move a validated run to a release candidate, write the Git manifest, and
  publish large assets to Lore.

lore-publish
  Import sources, generated tiles, packed bundles, validation sheets, and
  repair artifacts into a Lore repository. Return the Lore revision hash for
  the Git release manifest.
```

### Job State

Use a SQLite job ledger plus JSON manifests:

```text
runs/<run-id>/
  run-manifest.json
  jobs.sqlite
  planned-grid.geojson
  source/
  controls/
  api-inputs/
  api-results/
  runtime-tiles/
  validation/
  release/
```

Each job row should include:

```json
{
  "job_id": "roscommon-z17-c0032-r0048",
  "kind": "base | local_override | seam_repair | full_api_panel",
  "status": "planned | inputs_ready | submitted | complete | validated | rejected | promoted",
  "source_tile_range": { "z": 17, "x0": 0, "x1": 0, "y0": 0, "y1": 0 },
  "output_size": "1536x1024",
  "quality": "medium",
  "model": "gpt-image-2",
  "openai_request_id": null,
  "input_sha256": [],
  "output_sha256": null,
  "estimated_cost_usd": 0.0,
  "actual_cost_observed": null,
  "validation": {
    "max_seam_to_control_ratio": null,
    "blank_check": null,
    "topology_review": "pending"
  }
}
```

### Failure Policy

- Missing source tiles block the affected chunk unless an explicit gap-fill
  policy is approved.
- API outputs that are blank, wrong size, unreadable, or off-layout are
  rejected, not repaired.
- Independent adjacent API panels are never trusted by overlap alone.
- A seam repair can pass metrics only with a before/after contact sheet and a
  topology-drift note.
- Full-county API generation must be behind a `--allow-full-api-county` flag
  and must print a cost estimate requiring explicit confirmation.

## Cost Model

The current OpenAI pricing page lists GPT Image 2 image generation at:

- standard: `$8/M` image input tokens and `$30/M` output tokens;
- batch: `$4/M` image input tokens and `$15/M` output tokens.

The image-generation guide's calculator currently lists GPT Image 2
`1536x1024` output costs as:

- low: `$0.005`;
- medium: `$0.041`;
- high: `$0.165`.

Full clipped county, 3x gameplay scale:

```text
77,715 z17 tiles * 256 * 256 pixels * 9 scale = 45.84 billion output pixels
1536 * 1024 = 1.57 million pixels per panel equivalent
45.84B / 1.57M = about 29,144 panel-equivalent outputs
```

Output-only cost:

| Multiplier | Panel equivalents |  Low | Medium |   High |
| ---------: | ----------------: | ---: | -----: | -----: |
|        1.0 |            29,144 | $146 | $1,195 | $4,809 |
|       1.25 |            36,430 | $182 | $1,494 | $6,011 |
|        1.5 |            43,716 | $219 | $1,792 | $7,213 |
|        2.0 |            58,288 | $291 | $2,390 | $9,618 |

Input token cost is the main uncertainty. If each request uses source/control
images equivalent to `2k-12k` image input tokens, then standard GPT Image 2
input cost adds approximately:

| Panel count | 2k input tokens/request |     4k |     8k |    12k |
| ----------: | ----------------------: | -----: | -----: | -----: |
|      29,144 |                    $466 |   $933 | $1,865 | $2,798 |
|      43,716 |                    $699 | $1,399 | $2,798 | $4,197 |
|      58,288 |                    $933 | $1,865 | $3,730 | $5,596 |

Batch pricing can halve those token costs if the request shape fits Batch and
the run can tolerate asynchronous 24-hour processing.

Budget guardrails:

- Full deterministic local county base: API cost `$0`.
- Named local overrides only, for example 200 medium panels plus repair budget:
  likely hundreds of dollars, not thousands.
- Full clipped county via GPT Image 2 medium: reserve `$3k-$8k`.
- Full clipped county via GPT Image 2 high: reserve `$8k-$18k`.
- Full rectangular bbox instead of clipped county: multiply by about `2.6x`.

## Recommendation

1. Generate the full county base locally with the deterministic pipeline.
2. Version source and generated heavy assets in a Lore pilot repository.
3. Ship PMTiles/MBTiles or packed local tile bundles with the game.
4. Use the API for local override plates around gameplay-dense exteriors.
5. Run a 100-panel API pilot before any full-county imagegen decision.
6. Avoid full-county API generation unless the deterministic county base fails
   the art target after cheaper local styling passes.

## Source Notes

- OpenAI pricing, image generation models:
  <https://platform.openai.com/docs/pricing>
- OpenAI image generation guide, size/quality/cost notes:
  <https://platform.openai.com/docs/guides/image-generation>
- OpenAI Batch API, 50% discount and image endpoint support:
  <https://platform.openai.com/docs/guides/batch>
- Lore overview:
  <https://lore.org/>
- Lore FAQ:
  <https://epicgames.github.io/lore/faq/>
