Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

# Judge — Stylized NLS Tile Pipeline

## Summary

The PR implements a pre-generated illustrated parchment tile pipeline for the Rundale
map, replacing the raw NLS historic OS Ireland tiles with a stylized "RDR2-inspired"
aesthetic when tiles are seeded offline. The implementation is clean, well-tested, and
correctly scoped.

## Criteria verification

**AC-1 (parchment pipeline):** Seven unit tests confirm the pipeline is deterministic,
produces correct palette mappings (ink, water, woodland, parchment background), and
round-trips through PNG encode/decode without data loss. Sobel edge detection is
hand-rolled using luma gradients to avoid imageproc API uncertainty. fastnoise-lite
Perlin grain is seeded by tile coordinates ensuring reproducible output.

**AC-2 (diffusion fallback):** Tested against an unreachable endpoint (port 1). The
fallback path returns a valid parchment PNG, confirming no tile is ever silently lost.
`tracing::warn!` is emitted to alert the operator.

**AC-3 (seed-tiles seeder):** The seeder correctly enumerates tiles from a bbox via
Web Mercator XYZ math (verified against known Roscommon coordinates at z=14). Bounded
concurrency via `Semaphore + JoinSet`, atomic writes via temp-rename, and disk cache
for raw tiles to support interrupted-and-resumed runs are all implemented.

**AC-4 (rundale-map config):** All 132 `parish-config` tests pass including the five
updated tests that verify the new 3-source count. The `rundale-map` source uses the
correct NLS S3 upstream URL and CC-BY attribution string.

**AC-5 (CC-BY licence):** `docs/licenses/NLS_CC-BY_derivative.txt` contains the
complete derivative notice per CC-BY requirements. `mods/rundale/mod.toml` references
the notice file. The `TileSourceConfig.attribution` string is surfaced in MapLibre's
attribution control for every map view.

**AC-6 (no regression):** Live transcript confirms the game boots, loads `mods/rundale/`
content, and processes movement commands correctly with the modified `mod.toml`.

## Debt assessment

No incomplete-implementation markers or placeholder patterns in changed files. The `TileCache`
is not modified — art generation is strictly offline, clean separation of concerns.
The `parish-tile-art` crate is correctly isolated as a leaf crate used only by
`parish-geo-tool`. No backend-specific deps were introduced into shared crates.

Pre-release coordination with NLS (`geo@nls.uk`) is documented in the licence notice
and design doc — this is an existing obligation, not new debt introduced by this PR.
