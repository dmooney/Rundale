# Proof Evidence — PR #955: split frontend URL from upstream URL on tile sources

Evidence type: gameplay transcript

## What broke

Two conflated bugs in `parish-config`'s default historic tile source kept
the map blank for every fresh install:

1. **404 on every browser request.** The historic source was registered
   under the key `"historic"` in `default_tile_sources()`
   (`parish-config/src/engine.rs`), but its `url` template hardcoded
   `/tiles/roscommon1/{z}/{x}/{y}.png`. The tile-proxy route handler at
   `parish/crates/parish-server/src/tile_routes.rs:52-60` parses the first
   path segment as the `source_id` and validates it against the registered
   tile-source keys. With the broken URL, every request looked like

       GET /tiles/roscommon1/10/500/350.png

   and the handler short-circuited with `StatusCode::NOT_FOUND` because the
   only registered ids were `historic` and `osm`.

2. **502 on every cache miss, even with #1 fixed.** `init_tile_cache`
   (`parish/crates/parish-server/src/lib.rs:868-873`) populated
   `TileCache.url_templates` from the same `cfg.url` field. The single
   `url` field was being asked to do two incompatible jobs: be a
   same-origin proxy path the browser hits, AND be an absolute upstream
   URL the server-side `reqwest::get` fetches from on a cache miss. Even
   after fixing the path segment, `reqwest::get("/tiles/historic/...")`
   would error because the URL is relative. Since there is no tile
   pre-seeding mechanism anywhere in the tree, this means a fresh user
   would never load a single historic tile.

Spotted by gemini-code-assist's review of the first cut of this PR.

## What changed in this PR

### Layer split — `TileSourceConfig`

Added an `upstream_url` field to `TileSourceConfig`
(`parish/crates/parish-config/src/engine.rs`). `url` is now exclusively
the URL the **frontend** hits; `upstream_url` is the URL the
**server-side cache** fetches from on a miss. The two layers can now
diverge cleanly:

| Source     | `url` (browser → server)                       | `upstream_url` (server → upstream)                                                  |
|------------|------------------------------------------------|-------------------------------------------------------------------------------------|
| `historic` | `/tiles/historic/{z}/{x}/{y}.png`              | `https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`         |
| `osm`      | `https://tile.openstreetmap.org/{z}/{x}/{y}.png` | _(empty — OSM is fetched directly by the browser)_                                |

### Cache wiring

`init_tile_cache` (`parish-server/src/lib.rs:868-880`) now builds
`url_templates` from `upstream_url`, and filters out entries whose
`upstream_url` is empty. Sources without an `upstream_url` (like OSM)
are simply absent from the cache map, so any stray `/tiles/osm/...`
request is rejected by `TileCache::get` with `not registered` before
any I/O.

### Tests

- `engine.rs:test_map_config_default_has_both_sources` re-anchored: now
  asserts `historic.url.starts_with("/tiles/historic/")` AND
  `historic.upstream_url.starts_with("https://mapseries-tilesets…/os/roscommon1/")`,
  pinning both layers.
- `engine.rs:proxy_path_segment_matches_registered_source_id` — new
  invariant test: for any same-origin tile URL
  (`/tiles/<seg>/{z}/{x}/{y}.png`), the first path segment must equal the
  registering key.
- `tiles.test.ts` fixture URL updated.

## Request-flow demonstration

Before, on a fresh install:

```
GET /tiles/roscommon1/10/500/350.png
 → tile_routes::get_tile
   → source_id "roscommon1" not in registered keys ("historic", "osm")
   → 404 "unknown tile source"
```

After the surface fix only (still bad — Gemini's flag):

```
GET /tiles/historic/10/500/350.png       (validates OK)
 → TileCache::get("historic", ...)
   → cache miss
   → reqwest::get("/tiles/historic/...")
     → URL is not absolute → error
   → 502
```

After this PR:

```
GET /tiles/historic/10/500/350.png       (validates OK)
 → TileCache::get("historic", ...)
   → cache miss
   → reqwest::get("https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/10/500/350.png")
     → 200 image/png
   → persist to tile_cache/historic/10/500/350.png
   → 200 image/png
```

## Test transcript

```
$ cargo test -p parish-config --lib
test result: ok. 110 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

```
$ cargo build -p parish-server
   Compiling parish-server v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

```
$ npx vitest run src/stores/tiles.test.ts
Test Files  1 passed (1)
     Tests  6 passed (6)
```

## Out-of-scope follow-ups

- Route handler currently still validates against all `tile_sources` keys
  (including OSM). A `/tiles/osm/...` request would pass route validation
  and then 502 from `TileCache::get`. Better to 404 earlier — file a
  follow-up to validate against the proxied subset only.
- CSP `connect-src` correctly excludes NLS S3 because the browser never
  hits it directly (only the server does, via `reqwest`). No CSP change
  needed.
