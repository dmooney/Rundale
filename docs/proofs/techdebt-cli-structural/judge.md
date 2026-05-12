Evidence type: transcript + code diff + test/clippy output

Verdict: sufficient

Technical debt: clear

All 12 TODO items (TD-002 through TD-012) from the parish-cli structural sweep are resolved:
- run_headless: 525 → 97 lines with 4 extracted helpers
- handle_headless_game_input: 237 → 81 lines with streaming extracted
- resolve_category_configs: 200 → 21 lines with 3 extracted helpers
- App struct: 74 → 62 fields via CategoryOverride dedup
- build_cli_category_overrides: 33 → 17 lines via tuple iteration
- Schedule event processors: merged into shared generic helper
- Snapshot loading: extracted into shared load_and_restore_snapshot
- Tier ticks: extracted into 6 dispatch_headless_* functions
- main(): 172 → 40 lines with 3 extracted helpers

cargo test -p parish: all pass
cargo clippy --all-targets -- -D warnings: clean
No behavior changes — pure structural refactor.
