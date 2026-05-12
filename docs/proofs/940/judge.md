Verdict: sufficient
Technical debt: clear

PR #940 changes a single configuration default — the real-time silence
threshold before nearby NPCs may spontaneously start banter — from 25 s to
120 s. The trigger logic, prompt construction, autonomous-chain caps, and
config-overlay paths are all untouched, so the behavioral surface is
exactly one number.

Evidence: the `default_config` unit test in `parish-core` (updated to
assert `120`) passes; the full `parish-config` (87) and `parish-core` (316)
lib suites pass with no regressions; the CLI binary unit suite (147) also
passes. The override path remains exercised by existing TOML-deserialization
tests that pin an explicit `60` and continue to pass unchanged. No
placeholder/todo debt markers were introduced.

Risk assessment: the change is config-only, default-only, and overrideable
via `parish.toml` under `[engine.session].idle_banter_after_secs`. Users
who explicitly set the value are unaffected. The two non-engine-config
fallbacks (`GameConfig::default()` and the parish-cli hardcoded init) were
updated in lockstep so the three default sites do not silently drift.
