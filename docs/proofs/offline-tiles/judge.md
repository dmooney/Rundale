# Judge Verdict — Offline Map Tile Bundling (Phase D.2, partial)

## Architectural fit

**Rule #1 (module ownership):** `TileCache` logic lives in `parish-core`. The
bundled-dir resolution is in `parish-server::init_tile_cache` (entry-point
wiring). No leaf-crate logic is duplicated in entry-point crates. Pass.

**Rule #9 (runtime paths from config, not cwd):** Bundled dir resolved once
at startup — env var `PARISH_BUNDLED_TILES_DIR` → TOML `[engine.map] bundled_tiles_dir`
→ `{data_dir}/tiles` default — and stored on `GlobalState`. No handler calls
`current_dir()` or probes the filesystem at request time. Pass.

**Rule #12 (cross-runtime orchestration in parish-core):** `TileCache` (the
lookup logic) lives in `parish-core`. Configuration assembly is in
`parish-server::init_tile_cache` — correctly scoped to the entry-point crate.
The Tauri and CLI entry points do not use `TileCache` (tile serving is
server-only); no changes needed there. Pass.

**Architecture-fitness test (rules #1/#2):** `parish-core` adds no new
dependencies. Pass.

**Backward compatibility:** `TileCache::new()` signature unchanged. All
existing call sites compile without modification. `MapConfig` gains one
`Option<PathBuf>` field with `#[serde(default)]` — existing `parish.toml`
files deserialise to `None`. No breaking changes. Pass.

## Security

- CodeQL taint chain: bundled-dir path construction reuses the existing
  `safe_dir` (derived from the config key via `file_name()` sanitiser), not
  the raw `source_id` HTTP param. SSRF and path-traversal protections intact.
- No external network code added in this PR (the bundled-dir tier is
  read-only filesystem only).

## Licence correction

Drive-by fix of a stale CC-BY-SA 3.0 claim across the repo. Direct
verification against the live NLS copyright page confirmed the actual
licence is plain CC-BY. The correction is more permissive than the prior
claim, so downstream relicensing under CC-BY-SA (the previous assumption)
remains valid via standard one-way CC compatibility. Attribution string
updated to NLS's documented preferred form: "Reproduced with the permission
of the National Library of Scotland".

## Scope reduction

The first cut of this PR included two scraper scripts (Rust `download-tiles`
subcommand + Python `download-nls-sheets.py`) for pre-seeding the bundled
dir. Both were removed before landing — see `evidence.md` "Why no scraper"
for the rationale. The runtime infra (bundled-dir lookup, config plumbing)
is shipped because it's a small, well-tested, opt-in no-op when unset, and
saves future code change once a tile bundle is delivered via NLS-coordinated
channels.

## Verdict

Verdict: sufficient

Technical debt: clear

All rules satisfied. Tests cover the three-tier lookup exhaustively. No
proof debt introduced. The deferred-scraper decision is documented in
`docs/design/map-evolution.md` Phase D.2 section so future contributors
have full context.
