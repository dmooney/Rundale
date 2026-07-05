# Graphics V2 Overhead Map Research Consolidation

Status: consolidated research note
Date: 2026-07-04
Scope: overhead historic-map-to-gameplay-map experiments, storage/versioning,
county/parish scale estimates, and provider image-generation tests

## Executive Read

The most reliable path is not to generate every map tile with an image model.
The map should be built as a deterministic local base layer from the historic
OS 6-inch source tiles, then use image generation only for small, important
local override plates that can be manually reviewed.

The image models tested so far can make attractive watercolor map art, but they
do not preserve geometry tightly enough to be trusted as the authoritative
gameplay surface. They straighten roads, regularize fields, shift building
relationships, and invent plausible enclosures. Cost is not the main blocker;
map fidelity is.

## Current Recommendation

1. Generate the parish/county base locally from source map tiles.
2. Keep the base map north-up and overhead.
3. Split runtime tiles mechanically from continuous parent mosaics/supertiles.
4. Validate seam continuity and lossless reassembly.
5. Store large source/generated assets outside Git, ideally in a Lore pilot
   repository once the workflow is proven.
6. Keep Git as the source of truth for code, docs, prompts, manifests, and
   release pointers.
7. Use the API only for high-value local override plates and masked repairs,
   with semantic controls and human review.

In plain terms: use the boring deterministic pipeline for geography, and use
image generation only where art quality matters enough to justify review.

## Proven Pipeline Results

Cycle CE proved the basic continuity rule:

- independent runtime-tile styling creates visible grid artifacts;
- rendering/stylizing one continuous supertile and then splitting it works;
- a continuous imagegen supertile can split cleanly;
- adjacent independently generated imagegen panels do not match reliably by
  overlap alone.

Cycle CF turned that into a production-shaped proof:

- source: real NLS Roscommon z17 tiles around Murphy's Farm;
- proof extent: `10x10` source tile area;
- exported runtime tiles: `100`;
- max reassembly error: `0`;
- max seam ratio: about `1.086`, below the `1.15` threshold;
- seam contracts: `18`;
- repair proof: a known failed adjacent-imagegen seam improved from `2.93` to
  `0.76` inside a bounded seam band, while still requiring topology review.

Key artifacts:

- `cycle-ce-county-tile-continuity/`
- `cycle-cf-production-county-pipeline/`
- `county-tile-continuity-plan.md`
- `full-county-map-generation-api-design.md`

## Data Scale

With the currently available NLS Roscommon source, the practical highest source
zoom is z17. A spot probe around Murphy's Farm returned z17 tiles and returned
404 for z18/z19.

Approximate full County Roscommon scale:

| Scope                   | Source z17 tiles | Source PNGs | 3x generated max layer | 3x plus lower pyramid |
| ----------------------- | ---------------: | ----------: | ---------------------: | --------------------: |
| Clipped county          |          ~77,715 |     ~2.7 GB |               ~65.7 GB |                ~87 GB |
| Rectangular county bbox |         ~205,288 |     ~7.0 GB |              ~173.6 GB |               ~231 GB |

Approximate Kilteevan Civil Parish scale:

| Scope                   | Source z17 tiles | Source PNGs | 3x generated max layer | 3x plus lower pyramid |
| ----------------------- | ---------------: | ----------: | ---------------------: | --------------------: |
| Clipped parish          |           ~1,158 |      ~40 MB |                ~1.0 GB |               ~1.3 GB |
| Rectangular parish bbox |           ~2,295 |      ~80 MB |                ~1.9 GB |               ~2.6 GB |

Kilteevan Civil Parish boundary reference:

- OSM/Overpass relation: `5247829`
- Name: `Kilteevan Civil Parish`
- Approximate polygon area from fetched boundary geometry: `33.8 km2`

## API Cost Estimates

The full county via API is possible but not currently recommended.

Full clipped County Roscommon, 3x gameplay scale:

- GPT Image 2 medium: roughly `$3k-$8k` all-in planning range;
- GPT Image 2 high: roughly `$8k-$18k`;
- rectangular bbox instead of clipping: multiply by about `2.6x`.

Kilteevan Civil Parish, 3x gameplay scale:

- GPT Image 2 medium: budget about `$100-$200`;
- GPT Image 2 high: budget about `$300-$600`.

Those ranges include practical cushion for input images, retries, rejects, and
repairs. Output-only estimates are lower, but output-only is not a realistic
production workflow.

The important conclusion is not "API is impossible." Parish-scale API runs are
financially plausible. The problem is that current image-to-image tests are not
geometrically faithful enough to scale.

## Provider Image Tests

Cycle CG tested one real Murphy z17 source tile against cheaper image-generation
models via OpenRouter, plus direct Google API attempts.

Direct Google:

- attempted first;
- blocked by HTTP 429 because Google AI Studio prepayment credits are depleted;
- no direct-Google outputs were produced.

OpenRouter tests:

| Model                                | Observed cost | Result                                                                |
| ------------------------------------ | ------------: | --------------------------------------------------------------------- |
| `black-forest-labs/flux.2-klein-4b`  |      `$0.015` | Not usable; too soft/small for 3x runtime tiles.                      |
| `google/gemini-3.1-flash-lite-image` | `$0.03393425` | Best cheap geometry tradeoff, still not production-usable.            |
| `google/gemini-3.1-flash-image`      |  `$0.0678685` | Not usable; recenters/invents roads and compound layout.              |
| `openai/gpt-image-1-mini`            |  `$0.0094855` | Not usable; invents a regular field grid.                             |
| `openai/gpt-image-1`                 |    `$0.04639` | Best-looking art, but still not usable as authoritative map geometry. |
| `sourceful/riverflow-v2.5-fast`      |   `$0.084868` | Attractive but invents too much enclosure/detail.                     |

The key contact sheets are:

- `cycle-cg-single-tile-provider-tests/single-tile-provider-comparison.png`
- `cycle-cg-single-tile-provider-tests/single-tile-gpt-image-1-comparison.png`

Cycle CG verdict:

None of the tested single-tile image-to-image models should be scaled to parish
generation with the current prompt/control setup. The consistent failure is
geometry, not cost.

## Storage And Versioning

Do not put the full map tile pyramid in Git.

Recommended split:

- Git: code, scripts, prompts, docs, release manifests, validation summaries,
  and small review sheets.
- Lore: source tiles, generated tile pyramids, packed PMTiles/MBTiles, large
  contact sheets, repair artifacts, and candidate releases.
- Game install: packed local map bundle such as PMTiles, MBTiles, or a
  compressed tile bundle.

Lore is a strong candidate because it is designed for large binary assets and
supports content-addressed storage, chunking, branching, and durable revision
history. It is still pre-stable, so the next step should be a small Lore pilot
before moving all map assets into it.

## Zoom Levels

The game can be installed locally, so the map does not need a web-style pyramid
for bandwidth reasons. It still benefits from lower zoom levels for rendering,
memory use, minimap/travel UI, and visual quality.

Recommendation:

- generate one max gameplay layer at 3x;
- generate lower overview levels deterministically from the base/max layer;
- do not call the API separately for lower zooms.

A full lower pyramid adds at most about `33%` storage over the max layer.

## Tooling Direction

The current reusable proof CLI is:

- `docs/graphics-v2/scripts/county_tile_pipeline.py`

The proposed full production tool is documented in:

- `docs/graphics-v2/overhead-art/full-county-map-generation-api-design.md`

Core commands for that future tool:

- `plan`
- `ingest-source`
- `build-base`
- `build-pyramid`
- `prepare-api-jobs`
- `submit-api-batch`
- `poll-api-batch`
- `collect-api-results`
- `validate`
- `repair-seams`
- `promote`
- `lore-publish`

The tool should keep a SQLite job ledger and JSON manifests so long-running map
generation is resumable, auditable, and cost-gated.

## Practical Next Step

Do not buy a full API parish generation run yet.

The next useful experiment is a parish-scale deterministic local base for
Kilteevan:

1. Fetch the Kilteevan Civil Parish boundary.
2. Build a clipped z17 source mosaic/job grid.
3. Generate the 3x deterministic base layer.
4. Build lower pyramid levels.
5. Export a packed local bundle.
6. Review whether that base is good enough for walking/minimap use.
7. Only then choose a few named locations for API local override plates.

That path keeps costs low, preserves cartography, and still leaves room for
better art where it matters.
