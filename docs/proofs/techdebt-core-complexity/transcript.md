# parish-core Complexity Refactor — TD-011 + TD-012

## What changed

### TD-011: `handle_command` match extraction

`parish/crates/parish-core/src/ipc/commands.rs`

The 434-line `handle_command` match (50+ arms) was refactored into a thin dispatch
that delegates to 9 sub-functions, each owning a coherent group of command variants:

| Function | Lines | Command groups |
|----------|-------|----------------|
| `handle_time_control_command` | 60 | Pause, Resume, Status, ShowSpeed, SetSpeed, InvalidSpeed, Wait, Tick |
| `handle_info_command` | 49 | About, NpcsHere, Time |
| `handle_sidebar_improv_command` | 18 | ToggleSidebar, ToggleImprov |
| `handle_provider_command` | 43 | ShowProvider, SetProvider, ShowModel, SetModel, ShowKey, SetKey |
| `handle_cloud_provider_command` | 45 | ShowCloud, SetCloudProvider, ShowCloudModel, SetCloudModel, ShowCloudKey, SetCloudKey |
| `handle_category_provider_command` | 50 | ShowCategoryProvider, SetCategoryProvider, ShowCategoryModel, SetCategoryModel, ShowCategoryKey, SetCategoryKey |
| `handle_preset_command` | 59 | ShowPreset, ApplyPreset |
| `handle_flag_command` | 30 | Flags, Flag(List/Enable/Disable), InvalidFlagName, InvalidBranchName |
| `handle_theme_command` | 45 | Theme |

**Before**: `handle_command` = 434 lines (match body only).
**After**: `handle_command` = 70 lines (dispatch), plus 9 sub-functions totaling ~399 lines.
The 50+ match arms are now grouped into 9 dispatch patterns, one per sub-function.

### TD-012: `build_npc_debug_list` sub-builders

`parish/crates/parish-core/src/debug_snapshot.rs`

The 184-line `build_npc_debug_list` with deeply nested closures was split into 6
named sub-builders:

| Function | Lines | Purpose |
|----------|-------|---------|
| `build_npc_schedule_debug` | 45 | Schedule variant resolution with active/current indicators |
| `build_npc_relationship_debug` | 30 | Relationship list with strength-descending sort |
| `build_npc_memory_debug` | 11 | Recent short-term memory entries (cap 10) |
| `build_npc_long_term_memory_debug` | 11 | Long-term memory entries |
| `build_npc_reaction_debug` | 11 | Reaction log entries (newest first) |
| `build_npc_deflated_summary_debug` | 9 | Optional deflated summary |

**Before**: `build_npc_debug_list` = 184 lines.
**After**: `build_npc_debug_list` = 71 lines, plus 6 sub-functions totaling ~117 lines.

## Files changed

- `parish/crates/parish-core/src/ipc/commands.rs` — TD-011
- `parish/crates/parish-core/src/debug_snapshot.rs` — TD-012
- `parish/crates/parish-core/TODO.md` — moved TD-011, TD-012 to Done

## Commands run

```
cargo test -p parish-core     # 318 unit + 84 integration = 402 tests, all pass
cargo clippy -p parish-core --all-targets -- -D warnings   # clean
```

## Test results

All 402 tests pass (parish-core unit tests + architecture_fitness + async_llm_integration
+ db_session_store + identity_contract + mod_artefact_malformed_input + save_integration
+ wiring_parity). No behavioral changes, no warnings.

## Risk mitigation

- Each sub-function is `match cmd { ... _ => unreachable!() }` so adding new variants
  causes a compile-time error (exhaustiveness).
- The public API surface (`pub fn handle_command`, `pub fn render_look_text`) is unchanged.
- All existing tests pass without modification — proof of behavioral equivalence.
