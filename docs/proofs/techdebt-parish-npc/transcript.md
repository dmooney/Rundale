Evidence type: gameplay transcript

# Proof: parish-npc TD-016 through TD-025

## Verification Commands Run

```bash
cd /Users/dmooney/.local/share/opencode/worktree/0f92a6c34fd6936c2d85b0ba887a0ef066b798dd/techdebt/parish-npc/parish
cargo fmt --all
cargo clippy -p parish-npc -p parish-core -p parish-tauri -p parish
cargo test -p parish-npc
```

## Results

- `cargo fmt --all`: clean (no changes)
- `cargo clippy -p parish-npc -p parish-core -p parish-tauri -p parish`: clean (no warnings)
- `cargo test -p parish-npc`: **400 passed, 0 failed**

## Behavior Safety

All changes are pure refactoring:
- No public API signatures changed (existing methods kept as thin wrappers)
- No game logic altered
- No new dependencies added
- Dead code removed without affecting call sites
