# Techdebt Phase 3.2: parish-inference architecture (TD-020 + TD-021)

## TD-020 — Extract `inference_with_timeout` helper

**Location:** `parish/crates/parish-inference/src/lib.rs`

**Before:** `spawn_inference_worker` (lines 668-813, ~146 lines) contained three nearly
identical `tokio::time::timeout` + `format!` blocks, one per dispatch arm
(streaming+JSON, streaming only, non-streaming). Each block: construct a
`Duration` from config, wrap a `client.generate_*()` call, match the
`tokio::time::timeout` result, and format an error message with the timeout
value and model name.

**After:** Extracted `inference_with_timeout(future, timeout, timeout_secs, model, label)`
async helper. Each arm now calls the helper with a descriptive label string.
`spawn_inference_worker` reduced to ~116 lines.

**Lines removed:** ~30 lines of duplicated match+format scaffolding.

## TD-021 — Replace hand-rolled XML tag parser with regex

**Location:** `parish/crates/parish-inference/src/anthropic_client.rs`

**Before:** Three functions — `neutralise_structural_tags` (byte-walking loop),
`match_structural_close_at` (tag matching with whitespace/case handling), and
`skip_ascii_ws` (whitespace skipper) — totalling ~68 lines of byte-level XML
close-tag detection. The `STRUCTURAL_TAGS` constant stored `(&[u8], &str)` pairs.

**After:** Replaced with a `LazyLock<Regex>` compiled from the tag-name list
(`(?i)<\s*/\s*(caller_system|engine_instruction)\s*>`). The
`neutralise_structural_tags` function now calls `regex::Regex::replace_all` with a
closure that maps captured tag names to bracketed sentinels via
`STRUCTURAL_TAGS`. `STRUCTURAL_TAGS` simplified to `&[(&str, &str)]`.

**Lines removed:** ~68 bytes of hand-rolled parsing, replaced by ~20 lines of
declarative regex. No new external dependency — `regex` was already in the
workspace manifest.

## Files changed

| File | Change |
|------|--------|
| `parish/crates/parish-inference/Cargo.toml` | Added `regex = { workspace = true }` |
| `parish/crates/parish-inference/src/lib.rs` | Added `inference_with_timeout` helper; simplified `spawn_inference_worker` timeout arms |
| `parish/crates/parish-inference/src/anthropic_client.rs` | Replaced `match_structural_close_at`/`skip_ascii_ws`/byte-loop `neutralise_structural_tags` with `LazyLock<Regex>` + `regex::Regex::replace_all` |
| `parish/crates/parish-inference/TODO.md` | Moved TD-020, TD-021 from Open to Done |

## Commands run

```
cargo test -p parish-inference       # 215 unit + 36 integration: all pass
cargo clippy -p parish-inference --all-targets -- -D warnings   # clean
```

## Test results

- 215 unit tests: 0 failed, 7 ignored (live API tests, require keys)
- 36 integration tests (wiremock): 0 failed
- clippy: 0 warnings
