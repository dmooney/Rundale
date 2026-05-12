# Proof Evidence — PR #955: align historic tile source URL with registered source id

Evidence type: gameplay transcript

## What broke

The historic tile source was registered under the key `"historic"` in
`default_tile_sources()` (`parish-config/src/engine.rs`), but the same entry's
`url` template hardcoded `/tiles/roscommon1/{z}/{x}/{y}.png`.

The tile-proxy route handler at
`parish/crates/parish-server/src/tile_routes.rs:52-60` parses the first path
segment as the `source_id` and validates it against the registered tile-source
keys before forwarding to `TileCache`. With the broken URL, every browser tile
request looked like

    GET /tiles/roscommon1/10/500/350.png

and the handler short-circuited with `StatusCode::NOT_FOUND` because the only
registered ids were `historic` and `osm`. The map stayed blank for every user
who selected the historic source (the default).

## What changed in this PR

- `parish/crates/parish-config/src/engine.rs:788` — URL template
  `/tiles/roscommon1/{z}/{x}/{y}.png` → `/tiles/historic/{z}/{x}/{y}.png`,
  so the path segment matches the registry key.
- `parish/crates/parish-config/src/engine.rs` — re-anchored the existing
  `test_map_config_default_has_both_sources` assertion off the literal string
  `"roscommon1"` and onto the invariant
  `historic.url.starts_with("/tiles/historic/")`.
- `parish/crates/parish-config/src/engine.rs` — added a new
  `proxy_path_segment_matches_registered_source_id` test that pins the
  invariant generically: every same-origin tile URL's first path segment
  must equal its registering key.
- `parish/apps/ui/src/stores/tiles.test.ts:25` — updated the frontend
  fixture URL to match.

## Request-flow demonstration

Before the fix, the same-origin request the browser produces against the
default historic source:

```
GET /tiles/roscommon1/10/500/350.png
 → parish-server tile_routes::get_tile
   → source_id = "roscommon1"
   → known = [("historic", _), ("osm", _)].iter().any(|(id,_)| id == "roscommon1")
   → known == false
   → 404 "unknown tile source"
```

After the fix:

```
GET /tiles/historic/10/500/350.png
 → parish-server tile_routes::get_tile
   → source_id = "historic"
   → known = true
   → TileCache::get("historic", 10, 500, 350)
     → cache hit:  read tile_cache/historic/10/500/350.png  → 200 image/png
     → cache miss: fetch upstream from url_templates["historic"], persist, serve
```

## Test transcript

```
$ cargo test -p parish-config --lib -- proxy_path_segment test_map_config_default
running 2 tests
test engine::tests::test_map_config_default_has_both_sources ... ok
test engine::tests::proxy_path_segment_matches_registered_source_id ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 108 filtered out
```

```
$ cargo test -p parish-config --lib
test result: ok. 110 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

```
$ npx vitest run src/stores/tiles.test.ts
Test Files  1 passed (1)
     Tests  6 passed (6)
```

## Known follow-up (not in this PR)

`init_tile_cache` (`parish-server/src/lib.rs:868-873`) populates
`TileCache.url_templates` from the same `url` field. After this PR, the
historic entry's `url` is a same-origin relative path, so
`reqwest::get("/tiles/historic/...")` on a cache miss will error with
"URL is not absolute" and return 502. Cached tiles serve fine because they
short-circuit before the upstream fetch. A separate `upstream_url` field
(or routing layer that knows the NLS S3 path) is needed to make cold-cache
historic tiles work end to end. The OSM source is unaffected — its `url`
is already an absolute upstream URL.
