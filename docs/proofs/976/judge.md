Verdict: sufficient
Technical debt: clear

PR #976 adds GitHub Models as an OpenAI-compatible inference provider. The
changes are well-scoped: a TOML provider file, a `completions_path` builder on
`OpenAiClient`, a guarded dispatch in `build_client` and `validate`, and a named
constructor. No enum variants were added (the TOML-based registry makes that
unnecessary). All three GitHub Models validation paths (success, 404 fallback,
auth failure) are covered by wiremock tests.

The eval harness (player agent, judge script, scenarios, rubrics, CI workflow)
is purely additive — new files under `parish/testing/eval/` and a new workflow
that only runs on schedule/dispatch. It does not alter any existing test or
gameplay path.

Evidence: 129 parish-config tests pass, 28 parish-inference lib tests pass
(including 3 GitHub Models–specific validate tests), fmt and clippy clean.
