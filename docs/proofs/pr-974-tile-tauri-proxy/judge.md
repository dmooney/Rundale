Verdict: sufficient
Technical debt: clear

PR #974 adds a `has_tile_proxy: bool` parameter to
`TileSourceSnapshot::list_from_map_config` so each runtime (server vs
Tauri) selects the URL form its webview can actually fetch.

Evidence: two unit tests in `parish/crates/parish-core/src/ipc/types.rs`
cover both modes — proxy mode keeps `/tiles/historic/...`, no-proxy mode
substitutes the upstream S3 URL for the historic source and keeps the
direct OSM URL untouched. Both tests pass.

The change is minimal and surgical: one signature change in
`parish-core`, plus a constant `true` / `false` at the two call sites in
`parish-server` and `parish-tauri`. No placeholder debt markers. No
behavior change for the server path.
