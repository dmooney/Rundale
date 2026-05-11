Verdict: sufficient
Technical debt: clear

Follow-up to #921 that finishes three items the prior PR marked Done in the table but did not fully resolve in code (TD-002 Cargo.toml, TD-009 no-op test, TD-010 weak test) and reconciles the parish-core TODO.md so the Open list reflects reality. All 339 parish-core tests pass (316 unit + 14 integration + 6 wiring + 3 architecture-fitness); `cargo clippy -p parish-core --all-targets -- -D warnings` is clean. Pure cleanup — Cargo.toml dep relocation, two dead-test deletions, TODO.md doc update. No production code modified, no gameplay path touched, so no gameplay transcript beyond the verification output is needed. The remaining `parish-core` debt (TD-005/006/008/011/012) stays in Follow-up with explicit reasons; no new debt was introduced.
