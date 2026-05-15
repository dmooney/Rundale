# Rust Coverage Expansion Proof

Date: 2026-05-15
Evidence type: gameplay transcript

Scope: targeted Rust test-coverage expansion across Parish/Rundale crates, plus minimal production fixes exposed by the new tests.

Changed coverage areas:

- `parish-config`: provider/env/CLI precedence, cwd isolation, malformed user config.
- `parish-input`: `/new` command policy, BYOK aliases, mention edge cases, LLM request contract.
- `parish-persistence`: WAL/durability settings, concurrent reads, corrupt journal JSON, dangling branch parent rejection.
- `parish-npc`: real Rundale NPC/world consistency integration coverage.
- `parish-types`: serde contracts, conversation/gossip ordering, hardcoded festival contract.
- `parish-world`: geo metadata, coordinate validation, BFS route contract, log cap, real Rundale weather hazards.
- `parish-geo-tool`: merge connection pruning, Overpass selector parity, classification branches, `relative_to` realignment behavior.
- `parish-npc-tool`: validator failure cases and deterministic generation.
- `parish-core`: event filtering, conversation runtime state, system-command effects, movement event ordering.
- `parish-mcp`: JSON-RPC id handling, tool registry contract, translator and backend error behavior.
- `parish-palette`: documented keyframe colors, contrast coverage, normalized public time inputs.
- `parish-server`: request-id middleware contract.

Commands run:

```text
rtk cargo fmt --all -- --check
passed

rtk git diff --check
passed

rtk cargo test -p parish-config
117 passed, 1 ignored

rtk cargo test -p parish-persistence
120 passed

rtk cargo test -p parish-palette
31 passed

rtk cargo test -p parish-types
134 passed

rtk cargo test -p parish-world
158 passed

rtk cargo test -p parish-npc-tool
42 passed

rtk cargo test -p parish-input --lib
145 passed

rtk cargo test -p parish-input
153 passed

rtk cargo test -p parish-npc --test rundale_data_consistency
3 passed

rtk cargo test -p parish-npc
433 passed

rtk cargo test -p parish-geo-tool
117 passed

rtk cargo test -p parish-server request_id_layer
2 passed, 274 filtered out

rtk cargo test -p parish-server
276 passed, 2 ignored

rtk cargo test -p parish --test world_graph_integration
29 passed

rtk cargo test -p parish-core try_recv_skips_filtered_events_without_blocking
1 passed, 414 filtered out

rtk cargo test -p parish-core sync_location
1 passed, 414 filtered out

rtk cargo test -p parish-core system_command
4 passed, 411 filtered out

rtk cargo test -p parish-core
414 passed, 4 ignored

rtk cargo test -p parish-mcp
41 passed
```

Notes:

- `parish-mcp`, `parish-input`, `parish-npc`, `parish-core`, and `parish-server`
  include tests that bind localhost through WireMock or local integration
  harnesses. They were rerun with local-port permission and passed.
- `rtk just agent-check` initially failed only because this proof bundle did not exist yet.
