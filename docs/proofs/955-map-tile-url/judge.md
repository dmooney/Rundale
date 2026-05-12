# Judge Verdict — PR #955: align historic tile source URL with registered source id

## Review scope

Reviewed the full diff of `claude/fix-map-tiles-display-FDvSO` against
`origin/main`: two production touchpoints (`engine.rs` URL template, frontend
test fixture) plus one new invariant test and an updated assertion in the
existing `test_map_config_default_has_both_sources`.

## Diagnosis

Independent verification of the bug claim:

- `parish/crates/parish-server/src/tile_routes.rs:52-60` validates the
  first path segment of `/tiles/{path}` against keys in
  `template_config.tile_sources` and returns `404` on mismatch.
- `parish/crates/parish-server/src/lib.rs:781` shows
  `template_config.tile_sources` is populated from
  `engine_config.map.id_label_pairs()`, which uses the BTreeMap keys
  (`"historic"`, `"osm"`) — not the per-source `url` strings.
- Before the patch, `default_tile_sources()` registered `"historic"` with
  `url = "/tiles/roscommon1/{z}/{x}/{y}.png"`. The frontend uses
  `tileSource.url` directly (`parish/apps/ui/src/lib/map/style.ts:368`), so
  every browser request hit the 404 path.

The fix is surgical and correct: rewrite the URL's path segment to match the
registering key, so route-handler validation passes and `TileCache` looks up
the right entry.

## Test coverage assessment

- `test_map_config_default_has_both_sources` previously asserted
  `historic.url.contains("roscommon1")`, which pinned the bug as
  "expected". The new assertion (`starts_with("/tiles/historic/")`) anchors
  the invariant instead of the implementation string.
- The new `proxy_path_segment_matches_registered_source_id` test generalises
  the invariant across all same-origin tile sources, so adding a future
  source under the same broken pattern fails at test time rather than at
  runtime.
- The frontend fixture (`tiles.test.ts`) was a copy of the broken URL; it's
  been updated and the 6 existing tests still pass.

## Risk assessment

Low. The change is a one-token edit to a default config string plus two
small test updates. No public API changes, no schema migrations, no behaviour
changes for the `osm` source (whose `url` is an absolute upstream URL and
correctly bypasses the proxy). User configs that explicitly override
`[engine.map.tile_sources.historic].url` are unaffected since
`apply_defaults` only fills in missing entries.

## Open issue acknowledged

The PR body and `evidence.md` both flag a latent issue in
`init_tile_cache` (`parish-server/src/lib.rs:868-873`): the same `url` field
is also used as the upstream-fetch template, so cache misses on the historic
source will now return 502 because `/tiles/historic/...` is not an absolute
URL. This is out of scope for this PR but should be tracked as a follow-up;
it's a separate, latent architectural conflation, not a regression
introduced by #955. Cached tiles (the common case) serve correctly.

## Verdict

Verdict: sufficient

Technical debt: clear
