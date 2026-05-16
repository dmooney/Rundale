# Proof Evidence — Offline Map Tile Bundling (Phase D.2, partial)

Evidence type: gameplay transcript

## What shipped

Three-tier tile lookup in `TileCache`: user cache dir → optional read-only
bundled dir → upstream fetch. Bundled-dir population path is **not** shipped
in this PR — see "Why no scraper" below. The runtime infra is wired so that
once a bundle is delivered (separately), populating `mods/rundale/tiles/`
(or any path resolved via env var / TOML) is a no-code-change operation.

## What changed

| File | Change |
|------|--------|
| `parish/crates/parish-core/src/tile_cache.rs` | `bundled_dir: Option<PathBuf>` field + `with_bundled_dir()` builder + 3-tier lookup in `get()` + 4 new tests |
| `parish/crates/parish-config/src/engine.rs` | `MapConfig::bundled_tiles_dir: Option<PathBuf>` (`#[serde(default)]`) + 3 new tests + CC-BY licence comment corrections |
| `parish/crates/parish-server/src/lib.rs` | `init_tile_cache` accepts `data_dir` + resolves bundled dir (env var → TOML → `{data_dir}/tiles` conventional default if directory exists) |
| `parish/parish.example.toml` | Documents `bundled_tiles_dir` option; corrects licence reference to CC-BY |
| `THIRD_PARTY_NOTICES.md` | Corrects NLS licence from CC-BY-SA 3.0 → CC-BY; documents required attribution string |
| `README.md` | Corrects NLS licence reference + attribution |
| `docs/design/map-evolution.md` | Phase D.2 marked partial; new "Offline tile bundling" section explains deferred scraper, MapTiler cost analysis, NLS WAF/email pathway; "Open Questions" updated |
| `.gitignore` | Adds `mods/rundale/tiles/` for the optional bundle directory |

## Test transcripts

```
$ cargo test -p parish-core --lib tile_cache
test tile_cache::tests::cache_dir_hit_takes_precedence_over_bundled_dir ... ok
test tile_cache::tests::bundled_dir_missing_silently_falls_through ... ok
test tile_cache::tests::bundled_dir_hit_skips_upstream ... ok
test tile_cache::tests::get_unknown_source_returns_config_error ... ok
test tile_cache::tests::get_empty_source_returns_config_error ... ok
test tile_cache::tests::get_unsafe_source_returns_config_error ... ok
test tile_cache::tests::tile_path_is_deterministic ... ok
test tile_cache::tests::bundled_dir_miss_falls_through_to_upstream ... ok
test tile_cache::tests::get_cache_miss_fetches_from_upstream_then_hit_reads_disk ... ok
test tile_cache::tests::get_upstream_failure_returns_network_error ... ok
test result: ok. 10 passed; 0 failed
```

```
$ cargo test -p parish-config --lib
test result: ok. 132 passed; 0 failed; 0 ignored
```

```
$ cargo build -p parish-server
    Finished `dev` profile target(s)
$ cargo build -p parish-geo-tool
    Finished `dev` profile target(s)
```

## Three-tier lookup demonstrated

`TileCache::get(source_id, z, x, y)` now resolves:

1. `cache_dir/{source_id}/{z}/{x}/{y}.png` — mutable per-user cache. Hit
   returns immediately.
2. `bundled_dir/{source_id}/{z}/{x}/{y}.png` — optional read-only bundle.
   Hit returns immediately without writing to `cache_dir`. Tested via
   `bundled_dir_hit_skips_upstream` (mock server with no mocks would 404 any
   request; bundled hit avoids it).
3. Upstream fetch → persisted to `cache_dir`. Tested via
   `bundled_dir_miss_falls_through_to_upstream`.

The `cache_dir_hit_takes_precedence_over_bundled_dir` test writes different
content to both directories and verifies `cache_dir` wins. The
`bundled_dir_missing_silently_falls_through` test confirms a configured-but-
nonexistent path doesn't crash — the cache simply falls through to the
upstream branch.

## Why no scraper

We considered two population paths and shelved both:

1. **Tile-scrape against NLS S3** (`mapseries-tilesets.s3.amazonaws.com`).
   Bucket is publicly served, no robots.txt, but NLS migrated their
   *official* tile service to MapTiler Cloud in April 2022 (metered: 100k/mo
   free, $0.10/1k overage = ~$620 for the island at z=12–17). Scraping the
   legacy S3 endpoint at ~6.3M tiles / 30–60 GB conflicts with the spirit of
   NLS guidance: "use of online service or tiles in commercial websites or
   applications must be confirmed from NLS's side".

2. **GeoTIFF-scrape against `maps.nls.uk`** for the 1,940 first-edition
   sheets. The host is behind AWS WAF — `curl` and Claude Code's `WebFetch`
   both get JS-challenge / 405 pages. The natural path is a
   headed/headless browser run or the documented email request flow
   (`maps@nls.uk`).

Current shipping behaviour therefore remains: server fetches tiles from
the NLS S3 bucket on demand, disk-caches each tile after first request,
serves cached copies thereafter. Bounded at one upstream hit per unique
tile per server instance.

## Licence correction (drive-by)

This PR also corrects a stale licence claim. The repo previously stated
**CC-BY-SA 3.0** in multiple sites (`engine.rs`, `THIRD_PARTY_NOTICES.md`,
`README.md`, `parish.example.toml`). Direct verification against the live
NLS copyright page (`https://maps.nls.uk/copyright.html`) and the per-sheet
viewer's licence link (`#noncommercial` anchor on the same page) showed the
actual licence is **CC-BY** (no version specified by NLS). The required
attribution per NLS is *"Reproduced with the permission of the National
Library of Scotland"*. Downstream may be relicensed under CC-BY-SA via the
standard one-way CC compatibility direction — so no Rundale-side relicensing
is required, only the textual correction.
