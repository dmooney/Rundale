Evidence type: transcript, diff, test output

Verdict: sufficient

Technical debt: clear

The extraction is a pure refactor — no behavior change. All 412 existing tests pass (including the critical `test_tier1_system_no_unsubstituted_placeholders` test that would catch any format-string drift). Clippy produces zero warnings. The prompt output is byte-for-byte identical: the constant reproduces the same string with `{`/`}` replacing the format-escaped `{{`/`}}`, and the ordering of push operations matches the original concatenation.
