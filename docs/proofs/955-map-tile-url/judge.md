# Judge Verdict — PR #955: split frontend URL from upstream URL on tile sources

## Review scope

Reviewed the full diff of `claude/fix-map-tiles-display-FDvSO` against
`origin/main`: the layer split on `TileSourceConfig` (new `upstream_url`
field), `init_tile_cache` rewiring in `parish-server`, the new
`proxy_path_segment_matches_registered_source_id` invariant test, updated
assertions in `test_map_config_default_has_both_sources`, and the frontend
test fixture update.

## Diagnosis

Independent verification of both bugs the PR addresses:

- **Bug #1 (404 on every request):**
  `parish-server/src/tile_routes.rs:52-60` validates the URL's source-id
  segment against `template_config.tile_sources` keys (populated from the
  `EngineConfig::map.tile_sources` BTreeMap via `id_label_pairs()` at
  `lib.rs:781`). Before the PR, the historic source was registered under
  `"historic"` but its `url` hardcoded `/tiles/roscommon1/...`, so every
  request 404'd.
- **Bug #2 (502 on every miss, latent):** `init_tile_cache`
  (`lib.rs:868-873` before this PR) populated the cache's
  `url_templates` from `cfg.url`. With `cfg.url` set to a same-origin
  relative path, `reqwest::get` on the upstream-fetch path would fail
  with "URL is not absolute". I confirmed no pre-seeding mechanism
  exists anywhere in the tree (`grep -r tile-cache` → only
  `mkdir_all`/runtime-path resolution, never population), so cache
  misses are the common case and the latent issue is user-blocking.

The fix is the correct layer split: `url` for the browser-facing URL,
`upstream_url` for the server-side fetch. Default values: historic gets
the proxy path *and* the absolute NLS S3 URL; OSM keeps its absolute
upstream URL as `url` (browser fetches directly) with empty
`upstream_url` (no proxying).

## Architectural fit

- Aligns with the layer-separation guidance referenced in Gemini's review:
  IPC configuration and engine configuration can diverge; same logic
  applies to "what the browser fetches" vs "what the server fetches".
- The `init_tile_cache` filter on `!cfg.upstream_url.is_empty()` means
  un-proxied sources naturally don't appear in `url_templates`, so
  `TileCache::get` rejects them with the existing "not registered"
  error rather than attempting a relative-URL fetch.
- No CSP changes required (browser still only connects to `self` for
  historic tiles and `tile.openstreetmap.org` for OSM, matching the
  existing `security_headers.rs` assertions).
- No mode-parity concerns: the tile-proxy route lives only in
  `parish-server`; Tauri and CLI builds don't ship the web tile-fetch
  path. (`parish-tauri` doesn't depend on `tile_routes`.)

## Test coverage assessment

- The new `proxy_path_segment_matches_registered_source_id` test enforces
  the invariant that any same-origin tile URL's first path segment
  matches its registering key — catches the exact regression class that
  shipped originally.
- The updated `test_map_config_default_has_both_sources` pins both
  layers: frontend URL (`/tiles/historic/`) AND upstream URL
  (`https://mapseries-tilesets…/os/roscommon1/`).
- Both pre-existing `tile_cache.rs` test paths (hit/miss/network-error)
  remain valid because they construct `TileCache` directly with a
  hand-rolled url_templates map.

## Risk assessment

Low-to-moderate. The new `upstream_url` field is `#[serde(default)]`, so
user TOML configs without it still deserialise (defaulting to empty,
which means "browser fetches directly", which matches OSM-style sources).
The only behaviour change for a user with a custom historic source is
that they now need to set both `url` and `upstream_url` for the cache to
fetch on misses — but that's a strict improvement: previously the cache
fetch was broken regardless.

## Open follow-ups (not in this PR)

- `tile_routes::get_tile` validates against the full `tile_sources` key
  set; a `/tiles/osm/...` request would pass validation and then 502
  from `TileCache::get`. Should be 404. Tracked in `evidence.md` as a
  follow-up.

## Verdict

Verdict: sufficient

Technical debt: clear
