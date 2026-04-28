# Regression Audit: Developer Tools

Scope: `parish-geo-tool` (OSM extraction, lat/lon pinning, world graph
building), `parish-npc-tool`. These don't ship to end users at runtime
but are used to *generate* shipped data (`mods/rundale/world.json` etc.),
so regressions can poison the default mod silently.

## 1. Sub-features audited

- `parish-geo-tool` modules: `extract.rs`, `cache.rs`, `connections.rs`,
  `descriptions.rs`, `lod.rs`, `merge.rs`, `osm_model.rs`, `output.rs`,
  `overpass.rs`, `pipeline.rs`, plus binaries under `bin/`
- `parish-geo-tool` features per repo skills doc: coordinate resolver
  (absolute + `relative_to` + graph-delta fallback), `geo_kind`
  (real/manual/fictional), `realign_rundale_coords`, Overpass/Nominatim
  integration
- `parish-npc-tool` (single `main.rs`)

## 2. Coverage matrix

| Sub-feature / module | In-source tests | Integration / dedicated tests dir |
|---|---|---|
| `parish-geo-tool` (whole crate) | 90 in-source `#[test]` markers across 12 source files | **none** — `crates/parish-geo-tool/tests/` does not exist |
| `parish-npc-tool` (whole crate) | 10 in-source `#[test]` markers in `main.rs` | **none** — `crates/parish-npc-tool/tests/` does not exist |
| Coordinate resolver (absolute + relative + graph-delta) | likely in-source in `parish-geo-tool/src/` (need closer inspection) | none |
| Overpass / Nominatim HTTP integration | unclear from grep; no `tests/http_mock_*` exists for geo-tool | none |
| `geo_kind` real/manual/fictional dispatch | in-source only | none |
| `realign_rundale_coords` (data-mutating subcommand) | in-source only | none |
| CLI argument parsing (clap) | in-source only | none |

## 3. Strong spots

- 90 in-source unit tests in `parish-geo-tool` is non-trivial for a
  developer tool — the team clearly cares about this surface.
- The crate is split into 12 focused modules (cache, connections,
  descriptions, extract, lod, merge, osm_model, output, overpass,
  pipeline) which makes per-module unit testing cheap.

## 4. Gaps

- **[P1] No HTTP-mock test for Overpass / Nominatim integration.**
  These are the upstream data sources; if the response shape changes
  (or our parsing drifts) the next `realign_rundale_coords` run could
  silently produce bad coordinates that ship in a future
  `mods/rundale/world.json`. Suggested integration test in
  `crates/parish-geo-tool/tests/overpass_mock_tests.rs` mirroring the
  pattern of `parish-inference/tests/http_mock_tests.rs`.
- **[P1] No round-trip test pinning a known place to known coordinates.**
  Add a fixture (a stored Overpass response for one Roscommon village)
  and a test asserting the pipeline produces the expected lat/lon
  pair. Costs ~50 lines, closes the silent-data-poisoning vector
  end-to-end.
- **[P1] Coordinate-resolver fallback chain (absolute → `relative_to`
  → graph-delta) is in-source-only.** A regression that always picks
  the wrong layer (e.g. always uses graph-delta when an absolute is
  available) would be silent. Suggested unit test asserting layer
  ordering on a synthetic graph.
- **[P2] `parish-npc-tool` is essentially untested at the integration
  level.** 10 in-source tests in a single 1-file binary is light.
  Suggested: at least one CLI smoke test exercising the binary's
  primary subcommand.
- **[P2] No CLI argument-parsing smoke test for either tool.** clap
  errors are usually fine, but flag renames are silent until the next
  user runs the command. Suggested: one `cargo run -p parish-geo-tool
  -- --help` snapshot.

## 5. Recommendations

1. **Add Overpass/Nominatim HTTP mocks** — same pattern as the
   inference crate, prevents silent geo-data poisoning.
2. **Pin a "known-place coordinate" round-trip test** so changes to
   the pipeline are caught before they ship to the default mod.
3. **Add a coordinate-resolver fallback-order unit test.**
4. **Promote dev-tool gaps from P2 → P1** if/when the team starts
   regularly running `realign_rundale_coords` to update shipped
   coordinates — until then, the runtime gameplay surface is higher
   priority.
