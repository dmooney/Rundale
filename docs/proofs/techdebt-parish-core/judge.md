Verdict: sufficient
Technical debt: clear
All changes were minimal, behavior-safe, and verified by `cargo test -p parish-core` (322 tests pass) and `cargo clippy -p parish-core -- -D warnings` (no warnings). Config items removed unused deps; code items eliminated dead code without behavior change; test items added meaningful coverage; doc items resolved contradictions. Deferred items are recorded as follow-up in TODO.md with explicit reasons. No gameplay code was modified, so no gameplay transcript is needed — this is a pure refactor/techdebt cleanup.
